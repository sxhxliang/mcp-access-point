use http::header;
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::config::{self, json_to_resource};

use super::{http_admin::RequestData, PluginValidatable};

pub(super) fn validate_api_key(request_data: &RequestData, api_key: &str) -> Result<(), String> {
    match request_data.get_header("x-api-key") {
        Some(key) if key.to_str().unwrap_or("") == api_key => Ok(()),
        _ => Err("Must provide API key".into()),
    }
}

pub(super) fn validate_content_type(request_data: &RequestData) -> Result<(), String> {
    match request_data.get_header(header::CONTENT_TYPE) {
        Some(content_type) if content_type.to_str().unwrap_or("") == "application/json" => Ok(()),
        _ => Err("Content-Type must be application/json".into()),
    }
}

pub(super) fn validate_resource(resource_type: &str, body_data: &[u8]) -> Result<(), String> {
    match resource_type {
        "routes" => {
            let route = validate_with_plugins::<config::Route>(body_data)?;
            route.validate().map_err(|e| e.to_string())
        }
        "upstreams" => {
            let upstream = json_to_resource::<config::Upstream>(body_data)
                .map_err(|e| format!("Invalid JSON data: {}", e))?;
            upstream.validate().map_err(|e| e.to_string())
        }
        "services" => {
            let service = validate_with_plugins::<config::Service>(body_data)?;
            service.validate().map_err(|e| e.to_string())
        }
        "global_rules" => {
            let rule = validate_with_plugins::<config::GlobalRule>(body_data)?;
            rule.validate().map_err(|e| e.to_string())
        }
        "ssls" => {
            let ssl = json_to_resource::<config::SSL>(body_data)
                .map_err(|e| format!("Invalid JSON data: {}", e))?;
            ssl.validate().map_err(|e| e.to_string())
        }
        _ => Err("Unsupported resource type".into()),
    }
}

pub(super) fn validate_with_plugins<T: PluginValidatable + DeserializeOwned>(
    body_data: &[u8],
) -> Result<T, String> {
    let resource =
        json_to_resource::<T>(body_data).map_err(|e| format!("Invalid JSON data: {}", e))?;
    resource.validate_plugins().map_err(|e| e.to_string())?;
    Ok(resource)
}
