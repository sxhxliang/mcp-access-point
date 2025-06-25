#![allow(clippy::upper_case_acronyms)]

use pingora::services::listening::Service;
use pingora_core::{
    apps::HttpServerOptions,
    listeners::tls::TlsSettings,
    server::{configuration::Opt, Server},
};
use pingora_proxy::{http_proxy_service_with_name, HttpProxy};
use sentry::IntoDsn;
use std::ops::DerefMut;
use tokio::sync::broadcast;

use access_point::admin::http_admin::AdminHttpApp;
use access_point::logging::Logger;
use access_point::proxy::{
    event::ProxyEventHandler,
    global_rule::load_static_global_rules,
    route::load_static_routes,
    service::load_static_services,
    ssl::{load_static_ssls, DynamicCert},
    upstream::load_static_upstreams,
};
use access_point::{
    config::{self, etcd::EtcdConfigSync, Config},
    proxy::mcp::load_static_mcp_services,
};
// use access_point::service::http::HttpService;
use access_point::service::mcp::MCPProxyService;

fn main() {
    // 加载配置和命令行参数
    // std::env::set_var("RUST_LOG", "info,pingora_core=warn");
    // std::env::set_var("RUST_LOG", "debug");
    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    std::env::set_var("RUST_LOG", format!("{log_level},pingora_core=warn, pingora_proxy=warn"));

    let cli_options = Opt::parse_args();
    let initial_config =
        Config::load_yaml_with_opt_override(&cli_options).expect("Failed to load initial configuration");

    // 初始化日志
    let logger = if let Some(log_cfg) = &initial_config.access_point.log {
        let logger = Logger::new(log_cfg.clone());
        logger.init_env_logger();
        Some(logger)
    } else {
        env_logger::init();
        None
    };

    // 配置同步
    // Decide whether to use Etcd or File Watcher for dynamic configuration.
    // For now, we'll prioritize Etcd if configured, otherwise use File Watcher if config path is available.
    let etcd_configured = initial_config.access_point.etcd.is_some();
    let config_file_path = cli_options.conf.clone(); // Clone to use later for watcher

    if etcd_configured {
        if let Some(etcd_cfg) = &initial_config.access_point.etcd {
            log::info!("Adding etcd config sync...");
            let event_handler = ProxyEventHandler::new(initial_config.pingora.work_stealing);
            let etcd_sync = EtcdConfigSync::new(
                etcd_cfg.clone(),
                Box::new(event_handler),
            );
             // access_point_server.add_service(etcd_sync); // Will be added later
        }
    } else {
        // Initial static load if not using etcd. Watcher will handle subsequent reloads.
        log::info!("Loading initial static services, upstreams, and routes from file...");
        load_static_upstreams(&initial_config).expect("Failed to load static upstreams");
        load_static_services(&initial_config).expect("Failed to load static services");
        load_static_global_rules(&initial_config).expect("Failed to load static global rules");
        load_static_routes(&initial_config).expect("Failed to load static routes");
        load_static_mcp_services(&initial_config).expect("Failed to load static mcp services");
        load_static_ssls(&initial_config).expect("Failed to load static ssls");
    }


    // 创建服务器实例
    // Clone cli_options for the server, as the original might be consumed or go out of scope
    let server_cli_options = Opt {
        conf: cli_options.conf.clone(), // Pass the config path if available
        ..cli_options // Copy other fields like daemon, test, etc.
    };
    let mut access_point_server = Server::new_with_opt_and_conf(Some(server_cli_options), initial_config.pingora.clone());


    // 添加日志服务
    if let Some(log_service) = logger {
        log::info!("Adding log sync service...");
        access_point_server.add_service(log_service);
    }

    // Add Etcd or File Watcher Service
    if etcd_configured {
        if let Some(etcd_cfg) = &initial_config.access_point.etcd {
            log::info!("Adding etcd config sync service...");
            let event_handler = ProxyEventHandler::new(initial_config.pingora.work_stealing);
            let etcd_sync_service = EtcdConfigSync::new(
                etcd_cfg.clone(),
                Box::new(event_handler),
            );
            access_point_server.add_service(etcd_sync_service);
        }
    } else if let Some(conf_path_str) = config_file_path {
        log::info!("File-based configuration: Adding config watcher service for {}", conf_path_str);
        // Pass a clone of initial_config and work_stealing flag
        match config::watcher::ConfigWatcherService::new(
            conf_path_str,
            initial_config.clone(), // Clone initial_config for the watcher
            initial_config.pingora.work_stealing,
        ) {
            Ok(watcher_service) => {
                access_point_server.add_service(watcher_service);
            }
            Err(e) => {
                log::error!("Failed to create ConfigWatcherService: {}", e);
                // Potentially exit or handle error appropriately
            }
        }
    } else {
        log::warn!("No configuration file path provided and Etcd not configured. Dynamic reloading will not be available.");
    }


    let (tx, _) = broadcast::channel(16);

    let mut http_service: Service<HttpProxy<MCPProxyService>> = http_proxy_service_with_name(
        &access_point_server.configuration, // This uses the server's internal config, which is based on initial_config.pingora
        MCPProxyService::new(tx),
        "access_point",
    );

    // 添加监听器
    log::info!("Adding listeners...");
    add_listeners(&mut http_service, &initial_config.access_point);

    // 添加扩展服务（如 Sentry 和 Prometheus, Admin）
    add_optional_services(&mut access_point_server, &initial_config.access_point, &initial_config); // Pass full config if needed by admin

    // 启动服务器
    log::info!("Bootstrapping...");
    access_point_server.bootstrap();
    log::info!("Bootstrapped. Adding Services...");
    access_point_server.add_service(http_service);

    log::info!("Starting Server...");
    for list_cfg in initial_config.access_point.listeners.iter() {
        let addr = &list_cfg.address.to_string();
        log::info!("🚀Listening on: {addr}");
        log::info!("🚀Endpoint:");
        log::info!("---->HTTP Endpoint: {addr}/mcp");
        log::info!("---->SSE  Endpoint: {addr}/sse");
        log::info!("🚀Multi-tenancy Endpoint:");
        initial_config.mcps.iter().for_each(|mcp| {
            let id = mcp.id.clone();
            log::info!("---->MCP ID: {id}");
            log::info!("-------->HTTP Endpoint: {addr}/api/{id}/mcp");
            log::info!("-------->SSE  Endpoint: {addr}/api/{id}/sse");
        });

    }
    
    access_point_server.run_forever();
}

