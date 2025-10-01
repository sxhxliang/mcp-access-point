use bytes::{BufMut, Bytes, BytesMut};
use pingora_proxy::Session;

use crate::{
    proxy::ProxyContext,
    service::{
        encoding::ContentEncoding,
        endpoint::{MCP_REQUEST_ID, MCP_SESSION_ID, MCP_STREAMABLE_HTTP},
        response_processor::ResponseProcessor,
    },
};

/// Body buffering and filtering handler
pub struct BodyHandler;

impl BodyHandler {
    /// Concatenates multiple body parts into a single Bytes
    pub fn concat_body_bytes(parts: &[Bytes]) -> Bytes {
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

    /// Buffers incoming body chunks
    pub fn buffer_body_chunk(body: &mut Option<Bytes>, ctx: &mut ProxyContext) {
        if let Some(b) = body {
            log::debug!("upstream body size: {}", b.len());
            ctx.body_buffer.push(b.clone());
            b.clear();
        } else {
            log::debug!("upstream response Body is None");
        }
    }

    /// Processes buffered body when stream ends
    pub fn process_buffered_body(
        session: &Session,
        ctx: &mut ProxyContext,
        processor: &ResponseProcessor,
        end_of_stream: bool,
    ) -> Option<Bytes> {
        let path = session.req_header().uri.path();
        log::debug!("Processing buffered body for path: {path}");

        let mut body_buffer = Some(Self::concat_body_bytes(&ctx.body_buffer));

        // SSE endpoint processing
        if let (Some(session_id), Some(request_id)) =
            (ctx.vars.get(MCP_SESSION_ID), ctx.vars.get(MCP_REQUEST_ID))
        {
            processor.process_json_rpc_response(
                ctx,
                request_id,
                &mut body_buffer,
                end_of_stream,
                Some(session_id.to_string()),
            );
        }

        // Handle MCP streaming HTTP responses
        Self::handle_streamable_http(ctx, processor, &mut body_buffer, end_of_stream);

        log::debug!("Encoding body buffer");
        ContentEncoding::encode(ctx, &body_buffer)
    }

    /// Handles streamable HTTP response types
    fn handle_streamable_http(
        ctx: &ProxyContext,
        processor: &ResponseProcessor,
        body_buffer: &mut Option<Bytes>,
        end_of_stream: bool,
    ) {
        match ctx.vars.get(MCP_STREAMABLE_HTTP) {
            Some(http_type) => match http_type.as_str() {
                "stream" => {
                    log::debug!("Handling streaming responses");
                    // Body will be replaced with "Accepted" in the caller
                }
                "stateless" => {
                    log::debug!("Handling stateless responses");
                    Self::process_stateless_response(ctx, processor, body_buffer, end_of_stream);
                }
                _ => log::error!("Invalid http_type value: {http_type}"),
            },
            None => {
                Self::process_default_response(ctx, processor, body_buffer, end_of_stream);
            }
        }
    }

    /// Processes stateless HTTP response
    fn process_stateless_response(
        ctx: &ProxyContext,
        processor: &ResponseProcessor,
        body_buffer: &mut Option<Bytes>,
        end_of_stream: bool,
    ) {
        if let Some(request_id) = ctx.vars.get(MCP_REQUEST_ID) {
            processor.process_json_rpc_response(ctx, request_id, body_buffer, end_of_stream, None);
        } else {
            log::warn!("MCP-REQUEST-ID not found");
        }
    }

    /// Processes default (non-streamable) response
    fn process_default_response(
        ctx: &ProxyContext,
        processor: &ResponseProcessor,
        body_buffer: &mut Option<Bytes>,
        end_of_stream: bool,
    ) {
        if let Some(request_id) = ctx.vars.get(MCP_REQUEST_ID) {
            processor.process_json_rpc_response(ctx, request_id, body_buffer, end_of_stream, None);
        } else {
            log::error!("MCP-REQUEST-ID not found");
        }
    }
}
