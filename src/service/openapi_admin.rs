//! OpenAPI Administration Service
//! 
//! This module provides HTTP endpoints for reloading OpenAPI specifications
//! using the existing OpenAPI loading infrastructure without requiring server restarts.
//! 
//! ## OpenAPI Reload API Changes
//! This entire file is part of the OpenAPI reload API feature.
//! 
//! ## Available Endpoints
//! 
//! - `POST /openapi/reload` - Reload all MCP services
//! - `POST /openapi/reload/{service_id}` - Reload specific MCP service  
//! - `GET /openapi/status` - Get status of all services
//! - `GET /openapi/health` - Health check endpoint

use crate::proxy::mcp::{
    reload_global_openapi_tools_from_service_config,
    MCP_SERVICE_MAP
};
use bytes::Bytes;
use http::{Method, StatusCode};
use pingora_error::{Error, Result};
use pingora_http::ResponseHeader;
use pingora_proxy::Session;
use serde::{Deserialize, Serialize};

/// Response structure for reload operations
#[derive(Debug, Serialize, Deserialize)]
pub struct ReloadResponse {
    pub success: bool,
    pub message: String,
    pub services_reloaded: Vec<String>,
    pub errors: Vec<String>,
}

/// Status information for a service
#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub service_id: String,
    pub tools_count: usize,
    pub status: String,
    pub last_updated: String,
}

/// Overall status response
#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub total_services: usize,
    pub total_tools: usize,
    pub services: Vec<ServiceStatus>,
}

/// OpenAPI Administration Handler
/// 
/// This module provides functions to handle OpenAPI-related administrative operations
/// integrated into the main MCP service.
pub struct OpenAPIAdminHandler;

impl OpenAPIAdminHandler {
    /// Create a new OpenAPI admin handler instance
    pub fn new() -> Self {
        Self
    }

    /// Handle POST /openapi/reload - reload all services
    pub async fn handle_reload_all(&self) -> Result<ReloadResponse> {
        log::info!("OpenAPI Reload API Changes: Reloading all MCP services");
        
        let mut services_reloaded = Vec::new();
        let mut errors = Vec::new();
        
        // Get all service IDs from the service map
        let service_ids: Vec<String> = {
            MCP_SERVICE_MAP.iter().map(|entry| entry.key().clone()).collect()
        };
        
        log::info!("OpenAPI Reload API Changes: Found {} services to reload", service_ids.len());
        
        // Reload each service
        for service_id in service_ids {
            match self.reload_service(&service_id).await {
                Ok(_) => {
                    services_reloaded.push(service_id.clone());
                    log::info!("OpenAPI Reload API Changes: Successfully reloaded service: {}", service_id);
                }
                Err(e) => {
                    let error_msg = format!("Failed to reload {}: {}", service_id, e);
                    errors.push(error_msg.clone());
                    log::error!("OpenAPI Reload API Changes: {}", error_msg);
                }
            }
        }
        
        let response = ReloadResponse {
            success: errors.is_empty(),
            message: if errors.is_empty() {
                format!("Successfully reloaded {} services", services_reloaded.len())
            } else {
                format!("Reloaded {} services with {} errors", services_reloaded.len(), errors.len())
            },
            services_reloaded,
            errors,
        };
        
        Ok(response)
    }
    
    /// Handle POST /openapi/reload/{service_id} - reload specific service
    pub async fn handle_reload_service(&self, service_id: &str) -> Result<ReloadResponse> {
        log::info!("OpenAPI Reload API Changes: Reloading service: {}", service_id);
        
        match self.reload_service(service_id).await {
            Ok(_) => {
                let response = ReloadResponse {
                    success: true,
                    message: format!("Successfully reloaded service: {}", service_id),
                    services_reloaded: vec![service_id.to_string()],
                    errors: vec![],
                };
                log::info!("OpenAPI Reload API Changes: Service {} reloaded successfully", service_id);
                Ok(response)
            }
            Err(e) => {
                let error_msg = format!("Failed to reload service {}: {}", service_id, e);
                let response = ReloadResponse {
                    success: false,
                    message: error_msg.clone(),
                    services_reloaded: vec![],
                    errors: vec![error_msg.clone()],
                };
                log::error!("OpenAPI Reload API Changes: {}", error_msg);
                Ok(response)
            }
        }
    }
    
