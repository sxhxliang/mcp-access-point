use async_trait::async_trait;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc, time::SystemTime};
use tokio::sync::RwLock;

use crate::{
    config::{self, json_to_resource, Config},
    proxy::{
        global_rule::{reload_global_plugin, ProxyGlobalRule, GLOBAL_RULE_MAP, load_static_global_rules},
        mcp::{ProxyMCPService, MCP_SERVICE_MAP, load_static_mcp_services},
        route::{reload_global_route_match, ProxyRoute, ROUTE_MAP, load_static_routes},
        service::{ProxyService, SERVICE_MAP, load_static_services},
        ssl::{ProxySSL, SSL_MAP, load_static_ssls},
        upstream::{ProxyUpstream, UPSTREAM_MAP, load_static_upstreams},
        MapOperations,
    },
};

use super::{
    resource_types::{
        BatchOperationRequest, BatchOperationResponse, ConfigChangeEvent, OperationType,
        ResourceOperationResult, ResourceStats, ResourceSummary, ResourceType, ValidationResult,
    },
    resource_validator::ResourceValidator,
};

/// Trait for managing resources with CRUD operations
#[async_trait]
pub trait ResourceCRUD<T> {
    async fn create(&self, id: String, data: &[u8]) -> Result<Arc<T>, String>;
    async fn get(&self, id: &str) -> Option<Arc<T>>;
    async fn update(&self, id: String, data: &[u8]) -> Result<Arc<T>, String>;
    async fn delete(&self, id: &str) -> Result<(), String>;
    async fn list(&self) -> Vec<Arc<T>>;
    async fn reload(&self, resources: Vec<Arc<T>>) -> Result<(), String>;
    fn get_count(&self) -> usize;
}

/// Event listener trait for configuration changes
#[async_trait]
pub trait ConfigChangeListener: Send + Sync {
    async fn on_config_change(&self, event: ConfigChangeEvent);
}

/// Main resource manager that coordinates all resource operations
pub struct ResourceManager {
    listeners: RwLock<Vec<Arc<dyn ConfigChangeListener>>>,
    work_stealing: bool,
    /// Optional config for reloading from source
    /// If None, reloading will only trigger reload hooks without loading from config
    config: Option<Arc<RwLock<Config>>>,
}

impl ResourceManager {
    pub fn new(work_stealing: bool) -> Self {
        Self {
            listeners: RwLock::new(Vec::new()),
            work_stealing,
            config: None,
        }
    }

    /// Create a new ResourceManager with config access for reloading
    pub fn new_with_config(work_stealing: bool, config: Arc<RwLock<Config>>) -> Self {
        Self {
            listeners: RwLock::new(Vec::new()),
            work_stealing,
            config: Some(config),
        }
    }

    /// Register a configuration change listener
    pub async fn register_listener(&self, listener: Arc<dyn ConfigChangeListener>) {
        self.listeners.write().await.push(listener);
    }

    /// Notify all listeners of a configuration change
    async fn notify_listeners(&self, event: ConfigChangeEvent) {
        let listeners = self.listeners.read().await;
        for listener in listeners.iter() {
            listener.on_config_change(event.clone()).await;
        }
    }

    /// Validate resource configuration
    pub fn validate_resource(
        &self,
        resource_type: ResourceType,
        resource_id: &str,
        data: &[u8],
    ) -> ValidationResult {
        ResourceValidator::validate_resource(resource_type, resource_id, data)
    }

    /// Validate resource deletion
    pub fn validate_deletion(
        &self,
        resource_type: ResourceType,
        resource_id: &str,
    ) -> ValidationResult {
        ResourceValidator::validate_deletion(resource_type, resource_id)
    }

    /// Get resource summary for all types
    pub fn get_resource_summary(&self) -> ResourceSummary {
        let mut stats = HashMap::new();
        let mut total_resources = 0;

        for &resource_type in ResourceType::all() {
            let count = match resource_type {
                ResourceType::Upstreams => UPSTREAM_MAP.len(),
                ResourceType::Services => SERVICE_MAP.len(),
                ResourceType::GlobalRules => GLOBAL_RULE_MAP.len(),
                ResourceType::Routes => ROUTE_MAP.len(),
                ResourceType::McpServices => MCP_SERVICE_MAP.len(),
                ResourceType::Ssls => SSL_MAP.len(),
            };

            total_resources += count;
            stats.insert(
                resource_type,
                ResourceStats {
                    resource_type,
                    count,
                    last_updated: Some(SystemTime::now()),
                },
            );
        }

        ResourceSummary {
            stats,
            total_resources,
        }
    }

