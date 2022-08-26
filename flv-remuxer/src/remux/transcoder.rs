use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Child;
use tokio::process::Command;
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptionItem {
    global: Option<Vec<String>>,
    input: Option<Vec<String>>,
    output: Option<Vec<String>>,
}

pub type TranscoderOptions = HashMap<String, OptionItem>;

pub struct Transcoder {
    source: String,
    ffmpeg: Option<Child>,
    options: Option<TranscoderOptions>,
}

impl Transcoder {
    pub fn with_option(source: &str, options: Option<TranscoderOptions>) -> Self {
        Self {
            source: source.to_string(),
            ffmpeg: None,
            options,
        }
    }

    fn find_options(&self, protocol: &str) -> Option<&OptionItem> {
        if self.options.is_some() {
            let result = self.options.as_ref().unwrap().get(protocol);
            if result.is_some() {
                log::info!(
                    "Configuration {} applied for protocol {}",
                    protocol,
                    protocol
                );
                return result;
            } else {
                log::info!("Configuration default applied for protocol {}", protocol);
                return self.options.as_ref().unwrap().get("default");
            }
        }
        return None;
    }

    fn create_ffmpeg_process(&self) -> io::Result<Child> {
        let mut command = Command::new("ffmpeg");
        let mut options: Option<&OptionItem> = None;
        if let Ok(url) = Url::parse(&self.source) {
            options = self.find_options(url.scheme());
        }
        command.stdout(Stdio::piped());
        if let Some(options) = options {
            if options.global.is_some() {
                command.args(options.global.as_ref().unwrap());
            }
            if options.input.is_some() {
                command.args(options.input.as_ref().unwrap());
            }
            command.args(&["-i", &self.source]);
            if options.output.is_some() {
                command.args(options.output.as_ref().unwrap());
            }
        } else {
            command.args(&["-re", "-i", &self.source,"-c:v","copy","-c:a","copy", "-f", "flv"]);
        }
        command
            .args(&["-"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let ffmpeg = command.spawn()?;
        Ok(ffmpeg)
    }

    pub async fn pull_data(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.ffmpeg.is_none() {
            self.ffmpeg = Some(self.create_ffmpeg_process()?);
        }
        if let Some(ref mut ffmpeg) = self.ffmpeg {
            if let Some(ref mut stdout) = ffmpeg.stdout {
                return stdout.read(buf).await;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "ffmpeg output pipe broken",
                ));
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "ffmpeg process interrupted",
            ));
        }
    }

    pub async fn stop(&mut self) -> io::Result<()> {
        if let Some(ref mut ffmpeg) = self.ffmpeg {
            return ffmpeg.kill().await;
        }
        Ok(())
    }
}
