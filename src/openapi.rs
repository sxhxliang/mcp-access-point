
use once_cell::sync::Lazy;

use http::{Uri, Method};
use serde::Deserialize;
use serde_json::Value;
use tokio::fs;
use std::{collections::HashMap, sync::{Arc, Mutex}};

use crate::{config::{MCPOpenAPI, UpstreamConfig}, types::{ListToolsResult, Tool, ToolInputSchema, ToolInputSchemaProperty}, utils::file::read_from_local_or_remote};
use crate::proxy::route::ProxyRoute;
use crate::proxy::route::MCP_ROUTE_MAP;

/// Global map to store global rules, initialized lazily.
pub static MCP_TOOLS_MAP: Lazy<Arc<Mutex<ListToolsResult>>> = Lazy::new(|| {
    Arc::new(Mutex::new(ListToolsResult::new(vec![])))
});

pub fn global_openapi_tools_fetch() -> Option<ListToolsResult> {
    // Lock the Mutex and clone the inner value to return as Arc
    MCP_TOOLS_MAP.lock().ok().map(|tools| tools.clone())
}

pub fn reload_global_openapi_tools(openapi_content: String) -> Result<ListToolsResult, Box<dyn std::error::Error>> {
    let spec: OpenApiSpec = OpenApiSpec::new(openapi_content)?;
    let tools = spec.load_openapi()?;

    // Lock the Mutex and update the global tools map
    let mut map = MCP_TOOLS_MAP.lock().map_err(|e| e.to_string())?;
    *map = ListToolsResult::new(tools.tools.clone());

    Ok(tools)
}

pub fn reload_global_openapi_tools_from_config(openapi_contents: Vec<MCPOpenAPI>) -> Result<ListToolsResult, Box<dyn std::error::Error>> {
    let mut tools: ListToolsResult = ListToolsResult::new(vec![]);
    for openapi_content in openapi_contents {
        let (_, content)= read_from_local_or_remote(&openapi_content.path)?;
        let mut spec: OpenApiSpec = OpenApiSpec::new(content)?;
        
        if let Ok(upstream_config) = openapi_content.parse_to_upstream_config(){
            spec.upstream = Some(upstream_config);
        }

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
    *map = ListToolsResult::new(tools.tools.clone());

    Ok(tools)
}



#[derive(Debug, Deserialize)]
pub struct OpenApiSpec {
    pub paths: HashMap<String, PathItem>,
    pub components: Option<Components>,
    pub upstream: Option<UpstreamConfig>,
}

#[derive(Debug, Deserialize)]
pub struct Components {
    pub schemas: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct PathItem {
    pub get: Option<Operation>,
    pub post: Option<Operation>,
    pub put: Option<Operation>,
    pub delete: Option<Operation>,
    pub patch: Option<Operation>,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
struct Parameter {
    name: String,
    #[serde(rename = "in")]
    in_location: String,
    required: Option<bool>,
    description: Option<String>,
    schema: Option<Schema>,
}

#[derive(Debug, Deserialize)]
struct RequestBody {
    description: Option<String>,
    content: HashMap<String, MediaType>,
    required: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MediaType {
    schema: Option<Schema>,
}

#[derive(Debug, Deserialize)]
struct Schema {
    #[serde(rename = "$ref")]
    ref_path: Option<String>,
    properties: Option<HashMap<String, RequestBodySchema>>,
    required: Option<Vec<String>>,
    #[serde(rename = "type")]
    schema_type: Option<String>,
}
#[derive(Debug, Deserialize)]
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
        Ok(ListToolsResult{
            tools
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
        let Some(operation_id) = &op.operation_id else { return };

        let mut params = Vec::new();

        
        if let Some(parameters) = &op.parameters {
            for param in parameters {
                let param_type = param.schema
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
                                let schema: Schema = serde_json::from_value(ref_schema.clone()).unwrap();
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

        let proxy_route = ProxyRoute {
            operation_id: operation_id.to_string(),
            path: path.parse::<Uri>().unwrap(),
            method,
            upstream: self.upstream.clone(),
            // request_body: params.clone(),
        };
        MCP_ROUTE_MAP.insert(operation_id.into(), Arc::new(proxy_route));
    

        let mut description = op.summary.clone().unwrap_or_default();
        if op.description.is_some() && op.summary != op.description {
            description.push_str(&format!("  Description: {}", op.description.clone().unwrap_or_default()));
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
            properties.insert(
                param.name.clone(),
                ToolInputSchemaProperty {
                    title: param.name.clone(),
                    prop_type: param.param_type.clone(),
                },
            );

            if param.required.unwrap() {
                required.push(param.name.clone());
            }
        }
        

        tools.push(Tool {
            name: operation_id.clone(),
            description: Some(description),
            input_schema: ToolInputSchema {
                properties: Some(properties),
                required: Some(required),
                schema_type: "object".to_string(),
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
            let param_type = subschema.schema_type
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