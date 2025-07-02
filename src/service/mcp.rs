use std::time::Duration;

use async_stream::stream;
use async_trait::async_trait;
use bytes::{Bytes, BytesMut, BufMut};
use futures::StreamExt;
use http::{
    header::{CACHE_CONTROL, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, TRANSFER_ENCODING},
    StatusCode,
};

use pingora::{
    modules::http::{compression::ResponseCompressionBuilder, grpc_web::GrpcWeb, HttpModules},
    protocols::http::compression::Algorithm,
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
        SERVER_WITH_AUTH,
    },
    jsonrpc::{create_json_rpc_response, JSONRPCRequest},
    plugin::ProxyPlugin,
    proxy::{global_rule::global_plugin_fetch, route::global_route_match_fetch, ProxyContext},
    service::endpoint::{self, MCP_REQUEST_ID, MCP_SESSION_ID, MCP_STREAMABLE_HTTP},
    sse_event::SseEvent,
    utils::{
        self,
        request::{match_api_path, PathMatch},
    },
};

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

    /// Handles Server-Sent Events (SSE) connection
    ///
    /// # Arguments
    /// * `session` - The HTTP session to establish SSE connection
    ///
    /// # Returns
    /// Result indicating success or failure
    pub async fn response_sse(&self, session: &mut Session) -> Result<bool> {
        // Build SSE headers
        let mut resp = ResponseHeader::build(StatusCode::OK, Some(4))?;
        resp.insert_header(CONTENT_TYPE, "text/event-stream")?;
        resp.insert_header(CACHE_CONTROL, "no-cache")?;
        session.write_response_header(Box::new(resp), false).await?;

        // Generate unique session ID
        let session_id = uuid::Uuid::new_v4().to_string();

        // Build message URL with optional auth token
        let message_url = self.build_sse_message_url(session, &session_id)?;

        // Subscribe to event channel
        let rx = self.tx.subscribe();

        // Create and handle SSE stream
        self.handle_sse_stream(session, &session_id, &message_url, rx)
            .await
    }

    /// Builds SSE message URL with optional auth token
    fn build_sse_message_url(&self, session: &mut Session, session_id: &str) -> Result<String> {
        let mut base_url = String::new();
        let mut query_params = format!("session_id={session_id}");

        // Handle auth token if enabled
        if SERVER_WITH_AUTH {
            let parsed = utils::request::query_to_map(&session.req_header().uri);
            if let Some(token) = parsed.get("token") {
                query_params.push_str(&format!("&token={token}"));
            }
        }

        // Handle tenant ID if present
        if let Some(tenant_id) = session.req_header_mut().remove_header("MCP_TENANT_ID") {
            let tenant_id = tenant_id.to_str().unwrap();
            log::debug!("tenant_id: {tenant_id}");
            base_url.push_str(&format!("/api/{tenant_id}"));
        }

        base_url.push_str(CLIENT_MESSAGE_ENDPOINT);
        Ok(format!("{base_url}?{query_params}"))
    }

    /// Handles SSE event stream
    async fn handle_sse_stream(
        &self,
        session: &mut Session,
        session_id: &str,
        message_url: &str,
        mut rx: broadcast::Receiver<SseEvent>,
    ) -> Result<bool> {
        let body = stream! {
            // Send initial connection event
            let event = SseEvent::new_event(session_id, "endpoint", message_url);
            yield event.to_bytes();

            // Process incoming events
            while let Ok(event) = rx.recv().await {
                log::debug!("Received SSE event for session: {session_id}");
                if event.session_id == session_id {
                    yield event.to_bytes();
                }
            }
        };

        // Stream events to client
        let mut body_stream = Box::pin(body);
        while let Some(chunk) = body_stream.next().await {
            session
                .write_response_body(Some(chunk.into()), false)
                .await
                .map_err(|e| {
                    log::error!(
                        "[SSE] Failed to send event, session_id: {session_id:?}, error: {e}"
                    );
                    e
                })?;
        }

        Ok(true)
    }
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

        if body.is_none() {
            log::warn!("Request body is empty");
            return Err(Error::err(ErrorType::ReadError)?);
        }

        serde_json::from_slice::<JSONRPCRequest>(&body.unwrap()).map_err(|e| {
            log::error!("Failed to parse JSON: {e}");
            Error::because(ErrorType::ReadError, "Failed to read request body:", e)
        })
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
        // denpend on the encoding type in the ctx.vars
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

