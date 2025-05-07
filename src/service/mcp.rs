use std::{str::FromStr, time::Duration};

use async_stream::stream;
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use http::{header, StatusCode};
use log::debug;
use pingora::{
    modules::http::{compression::ResponseCompressionBuilder, grpc_web::GrpcWeb, HttpModules},
    ErrorType,
};
use pingora_core::upstreams::peer::HttpPeer;
use pingora_error::{Error, Result};
use pingora_http::{RequestHeader, ResponseHeader};
use pingora_proxy::{ProxyHttp, Session};

use serde_json::Map;
use tokio::sync::broadcast;

use crate::{proxy::upstream::upstream_fetch, sse_event::SseEvent, types::RequestId};

use crate::{
    config::{
        CLIENT_MESSAGE_ENDPOINT, CLIENT_SSE_ENDPOINT, CLIENT_STREAMABLE_HTTP_ENDPOINT,
        ERROR_MESSAGE, SERVER_WITH_AUTH,
    },
    jsonrpc::{ErrorCode, JSONRPCError, JSONRPCErrorDetails, JSONRPCRequest, JSONRPCResponse},
    mcp,
    plugin::ProxyPlugin,
    proxy::{global_rule::global_plugin_fetch, route::global_route_match_fetch, ProxyContext},
    types::{CallToolResult, CallToolResultContentItem, TextContent},
    utils,
};

