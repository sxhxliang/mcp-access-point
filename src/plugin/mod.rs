pub mod brotli;
pub mod cors;
pub mod echo;
pub mod file_logger;
pub mod grpc_web;
pub mod gzip;
pub mod ip_restriction;
pub mod jwt_auth;
pub mod key_auth;
pub mod limit_count;
pub mod prometheus;
pub mod proxy_rewrite;
pub mod redirect;
pub mod request_id;

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;
use once_cell::sync::Lazy;
use pingora::OkOrErr;
use pingora_error::{Error, ErrorType::ReadError, Result};
use pingora_http::{RequestHeader, ResponseHeader};
use pingora_proxy::Session;
use regex::Regex;
use serde_yaml::Value as YamlValue;

use crate::proxy::ProxyContext;

/// Type alias for plugin initialization functions
pub type PluginCreateFn = fn(YamlValue) -> Result<Arc<dyn ProxyPlugin>>;

/// Registry of plugin builders
static PLUGIN_BUILDER_REGISTRY: Lazy<HashMap<&'static str, PluginCreateFn>> = Lazy::new(|| {
    let arr: Vec<(&str, PluginCreateFn)> = vec![
        (
            file_logger::PLUGIN_NAME,
            file_logger::create_file_logger_plugin,
        ), // 399
        (echo::PLUGIN_NAME, echo::create_echo_plugin), // 412
        (
            prometheus::PLUGIN_NAME, // 500
            prometheus::create_prometheus_plugin,
        ),
        (
            limit_count::PLUGIN_NAME, // 503
            limit_count::create_limit_count_plugin,
        ),
        (
            grpc_web::PLUGIN_NAME, // 505
            grpc_web::create_grpc_web_plugin,
        ),
        (redirect::PLUGIN_NAME, redirect::create_redirect_plugin), // 900
        (gzip::PLUGIN_NAME, gzip::create_gzip_plugin),             // 995
        (brotli::PLUGIN_NAME, brotli::create_brotli_plugin),       // 996
        (
            proxy_rewrite::PLUGIN_NAME, // 1008
            proxy_rewrite::create_proxy_rewrite_plugin,
        ),
        (
            request_id::PLUGIN_NAME,
            request_id::create_request_id_plugin,
        ), // 12015
        (
            key_auth::PLUGIN_NAME, // 2500
            key_auth::create_key_auth_plugin,
        ),
        (
            jwt_auth::PLUGIN_NAME, // 2510
            jwt_auth::create_jwt_auth_plugin,
        ),
        (
            ip_restriction::PLUGIN_NAME, // 3000
            ip_restriction::create_ip_restriction_plugin,
        ),
        (cors::PLUGIN_NAME, cors::create_cors_plugin), // 4000
    ];
    arr.into_iter().collect()
});

/// Builds a plugin instance based on its name and configuration.
///
/// # Arguments
/// - `name`: The name of the plugin to be created.
/// - `cfg`: The configuration for the plugin, provided as a `YamlValue`.
///
/// # Returns
/// - `Result<Arc<dyn ProxyPlugin>>`: On success, returns a reference-counted pointer to the created plugin instance.
///   On failure, returns an error.
///
/// # Errors
/// - `ReadError`: Returned if the plugin name is not found in the `PLUGIN_BUILDER_REGISTRY`.
///
/// # Notes
/// - This function retrieves the appropriate plugin builder from a global registry and invokes it with the provided configuration.
pub fn build_plugin(name: &str, cfg: YamlValue) -> Result<Arc<dyn ProxyPlugin>> {
    let builder = PLUGIN_BUILDER_REGISTRY
        .get(name)
        .or_err(ReadError, "Unknown plugin type")?;
    builder(cfg)
}

#[async_trait]
pub trait ProxyPlugin: Send + Sync {
    /// Return the name of this plugin
    ///
    /// # Returns
    /// * `&str` - The name of this plugin
    fn name(&self) -> &str;

    /// Return the priority of this plugin
    ///
    /// # Returns
    /// * `i32` - The priority of this plugin
    fn priority(&self) -> i32;

