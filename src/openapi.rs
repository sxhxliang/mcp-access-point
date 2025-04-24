use hickory_resolver::proto::op::header;
use once_cell::sync::Lazy;

use http::{Method, Uri};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::fs;

use crate::{
    config::{MCPOpenAPIConfig, MCPRouteMetaInfo, MCP_ROUTE_META_INFO_MAP},
    types::{ListToolsResult, Tool, ToolInputSchema},
    utils::file::read_from_local_or_remote,
};

/// Global map to store global rules, initialized lazily.
pub static MCP_TOOLS_MAP: Lazy<Arc<Mutex<ListToolsResult>>> =
    Lazy::new(|| Arc::new(Mutex::new(ListToolsResult { meta: Map::new(), next_cursor: None, tools: vec![] })));

pub fn global_openapi_tools_fetch() -> Option<ListToolsResult> {
    // Lock the Mutex and clone the inner value to return as Arc
    MCP_TOOLS_MAP.lock().ok().map(|tools| tools.clone())
}

pub fn reload_global_openapi_tools(
    openapi_content: String,
) -> Result<ListToolsResult, Box<dyn std::error::Error>> {
    let spec: OpenApiSpec = OpenApiSpec::new(openapi_content)?;
    let tools = spec.load_openapi()?;

    // Lock the Mutex and update the global tools map
    let mut map = MCP_TOOLS_MAP.lock().map_err(|e| e.to_string())?;
    *map = ListToolsResult { meta: Map::new(), next_cursor: None, tools:tools.tools.clone()};

    Ok(tools)
}

pub fn reload_global_openapi_tools_from_config(
    mcp_cfgs: Vec<MCPOpenAPIConfig>,
) -> Result<ListToolsResult, Box<dyn std::error::Error>> {
    let mut tools: ListToolsResult = ListToolsResult { meta: Map::new(), next_cursor: None, tools: vec![] };
    for mcp_cfg in mcp_cfgs {
        let (_, content) = read_from_local_or_remote(&mcp_cfg.path)?;
        let mut spec: OpenApiSpec = OpenApiSpec::new(content)?;

        if let Some(upstream_id) = mcp_cfg.upstream_id.clone() {
            spec.upstream_id = Some(upstream_id);
        } else {
            log::warn!("No upstream_id found in openapi content");
        }

        spec.mcp_config = Some(mcp_cfg);

        if tools.tools.is_empty() {
            tools = spec.load_openapi()?;
        } else {
            for tool in spec.load_openapi()?.tools {
                tools.tools.push(tool);
            }
        }
    }
    // Lock the Mutex and update the global tools map
    let mut map = MCP_TOOLS_MAP.lock().map_err(|e| e.to_string())?;
    *map = ListToolsResult{
        meta: Map::new(),
        next_cursor: None,
        tools: tools.tools.clone(),
    };

    Ok(tools)
}

#[derive(Debug, Deserialize, Clone)]
pub struct OpenApiSpec {
    pub paths: HashMap<String, PathItem>,
    pub components: Option<Components>,
    pub upstream_id: Option<String>,
    pub mcp_config: Option<MCPOpenAPIConfig>,
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

pub async fn openapi_to_tools() -> Result<ListToolsResult, Box<dyn std::error::Error>> {
    let content = fs::read_to_string("openapi.json").await?;
    // Deserialize JSON
    let spec: OpenApiSpec = OpenApiSpec::new(content)?;
    let tools = spec.load_openapi()?;
    Ok(tools)
}
impl OpenApiSpec {
    pub fn new(content: String) -> Result<Self, Box<dyn std::error::Error>> {
        let spec: OpenApiSpec = serde_json::from_str(&content)?;
        Ok(spec)
    }
    pub fn load_openapi(&self) -> Result<ListToolsResult, Box<dyn std::error::Error>> {
        let mut tools: Vec<Tool> = Vec::new();

        for (path, item) in &self.paths {
            log::debug!("Processing path: {}", path);
            self.process_method(&item.get, path, Method::GET, &mut tools);
            self.process_method(&item.post, path, Method::POST, &mut tools);
            self.process_method(&item.put, path, Method::PUT, &mut tools);
            self.process_method(&item.delete, path, Method::DELETE, &mut tools);
            self.process_method(&item.patch, path, Method::PATCH, &mut tools);
        }
        Ok(ListToolsResult {
            tools,
            meta: Map::new(),
            next_cursor: None,
        })
    }

    pub fn process_method(
        &self,
        operation: &Option<Operation>,
        path: &str,
        method: Method,
        tools: &mut Vec<Tool>,
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

        let headers = if let Some(headers) = &mcp_config.upstream_config {
            headers.headers.clone()
        } else {
            Some(HashMap::new())
        };

        // Construct MCPRouteMetaInfo with improved readability
        let mcp_route_meta_info = MCPRouteMetaInfo {
            operation_id: operation_id.to_string(),
            path: path.parse::<Uri>().unwrap(),
            method,
            upstream_id: self.upstream_id.clone(), // Consider avoiding clone if possible
            headers,
            // request_body: params.clone(),
        };
        MCP_ROUTE_META_INFO_MAP.insert(operation_id.into(), Arc::new(mcp_route_meta_info));

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

        let mut properties = HashMap::new();
        let mut required = Vec::new();

        for param in &params {
            let mut prop_type = Map::new();
            prop_type.insert("title".into(), Value::String(param.name.clone()));
            prop_type.insert("prop_type".into(), Value::String(param.param_type.clone()));
            properties.insert(
                param.name.clone(),
                prop_type
            );

            if param.required.unwrap() {
                required.push(param.name.clone());
            }
        }

        tools.push(Tool {
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