    /// Create a resource
    pub async fn create_resource(
        &self,
        resource_type: ResourceType,
        resource_id: String,
        data: &[u8],
    ) -> Result<ResourceOperationResult, String> {
        // Validate the resource first
        let validation = self.validate_resource(resource_type, &resource_id, data);
        if !validation.valid {
            let error_messages: Vec<String> = validation.errors.iter().map(|e| e.message.clone()).collect();
            return Ok(ResourceOperationResult {
                success: false,
                message: format!("Validation failed: {}", error_messages.join(", ")),
                resource_type,
                resource_id: Some(resource_id),
                timestamp: SystemTime::now(),
            });
        }

        let result = match resource_type {
            ResourceType::Upstreams => {
                self.create_upstream(resource_id.clone(), data).await
            }
            ResourceType::Services => {
                self.create_service(resource_id.clone(), data).await
            }
            ResourceType::GlobalRules => {
                self.create_global_rule(resource_id.clone(), data).await
            }
            ResourceType::Routes => {
                self.create_route(resource_id.clone(), data).await
            }
            ResourceType::McpServices => {
                self.create_mcp_service(resource_id.clone(), data).await
            }
            ResourceType::Ssls => {
                self.create_ssl(resource_id.clone(), data).await
            }
        };

        let operation_result = match result {
            Ok(_) => {
                self.notify_listeners(ConfigChangeEvent {
                    resource_type,
                    resource_id: resource_id.clone(),
                    operation: OperationType::Create,
                    timestamp: SystemTime::now(),
                    user: None,
                }).await;

                ResourceOperationResult {
                    success: true,
                    message: format!("Resource '{}' created successfully", resource_id),
                    resource_type,
                    resource_id: Some(resource_id),
                    timestamp: SystemTime::now(),
                }
            }
            Err(e) => ResourceOperationResult {
                success: false,
                message: e,
                resource_type,
                resource_id: Some(resource_id),
                timestamp: SystemTime::now(),
            },
        };

        Ok(operation_result)
    }

    /// Update a resource
    pub async fn update_resource(
        &self,
        resource_type: ResourceType,
        resource_id: String,
        data: &[u8],
    ) -> Result<ResourceOperationResult, String> {
        // Validate the resource first
        let validation = self.validate_resource(resource_type, &resource_id, data);
        if !validation.valid {
            let error_messages: Vec<String> = validation.errors.iter().map(|e| e.message.clone()).collect();
            return Ok(ResourceOperationResult {
                success: false,
                message: format!("Validation failed: {}", error_messages.join(", ")),
                resource_type,
                resource_id: Some(resource_id),
                timestamp: SystemTime::now(),
            });
        }

        let result = match resource_type {
            ResourceType::Upstreams => {
                self.update_upstream(resource_id.clone(), data).await
            }
            ResourceType::Services => {
                self.update_service(resource_id.clone(), data).await
            }
            ResourceType::GlobalRules => {
                self.update_global_rule(resource_id.clone(), data).await
            }
            ResourceType::Routes => {
                self.update_route(resource_id.clone(), data).await
            }
            ResourceType::McpServices => {
                self.update_mcp_service(resource_id.clone(), data).await
            }
            ResourceType::Ssls => {
                self.update_ssl(resource_id.clone(), data).await
            }
        };

        let operation_result = match result {
            Ok(_) => {
                self.notify_listeners(ConfigChangeEvent {
                    resource_type,
                    resource_id: resource_id.clone(),
                    operation: OperationType::Update,
                    timestamp: SystemTime::now(),
                    user: None,
                }).await;

                ResourceOperationResult {
                    success: true,
                    message: format!("Resource '{}' updated successfully", resource_id),
                    resource_type,
                    resource_id: Some(resource_id),
                    timestamp: SystemTime::now(),
                }
            }
            Err(e) => ResourceOperationResult {
                success: false,
                message: e,
                resource_type,
                resource_id: Some(resource_id),
                timestamp: SystemTime::now(),
            },
        };

        Ok(operation_result)
    }

