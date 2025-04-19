use pingora::{proxy::Session, Result};

use crate::{
    service::ModelContextProtocolProxy,
    sse_event::SseEvent,
    types::{
        JSONRPCRequest, JSONRPCResponse, ListResourcesResult, ReadResourceResult, Resource,
        ResourceContents, ResourceTemplate, ResourceTemplateListResult, TextResourceContents,
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
    session_id: &str,
    mcp_proxy: &ModelContextProtocolProxy,
    session: &mut Session,
    request: &JSONRPCRequest,
) -> Result<bool> {
    let mut request_id = 0;
    if request.id.is_some() {
        request_id = request.id.unwrap();
    }
    match request.method.as_str() {
        "resources/subscribe" => {
            // Todo: handle subscription
            log::debug!("resources/subscribe");
            return Ok(true);
        }
        "resources/unsubscribe" => {
            // Todo: handle unsubscription
            log::debug!("resources/unsubscribe");
            return Ok(true);
        }
        "resources/list" => {
            let result = ListResourcesResult {
                resources: vec![Resource {
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

            let event =
                SseEvent::new_event(session_id, "message", &serde_json::to_string(&res).unwrap());
            let _ = mcp_proxy.tx.send(event);
            mcp_proxy.response_accepted(session).await?;
            log::debug!("resources/list");
            return Ok(true);
        }
        "resources/read" => {
            log::debug!("resources/read");
            if request.params.is_some() {
                let params = request.params.as_ref().unwrap();
                if let Some(uri) = params.get("uri") {
                    log::info!("resources/read uri: {}", uri);
                    let result = ReadResourceResult {
                        contents: vec![ResourceContents::Text(TextResourceContents {
                            uri: uri.to_string(),
                            mime_type: Some("text/plain".to_string()),
                            text: "[mock data] resources/read".to_string(),
                        })],
                    };
                    let res =
                        JSONRPCResponse::new(request_id, serde_json::to_value(result).unwrap());
                    let event = SseEvent::new_event(
                        session_id,
                        "message",
                        &serde_json::to_string(&res).unwrap(),
                    );
                    let _ = mcp_proxy.tx.send(event);
                }
            }

            mcp_proxy.response_accepted(session).await?;
            return Ok(true);
        }
        "resources/templates/list" => {
            let result = ResourceTemplateListResult {
                resource_templates: vec![
                    ResourceTemplate {
                        uri_template: "greeting://{name}".to_string(),
                        name: "get_greeting".to_string(),
                        description: Some("Get a personalized greeting".to_string()),
                        mime_type: Some("image/jpeg".to_string()),
                    },
                    ResourceTemplate {
                        uri_template: "users://{user_id}/profile".to_string(),
                        name: "get_user_profile".to_string(),
                        description: Some("Dynamic user data".to_string()),
                        mime_type: None,
                    },
                ],
            };

            let res = JSONRPCResponse::new(request_id, serde_json::to_value(result).unwrap());

            let event =
                SseEvent::new_event(session_id, "message", &serde_json::to_string(&res).unwrap());
            let _ = mcp_proxy.tx.send(event);

            log::debug!("resources/templates/list");
            mcp_proxy.response_accepted(session).await?;
            return Ok(true);
        }
        _ => {
            let _ = mcp_proxy.tx.send(SseEvent::new(session_id, "Accepted"));
            mcp_proxy.response_accepted(session).await?;
            return Ok(true);
        }
    }
    Ok(false)
}
