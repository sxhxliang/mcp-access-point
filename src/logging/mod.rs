use std::io::{self, Write};

use async_trait::async_trait;
use env_logger::Builder;
#[cfg(unix)]
use pingora::server::ListenFds;
use pingora::{server::ShutdownWatch, services::Service};
use tokio::{
    fs::{create_dir_all, metadata, OpenOptions},
    io::{AsyncWriteExt, BufWriter},
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    time::{interval, Duration},
};

use crate::config;

pub struct AsyncWriter {
    sender: UnboundedSender<Vec<u8>>,
}

impl Write for AsyncWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let data = buf.to_vec();
        self.sender.send(data).map_err(io::Error::other)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct Logger {
    sender: UnboundedSender<Vec<u8>>,
    receiver: UnboundedReceiver<Vec<u8>>,
    config: config::Log,
}

impl Logger {
    pub fn new(config: config::Log) -> Self {
        let (sender, receiver) = unbounded_channel::<Vec<u8>>();
        Self {
            sender,
            receiver,
            config,
        }
    }

    fn create_async_writer(&self) -> AsyncWriter {
        AsyncWriter {
            sender: self.sender.clone(),
        }
    }

    pub fn init_env_logger(&self) {
        let writer = self.create_async_writer();
        Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .target(env_logger::Target::Pipe(Box::new(writer)))
            .init();
    }
}

#[async_trait]
impl Service for Logger {
    async fn start_service(
        &mut self,
        #[cfg(unix)] _fds: Option<ListenFds>,
        mut shutdown: ShutdownWatch,
        _listeners_per_fd: usize,
    ) {
        let log_file_path = &self.config.path;

        if let Some(parent) = std::path::Path::new(log_file_path).parent() {
            if metadata(parent).await.is_err() {
                create_dir_all(parent)
                    .await
                    .expect("Failed to create log path")
            }
        }

        let mut file = BufWriter::new(
            OpenOptions::new()
                .write(true)
                .append(true)
                .create(true)
                // .mode(0o644)
                .open(log_file_path)
                .await
                .expect("Failed to open or create log file"),
        );

        let mut flush_interval = interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                biased;
                // Shutdown signal handling
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        log::info!("Shutdown signal received, stopping write log");
                        break;
                    }
                },
                _ = flush_interval.tick() => {
                    if let Err(e) = file.flush().await {
                        log::error!("Failed to flush to log file: {e}");
                    }
                },
                data = self.receiver.recv() => {
                    match data {
                        Some(data) => {
                            if let Err(e) = file.write_all(&data).await {
                                log::error!("Failed to write to log file: {e}");
                            }
                        }
                        None => {
                            log::info!("Log channel closed, stopping write log");
                            break;
                        }
                    }
                }
            }
        }

        if let Err(e) = file.flush().await {
            log::error!("Failed to flush log file: {e}");
        }
    }

    fn name(&self) -> &'static str {
        "log sync"
    }

    fn threads(&self) -> Option<usize> {
        Some(1)
    }
}