    /// Delete a resource
    pub async fn delete_resource(
        &self,
        resource_type: ResourceType,
        resource_id: String,
    ) -> Result<ResourceOperationResult, String> {
        // Validate deletion
        let validation = self.validate_deletion(resource_type, &resource_id);
        if !validation.valid {
            let error_messages: Vec<String> = validation.errors.iter().map(|e| e.message.clone()).collect();
            return Ok(ResourceOperationResult {
                success: false,
                message: format!("Cannot delete resource: {}", error_messages.join(", ")),
                resource_type,
                resource_id: Some(resource_id),
                timestamp: SystemTime::now(),
            });
        }

        let result = match resource_type {
            ResourceType::Upstreams => {
                UPSTREAM_MAP.remove(&resource_id);
                Ok(())
            }
            ResourceType::Services => {
                SERVICE_MAP.remove(&resource_id);
                Ok(())
            }
            ResourceType::GlobalRules => {
                GLOBAL_RULE_MAP.remove(&resource_id);
                reload_global_plugin();
                Ok(())
            }
            ResourceType::Routes => {
                ROUTE_MAP.remove(&resource_id);
                reload_global_route_match();
                Ok(())
            }
            ResourceType::McpServices => {
                MCP_SERVICE_MAP.remove(&resource_id);
                Ok(())
            }
            ResourceType::Ssls => {
                SSL_MAP.remove(&resource_id);
                crate::proxy::ssl::reload_global_ssl_match();
                Ok::<(), String>(())
            }
        };

        let operation_result = match result {
            Ok(_) => {
                self.notify_listeners(ConfigChangeEvent {
                    resource_type,
                    resource_id: resource_id.clone(),
                    operation: OperationType::Delete,
                    timestamp: SystemTime::now(),
                    user: None,
                }).await;

                ResourceOperationResult {
                    success: true,
                    message: format!("Resource '{}' deleted successfully", resource_id),
                    resource_type,
                    resource_id: Some(resource_id),
                    timestamp: SystemTime::now(),
                }
            }
            Err(e) => ResourceOperationResult {
                success: false,
                message: format!("Failed to delete resource: {:?}", e),
                resource_type,
                resource_id: Some(resource_id),
                timestamp: SystemTime::now(),
            },
        };

        Ok(operation_result)
    }

    /// Get resource by ID
    pub fn get_resource(&self, resource_type: ResourceType, resource_id: &str) -> Option<Value> {
        match resource_type {
            ResourceType::Upstreams => {
                UPSTREAM_MAP.get(resource_id).map(|r| serde_json::to_value(&r.inner).unwrap())
            }
            ResourceType::Services => {
                SERVICE_MAP.get(resource_id).map(|r| serde_json::to_value(&r.inner).unwrap())
            }
            ResourceType::GlobalRules => {
                GLOBAL_RULE_MAP.get(resource_id).map(|r| serde_json::to_value(&r.inner).unwrap())
            }
            ResourceType::Routes => {
                ROUTE_MAP.get(resource_id).map(|r| serde_json::to_value(&r.inner).unwrap())
            }
            ResourceType::McpServices => {
                MCP_SERVICE_MAP.get(resource_id).map(|r| serde_json::to_value(&r.inner).unwrap())
            }
            ResourceType::Ssls => {
                SSL_MAP.get(resource_id).map(|r| serde_json::to_value(&r.inner).unwrap())
            }
        }
    }

    /// List all resources of a type
    pub fn list_resources(&self, resource_type: ResourceType) -> Vec<Value> {
        match resource_type {
            ResourceType::Upstreams => {
                UPSTREAM_MAP.iter().map(|r| serde_json::to_value(&r.inner).unwrap()).collect()
            }
            ResourceType::Services => {
                SERVICE_MAP.iter().map(|r| serde_json::to_value(&r.inner).unwrap()).collect()
            }
            ResourceType::GlobalRules => {
                GLOBAL_RULE_MAP.iter().map(|r| serde_json::to_value(&r.inner).unwrap()).collect()
            }
            ResourceType::Routes => {
                ROUTE_MAP.iter().map(|r| serde_json::to_value(&r.inner).unwrap()).collect()
            }
            ResourceType::McpServices => {
                MCP_SERVICE_MAP.iter().map(|r| serde_json::to_value(&r.inner).unwrap()).collect()
            }
            ResourceType::Ssls => {
                SSL_MAP.iter().map(|r| serde_json::to_value(&r.inner).unwrap()).collect()
            }
        }
    }