    /// Handle GET /openapi/status - get status of all services
    pub async fn handle_status(&self) -> Result<StatusResponse> {
        log::debug!("OpenAPI Reload API Changes: Getting status of all services");
        
        let mut services = Vec::new();
        let mut total_tools = 0;
        
        // Get service information from the static maps
        for entry in MCP_SERVICE_MAP.iter() {
            let service_id = entry.key();
            let tools_count = if let Some(service) = MCP_SERVICE_MAP.get(service_id) {
                service.value().get_tools()
                    .map(|tools| tools.tools.len())
                    .unwrap_or(0)
            } else {
                0
            };
            
            total_tools += tools_count;
            
            services.push(ServiceStatus {
                service_id: service_id.clone(),
                tools_count,
                status: "active".to_string(),
                last_updated: chrono::Utc::now().to_rfc3339(),
            });
        }
        
        let response = StatusResponse {
            total_services: services.len(),
            total_tools,
            services,
        };
        
        log::debug!("OpenAPI Reload API Changes: Status retrieved - {} services, {} tools", 
                   response.total_services, response.total_tools);
        
        Ok(response)
    }
    
    /// Handle GET /openapi/health - health check
    pub async fn handle_health(&self) -> Result<serde_json::Value> {
        log::debug!("OpenAPI Reload API Changes: Health check requested");
        
        let health = serde_json::json!({
            "status": "healthy",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "service": "openapi-admin"
        });
        
        Ok(health)
    }
    
    /// Reload a specific service using existing infrastructure
    async fn reload_service(&self, service_id: &str) -> Result<()> {
        log::debug!("OpenAPI Reload API Changes: Starting reload for service: {}", service_id);
        
        // Get the service configuration from the service map
        let service = MCP_SERVICE_MAP.get(service_id)
            .ok_or_else(|| Error::new_str("Service not found"))?;
            
        // Using existing reload function
        match reload_global_openapi_tools_from_service_config(&service.value().inner) {
            Ok(_) => {
                log::info!("OpenAPI Reload API Changes: Service {} reloaded successfully", service_id);
                Ok(())
            }
            Err(e) => {
                log::error!("OpenAPI Reload API Changes: Failed to reload service {}: {}", service_id, e);
                Err(Error::new_str("Reload failed"))
            }
        }
    }
    
    /// Helper method to create JSON responses
    fn json_response<T: Serialize>(&self, status: StatusCode, data: &T) -> Result<ResponseHeader> {
        let json = serde_json::to_string(data)
            .map_err(|e| Error::new_str("JSON serialization error"))?;
        
        let mut response = ResponseHeader::build(status, None)
            .map_err(|e| Error::new_str("Response building error"))?;
        
        response.insert_header("Content-Type", "application/json")
            .map_err(|e| Error::new_str("Header insertion error"))?;
        response.insert_header("Cache-Control", "no-cache")
            .map_err(|e| Error::new_str("Header insertion error"))?;
        response.insert_header("Content-Length", &json.len().to_string())
            .map_err(|e| Error::new_str("Header insertion error"))?;
            
        Ok(response)
    }
    
    /// Helper method to write JSON response body
    async fn write_json_response<T: Serialize>(&self, session: &mut Session, status: StatusCode, data: &T) -> Result<()> {
        let json = serde_json::to_string(data)
            .map_err(|e| Error::new_str("JSON serialization error"))?;
            
        let response_header = self.json_response(status, data)?;
        
        session.write_response_header(Box::new(response_header), false).await?;
        session.write_response_body(Some(Bytes::from(json)), true).await?;
        
        Ok(())
    }
    
