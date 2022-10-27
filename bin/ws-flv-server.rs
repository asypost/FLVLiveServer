use clap::Parser;
use flv_remuxer::RemuxManager;
use flv_remuxer::TranscoderOptions;
use futures_util::{SinkExt, StreamExt};
use log::{self, LevelFilter};
use serde_json;
use simple_logger::SimpleLogger;
use std;
use std::env;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::RwLock;
use tokio::{
    self,
    net::{TcpListener, TcpStream},
};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::tungstenite::{
    handshake::client::{Request, Response},
    Message,
};
use url::Url;

mod allocator;
mod cli_args;

#[tokio::main]
async fn main() {
    SimpleLogger::new()
        .with_level(LevelFilter::Warn)
        .env()
        .with_local_timestamps()
        .with_colors(true)
        .init()
        .unwrap();

    let cli = cli_args::CliArgs::parse();
    let timeout =  Some(cli.timeout * 1000000);
   
    let config = match cli.config {
        Some(c) => c.to_owned(),
        None => {
            let path = env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf()
                .join("config.json");
            path.to_str().unwrap().to_owned()
        }
    };

    let mut transcoder_options: Option<TranscoderOptions> = None;

    if Path::new(&config).exists() {
        let config_fp = File::open(config).expect("Failed to open");
        let options = serde_json::from_reader(&config_fp).expect("Read transcoder config failed");
        transcoder_options = Some(options);
        log::info!("Configuration file loaded");
    } else {
        log::warn!("Configuration file does not exists");
    }

    let address = format!("{}:{}", &cli.host, &cli.port);
    let listener = TcpListener::bind(&address)
        .await
        .expect("Failed to start server");

    log::info!("Server running at: {}", &address);

    let transmux_manager = if let Some(timeout) = timeout {
        Arc::new(RwLock::new(RemuxManager::with_timeout_and_options(
            timeout,
            transcoder_options,
        )))
    } else {
        Arc::new(RwLock::new(RemuxManager::new()))
    };

    let manager = transmux_manager.clone();
    let transmux_task = tokio::spawn(async move {
        loop {
            if manager.write().await.next().await == false {
                break;
            }
        }
    });

    let manager = transmux_manager.clone();
    let accept_task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(handle_connection(stream, manager.clone()));
        }
    });

    let signal_task = tokio::spawn(async move {
        if let Ok(_) = signal::ctrl_c().await {
            transmux_manager.write().await.stop();
            log::info!("Server stoped");
        } else {
            panic!("Can't listen ctrl-c")
        }
    });
    tokio::select!(
        _ = signal_task=>{
        }
        _ = transmux_task=>{
        }
        _ = accept_task=>{
        }
    );
}

async fn handle_connection(
    stream: TcpStream,
    transmux_manager: Arc<RwLock<RemuxManager>>,
) -> Result<(), Error> {
    let address = stream.peer_addr().unwrap();
    let mut config = WebSocketConfig::default();
    config.accept_unmasked_frames = true;
    let mut path: Option<String> = None;

    let ws_stream = tokio_tungstenite::accept_hdr_async_with_config(
        stream,
        |request: &Request, response: Response| {
            path = Some(request.uri().to_string());
            Ok(response)
        },
        Some(config),
    )
    .await?;

    let mut source: Option<String> = None;
    if let Some(seg) = path {
        if let Ok(url) = Url::parse("http://127.0.0.1") {
            if let Ok(url) = url.join(&seg) {
                if let Some((_k, v)) = url.query_pairs().find(|(k, _v)| k == "url") {
                    source = Some(v.into());
                }
            }
        }
    }
    if let Some(source) = source {
        log::info!("Connection from {}", address);
        let (mut sender, mut _receiver) = ws_stream.split();
        let mut rx = transmux_manager
            .write()
            .await
            .subscribe(&source, &address.to_string())
            .await;
        loop {
            if let Some(message) = rx.recv().await {
                match message {
                    flv_remuxer::Message::Data(data) => {
                        if let Err(e) = sender.send(Message::Binary(data)).await {
                            log::error!("Send data to {} failed: {}", &address, &e);
                            break;
                        }
                    }
                    flv_remuxer::Message::Eof => {
                        break;
                    }
                    flv_remuxer::Message::Error(e) => {
                        log::error!("remux failed: {}", &e);
                        break;
                    }
                }
            } else {
                break;
            }
        }
        rx.close();
        let _ = sender.close().await;
    }
    log::info!("Connection closed :{}", address);
    Ok(())
}