    /// Execute batch operations
    pub async fn execute_batch_operations(
        &self,
        request: BatchOperationRequest,
    ) -> Result<BatchOperationResponse, String> {
        let dry_run = request.dry_run.unwrap_or(false);

        // Validate batch operations
        let validation = ResourceValidator::validate_batch_operations(&request.operations);
        if !validation.valid {
            let error_messages: Vec<String> = validation.errors.iter().map(|e| e.message.clone()).collect();
            return Ok(BatchOperationResponse {
                success: false,
                results: vec![],
                summary: format!("Batch validation failed: {}", error_messages.join(", ")),
                dry_run,
            });
        }

        let mut results = Vec::new();
        let mut success_count = 0;

        for operation in request.operations {
            let result = if dry_run {
                // In dry run mode, just validate without executing
                let validation = match operation.operation_type {
                    OperationType::Create | OperationType::Update => {
                        if let Some(ref data) = operation.data {
                            let data_bytes = serde_json::to_vec(data).map_err(|e| e.to_string())?;
                            self.validate_resource(operation.resource_type, &operation.resource_id, &data_bytes)
                        } else {
                            ValidationResult {
                                valid: false,
                                errors: vec![super::resource_types::ValidationError {
                                    field: "data".to_string(),
                                    message: "Data is required for create/update operations".to_string(),
                                    error_type: super::resource_types::ValidationErrorType::InvalidFormat,
                                }],
                                warnings: vec![],
                            }
                        }
                    },
                    OperationType::Delete => {
                        self.validate_deletion(operation.resource_type, &operation.resource_id)
                    },
                    OperationType::Reload => {
                        ValidationResult {
                            valid: true,
                            errors: vec![],
                            warnings: vec![],
                        }
                    },
                };

                ResourceOperationResult {
                    success: validation.valid,
                    message: if validation.valid {
                        format!("Dry run validation passed for {} operation on {}:{}",
                               operation.operation_type, operation.resource_type, operation.resource_id)
                    } else {
                        let error_messages: Vec<String> = validation.errors.iter().map(|e| e.message.clone()).collect();
                        format!("Dry run validation failed: {}", error_messages.join(", "))
                    },
                    resource_type: operation.resource_type,
                    resource_id: Some(operation.resource_id),
                    timestamp: SystemTime::now(),
                }
            } else {
                // Execute the actual operation
                match operation.operation_type {
                    OperationType::Create => {
                        if let Some(data) = operation.data {
                            let data_bytes = serde_json::to_vec(&data).map_err(|e| e.to_string())?;
                            self.create_resource(operation.resource_type, operation.resource_id, &data_bytes).await?
                        } else {
                            ResourceOperationResult {
                                success: false,
                                message: "Data is required for create operation".to_string(),
                                resource_type: operation.resource_type,
                                resource_id: Some(operation.resource_id),
                                timestamp: SystemTime::now(),
                            }
                        }
                    },
                    OperationType::Update => {
                        if let Some(data) = operation.data {
                            let data_bytes = serde_json::to_vec(&data).map_err(|e| e.to_string())?;
                            self.update_resource(operation.resource_type, operation.resource_id, &data_bytes).await?
                        } else {
                            ResourceOperationResult {
                                success: false,
                                message: "Data is required for update operation".to_string(),
                                resource_type: operation.resource_type,
                                resource_id: Some(operation.resource_id),
                                timestamp: SystemTime::now(),
                            }
                        }
                    },
                    OperationType::Delete => {
                        self.delete_resource(operation.resource_type, operation.resource_id).await?
                    },
                    OperationType::Reload => {
                        self.reload_resource_type(operation.resource_type).await?
                    },
                }
            };

            if result.success {
                success_count += 1;
            }
            results.push(result);
        }

        Ok(BatchOperationResponse {
            success: success_count == results.len(),
            summary: format!("{}/{} operations completed successfully", success_count, results.len()),
            results,
            dry_run,
        })
    }

