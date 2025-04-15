use std::fs;
use std::path::Path;

use clap::Parser;
use notify::Watcher;
use tokio::sync::broadcast;
use pingora::{prelude::*, proxy::http_proxy_service_with_name, services::Service};


use mcp_access_point::utils::file::read_from_local_or_remote;
use mcp_access_point::cli;
use mcp_access_point::config::{UpstreamConfig, CLIENT_SSE_ENDPOINT, UPSTREAM_CONFIG};
use mcp_access_point::openapi::reload_global_openapi_tools;
use mcp_access_point::proxy::ModelContextProtocolProxy;
use mcp_access_point::admin;

fn main() {
    // std::env::set_var("RUST_LOG", "DEBUG");
    env_logger::init();

    let args = cli::Cli::parse();
    //

    {
        let upstream = UpstreamConfig::parse(&args.upstream);
        match upstream {
            Ok(upstream) => {
                let mut proxy_config = UPSTREAM_CONFIG.write().unwrap();
                proxy_config.ip = upstream.ip;
                proxy_config.port = upstream.port;
            }
            Err(e) => {
                log::error!("Failed to parse upstream address: {}", e);
                return;
            }
        }
    }
    // watch the openapi file
    let filename = args.file.clone();
    let res = read_from_local_or_remote(&filename);
    let (is_remote, content) = match res {
        Ok((is_remote, content)) => (is_remote, content),
        Err(e) => {
            log::error!("Failed to read the openapi file: {}", e);
            return;
        }
    };

    reload_global_openapi_tools(content).expect("Failed to reload openapi tools");
    if !is_remote {
        let mut watcher = notify::recommended_watcher(move |res| match res {
            Ok(event) => {
                log::info!("file watcher: {event:?}");
                let content = fs::read_to_string(Path::new(&filename))
                    .expect("Failed to read the openapi file");
                reload_global_openapi_tools(content).expect("Failed to reload openapi tools");
            }
            Err(e) => panic!("watch error: {:?}", e),
        })
        .unwrap();

        watcher
            .watch(Path::new(&args.file), notify::RecursiveMode::NonRecursive)
            .unwrap();
    }
    // build the server
    let mut my_server = Server::new(Some(Opt::default())).unwrap();
    my_server.bootstrap();

    let admin_service_http = admin::admin_http_service("0.0.0.0:6345");

    let (tx, _) = broadcast::channel(16);

    let mut lb_service: pingora::services::listening::Service<
        pingora::proxy::HttpProxy<ModelContextProtocolProxy>,
    > = http_proxy_service_with_name(
        &my_server.configuration,
        ModelContextProtocolProxy::new(tx),
        "mcprouter",
    );

    let addr = format!("0.0.0.0:{:?}", args.port);
    println!("parse openapi file: {}", &args.file);
    println!("Listening on: {}", &addr);
    println!(
        "MCP server enterpoint: {}",
        &format!("http://{addr}{CLIENT_SSE_ENDPOINT}")
    );
    lb_service.add_tcp(&addr);

    log::info!("The cargo manifest dir is: {}", env!("CARGO_MANIFEST_DIR"));

    let services: Vec<Box<dyn Service>> = vec![Box::new(lb_service), Box::new(admin_service_http)];

    my_server.add_services(services);
    my_server.run_forever();
}
