use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use http::{
    header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    StatusCode,
};

use pingora::{
    modules::http::{compression::ResponseCompressionBuilder, grpc_web::GrpcWeb, HttpModules},
    ErrorType,
};
use pingora_core::upstreams::peer::HttpPeer;
use pingora_error::{Error, Result};
use pingora_http::{RequestHeader, ResponseHeader};
use pingora_proxy::{ProxyHttp, Session};

use tokio::sync::broadcast;

use crate::{
    config::{
        CLIENT_MESSAGE_ENDPOINT, CLIENT_SSE_ENDPOINT, CLIENT_STREAMABLE_HTTP_ENDPOINT,
    },
    jsonrpc::{create_json_rpc_response, JSONRPCRequest},
    plugin::ProxyPlugin,
    proxy::{global_rule::global_plugin_fetch, route::global_route_match_fetch, ProxyContext},
    service::{
        body::{concat_body_bytes, decode_body, encode_body},
        constants::{MCP_REQUEST_ID, MCP_SESSION_ID, MCP_STREAMABLE_HTTP, MCP_TENANT_ID, NEW_BODY, NEW_BODY_LEN},
        endpoint::{self},
    },
    sse_event::SseEvent,
    utils::{
        request::{match_api_path, PathMatch, apply_chunked_encoding},
    },
};

// Strategy for handling upstream response body
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyHandlingStrategy {
    // Buffer all chunks and handle at end_of_stream
    Aggregate,
    // Do not buffer, let body pass through to downstream
    PassThrough,
    // Drop chunks and return an Accepted response when done
    DropAndAccept,
}

/// Proxy service.
///
/// Manages the proxying of requests to upstream servers.
// #[derive(Default)]
pub struct MCPProxyService {
    /// SSE event channel
    pub tx: broadcast::Sender<SseEvent>,
}