    /// Reload all resources of a specific type
    /// If config is available, reloads from the configuration source
    /// Otherwise, only triggers reload hooks for the resource type
    pub async fn reload_resource_type(&self, resource_type: ResourceType) -> Result<ResourceOperationResult, String> {
        let mut reloaded_from_config = false;
        let mut reload_count = 0;

        // If we have config access, reload from configuration source
        if let Some(config_ref) = &self.config {
            let config = config_ref.read().await;

            match resource_type {
                ResourceType::Upstreams => {
                    // Clear existing upstreams first
                    let old_count = UPSTREAM_MAP.len();
                    UPSTREAM_MAP.clear();

                    // Reload from config
                    load_static_upstreams(&config)
                        .map_err(|e| format!("Failed to reload upstreams from config: {e}"))?;

                    reload_count = UPSTREAM_MAP.len();
                    reloaded_from_config = true;
                    log::info!("Reloaded {} upstreams from config (was {})", reload_count, old_count);
                }
                ResourceType::Services => {
                    // Clear existing services first
                    let old_count = SERVICE_MAP.len();
                    SERVICE_MAP.clear();

                    // Reload from config
                    load_static_services(&config)
                        .map_err(|e| format!("Failed to reload services from config: {e}"))?;

                    reload_count = SERVICE_MAP.len();
                    reloaded_from_config = true;
                    log::info!("Reloaded {} services from config (was {})", reload_count, old_count);
                }
                ResourceType::GlobalRules => {
                    // Clear existing global rules first
                    let old_count = GLOBAL_RULE_MAP.len();
                    GLOBAL_RULE_MAP.clear();

                    // Reload from config
                    load_static_global_rules(&config)
                        .map_err(|e| format!("Failed to reload global rules from config: {e}"))?;

                    reload_count = GLOBAL_RULE_MAP.len();
                    reloaded_from_config = true;

                    // Trigger reload hook
                    reload_global_plugin();
                    log::info!("Reloaded {} global rules from config (was {}) and triggered plugin reload", reload_count, old_count);
                }
                ResourceType::Routes => {
                    // Clear existing routes first
                    let old_count = ROUTE_MAP.len();
                    ROUTE_MAP.clear();

                    // Reload from config
                    load_static_routes(&config)
                        .map_err(|e| format!("Failed to reload routes from config: {e}"))?;

                    reload_count = ROUTE_MAP.len();
                    reloaded_from_config = true;

                    // Trigger reload hook
                    reload_global_route_match();
                    log::info!("Reloaded {} routes from config (was {}) and triggered route match reload", reload_count, old_count);
                }
                ResourceType::McpServices => {
                    // Clear existing MCP services first
                    let old_count = MCP_SERVICE_MAP.len();
                    MCP_SERVICE_MAP.clear();

                    // Reload from config
                    load_static_mcp_services(&config)
                        .map_err(|e| format!("Failed to reload MCP services from config: {e}"))?;

                    reload_count = MCP_SERVICE_MAP.len();
                    reloaded_from_config = true;
                    log::info!("Reloaded {} MCP services from config (was {})", reload_count, old_count);
                }
                ResourceType::Ssls => {
                    // Clear existing SSLs first
                    let old_count = SSL_MAP.len();
                    SSL_MAP.clear();

                    // Reload from config
                    load_static_ssls(&config)
                        .map_err(|e| format!("Failed to reload SSLs from config: {e}"))?;

                    reload_count = SSL_MAP.len();
                    reloaded_from_config = true;

                    // Trigger reload hook
                    crate::proxy::ssl::reload_global_ssl_match();
                    log::info!("Reloaded {} SSLs from config (was {}) and triggered SSL match reload", reload_count, old_count);
                }
            }
        } else {
            // No config available, only trigger reload hooks
            match resource_type {
                ResourceType::GlobalRules => {
                    reload_global_plugin();
                    log::info!("Triggered global plugin reload (no config reload)");
                }
                ResourceType::Routes => {
                    reload_global_route_match();
                    log::info!("Triggered route match reload (no config reload)");
                }
                ResourceType::Ssls => {
                    crate::proxy::ssl::reload_global_ssl_match();
                    log::info!("Triggered SSL match reload (no config reload)");
                }
                _ => {
                    log::warn!("No reload hooks available for resource type '{}' and no config access", resource_type);
                }
            }
        }

        // Notify listeners of the reload event
        self.notify_listeners(ConfigChangeEvent {
            resource_type,
            resource_id: "*".to_string(), // Indicates bulk reload
            operation: OperationType::Reload,
            timestamp: SystemTime::now(),
            user: None,
        }).await;

        let message = if reloaded_from_config {
            format!("Resource type '{}' reloaded successfully from config ({} resources)", resource_type, reload_count)
        } else {
            format!("Resource type '{}' reload hooks triggered successfully (no config reload)", resource_type)
        };

        Ok(ResourceOperationResult {
            success: true,
            message,
            resource_type,
            resource_id: None,
            timestamp: SystemTime::now(),
        })
    }

