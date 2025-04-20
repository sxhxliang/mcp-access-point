use std::sync::Arc;

use async_trait::async_trait;
use log::info;
use pingora_core::{Error, Result};
use pingora_error::{ErrorType::ReadError, OrErr};
use pingora_proxy::Session;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

use crate::{proxy::ProxyContext, utils::request};

use super::ProxyPlugin;

pub const PLUGIN_NAME: &str = "file-logger";

pub fn create_file_logger_plugin(cfg: YamlValue) -> Result<Arc<dyn ProxyPlugin>> {
    let config: PluginConfig = serde_yaml::from_value(cfg)
        .or_err_with(ReadError, || "Invalid file logger plugin config")?;

    let log_format = LogFormat::parse(&config.log_format)?;

    Ok(Arc::new(PluginFileLogger { log_format }))
}

/// Configuration for the file logger plugin.
#[derive(Default, Debug, Serialize, Deserialize)]
struct PluginConfig {
    #[serde(default = "PluginConfig::default_log_format")]
    log_format: String,
}

impl PluginConfig {
    fn default_log_format() -> String {
        "$remote_addr \"$request_method $uri\" $status".to_string()
    }
}

pub struct PluginFileLogger {
    log_format: LogFormat,
}

#[async_trait]
impl ProxyPlugin for PluginFileLogger {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn priority(&self) -> i32 {
        399
    }

    async fn logging(&self, session: &mut Session, e: Option<&Error>, ctx: &mut ProxyContext) {
        info!("{}", self.log_format.render(session, e, ctx));
    }
}

#[derive(Debug)]
enum Segment {
    Static(String),
    Variable(String),
}

#[derive(Debug)]
struct LogFormat {
    segments: Vec<Segment>,
}

impl LogFormat {
    fn parse(format: &str) -> Result<Self> {
        let re = Regex::new(r"\$[a-zA-Z0-9_]+")
            .or_err_with(ReadError, || "Failed to parse log format")?;
        let mut segments = Vec::new();
        let mut last_pos = 0;

        for mat in re.find_iter(format) {
            // 添加变量前的静态部分
            if last_pos < mat.start() {
                segments.push(Segment::Static(format[last_pos..mat.start()].to_string()));
            }
            // 添加变量（移除 $ 前缀）
            segments.push(Segment::Variable(mat.as_str()[1..].to_string()));
            last_pos = mat.end();
        }

        // 添加剩余的静态部分
        if last_pos < format.len() {
            segments.push(Segment::Static(format[last_pos..].to_string()));
        }

        Ok(LogFormat { segments })
    }

    fn render(&self, session: &mut Session, e: Option<&Error>, ctx: &mut ProxyContext) -> String {
        // 预估容量：模板长度 + 变量值的平均长度
        let estimated_len = self.segments.iter().fold(0, |acc, seg| {
            acc + match seg {
                Segment::Static(s) => s.len(),
                Segment::Variable(var) => match var.as_str() {
                    "status" => 4,            // 3-4 字节（如 "200"）
                    "request_method" => 8,    // 3-7 字节（如 "GET"）
                    "request_id" => 36,       // UUID 长度
                    "http_user_agent" => 128, // 浏览器 UA 可能较长
                    _ => 32,                  // 默认值
                },
            }
        });

        // 创建输出字符串并预分配容量
        let mut output = String::with_capacity(estimated_len);

        for segment in &self.segments {
            match segment {
                Segment::Static(text) => output.push_str(text),
                Segment::Variable(var) => {
                    let value = match var.as_str() {
                        "request_method" => session.req_header().method.as_str(),
                        "uri" => session.req_header().uri.path(),
                        "query_string" => session.req_header().uri.query().unwrap_or_default(),
                        "http_host" => session.req_header().uri.host().unwrap_or_default(),
                        "request_time" => &ctx.request_start.elapsed().as_millis().to_string(),
                        "http_user_agent" => {
                            request::get_req_header_value(session.req_header(), "user-agent")
                                .unwrap_or_default()
                        }
                        "http_referer" => {
                            request::get_req_header_value(session.req_header(), "referer")
                                .unwrap_or_default()
                        }
                        "remote_addr" => &session
                            .client_addr()
                            .map(ToString::to_string)
                            .unwrap_or_default(),
                        "remote_port" => &session
                            .client_addr()
                            .and_then(|s| s.as_inet())
                            .map_or_else(|| "".to_string(), |i| i.port().to_string()),
                        "server_addr" => &session
                            .server_addr()
                            .map_or_else(|| "".to_string(), |addr| addr.to_string()),
                        "status" => &session
                            .response_written()
                            .map(|v| v.status.as_u16().to_string())
                            .unwrap_or_default(),
                        "server_protocol" => {
                            if session.is_http2() {
                                "http/2"
                            } else {
                                "http/1.1"
                            }
                        }
                        "request_id" => ctx.vars.get("request-id").map_or("", |s| s.as_str()),
                        "body_bytes_sent" => &session.body_bytes_sent().to_string(),
                        "error" => &e.map(|e| e.to_string()).unwrap_or_default(),
                        _ => "",
                    };
                    output.push_str(value);
                }
            }
        }

        output
    }
}
