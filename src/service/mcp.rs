use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use http::{
    header::{CONTENT_ENCODING, CONTENT_TYPE},
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
    jsonrpc::JSONRPCRequest,
    plugin::ProxyPlugin,
    proxy::{global_rule::global_plugin_fetch, route::global_route_match_fetch, ProxyContext},
    service::{
        body_handler::BodyHandler,
        endpoint::{MCP_REQUEST_ID, MCP_SESSION_ID, MCP_STREAMABLE_HTTP},
        request_handler::RequestHandler,
        response::ResponseBuilder,
        response_processor::ResponseProcessor,
        sse::SseHandler,
    },
    sse_event::SseEvent,
    utils::request::apply_chunked_encoding,
};

/// Proxy service.
///
/// Manages the proxying of requests to upstream servers.
pub struct MCPProxyService {
    /// SSE handler for managing Server-Sent Events connections
    sse_handler: SseHandler,
}

/// HTTP proxy service implementation.
/// Implements the response handling logic for the proxy service.
impl MCPProxyService {
    /// Creates a new MCPProxyService instance
    pub fn new(tx: broadcast::Sender<SseEvent>) -> Self {
        Self {
            sse_handler: SseHandler::new(tx),
        }
    }

    /// Builds and sends an accepted response with empty body
    pub async fn response_accepted(&self, session: &mut Session) -> Result<()> {
        ResponseBuilder::send_accepted(session).await
    }

    /// Builds and sends a JSON response
    pub async fn response(
        &self,
        session: &mut Session,
        code: StatusCode,
        data: String,
    ) -> Result<bool> {
        ResponseBuilder::send_json(session, code, data).await
    }

    /// Handles Server-Sent Events (SSE) connection
    pub async fn response_sse(&self, session: &mut Session) -> Result<bool> {
        self.sse_handler.handle_connection(session).await
    }

    /// Gets a reference to the SSE event sender
    pub fn event_sender(&self) -> &broadcast::Sender<SseEvent> {
        self.sse_handler.sender()
    }

    /// Parses JSON-RPC request from session body
    pub async fn parse_json_rpc_request(&self, session: &mut Session) -> Result<JSONRPCRequest> {
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
        // Check if route exists, handle MCP endpoints if not
        if ctx.route.is_none() {
            let path = session.req_header().uri.path();
            log::debug!("Route not found, checking MCP services for path: {path}");

            if !RequestHandler::is_known_endpoint(path) {
                return RequestHandler::handle_unknown_route(session).await;
            }
        }

        // Execute global rule plugins
        if ctx
            .global_plugin
            .clone()
            .request_filter(session, ctx)
            .await?
        {
            return Ok(true);
        }

        // Execute route-specific plugins
        ctx.plugin.clone().request_filter(session, ctx).await?;

        log::info!("request_filter completed, allowing proxy to upstream");

        // Route request to appropriate handler
        let path = session.req_header().uri.path().to_string();
        log::debug!(
            "Request path: {:?}",
            session.req_header().uri.path_and_query()
        );

        if let Some(result) = RequestHandler::route_request(&path, ctx, self, session).await? {
            return Ok(result);
        }

        Ok(false)
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
        if ctx.vars.contains_key(MCP_STREAMABLE_HTTP) {
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
        let is_direct_http_response = ctx.vars.contains_key(MCP_REQUEST_ID)
            && !ctx.vars.contains_key(MCP_SESSION_ID)
            && !ctx.vars.contains_key(MCP_STREAMABLE_HTTP);

        if is_direct_http_response {
            log::info!("Preserving original headers for direct HTTP response");
            // For direct HTTP responses (tools/call), preserve the original Content-Length
            // Don't modify transfer encoding - let the upstream response pass through as-is
        } else {
            // For MCP JSON-RPC responses, we need chunked encoding because body size may change
            apply_chunked_encoding(upstream_response);
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
            Some(upstream_response.headers.capacity()),
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
    ///
    /// WARNING: This function buffers the entire upstream response in memory
    /// until `end_of_stream` is true. For large responses, this may cause
    /// high memory consumption and potential OOM errors.
    fn upstream_response_body_filter(
        &self,
        session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        let path = session.req_header().uri.path();
        log::debug!("upstream_response_body_filter for path: {path}");
        log::debug!("end_of_stream: {end_of_stream}");

        // Buffer body chunks
        BodyHandler::buffer_body_chunk(body, ctx);

        // Process buffered body when stream ends
        if end_of_stream {
            let processor = ResponseProcessor::new(self.sse_handler.sender());
            let processed_body =
                BodyHandler::process_buffered_body(session, ctx, &processor, end_of_stream);

            // Handle special case for streaming responses
            if ctx.vars.get(MCP_STREAMABLE_HTTP).map(|v| v.as_str()) == Some("stream") {
                log::debug!("Handling streaming responses");
                *body = Some(Bytes::from("Accepted"));
            } else {
                *body = processed_body;
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