    /// Reload configuration from file
    /// This method loads a new config from file and reloads all resources
    /// Note: This version works independently of whether ResourceManager has config access
    pub async fn reload_config_from_file(&self, config_path: &str) -> Result<ResourceOperationResult, String> {
        // Load the new config from file
        let new_config = crate::config::Config::load_from_yaml(config_path)
            .map_err(|e| format!("Failed to load config from file '{}': {}", config_path, e))?;

        // Update the internal config if we have access
        if let Some(config_ref) = &self.config {
            let config = config_ref.write().await;
            // We can't assign a new config directly due to clone limitations
            // Instead, we'll update individual fields that can be updated
            log::warn!("Internal config update skipped due to Config clone limitations");
        }

        // Clear all existing resources and reload from the new config
        let mut reload_results = Vec::new();
        let mut success_count = 0;

        for &resource_type in ResourceType::all() {
            let result = match resource_type {
                ResourceType::Upstreams => {
                    let old_count = UPSTREAM_MAP.len();
                    UPSTREAM_MAP.clear();
                    match load_static_upstreams(&new_config) {
                        Ok(_) => {
                            let new_count = UPSTREAM_MAP.len();
                            log::info!("Reloaded {} upstreams from config file (was {})", new_count, old_count);
                            ResourceOperationResult {
                                success: true,
                                message: format!("Reloaded {} upstreams from config file", new_count),
                                resource_type,
                                resource_id: None,
                                timestamp: SystemTime::now(),
                            }
                        }
                        Err(e) => ResourceOperationResult {
                            success: false,
                            message: format!("Failed to reload upstreams: {}", e),
                            resource_type,
                            resource_id: None,
                            timestamp: SystemTime::now(),
                        },
                    }
                }
                ResourceType::Services => {
                    let old_count = SERVICE_MAP.len();
                    SERVICE_MAP.clear();
                    match load_static_services(&new_config) {
                        Ok(_) => {
                            let new_count = SERVICE_MAP.len();
                            log::info!("Reloaded {} services from config file (was {})", new_count, old_count);
                            ResourceOperationResult {
                                success: true,
                                message: format!("Reloaded {} services from config file", new_count),
                                resource_type,
                                resource_id: None,
                                timestamp: SystemTime::now(),
                            }
                        }
                        Err(e) => ResourceOperationResult {
                            success: false,
                            message: format!("Failed to reload services: {}", e),
                            resource_type,
                            resource_id: None,
                            timestamp: SystemTime::now(),
                        },
                    }
                }
                ResourceType::GlobalRules => {
                    let old_count = GLOBAL_RULE_MAP.len();
                    GLOBAL_RULE_MAP.clear();
                    match load_static_global_rules(&new_config) {
                        Ok(_) => {
                            let new_count = GLOBAL_RULE_MAP.len();
                            reload_global_plugin();
                            log::info!("Reloaded {} global rules from config file (was {}) and triggered plugin reload", new_count, old_count);
                            ResourceOperationResult {
                                success: true,
                                message: format!("Reloaded {} global rules from config file", new_count),
                                resource_type,
                                resource_id: None,
                                timestamp: SystemTime::now(),
                            }
                        }
                        Err(e) => ResourceOperationResult {
                            success: false,
                            message: format!("Failed to reload global rules: {}", e),
                            resource_type,
                            resource_id: None,
                            timestamp: SystemTime::now(),
                        },
                    }
                }
                ResourceType::Routes => {
                    let old_count = ROUTE_MAP.len();
                    ROUTE_MAP.clear();
                    match load_static_routes(&new_config) {
                        Ok(_) => {
                            let new_count = ROUTE_MAP.len();
                            reload_global_route_match();
                            log::info!("Reloaded {} routes from config file (was {}) and triggered route match reload", new_count, old_count);
                            ResourceOperationResult {
                                success: true,
                                message: format!("Reloaded {} routes from config file", new_count),
                                resource_type,
                                resource_id: None,
                                timestamp: SystemTime::now(),
                            }
                        }
                        Err(e) => ResourceOperationResult {
                            success: false,
                            message: format!("Failed to reload routes: {}", e),
                            resource_type,
                            resource_id: None,
                            timestamp: SystemTime::now(),
                        },
                    }
                }
                ResourceType::McpServices => {
                    let old_count = MCP_SERVICE_MAP.len();
                    MCP_SERVICE_MAP.clear();
                    match load_static_mcp_services(&new_config) {
                        Ok(_) => {
                            let new_count = MCP_SERVICE_MAP.len();
                            log::info!("Reloaded {} MCP services from config file (was {})", new_count, old_count);
                            ResourceOperationResult {
                                success: true,
                                message: format!("Reloaded {} MCP services from config file", new_count),
                                resource_type,
                                resource_id: None,
                                timestamp: SystemTime::now(),
                            }
                        }
                        Err(e) => ResourceOperationResult {
                            success: false,
                            message: format!("Failed to reload MCP services: {}", e),
                            resource_type,
                            resource_id: None,
                            timestamp: SystemTime::now(),
                        },
                    }
                }
                ResourceType::Ssls => {
                    let old_count = SSL_MAP.len();
                    SSL_MAP.clear();
                    match load_static_ssls(&new_config) {
                        Ok(_) => {
                            let new_count = SSL_MAP.len();
                            crate::proxy::ssl::reload_global_ssl_match();
                            log::info!("Reloaded {} SSLs from config file (was {}) and triggered SSL match reload", new_count, old_count);
                            ResourceOperationResult {
                                success: true,
                                message: format!("Reloaded {} SSLs from config file", new_count),
                                resource_type,
                                resource_id: None,
                                timestamp: SystemTime::now(),
                            }
                        }
                        Err(e) => ResourceOperationResult {
                            success: false,
                            message: format!("Failed to reload SSLs: {}", e),
                            resource_type,
                            resource_id: None,
                            timestamp: SystemTime::now(),
                        },
                    }
                }
            };

            if result.success {
                success_count += 1;
            }
            reload_results.push(result);
        }

        // Notify listeners of the config reload event
        self.notify_listeners(ConfigChangeEvent {
            resource_type: ResourceType::Upstreams, // Placeholder for config reload
            resource_id: "*".to_string(), // Indicates full config reload
            operation: OperationType::Reload,
            timestamp: SystemTime::now(),
            user: None,
        }).await;

        let all_success = success_count == ResourceType::all().len();
        let message = if all_success {
            format!("Configuration reloaded successfully from '{}' (all {} resource types)", config_path, ResourceType::all().len())
        } else {
            format!("Configuration reload from '{}' completed with errors ({}/{} resource types successful)", config_path, success_count, ResourceType::all().len())
        };

        log::info!("{}", message);

        Ok(ResourceOperationResult {
            success: all_success,
            message,
            resource_type: ResourceType::Upstreams, // Placeholder, represents all types
            resource_id: Some("*".to_string()),
            timestamp: SystemTime::now(),
        })
    }

