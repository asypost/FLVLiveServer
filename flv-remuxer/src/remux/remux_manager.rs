use super::remuxer::{self, Remuxer};
use super::subscriber::Message;
use super::transcoder::TranscoderOptions;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::Mutex;

pub struct RemuxManager {
    transmuxers: HashMap<String, Arc<Mutex<Remuxer>>>,
    running: bool,
    timeout: Option<u64>,
    dead_remuxers: Arc<Mutex<Vec<String>>>,
    transcoder_options: Option<TranscoderOptions>,
}

impl RemuxManager {
    pub fn new() -> Self {
        Self {
            transmuxers: HashMap::new(),
            running: true,
            timeout: None,
            dead_remuxers: Arc::new(Mutex::new(Vec::new())),
            transcoder_options: None,
        }
    }

    pub fn with_timeout_and_options(
        timeout_micros: u64,
        transcoder_options: Option<TranscoderOptions>,
    ) -> Self {
        Self {
            transmuxers: HashMap::new(),
            running: true,
            timeout: Some(timeout_micros),
            dead_remuxers: Arc::new(Mutex::new(Vec::new())),
            transcoder_options,
        }
    }

    pub async fn subscribe(
        &mut self,
        source: &str,
        subscriber_address: &str,
    ) -> UnboundedReceiver<Message> {
        match self.transmuxers.get_mut(source) {
            Some(transmux) => transmux.lock().await.subscribe(subscriber_address),
            None => {
                let mut transmux = Remuxer::new_with_timeout_and_options(
                    source,
                    self.timeout,
                    self.transcoder_options.clone(),
                );
                let rx = transmux.subscribe(subscriber_address);
                self.transmuxers
                    .insert(source.to_string(), Arc::new(Mutex::new(transmux)));
                rx
            }
        }
    }

    pub async fn next(&mut self) -> bool {
        if self.transmuxers.len() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        } else {
            let mut created_task_count: usize = 0;
            for (source, tranmuxer) in &self.transmuxers {
                if let Ok(mut t) = tranmuxer.try_lock() {
                    let src = source.clone();
                    let dead = self.dead_remuxers.clone();
                    match t.state() {
                        remuxer::RemuxerState::Idle => {
                            created_task_count += 1;
                            let trans = tranmuxer.clone();
                            t.set_state(remuxer::RemuxerState::Pending);
                            tokio::task::spawn(async move {
                                let remuxer = &mut *trans.lock().await;
                                if let Err(e) = remuxer.next().await {
                                    log::warn!("Transmuxer closed due to {}", e);
                                    dead.lock().await.push(src);
                                } else {
                                    remuxer.set_state(remuxer::RemuxerState::Idle);
                                }
                            });
                        }
                        remuxer::RemuxerState::Dead => {
                            dead.lock().await.push(src);
                        }
                        _ => {}
                    }
                }
            }

            let mut dead_remuxers = self.dead_remuxers.lock().await;
            for key in &mut *dead_remuxers {
                if self.transmuxers.contains_key(key) {
                    self.transmuxers.remove(key);
                }
            }
            if dead_remuxers.len() > 0 {
                log::info!("Remuxer alive count:{}", self.transmuxers.len());
                dead_remuxers.clear();
                drop(dead_remuxers);
            }

            if created_task_count == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        return self.running;
    }

    pub fn stop(&mut self) {
        self.running = false;
    }
}