    /// Extract service ID from path like "/openapi/reload/service123"
    pub fn extract_service_id(path: &str) -> Option<&str> {
        path.strip_prefix("/openapi/reload/")
    }
}

/// Main entry point for handling OpenAPI admin requests
/// Called from the MCP service when a request starts with "/openapi/"
pub async fn handle_openapi_request(path: &str, session: &mut Session) -> Result<bool> {
    log::debug!("OpenAPI Reload API Changes: Handling OpenAPI request: {}", path);
    
    let handler = OpenAPIAdminHandler::new();
    let method = &session.req_header().method;
    
    match (method, path) {
        // POST /openapi/reload - reload all services
        (&Method::POST, "/openapi/reload") => {
            let response = handler.handle_reload_all().await?;
            handler.write_json_response(session, StatusCode::OK, &response).await?;
        }
        
        // POST /openapi/reload/{service_id} - reload specific service
        (&Method::POST, path) if path.starts_with("/openapi/reload/") => {
            if let Some(service_id) = OpenAPIAdminHandler::extract_service_id(path) {
                let response = handler.handle_reload_service(service_id).await?;
                handler.write_json_response(session, StatusCode::OK, &response).await?;
            } else {
                return Ok(false); // Invalid path format
            }
        }
        
        // GET /openapi/status - get status
        (&Method::GET, "/openapi/status") => {
            let response = handler.handle_status().await?;
            handler.write_json_response(session, StatusCode::OK, &response).await?;
        }
        
        // GET /openapi/health - health check
        (&Method::GET, "/openapi/health") => {
            let response = handler.handle_health().await?;
            handler.write_json_response(session, StatusCode::OK, &response).await?;
        }
        
        // Unknown endpoint
        _ => {
            log::warn!("OpenAPI Reload API Changes: Unknown endpoint: {} {}", method, path);
            let error = serde_json::json!({
                "error": "Not Found",
                "message": format!("Endpoint {} {} not found", method, path),
                "available_endpoints": [
                    "POST /openapi/reload",
                    "POST /openapi/reload/{service_id}",
                    "GET /openapi/status",
                    "GET /openapi/health"
                ]
            });
            handler.write_json_response(session, StatusCode::NOT_FOUND, &error).await?;
        }
    }
    
    Ok(true) // Request handled
}