    // Individual resource CRUD implementations
    async fn create_upstream(&self, id: String, data: &[u8]) -> Result<(), String> {
        let mut upstream = json_to_resource::<config::Upstream>(data)
            .map_err(|e| format!("Invalid upstream data: {e}"))?;
        upstream.id = id.clone();

        let proxy_upstream = ProxyUpstream::new_with_health_check(upstream, self.work_stealing)
            .map_err(|e| format!("Failed to create proxy upstream: {e}"))?;

        UPSTREAM_MAP.insert_resource(Arc::new(proxy_upstream));
        Ok(())
    }

    async fn update_upstream(&self, id: String, data: &[u8]) -> Result<(), String> {
        let mut upstream = json_to_resource::<config::Upstream>(data)
            .map_err(|e| format!("Invalid upstream data: {e}"))?;
        upstream.id = id.clone();

        let proxy_upstream = ProxyUpstream::new_with_health_check(upstream, self.work_stealing)
            .map_err(|e| format!("Failed to create proxy upstream: {e}"))?;

        UPSTREAM_MAP.insert_resource(Arc::new(proxy_upstream));
        Ok(())
    }

    async fn create_service(&self, id: String, data: &[u8]) -> Result<(), String> {
        let mut service = json_to_resource::<config::Service>(data)
            .map_err(|e| format!("Invalid service data: {e}"))?;
        service.id = id.clone();

        let proxy_service = ProxyService::new_with_upstream_and_plugins(service, self.work_stealing)
            .map_err(|e| format!("Failed to create proxy service: {e}"))?;

        SERVICE_MAP.insert_resource(Arc::new(proxy_service));
        Ok(())
    }

    async fn update_service(&self, id: String, data: &[u8]) -> Result<(), String> {
        let mut service = json_to_resource::<config::Service>(data)
            .map_err(|e| format!("Invalid service data: {e}"))?;
        service.id = id.clone();

        let proxy_service = ProxyService::new_with_upstream_and_plugins(service, self.work_stealing)
            .map_err(|e| format!("Failed to create proxy service: {e}"))?;

        SERVICE_MAP.insert_resource(Arc::new(proxy_service));
        Ok(())
    }

    async fn create_global_rule(&self, id: String, data: &[u8]) -> Result<(), String> {
        let mut rule = json_to_resource::<config::GlobalRule>(data)
            .map_err(|e| format!("Invalid global rule data: {e}"))?;
        rule.id = id.clone();

        let proxy_rule = ProxyGlobalRule::new_with_plugins(rule)
            .map_err(|e| format!("Failed to create proxy global rule: {e}"))?;

        GLOBAL_RULE_MAP.insert_resource(Arc::new(proxy_rule));
        reload_global_plugin();
        Ok(())
    }

    async fn update_global_rule(&self, id: String, data: &[u8]) -> Result<(), String> {
        let mut rule = json_to_resource::<config::GlobalRule>(data)
            .map_err(|e| format!("Invalid global rule data: {e}"))?;
        rule.id = id.clone();

        let proxy_rule = ProxyGlobalRule::new_with_plugins(rule)
            .map_err(|e| format!("Failed to create proxy global rule: {e}"))?;

        GLOBAL_RULE_MAP.insert_resource(Arc::new(proxy_rule));
        reload_global_plugin();
        Ok(())
    }

