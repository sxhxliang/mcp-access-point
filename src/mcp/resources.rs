#[warn(dead_code)]
use pingora::{proxy::Session, Result};
use pingora_proxy::ProxyHttp;
use serde_json::Map;

use crate::{
    jsonrpc::{JSONRPCRequest, JSONRPCResponse},
    mcp::send_json_response,
    service::mcp::MCPProxyService,
    types::{
        ListResourceTemplatesResult, ListResourcesResult, ReadResourceResult,
        ReadResourceResultContentsItem, RequestId, Resource, ResourceTemplate,
        TextResourceContents,
    },
};

pub struct ResourceManager {
    resources: Vec<Resource>,
    templates: Vec<ResourceTemplate>,
}

impl ResourceManager {
    pub fn new() -> Self {
        ResourceManager {
            resources: vec![],
            templates: vec![],
        }
    }

    fn add_resource(&mut self, resource: Resource) {
        self.resources.push(resource);
    }
    fn add_template(&mut self, resource_template: ResourceTemplate) {
        self.templates.push(resource_template);
    }
    fn get_resources(&self) -> &Vec<Resource> {
        &self.resources
    }
    fn get_resource_by_uri(&self, uri: &str) -> Option<&Resource> {
        self.resources.iter().find(|r| r.uri == uri)
    }
    fn get_templates(&self) -> &Vec<ResourceTemplate> {
        &self.templates
    }
}

pub async fn request_processing(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    session_id: &str,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
    request: &JSONRPCRequest,
    stream: bool,
) -> Result<bool> {
    let request_id = request.id.clone().unwrap_or(RequestId::Integer(0));
    match request.method.as_str() {
        "resources/subscribe" => {
            // Todo: handle subscription
            log::debug!("resources/subscribe");
            Ok(true)
        }
        "resources/unsubscribe" => {
            // Todo: handle unsubscription
            log::debug!("resources/unsubscribe");
            Ok(true)
        }
        "resources/list" => {
            let result = ListResourcesResult {
                meta: Map::new(),
                next_cursor: None,
                resources: vec![Resource {
                    annotations: None,
                    uri: "file:///logs/app.log".to_string(),
                    name: "Application Logs".to_string(),
                    description: Some(
                        "[mock data]application logs with timestamp, level, message".to_string(),
                    ),
                    mime_type: Some("text/plain".to_string()),
                    size: None,
                }],
            };
            let res = JSONRPCResponse::new(request_id, serde_json::to_value(result).unwrap());
            send_json_response(mcp_proxy, session, &res, stream, session_id).await?;

            log::debug!("resources/list");
            Ok(true)
        }
        "resources/read" => {
            log::debug!("resources/read");
            if request.params.is_some() {
                let params = request.params.as_ref().unwrap();
                if let Some(uri) = params.get("uri") {
                    log::info!("resources/read uri: {}", uri);
                    let result = ReadResourceResult {
                        meta: Map::new(),
                        contents: vec![ReadResourceResultContentsItem::TextResourceContents(
                            TextResourceContents {
                                uri: uri.to_string(),
                                mime_type: Some("text/plain".to_string()),
                                text: "[mock data] resources/read".to_string(),
                            },
                        )],
                    };
                    let res =
                        JSONRPCResponse::new(request_id, serde_json::to_value(result).unwrap());

                    send_json_response(mcp_proxy, session, &res, stream, session_id).await?;
                }
            }

            mcp_proxy.response_accepted(session).await?;
            Ok(true)
        }
        "resources/templates/list" => {
            let result = ListResourceTemplatesResult {
                meta: Map::new(),
                next_cursor: None,
                resource_templates: vec![
                    ResourceTemplate {
                        annotations: None,
                        uri_template: "greeting://{name}".to_string(),
                        name: "get_greeting".to_string(),
                        description: Some("Get a personalized greeting".to_string()),
                        mime_type: Some("image/jpeg".to_string()),
                    },
                    ResourceTemplate {
                        annotations: None,
                        uri_template: "users://{user_id}/profile".to_string(),
                        name: "get_user_profile".to_string(),
                        description: Some("Dynamic user data".to_string()),
                        mime_type: None,
                    },
                ],
            };

            let res = JSONRPCResponse::new(request_id, serde_json::to_value(result).unwrap());

            send_json_response(mcp_proxy, session, &res, stream, session_id).await?;
            Ok(true)
        }
        _ => {
            let res = JSONRPCResponse::new(request_id, serde_json::to_value("{}").unwrap());
            send_json_response(mcp_proxy, session, &res, stream, session_id).await?;

            Ok(true)
        }
    }
}