const STREAMABLE_HTTP: &str = "streamable_http";
/// Proxy service.
///
/// Manages the proxying of requests to upstream servers.
// #[derive(Default)]
pub struct MCPProxyService {
    pub tx: broadcast::Sender<SseEvent>,
}

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

        resp.insert_header(header::CONTENT_TYPE, content_type)?;

        if let Some(body) = &body {
            resp.insert_header(header::CONTENT_LENGTH, body.len().to_string())?;
        }

        session.write_response_header(Box::new(resp), false).await?;

        session.write_response_body(body, true).await.map_err(|e| {
            log::error!("Failed to write response body: {}", e);
            e
        })?;

        Ok(true)
    }
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
        resp.insert_header(header::CONTENT_TYPE, "text/event-stream")?;
        resp.insert_header(header::CACHE_CONTROL, "no-cache")?;
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
        if SERVER_WITH_AUTH {
            let parsed = utils::request::query_to_map(&session.req_header().uri);
            let token = match parsed.get("token") {
                Some(data) => data,
                None => "",
            };
            Ok(format!(
                "{CLIENT_MESSAGE_ENDPOINT}?session_id={session_id}&token={token}"
            ))
        } else {
            Ok(format!("{CLIENT_MESSAGE_ENDPOINT}?session_id={session_id}"))
        }
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
                log::debug!("Received SSE event for session: {}", session_id);
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
                    log::error!("Failed to send SSE event: {}", e);
                    e
                })?;
        }

        Ok(true)
    }
    /// Parses JSON-RPC request from session body
    async fn parse_json_rpc_request(&self, session: &mut Session) -> Result<JSONRPCRequest> {
        let body = session
            .downstream_session
            .read_request_body()
            .await
            .map_err(|e| {
                log::error!("Failed to read request body: {}", e);
                return Error::because(ErrorType::ReadError, "Failed to read request body:", e);
            })?;

        if body.is_none() {
            log::warn!("Request body is empty");
            return Err(Error::err(ErrorType::ReadError)?);
        }

        serde_json::from_slice::<JSONRPCRequest>(&body.unwrap()).map_err(|e| {
            log::error!("Failed to parse JSON: {}", e);
            return Error::because(ErrorType::ReadError, "Failed to read request body:", e);
        })
    }
}

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
            log::warn!("Route({:?}) not found", session.req_header().uri);
            if session.req_header().uri.path() != CLIENT_SSE_ENDPOINT
                && session.req_header().uri.path() != CLIENT_MESSAGE_ENDPOINT
                && session.req_header().uri.path() != CLIENT_STREAMABLE_HTTP_ENDPOINT
            {
                // Handle the case where the route is not found
                // and the request is for the SSE endpoint
                log::warn!("Route not found, responding with 404");
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

        log::debug!(
            "Request path: {:?}",
            session.req_header().uri.path_and_query()
        );
        let path = session.req_header().uri.path();

        // 2025-03-26 specification protocol;
        if path == CLIENT_STREAMABLE_HTTP_ENDPOINT {
            let mcp_session_id = session.req_header().headers.get("mcp-session-id");

            // Handle GET requests for SSE streams (using built-in support from StreamableHTTP)
            if session.req_header().method == http::Method::GET {
                ctx.vars
                    .insert(STREAMABLE_HTTP.to_string(), "stream".to_string());
                log::debug!("Handle GET requests for SSE streams (using built-in support from StreamableHTTP)");
                // Check for Last-Event-ID header for resumability
                let last_event_id = session.req_header().headers.get("last-event-id");
                log::debug!("req_header: {:?}", session.req_header());
                if let Some(last_event_id) = last_event_id {
                    log::info!(
                        "Client reconnecting with Last-Event-ID: {:?}",
                        last_event_id
                    );
                } else {
                    log::info!(
                        "Establishing new SSE stream for session {:?}",
                        mcp_session_id
                    );
                }
                return self.response_sse(session).await;
            } else if session.req_header().method == http::Method::POST {
                ctx.vars
                    .insert(STREAMABLE_HTTP.to_string(), "stateless".to_string());
                //  Headers({'host': '0.0.0.0:3000', 'connection': 'keep-alive', 'accept': 'application/json, text/event-stream', 'content-type': 'application/json', 'accept-language': '*', 'sec-fetch-mode': 'cors', 'user-agent': 'node', 'accept-encoding': 'gzip, deflate', 'content-length': '205'})
                // {'jsonrpc': '2.0', 'id': 0, 'method': 'initialize', 'params': {'protocolVersion': '2024-11-05', 'capabilities': {'sampling': {}, 'roots': {'listChanged': True}}, 'clientInfo': {'name': 'mcp-inspector', 'version': '0.11.0'}}}
                // Handle POST requests for initialization or resuming a stream
                log::debug!("Handle POST requests for initialization or resuming a stream");
                if let Some(_mcp_session_id) = mcp_session_id {
                    // TODO Reuse existing transport
                    log::debug!("Reuse existing transport");
                } else {
                    // match self.parse_json_rpc_request(session).await {
                    //     Ok(request) => {
                    //         return mcp::request_processing_streamable_http(
                    //             ctx,
                    //             "session_id",
                    //             self,
                    //             session,
                    //             &request,
                    //         )
                    //         .await;
                    //     }
                    //     Err(e) => {
                    //         log::error!("Failed to process JSON-RPC request: {}", e);
                    //     }
                    // }
                    let body = match session.downstream_session.read_request_body().await {
                        Ok(body) => body,
                        Err(e) => {
                            // Handle read error gracefully
                            log::debug!("Failed to read request body: {}", e);
                            return Err(e); // Propagate the error or handle it as needed
                        }
                    };

                    log::debug!("Request body: {:#?}", &body);

                    if let Some(ref body) = body {
                        match serde_json::from_slice::<JSONRPCRequest>(body) {
                            Ok(request) => {
                                return mcp::request_processing_streamable_http(
                                    ctx,
                                    "session_id",
                                    self,
                                    session,
                                    &request.clone(),
                                )
                                .await;
                            }
                            Err(e) => {
                                // Handle JSON parsing errors gracefully
                                log::debug!("Failed to parse request body as JSON: {}", e);
                            }
                        }
                    } else {
                        // Handle the case where the body is None
                        log::debug!("Request body is empty");
                    }
                }

                return Ok(false);
            }
        }

        // 2024-11-05 specification protocol;
        if path == CLIENT_SSE_ENDPOINT {
            self.response_sse(session).await
        } else if path == CLIENT_MESSAGE_ENDPOINT {
            match self.parse_json_rpc_request(session).await {
                Ok(request) => {
                    let parsed = utils::request::query_to_map(&session.req_header().uri);
                    let session_id = parsed.get("session_id").unwrap();
                    log::info!("session_id: {}", session_id);
                    let _ = session
                        .req_header_mut()
                        .append_header("MCP-SESSION-ID", session_id);
                    if request.id.is_some() {
                        let _ = session.req_header_mut().append_header(
                            "MCP-REQUEST-ID",
                            request.id.clone().unwrap().to_string(),
                        );
                    }

                    return mcp::request_processing(
                        ctx,
                        session_id,
                        self,
                        session,
                        &request.clone(),
                    )
                    .await;
                }
                Err(e) => {
                    log::error!("Failed to parse JSON: {}", e);
                }
            }
            Ok(false)
        } else {
            Ok(false)
        }
    }

    /// Selects an upstream peer for the request
    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let peer = match ctx.route.clone().as_ref() {
            Some(route) => route.select_http_peer(session),
            None => {
                let upstream_id = session
                    .req_header_mut()
                    .remove_header("upstream_id")
                    .unwrap();
                log::info!("upstream_peer upstream_id: {:?}", upstream_id);
                let upstream = upstream_fetch(upstream_id.to_str().unwrap());
                match upstream {
                    Some(_upstream) => {
                        log::info!("upstream_peer upstream found ");
                    }
                    None => {
                        log::warn!("upstream_peer upstream not found");
                    }
                };

                // let upstream = upstream_fetch(upstream_id.to_str().unwrap()).unwrap();
                // let peer = upstream.select_backend(session);
                ctx.route_mcp.clone().unwrap().select_http_peer(session)
                // log::warn!("Route not found");
            }
        };
        // log::info!("upstream_peer: {:?}", ctx.route_mcp.unwrap().inner);
        // let peer = ctx.route.as_ref().unwrap().select_http_peer(session);
        if let Ok(ref peer) = peer {
            ctx.vars
                .insert("upstream".to_string(), peer._address.to_string());
        }
        log::info!("upstream peer: {:?}", peer);
        peer
    }

    // Modify the request before it is sent to the upstream
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

        // rewrite host header
        if let Some(upstream) = ctx.route.as_ref().and_then(|r| r.resolve_upstream()) {
            upstream.upstream_host_rewrite(upstream_request);
        }
        log::info!("upstream host: {:?}", upstream_request.headers);
        Ok(())
    }

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

        // Remove content-length because the size of the new body is unknown
        upstream_response.remove_header("Content-Length");
        upstream_response
            .insert_header("Transfer-Encoding", "Chunked")
            .unwrap();

        // execute plugins
        ctx.plugin
            .clone()
            .response_filter(session, upstream_response, ctx)
            .await
    }
    fn upstream_response_body_filter(
        &self,
        session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        let path = session.req_header().uri.path();
        log::debug!(
            "Filters upstream_response_body_filter, Request path: {}",
            path
        );

        // Log only the size of the body to avoid exposing sensitive data
        if let Some(body) = body {
            log::debug!("upstream body {:?}", body);
        } else {
            log::info!("upstream response Body is None");
        }

        // Safely retrieve headers
        let headers = &session.req_header().headers;
        let session_id_header = headers.get("MCP-SESSION-ID");
        let request_id_header = headers.get("MCP-REQUEST-ID");
        log::debug!("session_id_header: {:?}", session_id_header);
        log::debug!("request_id_header: {:?}", request_id_header);

        // SSE endpoint handling
        if let (Some(session_id_header), Some(request_id_header)) =
            (session_id_header, request_id_header)
        {
            if let (Ok(session_id), Ok(request_id)) =
                (session_id_header.to_str(), request_id_header.to_str())
            {
                // Construct the result object

                let result = CallToolResult {
                    meta: Map::new(),
                    content: vec![CallToolResultContentItem::TextContent(TextContent {
                        type_: "text".to_string(),
                        text: body.as_ref().map_or_else(
                            || ERROR_MESSAGE.to_string(),
                            |b| String::from_utf8_lossy(b).to_string(),
                        ),
                        annotations: None,
                    })],
                    is_error: Some(false),
                };
                // Convert the result to JSON-RPC response

                if let Ok(request_id) = request_id.parse::<i64>() {
                    let res = JSONRPCResponse::new(
                        RequestId::from(request_id),
                        serde_json::to_value(result).unwrap(),
                    );
                    let event = SseEvent::new_event(
                        session_id,
                        "message",
                        &serde_json::to_string(&res).unwrap(),
                    );
                    // Send the event (placeholder for actual implementation)
                    if let Err(e) = self.tx.send(event) {
                        log::error!("Failed to send SSE event: {}", e);
                    }
                } else {
                    log::error!("Invalid MCP-REQUEST-ID format");
                }
            } else {
                log::error!("Headers contain invalid characters");
            }
            *body = Some(Bytes::from("Accepted"));
        }

        if end_of_stream {
            log::debug!("upstream_response_body_filter End of stream reached");
        }

        match ctx.vars.get(STREAMABLE_HTTP) {
            Some(http_type) => {
                match http_type.as_str() {
                    "stream" => {
                        // Handle streaming responses
                        log::debug!("Handling streaming responses");
                        *body = Some(Bytes::from("Accepted"));
                    }
                    "stateless" => {
                        // Handle stateless responses
                        log::debug!("Handling stateless responses");
                        let result = CallToolResult {
                            meta: Map::new(),
                            content: vec![CallToolResultContentItem::TextContent(TextContent {
                                type_: "text".to_string(),
                                text: body.as_ref().map_or_else(
                                    || ERROR_MESSAGE.to_string(),
                                    |b| String::from_utf8_lossy(b).to_string(),
                                ),
                                annotations: None,
                            })],
                            is_error: Some(false),
                        };
                        let res = JSONRPCResponse::new(
                            RequestId::from(0),
                            serde_json::to_value(result).unwrap(),
                        );
                        // TODO Send the response to the client
                        let data_body = serde_json::to_string(&res).unwrap();
                        log::debug!("data_body: {:?}", data_body);
                        *body = Some(Bytes::copy_from_slice(data_body.as_bytes()));
                    }
                    _ => {
                        log::error!("Invalid http_type value");
                    }
                }
            }
            None => {
                // let result = CallToolResult {
                //     meta: Map::new(),
                //     content: vec![CallToolResultContentItem::TextContent(TextContent {
                //         type_: "text".to_string(),
                //         text: body.as_ref().map_or_else(
                //             || ERROR_MESSAGE.to_string(),
                //             |b| String::from_utf8_lossy(b).to_string(),
                //         ),
                //         annotations: None,
                //     })],
                //     is_error: Some(false),
                // };
                // let res =
                //     JSONRPCResponse::new(RequestId::from(0), serde_json::to_value(result).unwrap());
                // // *body = Some(Bytes::from(serde_json::to_string(&result).unwrap()));
                // // session.downstream_session.write_response_header(resp)
                // let data_body = serde_json::to_string(&res).unwrap();
                // log::debug!("data_body: {:?}", data_body);
                // // *body = Some(Bytes::copy_from_slice(data_body.as_bytes()));
            }
        };
        Ok(())
    }

    fn response_body_filter(
        &self,
        session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<Option<Duration>> {
        // execute global rule plugins
        ctx.global_plugin
            .clone()
            .response_body_filter(session, body, end_of_stream, ctx)?;

        // execute plugins
        ctx.plugin
            .clone()
            .response_body_filter(session, body, end_of_stream, ctx)?;

        if end_of_stream {
            log::debug!("response_body_filter End of stream reached");
        }
        // if end_of_stream {
        //     // This is the last chunk, we can process the data now
        //     let json_body: Resp = serde_json::de::from_slice(&ctx.buffer).unwrap();
        //     let yaml_body = serde_yaml::to_string(&json_body).unwrap();
        //     *body = Some(Bytes::copy_from_slice(yaml_body.as_bytes()));
        // }
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
