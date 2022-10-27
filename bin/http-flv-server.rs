use clap::Parser;
use flv_remuxer::{RemuxManager, TranscoderOptions};
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use log::{self, LevelFilter};
use simple_logger::SimpleLogger;
use std::convert::Infallible;
use std::env;
use std::fs::File;
use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tokio;
use tokio::signal;
use tokio::sync::RwLock;
use url::Url;

mod remuxer_stream_ext;
mod allocator;
mod cli_args;

type GenericError = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, GenericError>;

const STEAM_PATH: &str = "/stream";

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
    let timeout =Some(cli.timeout * 1000000);
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
    let address = format!("{}:{}", &cli.host, &cli.port);
    let address = std::net::SocketAddr::from_str(&address).unwrap();
    let server = run_server(&address, manager);

    tokio::select!(
        _ = transmux_task=>{
        }
        _= server =>{

        }
    );
}

async fn run_server(address: &SocketAddr, remux_manager: Arc<RwLock<RemuxManager>>) -> Result<()> {
    let manager = remux_manager.clone();
    let service = make_service_fn(move |connection: &AddrStream| {
        let manager = remux_manager.clone();
        let remote_address = Arc::new(connection.remote_addr().clone());
        async {
            Ok::<_, Infallible>(service_fn(move |req| {
                handle_connection(remote_address.clone(), req, manager.clone())
            }))
        }
    });

    let server = Server::bind(address).serve(service);
    log::info!("Server running at: http://{}{}", &address, STEAM_PATH);
    if let Err(e) = server
        .with_graceful_shutdown(async move {
            if let Ok(_) = signal::ctrl_c().await {
                manager.write().await.stop();
                log::info!("Server stoped");
            } else {
                panic!("Can't listen Ctrl+C sinal");
            }
        })
        .await
    {
        log::error!("Start server failed: {}", e);
    }

    Ok(())
}

async fn handle_connection(
    remote_address: Arc<SocketAddr>,
    request: Request<Body>,
    remux_manager: Arc<RwLock<RemuxManager>>,
) -> Result<Response<Body>> {
    let path = request.uri().to_string();
    let mut source: Option<String> = None;
    if let Ok(request_url) = Url::parse("http://127.0.0.1") {
        if let Ok(request_url) = request_url.join(&path) {
            if request_url.path() == STEAM_PATH {
                if let Some((_k, v)) = request_url.query_pairs().find(|(k, _v)| k == "url") {
                    source = Some(v.into());
                }
            }
        }
    }
    if let Some(source) = source {
        let rx = remux_manager
            .write()
            .await
            .subscribe(&source, &remote_address.to_string())
            .await;
        let rx_stream = remuxer_stream_ext::FlvRemuxerMessageReceiver::new(rx);
        let mut response = Response::new(Body::wrap_stream(rx_stream));
        response
            .headers_mut()
            .append("Access-Control-Allow-Origin", "*".parse().unwrap());
        return Ok(response);
    }
    let mut response = Response::new(Body::from("Bad Request"));
    response
        .headers_mut()
        .append("Access-Control-Allow-Origin", "*".parse().unwrap());
    *response.status_mut() = StatusCode::BAD_REQUEST;
    return Ok(response);
}
