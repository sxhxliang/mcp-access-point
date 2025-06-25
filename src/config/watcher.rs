use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use notify::{event::ModifyKind, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use pingora_core::{server::ShutdownWatch, services::Service};
use tokio::sync::mpsc;

use crate::Config;

#[cfg(unix)]
use pingora_core::server::ListenFds;

/// Service that watches for configuration file changes and reloads the configuration.
pub struct ConfigWatcherService {
    config_path: PathBuf,
    current_config: Arc<Mutex<Config>>, // To store and compare the currently active config
    // We'll need a way to trigger the actual reload logic, perhaps by sending the new Config
    // to another part of the system or by directly calling update functions.
    // For now, let's focus on watching and parsing.
    work_stealing: bool, // Needed for creating proxy objects
}

impl ConfigWatcherService {
    pub fn new(
        config_path: String,
        initial_config: Config,
        work_stealing: bool,
    ) -> Result<Self, anyhow::Error> {
        Ok(Self {
            config_path: PathBuf::from(config_path),
            current_config: Arc::new(Mutex::new(initial_config)),
            work_stealing,
        })
    }

    async fn run_watcher_loop(&self, mut shutdown: ShutdownWatch) {
        let (tx, mut rx) = mpsc::channel(1);

        let path_to_watch = self.config_path.clone();
        let mut watcher = match RecommendedWatcher::new(
            move |res| {
                if let Ok(event) = res {
                    if matches!(event.kind, EventKind::Modify(ModifyKind::Data(_)) | EventKind::Modify(ModifyKind::Name(_))) {
                        // Forward the event, or just a signal
                        if tx.blocking_send(()).is_err() {
                            log::error!("Config watcher: Failed to send notification, receiver dropped.");
                        }
                    }
                } else if let Err(e) = res {
                    log::error!("Config watcher error: {:?}", e);
                }
            },
            notify::Config::default(),
        ) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to create config file watcher: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&path_to_watch, RecursiveMode::NonRecursive) {
            log::error!(
                "Failed to start watching config file {}: {}",
                path_to_watch.display(),
                e
            );
            return;
        }

        log::info!(
            "Started watching config file for changes: {}",
            self.config_path.display()
        );

        loop {
            tokio::select! {
                biased;
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        log::info!("Config watcher service shutting down.");
                        break;
                    }
                }
                Some(_) = rx.recv() => {
                    // Debounce: wait a short period for more events
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    // Drain any other events that arrived during the debounce period
                    while rx.try_recv().is_ok() {}

                    log::info!("Config file change detected: {}", self.config_path.display());
                    self.reload_config().await;
                }
            }
        }
    }

    async fn reload_config(&self) {
        log::info!("Attempting to reload configuration from {}", self.config_path.display());
        match Config::load_from_yaml(&self.config_path) {
            Ok(new_config) => {
                log::info!("Successfully loaded new configuration.");
                // Here we will implement the logic to compare and apply the new_config
                // For now, just update the current_config
                let mut current_config_guard = self.current_config.lock().unwrap();

                // Clone the old config for comparison before updating
                let old_config = current_config_guard.clone();

                // Apply changes using the diff_apply module
                diff_apply::apply_config_changes(&new_config, &old_config, self.work_stealing);

                // After successful application of changes, update the stored current_config
                *current_config_guard = new_config;
                log::info!("Configuration updated successfully via watcher.");

            }
            Err(e) => {
                log::error!("Failed to reload configuration via watcher: {}", e);
                // Keep using the old configuration
            }
        }
    }
}

#[async_trait]
impl Service for ConfigWatcherService {
    async fn start_service(
        &mut self,
        #[cfg(unix)] _fds: Option<ListenFds>,
        shutdown: ShutdownWatch,
        _listeners_per_fd: usize,
    ) {
        self.run_watcher_loop(shutdown).await;
    }

    fn name(&self) -> &'static str {
        "Config File Watcher"
    }

    fn threads(&self) -> Option<usize> {
        Some(1) // Watcher typically needs only one thread
    }
}

// Need to add the actual update logic in reload_config
// This will involve functions similar to those in ProxyEventHandler,
// but adapted to work with Vec<ConfigResourceType> instead of etcd GetResponse.

mod diff_apply; // Ensure this module is declared

// No factory module needed for now as per latest understanding
// pub mod factory;
