use bytes::Bytes;
use tokio::sync::broadcast;

use crate::{
    jsonrpc::create_json_rpc_response, proxy::ProxyContext, service::encoding::ContentEncoding,
    sse_event::SseEvent,
};

/// JSON-RPC response processor
pub struct ResponseProcessor<'a> {
    event_sender: &'a broadcast::Sender<SseEvent>,
}

impl<'a> ResponseProcessor<'a> {
    /// Creates a new ResponseProcessor
    pub fn new(event_sender: &'a broadcast::Sender<SseEvent>) -> Self {
        Self { event_sender }
    }

    /// Handles JSON-RPC response processing
    ///
    /// # Arguments
    /// * `ctx` - Proxy context containing encoding information
    /// * `request_id` - JSON-RPC request ID
    /// * `body` - Response body to process
    /// * `end_of_stream` - Whether this is the final chunk
    /// * `session_id` - Optional SSE session ID
    pub fn process_json_rpc_response(
        &self,
        ctx: &ProxyContext,
        request_id: &str,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        session_id: Option<String>,
    ) {
        // Decode the body if it is encoded
        if let Some(encoding) = ContentEncoding::decode(ctx, body) {
            *body = Some(encoding);
        }

        match create_json_rpc_response(request_id, body) {
            Ok(res) => match serde_json::to_string(&res) {
                Ok(json_res) => {
                    self.send_response(json_res, body, end_of_stream, session_id);
                }
                Err(e) => log::error!("Failed to serialize JSON response: {e}"),
            },
            Err(e) => log::error!("Failed to create JSON-RPC response: {e}"),
        }
    }

    /// Sends response via SSE or HTTP depending on session_id
    fn send_response(
        &self,
        json_res: String,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        session_id: Option<String>,
    ) {
        match session_id {
            Some(session_id) => {
                self.send_sse_response(session_id, json_res);
            }
            None => {
                self.send_http_response(json_res, body, end_of_stream);
            }
        }
    }

    /// Sends response via SSE
    fn send_sse_response(&self, session_id: String, json_res: String) {
        log::debug!("[SSE] Sending response, session_id: {session_id:?}");
        let event = SseEvent::new_event(session_id.as_str(), "message", &json_res);
        if let Err(e) = self.event_sender.send(event) {
            log::error!("[SSE] Failed to send event, session_id: {session_id:?}, error: {e}");
        }
    }

    /// Sends response via HTTP
    fn send_http_response(&self, json_res: String, body: &mut Option<Bytes>, end_of_stream: bool) {
        log::debug!("[StreamableHTTP] Sending response");
        if end_of_stream {
            *body = Some(Bytes::copy_from_slice(json_res.as_bytes()));
        }
    }
}
