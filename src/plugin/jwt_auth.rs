use std::sync::Arc;

use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use http::{header, StatusCode};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use pingora_error::{ErrorType::ReadError, OrErr, Result};
use pingora_http::ResponseHeader;
use pingora_proxy::Session;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

use crate::{proxy::ProxyContext, utils::request};

use super::ProxyPlugin;

pub const PLUGIN_NAME: &str = "jwt-auth";

/// Creates an JWT Auth plugin instance with the given configuration.
pub fn create_jwt_auth_plugin(cfg: YamlValue) -> Result<Arc<dyn ProxyPlugin>> {
    let config: PluginConfig =
        serde_yaml::from_value(cfg).or_err_with(ReadError, || "Invalid jwt auth plugin config")?;

    Ok(Arc::new(PluginJWTAuth { config }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// HTTP Header 中 JWT 的字段名，默认是 `authorization`
    #[serde(default = "PluginConfig::default_header")]
    pub header: String,

    /// Query 参数中 JWT 的字段名，默认是 `jwt`
    #[serde(default = "PluginConfig::default_query")]
    pub query: String,

    /// Cookie 中 JWT 的字段名，默认是 `jwt`
    #[serde(default = "PluginConfig::default_cookie")]
    pub cookie: String,

    /// 是否在认证后隐藏凭据（从 Header/Query/Cookie 中删除）
    #[serde(default)]
    pub hide_credentials: bool,

    /// 是否把 JWT Payload 存入 Context 中（你可以将其存入请求上下文）
    #[serde(default)]
    pub store_in_ctx: bool,

    /// 对称加密的 Secret（或私钥 base64）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,

    /// 使用的签名算法
    #[serde(default = "PluginConfig::default_algorithm")]
    pub algorithm: Algorithm,

    /// 是否是 base64 编码的 Secret
    #[serde(default)]
    pub base64_secret: bool,

    /// token 生命周期宽限期（秒）
    #[serde(default)]
    pub lifetime_grace_period: u64,

    /// 公钥（用于 RS256 / ES256）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
}

impl PluginConfig {
    fn default_header() -> String {
        "authorization".to_string()
    }

    fn default_query() -> String {
        "jwt".to_string()
    }

    fn default_cookie() -> String {
        "jwt".to_string()
    }

    fn default_algorithm() -> Algorithm {
        Algorithm::HS256
    }

    fn get_decoding_key(&self) -> Result<DecodingKey, &'static str> {
        match self.algorithm {
            Algorithm::HS256 | Algorithm::HS512 => {
                let secret = self.secret.as_ref().ok_or("missing secret")?;
                let key: Vec<u8> = if self.base64_secret {
                    general_purpose::STANDARD
                        .decode(secret)
                        .map_err(|_| "invalid base64")?
                } else {
                    secret.as_bytes().to_vec()
                };
                Ok(DecodingKey::from_secret(&key))
            }
            Algorithm::RS256 | Algorithm::ES256 => {
                let public_key = self.public_key.as_ref().ok_or("missing public_key")?;
                Ok(DecodingKey::from_rsa_pem(public_key.as_bytes()).map_err(|_| "bad pem")?)
            }
            _ => Err("unsupported algorithm"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    // 标准 JWT Claims
    exp: Option<i64>,
    iat: Option<i64>,
    nbf: Option<i64>,
    // 自定义 claims
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

pub struct PluginJWTAuth {
    config: PluginConfig,
}

#[async_trait]
impl ProxyPlugin for PluginJWTAuth {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn priority(&self) -> i32 {
        2510
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut ProxyContext) -> Result<bool> {
        let token = self.extract_token(session);
        let token = match token {
            Some(t) => t,
            None => {
                self.send_unauthorized_response(session).await?;
                return Ok(true);
            }
        };

        let key = match self.config.get_decoding_key() {
            Ok(k) => k,
            Err(_) => {
                self.send_unauthorized_response(session).await?;
                return Ok(true);
            }
        };

        // 解析 JWT
        let mut validation = Validation::new(self.config.algorithm);
        validation.leeway = self.config.lifetime_grace_period;

        let token_data = match decode::<Claims>(&token, &key, &validation) {
            Ok(data) => data,
            Err(_) => {
                self.send_unauthorized_response(session).await?;
                return Ok(true);
            }
        };

        if self.config.store_in_ctx {
            let payload_value = serde_json::to_value(&token_data.claims.extra)
                .or_err_with(ReadError, || "Invalid jwt auth payload")?;
            ctx.vars
                .insert("jwt-auth-payload".to_string(), payload_value.to_string());
        }

        Ok(false)
    }
}

impl PluginJWTAuth {
    fn extract_token(&self, session: &mut Session) -> Option<String> {
        // 1. header
        let mut token_to_return: Option<String> = None;
        let mut should_remove_header = false;

        // Scope for immutable borrow
        {
            if let Some(header_val) =
                request::get_req_header_value(session.req_header(), &self.config.header)
            {
                // Determine the token value based on the immutable reference
                if header_val.to_lowercase().starts_with("bearer ") {
                    token_to_return = Some(header_val[7..].to_string());
                } else {
                    token_to_return = Some(header_val.to_string());
                }

                // Decide if removal is needed, but don't do it yet
                if self.config.hide_credentials {
                    should_remove_header = true;
                }
            }
        } // Immutable borrow of session.req_header() ends here

        // Perform mutable operation *after* the immutable borrow is finished
        if should_remove_header {
            session.req_header_mut().remove_header(&self.config.header);
        }

        // Return the token if found in the header
        if token_to_return.is_some() {
            return token_to_return;
        }

        // 2. query 参数
        let mut should_remove_query = false;

        // Scope for immutable borrow
        {
            // Immutable borrow here
            if let Some(query) = request::get_query_value(session.req_header(), &self.config.query)
            {
                // Store the value needed for return *before* any potential mutation
                token_to_return = Some(query.to_string());

                // Decide if removal is needed, but don't do it yet
                if self.config.hide_credentials {
                    should_remove_query = true;
                }
            }
        } // Immutable borrow (related to `query`) ends here

        // Perform mutable operation *after* the immutable borrow is finished
        if should_remove_query {
            // Mutable borrow here is now safe
            let _ = request::remove_query_from_header(session.req_header_mut(), &self.config.query);
        }

        // Return the stored token if found in the query
        if token_to_return.is_some() {
            return token_to_return;
        }

        // 3. cookie
        if let Some(cookie) = request::get_cookie_value(session.req_header(), &self.config.cookie) {
            // TODO remove cookie
            return Some(cookie.to_string());
        }

        None
    }

    async fn send_unauthorized_response(&self, session: &mut Session) -> Result<()> {
        let msg = "Invalid jwt authorization";

        let mut header = ResponseHeader::build(StatusCode::UNAUTHORIZED, None)?;
        header.insert_header(header::CONTENT_LENGTH, msg.len().to_string())?;
        header.insert_header("WWW-Authenticate", "Bearer error=\"invalid_token\"")?;
        session
            .write_response_header(Box::new(header), false)
            .await?;
        session.write_response_body(Some(msg.into()), true).await?;
        Ok(())
    }
}