// OpenAPI Reload API Changes: Inline test module
#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::mcp::{MCP_SERVICE_MAP, ProxyMCPService};
    use crate::config::MCPService;
    use std::sync::Arc;
    use tokio;

    /// Helper function to setup test MCP services
    fn setup_test_services() {
        // Clear existing services
        MCP_SERVICE_MAP.clear();
        
        // Create mock service configurations
        let weather_config = MCPService {
            id: "weather-service".to_string(),
            ..Default::default()
        };
        
        let bookings_config = MCPService {
            id: "bookings-service".to_string(),
            ..Default::default()
        };
        
        // Insert test services into the global map
        let weather_service = Arc::new(ProxyMCPService {
            inner: weather_config,
            routes: None,
            upstream: None,
            plugins: Vec::new(),
        });
        let bookings_service = Arc::new(ProxyMCPService {
            inner: bookings_config,
            routes: None,
            upstream: None,
            plugins: Vec::new(),
        });
        
        MCP_SERVICE_MAP.insert("weather-service".to_string(), weather_service);
        MCP_SERVICE_MAP.insert("bookings-service".to_string(), bookings_service);
    }

    /// Test 1: Complete reload all services flow
    #[tokio::test]
    async fn test_complete_reload_all_services_flow() {
        setup_test_services();
        
        let handler = OpenAPIAdminHandler::new();
        
        // Test the complete reload all flow
        let result = handler.handle_reload_all().await;
        
        assert!(result.is_ok(), "Reload all should succeed");
        
        let response = result.unwrap();
        
        // Verify response structure
        assert_eq!(response.services_reloaded.len(), 2, "Should reload 2 services");
        assert!(response.services_reloaded.contains(&"weather-service".to_string()));
        assert!(response.services_reloaded.contains(&"bookings-service".to_string()));
        
        // Since we can't actually reload without real OpenAPI specs, 
        // we expect some errors but the structure should be correct
        assert!(response.message.contains("services"), "Message should mention services");
        
        println!("Reload all services test passed: {:?}", response);
    }

    /// Test 2: Complete reload single service flow
    #[tokio::test]
    async fn test_complete_reload_single_service_flow() {
        setup_test_services();
        
        let handler = OpenAPIAdminHandler::new();
        
        // Test reloading a specific service
        let result = handler.handle_reload_service("weather-service").await;
        
        assert!(result.is_ok(), "Reload single service should return a result");
        
        let response = result.unwrap();
        
        // Verify response structure for single service
        if response.success {
            assert_eq!(response.services_reloaded.len(), 1);
            assert_eq!(response.services_reloaded[0], "weather-service");
            assert!(response.errors.is_empty());
        } else {
            // If it fails (expected without real OpenAPI), verify error structure
            assert!(response.services_reloaded.is_empty());
            assert!(!response.errors.is_empty());
            assert!(response.message.contains("weather-service"));
        }
        
        println!("Reload single service test passed: {:?}", response);
    }

    /// Test 3: Error handling for non-existent service
    #[tokio::test]
    async fn test_reload_nonexistent_service_error_handling() {
        setup_test_services();
        
        let handler = OpenAPIAdminHandler::new();
        
        // Test reloading a non-existent service
        let result = handler.handle_reload_service("nonexistent-service").await;
        
        assert!(result.is_ok(), "Should return error response, not panic");
        
        let response = result.unwrap();
        
        // Verify proper error handling
        assert!(!response.success, "Should indicate failure");
        assert!(response.services_reloaded.is_empty(), "No services should be reloaded");
        assert!(!response.errors.is_empty(), "Should contain error messages");
        assert!(response.message.contains("nonexistent-service"), "Error should mention the service");
        
        println!("Error handling test passed: {:?}", response);
    }

    /// Test 4: Status endpoint with real service data
    #[tokio::test]
    async fn test_status_endpoint_with_real_service_data() {
        setup_test_services();
        
        let handler = OpenAPIAdminHandler::new();
        
        // Test the status endpoint
        let result = handler.handle_status().await;
        
        assert!(result.is_ok(), "Status endpoint should succeed");
        
        let response = result.unwrap();
        
        // Verify status response structure with real data
        assert_eq!(response.total_services, 2, "Should show 2 services");
        assert_eq!(response.services.len(), 2, "Should list 2 services");
        
        // Verify service details
        let service_ids: Vec<&str> = response.services.iter()
            .map(|s| s.service_id.as_str())
            .collect();
        
        assert!(service_ids.contains(&"weather-service"), "Should include weather-service service");
        assert!(service_ids.contains(&"bookings-service"), "Should include bookings-service service");
        
        // Verify each service has proper status structure
        for service in &response.services {
            assert!(!service.service_id.is_empty(), "Service ID should not be empty");
            assert_eq!(service.status, "active", "Service should be active");
            assert!(!service.last_updated.is_empty(), "Should have timestamp");
            // tools_count might be 0 since we're not loading real OpenAPI specs
        }
        
        println!("Status endpoint test passed: {} services, {} total tools", 
                response.total_services, response.total_tools);
    }

    /// Test 5: Health endpoint basic functionality
    #[tokio::test]
    async fn test_health_endpoint_functionality() {
        let handler = OpenAPIAdminHandler::new();
        
        let result = handler.handle_health().await;
        
        assert!(result.is_ok(), "Health endpoint should succeed");
        
        let health = result.unwrap();
        
        // Verify health response structure
        assert_eq!(health["status"], "healthy");
        assert_eq!(health["service"], "openapi-admin");
        assert!(health["timestamp"].is_string(), "Should have timestamp");
        
        println!("Health endpoint test passed: {:?}", health);
    }
}