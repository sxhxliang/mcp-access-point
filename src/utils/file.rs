use std::{fs, path::Path};

use url::Url;

use log::{error, info};
use reqwest::blocking::Client;
use serde_json::Value;

pub fn read_from_local_or_remote(filename: &str) -> Result<(bool, String), String> {
    let is_url = Url::parse(filename);

    match is_url {
        Ok(_) => {
            info!("openapi file is a URL: {}", filename);
            let client = Client::new();
            match client.get(filename).send() {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<Value>() {
                            Ok(openapi_data) => match serde_json::to_string(&openapi_data) {
                                Ok(content) => Ok((true, content)),
                                Err(e) => {
                                    error!("Failed to serialize openapi data: {}", e);
                                    Err("Failed to serialize openapi data".to_string())
                                }
                            },
                            Err(e) => {
                                error!("Failed to parse openapi file as JSON: {}", e);
                                Err("Failed to parse openapi file as JSON".to_string())
                            }
                        }
                    } else {
                        error!("HTTP request failed with status: {}", response.status());
                        Err(format!(
                            "HTTP request failed with status: {}",
                            response.status()
                        ))
                    }
                }
                Err(e) => {
                    error!("Failed to send HTTP request: {}", e);
                    Err(format!("Failed to send HTTP request: {}", e))
                }
            }
        }
        Err(_) => {
            info!("openapi file is a local file: {}", filename);
            match fs::read_to_string(Path::new(filename)) {
                Ok(content) => Ok((false, content)),
                Err(e) => {
                    error!("Failed to read local openapi file: {}", e);
                    Err(format!("Failed to read local openapi file: {}", e))
                }
            }
        }
    }
}