    async fn create_route(&self, id: String, data: &[u8]) -> Result<(), String> {
        let mut route = json_to_resource::<config::Route>(data)
            .map_err(|e| format!("Invalid route data: {e}"))?;
        route.id = id.clone();

        let proxy_route = ProxyRoute::new_with_upstream_and_plugins(route, self.work_stealing)
            .map_err(|e| format!("Failed to create proxy route: {e}"))?;

        ROUTE_MAP.insert_resource(Arc::new(proxy_route));
        reload_global_route_match();
        Ok(())
    }

    async fn update_route(&self, id: String, data: &[u8]) -> Result<(), String> {
        let mut route = json_to_resource::<config::Route>(data)
            .map_err(|e| format!("Invalid route data: {e}"))?;
        route.id = id.clone();

        let proxy_route = ProxyRoute::new_with_upstream_and_plugins(route, self.work_stealing)
            .map_err(|e| format!("Failed to create proxy route: {e}"))?;

        ROUTE_MAP.insert_resource(Arc::new(proxy_route));
        reload_global_route_match();
        Ok(())
    }

    async fn create_mcp_service(&self, id: String, data: &[u8]) -> Result<(), String> {
        let mut mcp_service = json_to_resource::<config::MCPService>(data)
            .map_err(|e| format!("Invalid MCP service data: {e}"))?;
        mcp_service.id = id.clone();

        let proxy_mcp_service = ProxyMCPService::new_with_routes_upstream_and_plugins(mcp_service, self.work_stealing)
            .map_err(|e| format!("Failed to create proxy MCP service: {e}"))?;

        MCP_SERVICE_MAP.insert_resource(Arc::new(proxy_mcp_service));
        Ok(())
    }

    async fn update_mcp_service(&self, id: String, data: &[u8]) -> Result<(), String> {
        let mut mcp_service = json_to_resource::<config::MCPService>(data)
            .map_err(|e| format!("Invalid MCP service data: {e}"))?;
        mcp_service.id = id.clone();

        let proxy_mcp_service = ProxyMCPService::new_with_routes_upstream_and_plugins(mcp_service, self.work_stealing)
            .map_err(|e| format!("Failed to create proxy MCP service: {e}"))?;

        MCP_SERVICE_MAP.insert_resource(Arc::new(proxy_mcp_service));
        Ok(())
    }

    async fn create_ssl(&self, id: String, data: &[u8]) -> Result<(), String> {
        let mut ssl = json_to_resource::<config::SSL>(data)
            .map_err(|e| format!("Invalid SSL data: {e}"))?;
        ssl.id = id.clone();

        let proxy_ssl = ProxySSL::from(ssl);

        SSL_MAP.insert_resource(Arc::new(proxy_ssl));
        crate::proxy::ssl::reload_global_ssl_match();
        Ok(())
    }

    async fn update_ssl(&self, id: String, data: &[u8]) -> Result<(), String> {
        let mut ssl = json_to_resource::<config::SSL>(data)
            .map_err(|e| format!("Invalid SSL data: {e}"))?;
        ssl.id = id.clone();

        let proxy_ssl = ProxySSL::from(ssl);

        SSL_MAP.insert_resource(Arc::new(proxy_ssl));
        crate::proxy::ssl::reload_global_ssl_match();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Upstream;

    #[tokio::test]
    async fn test_create_upstream() {
        let manager = ResourceManager::new(false);

        let upstream = Upstream {
            id: "test-upstream".to_string(),
            nodes: vec!["127.0.0.1:8080".to_string()],
            ..Default::default()
        };

        let data = serde_json::to_vec(&upstream).unwrap();
        let result = manager.create_resource(
            ResourceType::Upstreams,
            "test-upstream".to_string(),
            &data,
        ).await;

        assert!(result.is_ok());
        let operation_result = result.unwrap();
        assert!(operation_result.success);

        // Verify the upstream was created
        let retrieved = manager.get_resource(ResourceType::Upstreams, "test-upstream");
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_validate_resource() {
        let manager = ResourceManager::new(false);

        let upstream = Upstream {
            id: "test-upstream".to_string(),
            nodes: vec![], // Empty nodes should fail validation
            ..Default::default()
        };

        let data = serde_json::to_vec(&upstream).unwrap();
        let validation = manager.validate_resource(
            ResourceType::Upstreams,
            "test-upstream",
            &data,
        );

        assert!(!validation.valid);
        assert!(!validation.errors.is_empty());
    }
}