use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use notify::{
    event::ModifyKind, Config as NotifyConfig, Event, EventKind, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher, WatcherKind
};
use pingora::Result;
use pingora_core::{server::ShutdownWatch, services::Service};
use tokio::sync::mpsc;
use futures::{
    channel::mpsc::{channel, Receiver},
    SinkExt, StreamExt,
};

use crate::{config::Config, proxy::{global_rule::load_static_global_rules, mcp::load_static_mcp_services, route::load_static_routes, service::load_static_services, ssl::load_static_ssls, upstream::load_static_upstreams}};

#[cfg(unix)]
use pingora_core::server::ListenFds;

/// Service that watches for configuration file changes and reloads the configuration.
pub struct ConfigWatcherService {
    config_path: PathBuf,
    // current_config: Arc<Mutex<Config>>, // To store and compare the currently active config
    // We'll need a way to trigger the actual reload logic, perhaps by sending the new Config
    // to another part of the system or by directly calling update functions.
    // For now, let's focus on watching and parsing.
    // work_stealing: bool, // Needed for creating proxy objects
}

impl ConfigWatcherService {
    pub fn new(config_path: &str) -> Result<Self> {
        Ok(Self {
            config_path: PathBuf::from(config_path.to_string()),
            // current_config: Arc::new(Mutex::new(initial_config)),
            // work_stealing,
        })
    }

    async fn run_watcher_loop(&self, mut shutdown: ShutdownWatch) {

        if let Err(e) = async_watch(self.config_path.clone()).await {
            println!("error: {:?}", e)
        }
        // let path_to_watch = self.config_path.clone();
        // let (tx, rx) = std::sync::mpsc::channel();
        // // This example is a little bit misleading as you can just create one Config and use it for all watchers.
        // // That way the pollwatcher specific stuff is still configured, if it should be used.
        // let mut watcher: Box<dyn Watcher> =
        //     if RecommendedWatcher::kind() == WatcherKind::PollWatcher {
        //         // custom config for PollWatcher kind
        //         let config = NotifyConfig::default().with_poll_interval(Duration::from_secs(1));
        //         Box::new(PollWatcher::new(tx, config).unwrap())
        //     } else {
        //         // use default config for everything else
        //         Box::new(RecommendedWatcher::new(tx, NotifyConfig::default()).unwrap())
        //     };

        // // watch some stuff
        // watcher
        //     .watch(&self.config_path,  RecursiveMode::Recursive)
        //     .unwrap();

        // // just print all events, this blocks forever
        // for e in rx {
            
        //     if let Ok(event) = e {
        //         match event.kind {
        //             EventKind::Modify(res) => {
        //                 // self.reload_config().await;
        //                 println!("{:?}", res);
        //             }
        //             _ => {}
        //         }
        //     }
        // }
    
    }

    async fn reload_config(&self) {
        // log::info!("Attempting to reload configuration from {}", self.config_path.display());
        match Config::load_from_yaml(&self.config_path) {
            Ok(new_config) => {
                log::info!("Successfully loaded new configuration.");
                log::info!("Loading services, upstreams, and routes...");
                load_static_upstreams(&new_config).expect("Failed to load static upstreams");
                load_static_services(&new_config).expect("Failed to load static services");
                load_static_global_rules(&new_config).expect("Failed to load static global rules");
                load_static_routes(&new_config).expect("Failed to load  static routes");
                load_static_mcp_services(&new_config).expect("Failed to load static mcp services");
                load_static_ssls(&new_config).expect("Failed to load  static ssls");
                log::info!("Configuration updated successfully via watcher.");
            }
            Err(e) => {
                log::error!("Failed to reload configuration via watcher: {}", e);
                // Keep using the old configuration
            }
        }
    }
}


fn async_watcher() -> notify::Result<(RecommendedWatcher, Receiver<notify::Result<Event>>)> {
    let (mut tx, rx) = channel(1);

    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let watcher = RecommendedWatcher::new(
        move |res| {
            futures::executor::block_on(async {
                tx.send(res).await.unwrap();
            })
        },
        NotifyConfig::default(),
    )?;

    Ok((watcher, rx))
}

async fn async_watch<P: AsRef<Path>>(path: P) -> notify::Result<()> {
    let (mut watcher, mut rx) = async_watcher()?;

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;

    while let Some(res) = rx.next().await {
        match res {
            Ok(event) => {
                if let EventKind::Modify(ModifyKind::Data(_)) = event.kind {
                    match Config::load_from_yaml(event.paths.first().unwrap()) {
                        Ok(new_config) => {
                            log::info!("Successfully loaded new configuration.");
                            log::info!("Loading services, upstreams, and routes...");
                            load_static_upstreams(&new_config).expect("Failed to load static upstreams");
                            load_static_services(&new_config).expect("Failed to load static services");
                            load_static_global_rules(&new_config).expect("Failed to load static global rules");
                            load_static_routes(&new_config).expect("Failed to load  static routes");
                            load_static_mcp_services(&new_config).expect("Failed to load static mcp services");
                            load_static_ssls(&new_config).expect("Failed to load  static ssls");
                            log::info!("Configuration updated successfully via watcher.");
                        }
                        Err(e) => {
                            log::error!("Failed to reload configuration via watcher: {}", e);
                            // Keep using the old configuration
                        }
                    }
                }
            },
            Err(e) => println!("watch error: {:?}", e),
        }
    }

    Ok(())
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