fn concat_body_bytes(parts: &[Bytes]) -> Bytes {
    let mut total_len = 0;
    for part in parts {
        total_len += part.len();
    }
    
    let mut buf = BytesMut::with_capacity(total_len);
    for part in parts {
        buf.put_slice(part);
    }
    buf.freeze()
}

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
        if ctx.route.is_none() {
            let uri = &session.req_header().uri;
            log::debug!("Route({uri:?}) not found, check MCP services");

            let path = uri.path();
            let is_known_endpoint = path == CLIENT_SSE_ENDPOINT
                || path == CLIENT_MESSAGE_ENDPOINT
                || path == CLIENT_STREAMABLE_HTTP_ENDPOINT
                || match_api_path(path) != PathMatch::NoMatch;

            if !is_known_endpoint {
                // Handle unknown route case
                log::warn!("Route not found for path: {path}");
                session
                    .respond_error(StatusCode::NOT_FOUND.as_u16())
                    .await?;
                return Ok(true);
            }
        }

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
        let path = session.req_header().uri.path();
        // log::debug!("===== Request: {:?}", session.req_header());
        // Handle the request based on the path

        match match_api_path(path) {
            PathMatch::Sse(tenant_id) => {
                log::debug!("SSE path: {path:?}");
                ctx.vars
                    .insert("MCP_TENANT_ID".to_string(), tenant_id.clone());
                let _ = session
                    .req_header_mut()
                    .insert_header("MCP_TENANT_ID", tenant_id.clone());
                return endpoint::handle_sse_endpoint(ctx, self, session).await;
            }
            PathMatch::Messages(tenant_id) => {
                log::debug!("Messages path: {path:?}");
                ctx.vars
                    .insert("MCP_TENANT_ID".to_string(), tenant_id.clone());
                let _ = session
                    .req_header_mut()
                    .insert_header("MCP_TENANT_ID", tenant_id.clone());
                return endpoint::handle_message_endpoint(ctx, self, session).await;
            }
            PathMatch::StreamableHttp(tenant_id) => {
                log::debug!("Streamable HTTP path: {path:?}");
                ctx.vars
                    .insert("MCP_TENANT_ID".to_string(), tenant_id.clone());
                let _ = session
                    .req_header_mut()
                    .insert_header("MCP_TENANT_ID", tenant_id.clone());
                return endpoint::handle_streamable_http_endpoint(ctx, self, session).await;
            }
            PathMatch::NoMatch => {
                log::debug!(
                    "No tenant match for path: {path:?}, using global mcp endpoint."
                );
                match path {
                    CLIENT_STREAMABLE_HTTP_ENDPOINT => {
                        // 2025-03-26 specification protocol;
                        return endpoint::handle_streamable_http_endpoint(ctx, self, session).await;
                    }
                    CLIENT_SSE_ENDPOINT => {
                        // 2024-11-05 specification protocol;
                        return endpoint::handle_sse_endpoint(ctx, self, session).await;
                    }
                    CLIENT_MESSAGE_ENDPOINT => {
                        // 2024-11-05 specification protocol;
                        return endpoint::handle_message_endpoint(ctx, self, session).await;
                    }
                    _ => Ok(false),
                }
            }
        }
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

        // Replace the body with the new body from ctx.vars["new_body"] if present
        if let Some(new_body) = ctx.vars.get("new_body") {
            let bytes = Bytes::from(new_body.clone());
            let len = bytes.len();
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
        // execute global rule plugins
        ctx.global_plugin
            .clone()
            .upstream_request_filter(session, upstream_request, ctx)
            .await?;

        // execute plugins
        ctx.plugin
            .clone()
            .upstream_request_filter(session, upstream_request, ctx)
            .await?;

        // rewrite host header and headers
        if let Some(upstream) = ctx.route.as_ref().and_then(|r| r.resolve_upstream()) {
            // rewrite or insert headers
            upstream.upstream_header_rewrite(upstream_request);
            upstream.upstream_host_rewrite(upstream_request);
        }
        //  insert headers from route configuration
        //  see details in the src/config/route.rs file
        for header in ctx.route.as_ref().unwrap().get_headers() {
            if header.0 == "Host" {
                continue;
            }
            upstream_request.insert_header(header.0, header.1.as_str())?;
        }
        // Set Content-Length header based on ctx.vars["new_body_len"] if present
        if let Some(len) = ctx.vars.get("new_body_len") {
            upstream_request.insert_header("Content-Length", len)?;
        }
        
        log::info!("upstream request headers: {:?}", upstream_request.headers);
        Ok(())
    }

    fn upstream_response_filter(
        &self,
        session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        if ctx.vars.get(MCP_STREAMABLE_HTTP).is_some() {
            // todo add support for content-type
            if let Some(content_type) = upstream_response.headers.get(CONTENT_TYPE) {
                if content_type.to_str().unwrap() != "application/json" {
                    log::warn!(
                        "upstream service response content-type is {:?} ,not \"application/json\"",
                        content_type.to_str().unwrap()
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

        // Check if this is a tools/call direct HTTP response that should preserve original headers
        let is_direct_http_response = ctx.vars.get(MCP_REQUEST_ID).is_some() 
            && ctx.vars.get(MCP_SESSION_ID).is_none() 
            && ctx.vars.get(MCP_STREAMABLE_HTTP).is_none();

        if is_direct_http_response {
            log::info!("Preserving original headers for direct HTTP response");
            // For direct HTTP responses (tools/call), preserve the original Content-Length
            // Don't modify transfer encoding - let the upstream response pass through as-is
        } else {
            // For MCP JSON-RPC responses, we need chunked encoding because body size may change
            upstream_response.remove_header(&CONTENT_LENGTH);
            upstream_response
                .insert_header(TRANSFER_ENCODING, "Chunked")
                .unwrap();
        }
        
        // get content encoding,
        // will be used to decompress the response body in the upstream_response_body_filter phase
        // see details in the upstream_response_body_filter function
        if let Some(encoding) = upstream_response.headers.get(CONTENT_ENCODING) {
            log::debug!("Content-Encoding: {:?}", encoding.to_str());
            log::debug!("upstream_response.headers: {:?}", upstream_response.headers);
            // insert content-encoding to ctx.vars
            // will be used in the upstream_response_body_filter phase
            ctx.vars.insert(
                CONTENT_ENCODING.to_string(),
                encoding.to_str().unwrap().to_string(),
            );
        }
        
        // Log the upstream response status for debugging
        log::info!("Upstream response status: {}", upstream_response.status);
        
        // Rebuild the response header with the original upstream status code
        let mut new_header = ResponseHeader::build(
            upstream_response.status,
            Some(upstream_response.headers.capacity())
        )?;
        for (key, value) in upstream_response.headers.iter() {
            new_header.insert_header(key, value)?;
        }

        *upstream_response = new_header;

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
        let path = session.req_header().uri.path();
        log::debug!(
            "Filters upstream_response_body_filter, Request path: {path}"
        );

        // buffer the data
        // Log only the size of the body to avoid exposing sensitive data
       // buffer the data
        // Log only the size of the body to avoid exposing sensitive data
        if let Some(b) = body {
            log::debug!("upstream body size: {}", b.len());
            ctx.body_buffer.push(b.clone());
            // drop the body
            b.clear();
        } else {
            log::debug!("upstream response Body is None");
        }
        log::debug!("【end_of_stream】: {end_of_stream}");
        if end_of_stream {
            let mut body_buffer = Some(concat_body_bytes(&ctx.body_buffer));
            // SSE endpoint processing
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

            // Handle mcp streaming http responses
            match ctx.vars.get(MCP_STREAMABLE_HTTP) {
                Some(http_type) => match http_type.as_str() {
                    "stream" => {
                        log::debug!("Handling streaming responses");
                        *body = Some(Bytes::from("Accepted"));
                    }
                    "stateless" => {
                        log::debug!("Handling stateless responses");
                        if let Some(request_id) = ctx.vars.get(MCP_REQUEST_ID) {
                            self.handle_json_rpc_response(ctx, request_id, &mut body_buffer, end_of_stream, None);
                        } else {
                            log::warn!("MCP-REQUEST-ID not found");
                        }
                    }
                    _ => log::error!("Invalid http_type value: {http_type}"),
                },
                None => {
                    if let Some(request_id) = ctx.vars.get(MCP_REQUEST_ID) {
                        self.handle_json_rpc_response(ctx, request_id, &mut body_buffer, end_of_stream, None);
                    } else {
                        log::error!("MCP-REQUEST-ID not found");
                    }
                }
            };

            log::debug!(
                "Decoding body {body_buffer:?}"
            );

            *body = encode_body(ctx, &body_buffer);
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

/// Decodes response body based on content-encoding header
fn decode_body(ctx: &<MCPProxyService as ProxyHttp>::CTX, body: &Option<Bytes>) -> Option<Bytes> {
    match ctx.vars.get(CONTENT_ENCODING.as_str()) {
        Some(content_encoding) => {
            log::debug!("Content-Encoding: {content_encoding:?}");
            // log::debug!("Body: {:?}", body);
            if content_encoding.contains("gzip") {
                let mut decompressor = Algorithm::Gzip.decompressor(true).unwrap();
                let res = decompressor
                    .encode(body.as_ref().unwrap().iter().as_slice(), true)
                    .ok();
                // log::debug!("Decompressed Body: {:?}", res);
                res
            } else {
                body.clone()
            }
        }
        None => body.clone(),
    }
}

/// Encodes response body based on content-encoding header
fn encode_body(ctx: &<MCPProxyService as ProxyHttp>::CTX, body: &Option<Bytes>) -> Option<Bytes> {
    match ctx.vars.get(CONTENT_ENCODING.as_str()) {
        Some(content_encoding) => {
            if content_encoding.contains("gzip") {
                let mut compressor = Algorithm::Gzip.compressor(5).unwrap();
                let compressed = compressor.encode(body.as_ref().unwrap().iter().as_slice(), true).unwrap();
                Some(compressed)
            } else {
                body.clone()
            }
        }
        None => body.clone(),
    }
}