    /// Handle the incoming request.
    ///
    /// In this phase, users can parse, validate, rate limit, perform access control and/or
    /// return a response for this request.
    /// Like APISIX rewrite access phase.
    ///
    /// # Arguments
    ///
    /// * `_session` - Mutable reference to the current session
    /// * `_ctx` - Mutable reference to the plugin context
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if a response was sent and the proxy should exit
    /// * `Ok(false)` if the proxy should continue to the next phase
    async fn request_filter(
        &self,
        _session: &mut Session,
        _ctx: &mut ProxyContext,
    ) -> Result<bool> {
        Ok(false)
    }

    /// Handle the incoming request before any downstream module is executed.
    async fn early_request_filter(
        &self,
        _session: &mut Session,
        _ctx: &mut ProxyContext,
    ) -> Result<()> {
        Ok(())
    }

    /// Modify the request before it is sent to the upstream
    ///
    /// # Arguments
    /// Like APISIX before_proxy phase.
    ///
    /// * `_session` - Mutable reference to the current session
    /// * `_upstream_request` - Mutable reference to the upstream request header
    /// * `_ctx` - Mutable reference to the plugin context
    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        _upstream_request: &mut RequestHeader,
        _ctx: &mut ProxyContext,
    ) -> Result<()> {
        Ok(())
    }

    /// Modify the response header before it is sent to the downstream
    ///
    /// # Arguments
    /// Like APISIX header_filter phase.
    ///
    /// * `_session` - Mutable reference to the current session
    /// * `_upstream_response` - Mutable reference to the upstream response header
    /// * `_ctx` - Mutable reference to the plugin context
    async fn response_filter(
        &self,
        _session: &mut Session,
        _upstream_response: &mut ResponseHeader,
        _ctx: &mut ProxyContext,
    ) -> Result<()> {
        Ok(())
    }

    /// Handle the response body chunks
    ///
    /// # Arguments
    /// Like APISIX body_filter phase.
    ///
    /// * `_session` - Mutable reference to the current session
    /// * `_body` - Mutable reference to an optional Bytes containing the body chunk
    /// * `_end_of_stream` - Boolean indicating if this is the last chunk
    /// * `_ctx` - Mutable reference to the plugin context
    fn response_body_filter(
        &self,
        _session: &mut Session,
        _body: &mut Option<Bytes>,
        _end_of_stream: bool,
        _ctx: &mut ProxyContext,
    ) -> Result<()> {
        Ok(())
    }

    /// This filter is called when the entire response is sent to the downstream successfully or
    /// there is a fatal error that terminate the request.
    ///
    /// An error log is already emitted if there is any error. This phase is used for collecting
    /// metrics and sending access logs.
    async fn logging(&self, _session: &mut Session, _e: Option<&Error>, _ctx: &mut ProxyContext) {}
}