// 添加监听器的辅助函数
fn add_listeners(
    http_service: &mut Service<HttpProxy<MCPProxyService>>,
    access_point_cfg: &config::AccessPointConfig, // Changed from &config::AccessPointConfig to avoid confusion
) {
    for list_cfg in access_point_cfg.listeners.iter() { // Use access_point_cfg
        if let Some(tls) = &list_cfg.tls {
            // ... TLS 配置
            let dynamic_cert = DynamicCert::new(tls);
            let mut tls_settings = TlsSettings::with_callbacks(dynamic_cert)
                .expect("Init dynamic cert shouldn't fail");

            tls_settings
                .deref_mut()
                .deref_mut()
                .set_max_proto_version(Some(pingora::tls::ssl::SslVersion::TLS1_3))
                .expect("Init dynamic cert shouldn't fail");

            if list_cfg.offer_h2 {
                tls_settings.enable_h2();
            }
            http_service.add_tls_with_settings(&list_cfg.address.to_string(), None, tls_settings);
        } else {
            // 无 TLS
            if list_cfg.offer_h2c {
                //... H2C 配置
                let http_logic = http_service.app_logic_mut().unwrap();
                let mut http_server_options = HttpServerOptions::default();
                http_server_options.h2c = true;
                http_logic.server_options = Some(http_server_options);
            }
            http_service.add_tcp(&list_cfg.address.to_string());
        }
    }
}

// 添加可选服务（如 Sentry 和 Prometheus, Admin）的辅助函数
// Updated to accept full Config for admin service if it needs more than AccessPointConfig
fn add_optional_services(server: &mut Server, access_point_cfg: &config::AccessPointConfig, full_config: &Config) {
    if let Some(sentry_cfg) = &access_point_cfg.sentry { // Use access_point_cfg
        log::info!("Adding Sentry config...");
        server.sentry = Some(sentry::ClientOptions {
            dsn: sentry_cfg
                .dsn
                .clone()
                .into_dsn()
                .expect("Invalid Sentry DSN"),
            ..Default::default()
        });
    }

    // Admin service might need the full config, not just access_point part
    if full_config.access_point.etcd.is_some() && full_config.access_point.admin.is_some() {
        log::info!("Adding Admin Service...");
        // Pass full_config or specific parts as needed by AdminHttpApp::admin_http_service
        let admin_service_http = AdminHttpApp::admin_http_service(full_config);
        server.add_service(admin_service_http);
    }

    if let Some(prometheus_cfg) = &access_point_cfg.prometheus { // Use access_point_cfg
        log::info!("Adding Prometheus Service...");
        let mut prometheus_service_http = Service::prometheus_http_service();
        prometheus_service_http.add_tcp(&prometheus_cfg.address.to_string());
        server.add_service(prometheus_service_http);
    }
}