/// HTTP proxy service implementation.
/// Implements the response handling logic for the proxy service.
/// This includes:
/// - Parses JSON-RPC request from session body
/// - handling JSON-RPC requests and delegating to the appropriate handler.
/// - handling SSE (Server-Sent Events) connections.
/// - building and sending HTTP responses to clients.
impl MCPProxyService {
    async fn ensure_known_route_or_404(
        &self,
        session: &mut Session,
        ctx: &mut ProxyContext,
    ) -> Result<bool> {
        if ctx.route.is_none() {
            let uri = &session.req_header().uri;
            log::debug!("Route({uri:?}) not found, check MCP services");

            let path = uri.path();
            if !is_known_mcp_path(path) {
                log::warn!("Route not found for path: {path}");
                session.respond_error(StatusCode::NOT_FOUND.as_u16()).await?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn dispatch_mcp_endpoints(
        &self,
        ctx: &mut ProxyContext,
        session: &mut Session,
        path: &str,
    ) -> Result<bool> {
        match match_api_path(path) {
            PathMatch::Sse(tenant_id) => {
                log::debug!("SSE path: {path:?}");
                set_tenant_context(ctx, session, tenant_id);
                endpoint::handle_sse_endpoint(ctx, self, session).await
            }
            PathMatch::Messages(tenant_id) => {
                log::debug!("Messages path: {path:?}");
                set_tenant_context(ctx, session, tenant_id);
                endpoint::handle_message_endpoint(ctx, self, session).await
            }
            PathMatch::StreamableHttp(tenant_id) => {
                log::debug!("Streamable HTTP path: {path:?}");
                set_tenant_context(ctx, session, tenant_id);
                endpoint::handle_streamable_http_endpoint(ctx, self, session).await
            }
            PathMatch::NoMatch => {
                log::debug!("No tenant match for path: {path:?}, using global mcp endpoint.");
                match path {
                    CLIENT_STREAMABLE_HTTP_ENDPOINT => {
                        endpoint::handle_streamable_http_endpoint(ctx, self, session).await
                    }
                    CLIENT_SSE_ENDPOINT => endpoint::handle_sse_endpoint(ctx, self, session).await,
                    CLIENT_MESSAGE_ENDPOINT => {
                        endpoint::handle_message_endpoint(ctx, self, session).await
                    }
                    _ => Ok(false),
                }
            }
        }
    }
    /// Helper method to build and send HTTP responses
    async fn build_and_send_response(
        &self,
        session: &mut Session,
        code: StatusCode,
        content_type: &str,
        body: Option<Bytes>,
    ) -> Result<bool> {
        let mut resp = ResponseHeader::build(code, None)?;

        resp.insert_header(CONTENT_TYPE, content_type)?;

        if let Some(body) = &body {
            resp.insert_header(CONTENT_LENGTH, body.len().to_string())?;
        }

        session.write_response_header(Box::new(resp), false).await?;

        session.write_response_body(body, true).await.map_err(|e| {
            log::error!("Failed to write response body: {e}");
            e
        })?;

        Ok(true)
    }
    /// Helper method to send SSE events
    pub fn new(tx: broadcast::Sender<SseEvent>) -> Self {
        Self { tx }
    }

    /// Builds and sends an accepted response with empty body
    ///
    /// # Arguments
    /// * `session` - The HTTP session to write response to
    ///
    /// # Returns
    /// Result indicating success or failure
    pub async fn response_accepted(&self, session: &mut Session) -> Result<()> {
        let _ = self
            .build_and_send_response(session, StatusCode::ACCEPTED, "text/plain", None)
            .await;
        Ok(())
    }

    /// Builds and sends a JSON response
    ///
    /// # Arguments
    /// * `session` - The HTTP session to write response to
    /// * `code` - HTTP status code
    /// * `data` - JSON string to send as response body
    ///
    /// # Returns
    /// Result indicating success or failure
    pub async fn response(
        &self,
        session: &mut Session,
        code: StatusCode,
        data: String,
    ) -> Result<bool> {
        let body = Bytes::from(data);
        self.build_and_send_response(session, code, "application/json", Some(body))
            .await
    }

    // SSE handling functions are split into src/service/sse.rs
    /// Parses JSON-RPC request from session body
    pub async fn parse_json_rpc_request(&self, session: &mut Session) -> Result<JSONRPCRequest> {
        // Read request body
        // You can only read the body once, so if you read it you have to send a response.
        // enable buffer would cache the request body, so that the request_body_filter will work fine
        session.enable_retry_buffering();
        let body = session
            .downstream_session
            .read_request_body()
            .await
            .map_err(|e| {
                log::error!("Failed to read request body: {e}");
                Error::because(ErrorType::ReadError, "Failed to read request body:", e)
            })?;

        let body = match body {
            Some(b) if !b.is_empty() => b,
            _ => {
                log::warn!("Request body is empty or None");
                return Err(Error::new(ErrorType::ReadError));
            }
        };

        serde_json::from_slice::<JSONRPCRequest>(&body)
            .map_err(|e| Error::because(ErrorType::ReadError, "Failed to parse JSON-RPC", e))
    }
    // Helper function to avoid code duplication
    pub fn handle_json_rpc_response(
        &self,
        ctx: &<MCPProxyService as ProxyHttp>::CTX,
        request_id: &str,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        session_id: Option<String>,
    ) {
        // Decode the body if it is encoded
        // depend on the encoding type in the ctx.vars
        if let Some(encoding) = decode_body(ctx, body) {
            // log::debug!(
            //     "Decoding body {:?}",
            //     String::from_utf8_lossy(&encoding).to_string()
            // );
            *body = Some(encoding);
        }
        match create_json_rpc_response(request_id, body) {
            Ok(res) => match serde_json::to_string(&res) {
                Ok(json_res) => match session_id {
                    Some(session_id) => {
                        log::debug!("[SSE] Sending response, session_id: {session_id:?}");
                        let event = SseEvent::new_event(session_id.as_str(), "message", &json_res);
                        if let Err(e) = self.tx.send(event) {
                            log::error!(
                                "[SSE] Failed to send event, session_id: {session_id:?}, error: {e}"
                            );
                        }
                    }
                    None => {
                        log::debug!("[StreamableHTTP] Sending response");
                        if end_of_stream {
                            *body = Some(Bytes::copy_from_slice(json_res.as_bytes()));
                        }
                    }
                },
                Err(e) => log::error!("Failed to serialize JSON response: {e}"),
            },
            Err(e) => log::error!("Failed to create JSON-RPC response: {e}"),
        }
    }
}

// concat_body_bytes is provided by src/service/body.rs

/// Implementation of ProxyHttp trait for MCPProxyService.
/// This implementation handles the HTTP requests and responses.
/// It uses the ProxyContext to store the context information.
#[async_trait]
impl ProxyHttp for MCPProxyService {
    type CTX = ProxyContext;

    /// Creates a new context for each request
    fn new_ctx(&self) -> Self::CTX {
        Self::CTX::default()
    }

    /// Set up downstream modules.
    ///
    /// set up [ResponseCompressionBuilder] for gzip and brotli compression.
    /// set up [GrpcWeb] for grpc-web protocol.
    fn init_downstream_modules(&self, modules: &mut HttpModules) {
        // Add disabled downstream compression module by default
        modules.add_module(ResponseCompressionBuilder::enable(0));
        // Add the gRPC web module
        modules.add_module(Box::new(GrpcWeb));
    }

    /// Handle the incoming request before any downstream module is executed.
    async fn early_request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<()> {
        // Match request to pipeline
        if let Some((route_params, route)) = global_route_match_fetch().match_request(session) {
            ctx.route_params = Some(route_params);
            ctx.route = Some(route.clone());
            ctx.plugin = route.build_plugin_executor();

            ctx.global_plugin = global_plugin_fetch();
        }

        // execute global rule plugins
        ctx.global_plugin
            .clone()
            .early_request_filter(session, ctx)
            .await?;
        log::debug!("ctx.route_params: {:?}", ctx.route_params);
        // execute plugins
        ctx.plugin.clone().early_request_filter(session, ctx).await
    }

    /// Filters incoming requests
    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool> {
        if self.ensure_known_route_or_404(session, ctx).await? { return Ok(true); }

        // execute global rule plugins
        if ctx
            .global_plugin
            .clone()
            .request_filter(session, ctx)
            .await?
        {
            return Ok(true);
        };

        // execute plugins
        ctx.plugin.clone().request_filter(session, ctx).await?;

        log::info!("request_filter completed, allowing proxy to upstream");

        log::debug!(
            "Request path: {:?}",
            session.req_header().uri.path_and_query()
        );
        let path = session.req_header().uri.path().to_string();
        // Dispatch to MCP endpoints when matched; otherwise continue upstream
        return self.dispatch_mcp_endpoints(ctx, session, path.as_str()).await;
    }

    /// Selects an upstream peer for the request
    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        // log::debug!("upstream_peer{:?}", ctx.route);
        let peer = match ctx.route.clone().as_ref() {
            Some(route) => route.select_http_peer(session),
            None => {
                //  Handle the case where the common route is not found
                log::debug!(
                    "upstream_peer upstream_id: {:#?}",
                    ctx.route_mcp.clone().unwrap().inner
                );
                //  handle the mcp route
                // ctx.route_mcp configuration is set in the request_filter phase
                // see details in the src/mcp/tools.rs file
                // and is used to select the upstream peer for the request.
                ctx.route_mcp.clone().unwrap().select_http_peer(session)
            }
        };

        if let Ok(ref peer) = peer {
            ctx.vars
                .insert("upstream".to_string(), peer._address.to_string());
        }
        log::info!("upstream peer: {:?}", peer);
        peer
    }

    async fn request_body_filter(
        &self,
        session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<()> {

        // Replace the body with the new body from ctx.vars[NEW_BODY] if present
        if let Some(new_body) = ctx.vars.get(NEW_BODY) {
            let bytes = Bytes::from(new_body.clone());
            *body = Some(bytes);
        }
           
        Ok(())
    }

    /// Modify the request before it is sent to the upstream
    ///
    /// This method is called before the request is sent to the upstream.
    /// It modifies the request header
    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        self.apply_upstream_request_plugins(session, upstream_request, ctx).await?;
        self.apply_upstream_header_rewrites(upstream_request, ctx);
        self.insert_headers_from_route_config(upstream_request, ctx)?;
        self.set_content_length_from_ctx(upstream_request, ctx)?;

        log::info!("upstream request headers: {:?}", upstream_request.headers);
        Ok(())
    }

    fn upstream_response_filter(
        &self,
        session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        if ctx.vars.contains_key(MCP_STREAMABLE_HTTP) {
            // todo add support for content-type
            if let Some(content_type) = upstream_response.headers.get(CONTENT_TYPE) {
                if content_type.to_str().map(|s| s != "application/json").unwrap_or(true) {
                    log::warn!(
                        "upstream service response content-type is {:?} ,not \"application/json\"",
                        content_type.to_str().unwrap_or("<invalid>")
                    );
                    // TODO add support for content-type other than application/json
                    // upstream_response
                    //     .insert_header(CONTENT_TYPE, "application/json")
                    //     .unwrap();
                }
            }
        }
        Ok(())
    }

    /// Modify the response before it is sent to the client.
    /// Get the content encoding from the response header
    async fn response_filter(
        &self,
        session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        // execute global rule plugins
        ctx.global_plugin
            .clone()
            .response_filter(session, upstream_response, ctx)
            .await?;

        self.apply_transfer_encoding_policy(ctx, upstream_response);
        record_content_encoding(ctx, upstream_response);
        log::info!("Upstream response status: {}", upstream_response.status);
        *upstream_response = rebuild_response_header(upstream_response)?;

        // execute plugins
        ctx.plugin
            .clone()
            .response_filter(session, upstream_response, ctx)
            .await
    }

    /// Filters the upstream response body.
    /// This method is called after the response body is received from the upstream.
    /// It decodes the response body if it is encoded.
    fn upstream_response_body_filter(
        &self,
        session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        // 策略：根据上下文决定直通、聚合或丢弃上游响应体（StreamableHTTP: stream）。
        let path = session.req_header().uri.path();
        log::debug!("Filters upstream_response_body_filter, Request path: {path}");

        match self.select_body_strategy(ctx) {
            BodyHandlingStrategy::PassThrough => {
                // 直通：不触碰 body，让 pingora 自行向下游转发，避免缓存占用。
                return Ok(());
            }
            BodyHandlingStrategy::DropAndAccept => {
                // 丢弃：不缓存上游响应体，仅在结束时返回 202 Accepted。
                if let Some(b) = body { b.clear(); }
                if end_of_stream { *body = Some(Bytes::from("Accepted")); }
                return Ok(());
            }
            BodyHandlingStrategy::Aggregate => {
                // 聚合：维持现有 JSON-RPC 打包流程，仅在结束时组装。
                self.buffer_body_chunk(body, ctx);
                if end_of_stream { self.process_end_of_stream(ctx, body, end_of_stream); }
            }
        }
        Ok(())
    }

    /// Filters the response body.
    /// This method is called after the response body is received from the upstream.
    fn response_body_filter(
        &self,
        session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<Option<Duration>> {
        log::debug!("response_body_filter");
        // execute global rule plugins
        ctx.global_plugin
            .clone()
            .response_body_filter(session, body, end_of_stream, ctx)?;

        // execute plugins
        ctx.plugin
            .clone()
            .response_body_filter(session, body, end_of_stream, ctx)?;

        Ok(None)
    }

    async fn logging(&self, session: &mut Session, e: Option<&Error>, ctx: &mut Self::CTX) {
        // execute global rule plugins
        ctx.global_plugin.clone().logging(session, e, ctx).await;

        // execute plugins
        ctx.plugin.clone().logging(session, e, ctx).await;
    }

    /// This filter is called when there is an error in the process of establishing a connection to the upstream.
    fn fail_to_connect(
        &self,
        _session: &mut Session,
        _peer: &HttpPeer,
        ctx: &mut Self::CTX,
        mut e: Box<Error>,
    ) -> Box<Error> {
        if let Some(route) = ctx.route.as_ref() {
            if let Some(upstream) = route.resolve_upstream() {
                if let Some(retries) = upstream.get_retries() {
                    if retries > 0 && ctx.tries < retries {
                        if let Some(timeout) = upstream.get_retry_timeout() {
                            if ctx.request_start.elapsed().as_millis() <= (timeout * 1000) as _ {
                                ctx.tries += 1;
                                e.set_retry(true);
                            }
                        }
                    }
                }
            }
        }
        e
    }
}

// Helpers for readability

fn is_known_mcp_path(path: &str) -> bool {
    path == CLIENT_SSE_ENDPOINT
        || path == CLIENT_MESSAGE_ENDPOINT
        || path == CLIENT_STREAMABLE_HTTP_ENDPOINT
        || match_api_path(path) != PathMatch::NoMatch
}

fn is_direct_http_response(ctx: &ProxyContext) -> bool {
    ctx.vars.contains_key(MCP_REQUEST_ID)
        && !ctx.vars.contains_key(MCP_SESSION_ID)
        && !ctx.vars.contains_key(MCP_STREAMABLE_HTTP)
}

fn record_content_encoding(ctx: &mut ProxyContext, upstream_response: &ResponseHeader) {
    if let Some(encoding) = upstream_response.headers.get(CONTENT_ENCODING) {
        log::debug!("Content-Encoding: {:?}", encoding.to_str());
        log::debug!("upstream_response.headers: {:?}", upstream_response.headers);
        if let Ok(enc) = encoding.to_str() {
            ctx.vars.insert(CONTENT_ENCODING.to_string(), enc.to_string());
        }
    }
}

fn rebuild_response_header(upstream_response: &ResponseHeader) -> Result<ResponseHeader> {
    let mut new_header = ResponseHeader::build(
        upstream_response.status,
        Some(upstream_response.headers.capacity()),
    )?;
    for (key, value) in upstream_response.headers.iter() {
        new_header.insert_header(key, value)?;
    }
    Ok(new_header)
}

// decode_body/encode_body are provided by src/service/body.rs

fn set_tenant_context(ctx: &mut ProxyContext, session: &mut Session, tenant_id: String) {
    ctx.vars.insert(MCP_TENANT_ID.to_string(), tenant_id.clone());
    let _ = session.req_header_mut().insert_header(MCP_TENANT_ID, tenant_id);
}

// Request/response small helpers

impl MCPProxyService {
    fn select_body_strategy(&self, ctx: &ProxyContext) -> BodyHandlingStrategy {
        // 直通策略：tools/call 直接 HTTP 响应，保留上游的传输特性，不缓存
        if is_direct_http_response(ctx) {
            return BodyHandlingStrategy::PassThrough;
        }
        // StreamableHTTP: stream 模式，丢弃上游正文并立即接受
        if let Some(kind) = ctx.vars.get(MCP_STREAMABLE_HTTP) {
            if kind == "stream" {
                return BodyHandlingStrategy::DropAndAccept;
            }
        }
        // 默认：聚合（用于 JSON-RPC 打包等场景）
        BodyHandlingStrategy::Aggregate
    }
    async fn apply_upstream_request_plugins(
        &self,
        session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut ProxyContext,
    ) -> Result<()> {
        ctx.global_plugin
            .clone()
            .upstream_request_filter(session, upstream_request, ctx)
            .await?;
        ctx.plugin
            .clone()
            .upstream_request_filter(session, upstream_request, ctx)
            .await
    }

    fn apply_upstream_header_rewrites(
        &self,
        upstream_request: &mut RequestHeader,
        ctx: &mut ProxyContext,
    ) {
        if let Some(upstream) = ctx.route.as_ref().and_then(|r| r.resolve_upstream()) {
            upstream.upstream_header_rewrite(upstream_request);
            upstream.upstream_host_rewrite(upstream_request);
        }
    }

    fn insert_headers_from_route_config(
        &self,
        upstream_request: &mut RequestHeader,
        ctx: &mut ProxyContext,
    ) -> Result<()> {
        if let Some(route) = ctx.route.as_ref() {
            for header in route.get_headers() {
                if header.0 == "Host" {
                    continue;
                }
                upstream_request.insert_header(header.0, header.1.as_str())?;
            }
        }
        Ok(())
    }

    fn set_content_length_from_ctx(
        &self,
        upstream_request: &mut RequestHeader,
        ctx: &mut ProxyContext,
    ) -> Result<()> {
        if let Some(len) = ctx.vars.get(NEW_BODY_LEN) {
            upstream_request.insert_header("Content-Length", len)?;
        }
        Ok(())
    }

    fn apply_transfer_encoding_policy(
        &self,
        ctx: &ProxyContext,
        upstream_response: &mut ResponseHeader,
    ) {
        if is_direct_http_response(ctx) {
            log::info!("Preserving original headers for direct HTTP response");
        } else {
            apply_chunked_encoding(upstream_response);
        }
    }

    fn buffer_body_chunk(&self, body: &mut Option<Bytes>, ctx: &mut ProxyContext) {
        if let Some(b) = body {
            log::debug!("upstream body size: {}", b.len());
            ctx.body_buffer.push(b.clone());
            b.clear();
        } else {
            log::debug!("upstream response Body is None");
        }
    }

    fn process_end_of_stream(
        &self,
        ctx: &mut ProxyContext,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
    ) {
        // 策略层已处理 stream(drop) 与 passthrough，这里只处理需要聚合场景。
        let mut body_buffer = Some(concat_body_bytes(&ctx.body_buffer));

        if let (Some(session_id), Some(request_id)) =
            (ctx.vars.get(MCP_SESSION_ID), ctx.vars.get(MCP_REQUEST_ID))
        {
            self.handle_json_rpc_response(
                ctx,
                request_id,
                &mut body_buffer,
                end_of_stream,
                Some(session_id.to_string()),
            );
        }

        match ctx.vars.get(MCP_STREAMABLE_HTTP) {
            Some(http_type) if http_type == "stateless" => {
                log::debug!("Handling stateless responses");
                if let Some(request_id) = ctx.vars.get(MCP_REQUEST_ID) {
                    self.handle_json_rpc_response(
                        ctx,
                        request_id,
                        &mut body_buffer,
                        end_of_stream,
                        None,
                    );
                } else {
                    log::warn!("MCP-REQUEST-ID not found");
                }
            }
            Some(_) | None => {
                if let Some(request_id) = ctx.vars.get(MCP_REQUEST_ID) {
                    self.handle_json_rpc_response(
                        ctx,
                        request_id,
                        &mut body_buffer,
                        end_of_stream,
                        None,
                    );
                } else {
                    log::error!("MCP-REQUEST-ID not found");
                }
            }
        }

        log::debug!("Decoding body {body_buffer:?}");
        *body = encode_body(ctx, &body_buffer);
    }
}