/// Applies regex-based URI rewriting based on provided precompiled patterns.
///
/// # Arguments
/// - `uri`: The input URI to be rewritten.
/// - `regex_patterns`: A slice of tuples containing precompiled `Regex` objects and their corresponding replacement templates.
///
/// # Returns
/// - `String`: The rewritten URI if a pattern matches, or the original URI if no match is found.
///
/// # Notes
/// - The regex patterns are precompiled during plugin creation (e.g., in `proxy_rewrite` or `redirect` plugins)
///   to avoid repeated compilation overhead.
/// - This function assumes that the regex patterns are valid, as they are validated during plugin configuration.
pub fn apply_regex_uri_template(uri: &str, regex_patterns: &[(Regex, String)]) -> String {
    for (re, redirect_template) in regex_patterns {
        if let Some(captures) = re.captures(uri) {
            // Generate new URI by replacing capture groups in the template
            let redirect_uri = captures
                .iter()
                .skip(1) // Skip the full match
                .enumerate()
                .fold(redirect_template.to_string(), |acc, (i, capture)| {
                    // Replace $1, $2, ... with capture groups
                    acc.replace(&format!("${}", i + 1), capture.unwrap().as_str())
                });
            return redirect_uri;
        }
    }

    // If no match, return original URI
    uri.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redirect_with_valid_match() {
        let regex_patterns = [
            (
                Regex::new(r"^/iresty/(.*)/(.*)/(.*)").unwrap(),
                "/$1-$2-$3".to_string(),
            ),
            (
                Regex::new(r"^/theothers/(.*)/(.*)").unwrap(),
                "/theothers/$1-$2".to_string(),
            ),
        ];
        let uri = "/iresty/a/b/c";

        let result = apply_regex_uri_template(uri, &regex_patterns);

        assert_eq!(result, "/a-b-c");
    }

    #[test]
    fn test_second_match_in_multi_patterns() {
        let regex_patterns = [
            (
                Regex::new(r"^/iresty/(.*)/(.*)/(.*)").unwrap(),
                "/$1-$2-$3".to_string(),
            ),
            (
                Regex::new(r"^/theothers/(.*)/(.*)").unwrap(),
                "/theothers/$1-$2".to_string(),
            ),
        ];
        let uri = "/theothers/x/y";

        let result = apply_regex_uri_template(uri, &regex_patterns);

        assert_eq!(result, "/theothers/x-y");
    }

    #[test]
    fn test_no_match_should_return_original_uri() {
        let regex_patterns = [
            (
                Regex::new(r"^/iresty/(.*)/(.*)/(.*)").unwrap(),
                "/$1-$2-$3".to_string(),
            ),
            (
                Regex::new(r"^/theothers/(.*)/(.*)").unwrap(),
                "/theothers/$1-$2".to_string(),
            ),
        ];
        let uri = "/api/test";

        let result = apply_regex_uri_template(uri, &regex_patterns);

        assert_eq!(result, "/api/test");
    }

    #[test]
    fn test_empty_uri() {
        let regex_patterns = [
            (
                Regex::new(r"^/iresty/(.*)/(.*)/(.*)").unwrap(),
                "/$1-$2-$3".to_string(),
            ),
            (
                Regex::new(r"^/theothers/(.*)/(.*)").unwrap(),
                "/theothers/$1-$2".to_string(),
            ),
        ];
        let uri = "";

        let result = apply_regex_uri_template(uri, &regex_patterns);

        assert_eq!(result, "");
    }

    #[test]
    fn test_uri_with_multiple_parts() {
        let regex_patterns = [
            (
                Regex::new(r"^/iresty/(.*)/(.*)/(.*)").unwrap(),
                "/$1-$2-$3".to_string(),
            ),
            (
                Regex::new(r"^/theothers/(.*)/(.*)").unwrap(),
                "/theothers/$1-$2".to_string(),
            ),
        ];
        let uri = "/iresty/a/b/c/d/e/f";

        let result = apply_regex_uri_template(uri, &regex_patterns);

        assert_eq!(result, "/a/b/c/d-e-f");
    }

    #[test]
    fn test_uri_with_special_characters() {
        let regex_patterns = [
            (
                Regex::new(r"^/iresty/(.*)/(.*)/(.*)").unwrap(),
                "/$1-$2-$3".to_string(),
            ),
            (
                Regex::new(r"^/theothers/(.*)/(.*)").unwrap(),
                "/theothers/$1-$2".to_string(),
            ),
        ];
        let uri = "/iresty/a/!/@";

        let result = apply_regex_uri_template(uri, &regex_patterns);

        assert_eq!(result, "/a-!-@");
    }

    #[test]
    fn test_empty_template_should_return_empty_string() {
        let regex_patterns = [
            (
                Regex::new(r"^/iresty/(.*)/(.*)/(.*)").unwrap(),
                "".to_string(),
            ),
            (
                Regex::new(r"^/theothers/(.*)/(.*)").unwrap(),
                "".to_string(),
            ),
        ];
        let uri = "/iresty/a/b/c";

        let result = apply_regex_uri_template(uri, &regex_patterns);

        assert_eq!(result, "");
    }
}
