use bytes::{Bytes, BytesMut, BufMut};
use http::header::CONTENT_ENCODING;
use pingora::protocols::http::compression::Algorithm;
use pingora_proxy::ProxyHttp;

// no-op

/// Concatenate a list of `Bytes` into a single `Bytes` buffer.
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

/// Decode body according to `Content-Encoding` stored in ctx vars.
pub fn decode_body(
    ctx: &<super::mcp::MCPProxyService as ProxyHttp>::CTX,
    body: &Option<Bytes>,
) -> Option<Bytes> {
    match ctx.vars.get(CONTENT_ENCODING.as_str()) {
        Some(content_encoding) => {
            if let Some(b) = body {
                if content_encoding.contains("gzip") {
                    log::debug!("Decompressing GZIP body");
                    let mut decompressor = Algorithm::Gzip.decompressor(true).unwrap();
                    return decompressor.encode(b.as_ref(), true).ok();
                }
            }
            body.clone()
        }
        None => body.clone(),
    }
}

/// Encode body according to `Content-Encoding` stored in ctx vars.
pub fn encode_body(
    ctx: &<super::mcp::MCPProxyService as ProxyHttp>::CTX,
    body: &Option<Bytes>,
) -> Option<Bytes> {
    match ctx.vars.get(CONTENT_ENCODING.as_str()) {
        Some(content_encoding) => {
            if let Some(b) = body {
                if content_encoding.contains("gzip") {
                    log::debug!("Compressing GZIP body");
                    let mut compressor = Algorithm::Gzip.compressor(5).unwrap();
                    return compressor.encode(b.as_ref(), true).ok();
                }
            }
            body.clone()
        }
        None => body.clone(),
    }
}
