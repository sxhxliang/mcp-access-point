use async_stream::stream;
use futures::StreamExt;
use http::header::{CACHE_CONTROL, CONTENT_TYPE};
use http::StatusCode;
use pingora_error::Result;
use pingora_http::ResponseHeader;
use pingora_proxy::Session;

use crate::config::{CLIENT_MESSAGE_ENDPOINT, SERVER_WITH_AUTH};
use crate::sse_event::SseEvent;
use crate::utils;

use super::mcp::MCPProxyService;

impl MCPProxyService {
    /// Handles Server-Sent Events (SSE) connection
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
        self.handle_sse_stream(session, &session_id, &message_url, rx).await
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
            match tenant_id.to_str() {
                Ok(id) => {
                    log::debug!("tenant_id: {id}");
                    base_url.push_str(&format!("/api/{id}"));
                }
                Err(e) => {
                    log::error!("MCP_TENANT_ID header contains invalid UTF-8: {e}");
                    return Err(pingora_error::Error::new(pingora::ErrorType::InvalidHTTPHeader));
                }
            }
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
        mut rx: tokio::sync::broadcast::Receiver<SseEvent>,
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
}
