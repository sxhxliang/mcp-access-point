pub mod route;
pub mod upstream;

use async_stream::stream;
use async_trait::async_trait;
use bytes::Bytes;

use futures::StreamExt;
use http::{header, StatusCode};
use pingora::http::ResponseHeader;
use pingora::prelude::HttpPeer;
use pingora::{Result,proxy::{ProxyHttp, Session}};
use tokio::sync::broadcast;


use crate::{mcp, utils};
use crate::types::{CallToolResult, Content, JSONRPCRequest, JSONRPCResponse, TextContent};
use crate::sse_event::SseEvent;
use crate::config::{CLIENT_MESSAGE_ENDPOINT, CLIENT_SSE_ENDPOINT, ERROR_MESSAGE, SERVER_WITH_AUTH, UPSTREAM_CONFIG};

pub struct ModelContextProtocolProxy {
    pub tx: broadcast::Sender<SseEvent>,
}

impl ModelContextProtocolProxy {
    pub fn new(tx: broadcast::Sender<SseEvent>) -> Self {
        Self { tx }
    }
}


impl ModelContextProtocolProxy {
    pub async fn response_accepted(&self, session: &mut Session) -> Result<()> {
        let mut resp = ResponseHeader::build(StatusCode::OK, Some(4))?;

        let body = Bytes::from("Accepted");
        resp.insert_header(header::CONTENT_TYPE, "text/plain")?;
        resp.insert_header(header::CONTENT_LENGTH, body.len().to_string())?;

        session.write_response_header(Box::new(resp), false).await?;

        session
            .write_response_body(Some(body.clone()), true)
            .await?;
        Ok(())
    }

    pub async fn response_sse(&self, session: &mut Session) -> Result<bool> {
        let mut resp = ResponseHeader::build(StatusCode::OK, Some(4))?;
        resp.insert_header(header::CONTENT_TYPE, "text/event-stream")?;
        resp.insert_header(header::CACHE_CONTROL, "no-cache")?;

        session.write_response_header(Box::new(resp), false).await?;

        let session_id = uuid::Uuid::new_v4().to_string();
                
        let message_url = if SERVER_WITH_AUTH {
            let parsed = utils::query_to_map(&session.req_header().uri);
            // let token = parsed.get("token");
            let token = match parsed.get("token") {
                Some(token) => token,
                None => {
                    log::error!("token is None");
                    ""
                }
            };
            format!("{CLIENT_MESSAGE_ENDPOINT}?session_id={session_id}&token={token}")
        } else {
            format!("{CLIENT_MESSAGE_ENDPOINT}?session_id={session_id}")
        };

        let mut rx = self.tx.subscribe();
        let body = stream! {
            let event = SseEvent::new_event(&session_id,"endpoint", &message_url);
            yield event.to_bytes();

            while let Ok(event) = rx.recv().await {
                log::info!("event session_id: {:?}", &event);
                if event.session_id == session_id {
                    yield event.to_bytes();
                }
            }
        };

        let mut body_stream = Box::pin(body);
        while let Some(chunk) = body_stream.next().await {
            if let Err(e) = session.write_response_body(Some(chunk.into()), false).await {
                log::error!("Failed to send SSE response: {}", e);
                break;
            }
        }
        Ok(true)
    }
}

#[async_trait]
impl ProxyHttp for ModelContextProtocolProxy {
    type CTX = ();
    fn new_ctx(&self) -> () {
    }

    /// Handle the incoming request.
    ///
    /// In this phase, users can parse, validate, rate limit, perform access control and/or
    /// return a response for this request.
    ///
    /// If the user already sent a response to this request, an `Ok(true)` should be returned so that
    /// the proxy would exit. The proxy continues to the next phases when `Ok(false)` is returned.
    ///
    /// By default this filter does nothing and returns `Ok(false)`.
    async fn request_filter(&self, session: &mut Session, _ctx: &mut Self::CTX) -> Result<bool> {
        log::debug!("Request path: {:?}", session.req_header().uri.path_and_query());
        let path = session.req_header().uri.path();
        if path == CLIENT_SSE_ENDPOINT {
            self.response_sse(session).await
        } else if path == CLIENT_MESSAGE_ENDPOINT {
            let body = session.downstream_session.read_request_body().await?;

            log::debug!("Request body: {:#?}", &body);
            if body.is_none() {
                log::warn!("Request body is none");
                return Ok(true);
            }

            match serde_json::from_slice::<JSONRPCRequest>(&body.unwrap()) {
                Ok(request) => {

                    let parsed = utils::query_to_map(&session.req_header().uri);
                    let session_id = parsed.get("session_id").unwrap();
                    log::info!("session_id: {}", session_id);
                    let _ = session.req_header_mut().append_header("MCP-SESSION-ID", session_id);
                    if request.id.is_some() {
                        let _ = session.req_header_mut().append_header("MCP-REQUEST-ID", request.id.unwrap());
                    }

                    return mcp::request_processing(session_id, self, session, &request).await;
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

    async fn request_body_filter(
        &self,
        _session: &mut Session,
        _body: &mut Option<Bytes>,
        _end_of_stream: bool,
        _ctx: &mut Self::CTX,
    ) -> Result<()>
    where
        Self::CTX: Send + Sync,
    {
        Ok(())
    }
    async fn upstream_peer(&self, session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
     
        // let upstream_peer = session.req_header_mut().remove_header("upstream_peer");
        // log::debug!("upstream_peer: {upstream_peer:?}");
        let config = UPSTREAM_CONFIG.read().unwrap();
        let addr = (config.ip.clone(), config.port);
        let peer = Box::new(HttpPeer::new(addr, false, "one.one.one.one".to_string()));
        Ok(peer)
    }

    fn upstream_response_filter(
        &self,
        session: &mut Session,
        upstream_response_header: &mut ResponseHeader,
        _ctx: &mut Self::CTX,
    ) {
        let path = session.req_header().uri.path();
        log::debug!("Filters upstream_response_filter, Request path: {}", path);
        upstream_response_header.insert_header("Server", "MCPServer").unwrap();
        log::debug!("upstream_response header: {:?}", upstream_response_header);

    }
    fn upstream_response_body_filter(
        &self,
        session: &mut Session,
        body: &mut Option<Bytes>,
        _end_of_stream: bool,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        let path = session.req_header().uri.path();
        log::debug!("Filters upstream_response_body_filter, Request path: {}", path);
    
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
    
        if let (Some(session_id_header), Some(request_id_header)) = (session_id_header, request_id_header) {
            
            if let (Ok(session_id), Ok(request_id)) = (
                session_id_header.to_str(),
                request_id_header.to_str(),
                
            ) {

                // Construct the result object

                let result = CallToolResult {
                    content: vec![Content::Text(TextContent {
                        text: body
                            .as_ref()
                            .map_or_else(|| ERROR_MESSAGE.to_string(), |b| String::from_utf8_lossy(b).to_string()),
                        annotations: None,
                    })],
                    is_error: Some(false),
                };
                // Convert the result to JSON-RPC response

                if let Ok(request_id) = request_id.parse::<i32>() {

                    let res = JSONRPCResponse::new(request_id, serde_json::to_value(result).unwrap());
                    let event = SseEvent::new_event(session_id, "message", &serde_json::to_string(&res).unwrap());
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
        Ok(())
    }

    async fn response_filter(
        &self,
        _session: &mut Session,
        _upstream_response: &mut ResponseHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()>
    where
        Self::CTX: Send + Sync,
    {
        Ok(())
    }
}
