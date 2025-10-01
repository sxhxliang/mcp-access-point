use bytes::Bytes;
use http::{header::CONTENT_TYPE, StatusCode};
use pingora_error::Result;
use pingora_http::ResponseHeader;
use pingora_proxy::Session;

/// HTTP response builder and sender
pub struct ResponseBuilder;

impl ResponseBuilder {
    /// Builds and sends HTTP responses
    ///
    /// # Arguments
    /// * `session` - The HTTP session to write response to
    /// * `code` - HTTP status code
    /// * `content_type` - Content-Type header value
    /// * `body` - Optional response body
    ///
    /// # Returns
    /// Result indicating success or failure
    pub async fn build_and_send(
        session: &mut Session,
        code: StatusCode,
        content_type: &str,
        body: Option<Bytes>,
    ) -> Result<bool> {
        let mut resp = ResponseHeader::build(code, None)?;
        resp.insert_header(CONTENT_TYPE, content_type)?;

        if let Some(body) = &body {
            resp.insert_header(http::header::CONTENT_LENGTH, body.len().to_string())?;
        }

        session.write_response_header(Box::new(resp), false).await?;
        session.write_response_body(body, true).await.map_err(|e| {
            log::error!("Failed to write response body: {e}");
            e
        })?;

        Ok(true)
    }

    /// Builds and sends an accepted response with empty body
    pub async fn send_accepted(session: &mut Session) -> Result<()> {
        let _ = Self::build_and_send(session, StatusCode::ACCEPTED, "text/plain", None).await;
        Ok(())
    }

    /// Builds and sends a JSON response
    pub async fn send_json(session: &mut Session, code: StatusCode, data: String) -> Result<bool> {
        let body = Bytes::from(data);
        Self::build_and_send(session, code, "application/json", Some(body)).await
    }
}
