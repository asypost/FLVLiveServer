use std::io;
use tokio::sync::mpsc::{error::SendError, UnboundedSender};

#[derive(Debug)]
pub enum Message {
    Eof,
    Error(io::Error),
    Data(Vec<u8>),
}

#[derive(Debug)]
pub(crate) struct Subscriber {
    pub id: String,
    pub video_start_timestamp: Option<u32>,
    pub audio_start_timestamp: Option<u32>,
    pub active: bool,
    tx: UnboundedSender<Message>,
    pub header_script_sended: bool,
    pub audio_codec_tag_sended: bool,
    pub video_codec_tag_sended: bool,
    pub pre_tag_size: u32,
}

impl Subscriber {
    pub fn new(tx: UnboundedSender<Message>, id: &str) -> Self {
        Self {
            video_start_timestamp: None,
            audio_start_timestamp: None,
            tx,
            header_script_sended: false,
            active: true,
            pre_tag_size: 0_u32,
            audio_codec_tag_sended: false,
            video_codec_tag_sended: false,
            id: id.to_string(),
        }
    }

    pub fn send(&self, message: Message) -> Result<(), SendError<Message>> {
        self.tx.send(message)?;
        Ok(())
    }
}
