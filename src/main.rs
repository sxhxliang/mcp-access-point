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
    let mut config =
        Config::load_yaml_with_opt_override(&cli_options).expect("Failed to load configuration");

    // 初始化日志
    let logger = if let Some(log_cfg) = &config.access_point.log {
        let logger = Logger::new(log_cfg.clone());
        logger.init_env_logger();
        Some(logger)
    } else {
        env_logger::init();
        None
    };

    // 配置同步
    let etcd_sync = if let Some(etcd_cfg) = &config.access_point.etcd {
        log::info!("Adding etcd config sync...");
        let event_handler = ProxyEventHandler::new(config.pingora.work_stealing);
        Some(EtcdConfigSync::new(
            etcd_cfg.clone(),
            Box::new(event_handler),
        ))
    } else {
        log::info!("Loading services, upstreams, and routes...");
        load_static_upstreams(&config).expect("Failed to load static upstreams");
        load_static_services(&config).expect("Failed to load static services");
        load_static_global_rules(&config).expect("Failed to load static global rules");
        load_static_routes(&config).expect("Failed to load  static routes");
        load_static_mcp_services(&config).expect("Failed to load static mcp services");
        load_static_ssls(&config).expect("Failed to load  static ssls");
        None
    };

    // 先添加可选服务（包括 Admin），传递完整的 config
    let admin_service = if config.access_point.admin.is_some() {
        log::info!("Creating Admin Service (Enhanced)...");
        Some(AdminHttpApp::admin_http_service(&config))
    } else {
        None
    };

    // 创建服务器实例，此时移动 config.pingora
    let mut access_point_server = Server::new_with_opt_and_conf(Some(cli_options), config.pingora);

    // 添加日志服务
    if let Some(log_service) = logger {
        log::info!("Adding log sync service...");
        access_point_server.add_service(log_service);
    }

    // 添加 Etcd 配置同步服务
    if let Some(etcd_service) = etcd_sync {
        log::info!("Adding etcd config sync service...");
        access_point_server.add_service(etcd_service);
    }

    // 添加 Admin 服务
    if let Some(admin_service) = admin_service {
        log::info!("Adding Admin Service (Enhanced)...");
        access_point_server.add_service(admin_service);
    }


    let (tx, _) = broadcast::channel(16);

    let mut http_service: Service<HttpProxy<MCPProxyService>> = http_proxy_service_with_name(
        &access_point_server.configuration,
        MCPProxyService::new(tx),
        "access_point",
    );

    // 添加监听器
    log::info!("Adding listeners...");
    add_listeners(&mut http_service, &config.access_point);

    // 添加扩展服务（如 Sentry 和 Prometheus）
    add_optional_services(&mut access_point_server, &config.access_point);

    // 启动服务器
    log::info!("Bootstrapping...");
    access_point_server.bootstrap();
    log::info!("Bootstrapped. Adding Services...");
    access_point_server.add_service(http_service);

    log::info!("Starting Server...");
    for list_cfg in config.access_point.listeners.iter() {
        let addr = &list_cfg.address.to_string();
        log::info!("🚀Listening on: {addr}");
        log::info!("🚀Endpoint:");
        log::info!("---->HTTP Endpoint: {addr}/mcp");
        log::info!("---->SSE  Endpoint: {addr}/sse");
        log::info!("🚀Multi-tenancy Endpoint:");
        config.mcps.iter().for_each(|mcp| {
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
    cfg: &config::AccessPointConfig,
) {
    for list_cfg in cfg.listeners.iter() {
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

// 添加可选服务（如 Sentry 和 Prometheus）的辅助函数
fn add_optional_services(server: &mut Server, cfg: &config::AccessPointConfig) {
    if let Some(sentry_cfg) = &cfg.sentry {
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

    if let Some(prometheus_cfg) = &cfg.prometheus {
        log::info!("Adding Prometheus Service...");
        let mut prometheus_service_http = Service::prometheus_http_service();
        prometheus_service_http.add_tcp(&prometheus_cfg.address.to_string());
        server.add_service(prometheus_service_http);
    }
}
