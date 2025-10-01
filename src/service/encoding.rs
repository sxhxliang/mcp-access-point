use bytes::Bytes;
use http::header::CONTENT_ENCODING;
use pingora::protocols::http::compression::Algorithm;

use crate::proxy::ProxyContext;

/// Content encoding/decoding handler
pub struct ContentEncoding;

impl ContentEncoding {
    /// Decodes response body based on content-encoding header in context
    ///
    /// # Arguments
    /// * `ctx` - Proxy context containing encoding information
    /// * `body` - Optional body bytes to decode
    ///
    /// # Returns
    /// Decoded body if encoding is present and supported, otherwise returns clone of input
    pub fn decode(ctx: &ProxyContext, body: &Option<Bytes>) -> Option<Bytes> {
        match ctx.vars.get(CONTENT_ENCODING.as_str()) {
            Some(content_encoding) => {
                if let Some(b) = body {
                    if content_encoding.contains("gzip") {
                        log::debug!("Decompressing GZIP body");
                        if let Some(mut decompressor) = Algorithm::Gzip.decompressor(true) {
                            return decompressor.encode(b.as_ref(), true).ok();
                        }
                    }
                }
                body.clone()
            }
            None => body.clone(),
        }
    }

    /// Encodes response body based on content-encoding header in context
    ///
    /// # Arguments
    /// * `ctx` - Proxy context containing encoding information
    /// * `body` - Optional body bytes to encode
    ///
    /// # Returns
    /// Encoded body if encoding is present and supported, otherwise returns clone of input
    pub fn encode(ctx: &ProxyContext, body: &Option<Bytes>) -> Option<Bytes> {
        match ctx.vars.get(CONTENT_ENCODING.as_str()) {
            Some(content_encoding) => {
                if let Some(b) = body {
                    if content_encoding.contains("gzip") {
                        log::debug!("Compressing GZIP body");
                        if let Some(mut compressor) = Algorithm::Gzip.compressor(5) {
                            return compressor.encode(b.as_ref(), true).ok();
                        }
                    }
                }
                body.clone()
            }
            None => body.clone(),
        }
    }
}
