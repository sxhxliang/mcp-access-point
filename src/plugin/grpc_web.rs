use std::sync::Arc;

use async_trait::async_trait;
use pingora::modules::http::grpc_web::GrpcWebBridge;
use pingora_error::Result;
use pingora_proxy::Session;
use serde_yaml::Value as YamlValue;

use crate::proxy::ProxyContext;

use super::ProxyPlugin;

pub const PLUGIN_NAME: &str = "grpc-web";

/// Creates a gRPC-Web plugin instance.
/// This plugin enables support for the gRPC-Web protocol in the proxy.
pub fn create_grpc_web_plugin(_cfg: YamlValue) -> Result<Arc<dyn ProxyPlugin>> {
    Ok(Arc::new(PluginGrpcWeb {}))
}

/// gRPC-Web Plugin implementation.
/// This plugin enables support for gRPC-Web protocol in the proxy.
pub struct PluginGrpcWeb;

#[async_trait]
impl ProxyPlugin for PluginGrpcWeb {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn priority(&self) -> i32 {
        505
    }

    async fn early_request_filter(
        &self,
        session: &mut Session,
        _ctx: &mut ProxyContext,
    ) -> Result<()> {
        let grpc = session
            .downstream_modules_ctx
            .get_mut::<GrpcWebBridge>()
            .expect("GrpcWebBridge module added");

        // initialize gRPC module for this request
        grpc.init();
        Ok(())
    }
}
