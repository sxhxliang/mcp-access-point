use dashmap::DashMap;

use http::Method;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::{collections::HashMap, sync::Arc};

use crate::{
    config::{MCPRouteMetaInfo, MCPService},
    types::{ListToolsResult, Tool, ToolInputSchema},
};

#[derive(Debug, Deserialize, Clone)]
pub struct OpenApiSpec {
    pub paths: HashMap<String, PathItem>,
    pub components: Option<Components>,
    pub upstream_id: Option<String>,
    pub mcp_config: Option<MCPService>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Components {
    pub schemas: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PathItem {
    pub get: Option<Operation>,
    pub post: Option<Operation>,
    pub put: Option<Operation>,
    pub delete: Option<Operation>,
    pub patch: Option<Operation>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Operation {
    #[serde(rename = "operationId")]
    operation_id: Option<String>,
    description: Option<String>,
    parameters: Option<Vec<Parameter>>,
    #[serde(rename = "requestBody")]
    request_body: Option<RequestBody>,
    summary: Option<String>,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
struct Parameter {
    name: String,
    #[serde(rename = "in")]
    in_location: String,
    required: Option<bool>,
    description: Option<String>,
    schema: Option<Schema>,
}

#[derive(Debug, Clone, Deserialize)]
struct RequestBody {
    description: Option<String>,
    content: HashMap<String, MediaType>,
    required: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct MediaType {
    schema: Option<Schema>,
}

#[derive(Debug, Deserialize, Clone)]
struct Schema {
    #[serde(rename = "$ref")]
    ref_path: Option<String>,
    properties: Option<HashMap<String, RequestBodySchema>>,
    required: Option<Vec<String>>,
    #[serde(rename = "type")]
    schema_type: Option<String>,
}
#[derive(Debug, Deserialize, Clone)]
struct RequestBodySchema {
    format: Option<String>,
    #[serde(rename = "type")]
    schema_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ParamInfo {
    name: String,
    description: String,
    required: Option<bool>,
    param_type: String,
}

impl OpenApiSpec {
    pub fn new(content: String) -> Result<Self, Box<dyn std::error::Error>> {
        let spec: OpenApiSpec = serde_json::from_str(&content)?;
        Ok(spec)
    }

    pub fn set_mcp_config(&mut self, mcp_config: MCPService) {
        if mcp_config.upstream_id.is_none() {
            panic!("upstream or upstream_id is required");
        }
        self.upstream_id = mcp_config.upstream_id.clone();
        self.mcp_config = Some(mcp_config);
    }
    pub fn load_openapi(
        &self,
    ) -> Result<(ListToolsResult, DashMap<String, Arc<MCPRouteMetaInfo>>), Box<dyn std::error::Error>>
    {
        let mut tools: Vec<Tool> = Vec::new();
        let mut mcp_route_metas: DashMap<String, Arc<MCPRouteMetaInfo>> = DashMap::new();
        for (path, item) in &self.paths {
            log::debug!("Processing path: {}", path);
            self.process_method(
                &item.get,
                path,
                Method::GET,
                &mut tools,
                &mut mcp_route_metas,
            );
            self.process_method(
                &item.post,
                path,
                Method::POST,
                &mut tools,
                &mut mcp_route_metas,
            );
            self.process_method(
                &item.put,
                path,
                Method::PUT,
                &mut tools,
                &mut mcp_route_metas,
            );
            self.process_method(
                &item.delete,
                path,
                Method::DELETE,
                &mut tools,
                &mut mcp_route_metas,
            );
            self.process_method(
                &item.patch,
                path,
                Method::PATCH,
                &mut tools,
                &mut mcp_route_metas,
            );
        }
        Ok((
            ListToolsResult {
                tools,
                meta: Map::new(),
                next_cursor: None,
            },
            mcp_route_metas,
        ))
    }

    pub fn process_method(
        &self,
        operation: &Option<Operation>,
        path: &str,
        method: Method,
        tools: &mut Vec<Tool>,
        mcp_route_metas: &mut DashMap<String, Arc<MCPRouteMetaInfo>>,
    ) {
        let Some(op) = operation else { return };
        let Some(operation_id) = &op.operation_id else {
            return;
        };

        let mut params = Vec::new();

        if let Some(parameters) = &op.parameters {
            for param in parameters {
                let param_type = param
                    .schema
                    .as_ref()
                    .and_then(|s| s.schema_type.clone())
                    .unwrap_or_else(|| "string".to_string());

                params.push(ParamInfo {
                    name: param.name.clone(),
                    description: param.description.clone().unwrap_or_default(),
                    required: Some(param.required.is_some() || param.in_location == "path"),
                    param_type,
                });
            }
        }

        if let Some(body) = &op.request_body {
            if let Some(media_type) = body.content.values().next() {
                // log::debug!("media_type: {:?}", media_type);
                if let Some(schema) = &media_type.schema {
                    let schema_ref = schema.ref_path.as_deref().unwrap_or("");
                    if !schema_ref.is_empty() {
                        // 处理引用的 schema
                        let schema_ref = schema_ref.trim_start_matches("#/components/schemas/");
                        // println!("schema_ref: {:?}", schema_ref);

                        if let Some(components) = &self.components {
                            // println!("components: {:?}", components);
                            if let Some(ref_schema) = components.schemas.get(schema_ref) {
                                // 处理引用的 schema
                                // println!("ref_schema:\n {:?}", ref_schema);
                                // 这里需要将 ref_schema 转换为 Schema 类型
                                let schema: Schema =
                                    serde_json::from_value(ref_schema.clone()).unwrap();
                                // println!("schema: \n {:?}", schema);
                                self.extract_schema_params(&schema, &mut params);
                            }
                        } else {
                            // 处理内联的 schema
                            self.extract_schema_params(schema, &mut params);
                        }
                        // println!("schema ref_path: {:?}", );
                        // self.extract_schema_params(schema, &mut params);
                    }
                }
                // break; // 只处理第一个 media_type
            }
        }

        // Safely extract headers with proper error handling
        let mcp_config = self.mcp_config.clone().unwrap();

        let headers = if let Some(headers) = &mcp_config.upstream {
            headers.headers.clone()
        } else {
            Some(HashMap::new())
        };

        let mut description = op.summary.clone().unwrap_or_default();
        if op.description.is_some() && op.summary != op.description {
            description.push_str(&format!(
                "  Description: {}",
                op.description.clone().unwrap_or_default()
            ));
        }
        // if !params.is_empty() {
        //     description.push_str("\n\nArgs:");
        //     for param in &params {
        //         description.push_str(&format!("\n    {}: {}", param.name, param.description));
        //     }
        // }
        // Construct MCPRouteMetaInfo with improved readability
        let mcp_route_meta_info = MCPRouteMetaInfo {
            operation_id: operation_id.to_string(),
            uri: path.to_string(),
            method: method.to_string(),
            upstream_id: self.upstream_id.clone(), // Consider avoiding clone if possible
            headers,
            ..Default::default() // request_body: params.clone(),
        };
        mcp_route_metas.insert(operation_id.into(), Arc::new(mcp_route_meta_info));
        let mut properties = HashMap::new();
        let mut required = Vec::new();

        for param in &params {
            let mut prop_type = Map::new();
            prop_type.insert("title".into(), Value::String(param.name.clone()));
            prop_type.insert("prop_type".into(), Value::String(param.param_type.clone()));
            properties.insert(param.name.clone(), prop_type);

            if param.required.unwrap() {
                required.push(param.name.clone());
            }
        }

        tools.push(Tool {
            annotations: None,
            name: operation_id.clone(),
            description: Some(description),
            input_schema: ToolInputSchema {
                properties,
                required,
                type_: "object".to_string(),
            },
        });
    }

    fn extract_schema_params(&self, schema: &Schema, params: &mut Vec<ParamInfo>) {
        let properties = match &schema.properties {
            Some(props) => props,
            None => return,
        };

        let required_fields = schema.required.as_deref().unwrap_or(&[]);

        for (name, subschema) in properties {
            let param_type = subschema
                .schema_type
                .as_deref()
                .unwrap_or("string")
                .to_string();

            params.push(ParamInfo {
                name: name.clone(),
                description: String::new(),
                required: Some(required_fields.contains(name)),
                param_type,
            });
        }
    }
}
