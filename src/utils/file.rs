use std::{fs, path::Path};

use url::Url;

use log::{error, info};

pub fn read_from_local_or_remote(filename: &str) -> Result<(bool, String), String> {
    let is_url = Url::parse(filename);

    match is_url {
        Ok(_) => {
            info!("openapi file is a URL: {filename}");
            // Use a synchronous, non-Tokio HTTP client to avoid runtime conflicts
            match ureq::get(filename).call() {
                Ok(response) => {
                    if response.status() >= 200 && response.status() < 300 {
                        match response.into_string() {
                            Ok(body) => Ok((true, body)),
                            Err(e) => {
                                error!("Failed to read HTTP response body: {e}");
                                Err(format!("Failed to read HTTP response body: {e}"))
                            }
                        }
                    } else {
                        error!("HTTP request failed with status: {}", response.status());
                        Err(format!("HTTP request failed with status: {}", response.status()))
                    }
                }
                Err(e) => {
                    error!("Failed to send HTTP request: {e}");
                    Err(format!("Failed to send HTTP request: {e}"))
                }
            }
        }
        Err(_) => {
            info!("openapi file is a local file: {filename}");
            match fs::read_to_string(Path::new(filename)) {
                Ok(content) => Ok((false, content)),
                Err(e) => {
                    error!("Failed to read local openapi file: {e}");
                    Err(format!("Failed to read local openapi file: {e}"))
                }
            }
        }
    }
}
