use std::collections::{HashMap, HashSet};
use validator::Validate;

use crate::{
    config::{self, json_to_resource, Identifiable},
    proxy::{
        global_rule::GLOBAL_RULE_MAP, mcp::MCP_SERVICE_MAP, route::ROUTE_MAP, service::SERVICE_MAP,
        ssl::SSL_MAP, upstream::UPSTREAM_MAP,
    },
};

use super::{
    resource_types::{
        ResourceType, ValidationError, ValidationErrorType, ValidationResult, ValidationWarning,
    },
    PluginValidatable,
};

/// Resource validator for runtime configuration management
pub struct ResourceValidator;

impl ResourceValidator {
    /// Validate a resource of the given type with the provided JSON data
    pub fn validate_resource(
        resource_type: ResourceType,
        resource_id: &str,
        data: &[u8],
    ) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Basic JSON validation and type-specific validation
        match Self::validate_resource_format(resource_type, data) {
            Ok(_) => {}
            Err(err_msgs) => {
                errors.extend(err_msgs.into_iter().map(|msg| ValidationError {
                    field: "format".to_string(),
                    message: msg,
                    error_type: ValidationErrorType::InvalidFormat,
                }));
            }
        }

        // Dependency validation
        match Self::validate_dependencies(resource_type, resource_id, data) {
            Ok(warns) => warnings.extend(warns),
            Err(err) => errors.push(err),
        }

        // Check for circular dependencies
        if let Err(err) = Self::check_circular_dependencies(resource_type, resource_id, data) {
            errors.push(err);
        }

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    /// Validate resource format and type-specific constraints
    fn validate_resource_format(
        resource_type: ResourceType,
        data: &[u8],
    ) -> Result<(), Vec<String>> {
        match resource_type {
            ResourceType::Upstreams => {
                let upstream = json_to_resource::<config::Upstream>(data)
                    .map_err(|e| vec![format!("Invalid JSON: {e}")])?;
                upstream.validate().map_err(|e| vec![e.to_string()])?;

                // Additional validation for upstreams
                if upstream.nodes.is_empty() {
                    return Err(vec!["Upstream must have at least one node".to_string()]);
                }
                Ok(())
            }
            ResourceType::Services => {
                let service = json_to_resource::<config::Service>(data)
                    .map_err(|e| vec![format!("Invalid JSON: {e}")])?;
                service.validate().map_err(|e| vec![e.to_string()])?;
                service
                    .validate_plugins()
                    .map_err(|e| vec![e.to_string()])?;
                Ok(())
            }
            ResourceType::GlobalRules => {
                let rule = json_to_resource::<config::GlobalRule>(data)
                    .map_err(|e| vec![format!("Invalid JSON: {e}")])?;
                rule.validate().map_err(|e| vec![e.to_string()])?;
                rule.validate_plugins().map_err(|e| vec![e.to_string()])?;
                Ok(())
            }
            ResourceType::Routes => {
                let route = json_to_resource::<config::Route>(data)
                    .map_err(|e| vec![format!("Invalid JSON: {e}")])?;
                route.validate().map_err(|e| vec![e.to_string()])?;
                route.validate_plugins().map_err(|e| vec![e.to_string()])?;
                Ok(())
            }
            ResourceType::McpServices => {
                let mcp_service = json_to_resource::<config::MCPService>(data)
                    .map_err(|e| vec![format!("Invalid JSON: {e}")])?;
                // MCPService doesn't have a validate method, only validate plugins
                for (name, value) in &mcp_service.plugins {
                    crate::plugin::build_plugin(name, value.clone())
                        .map_err(|e| vec![format!("Plugin validation failed: {e}")])?;
                }
                Ok(())
            }
            ResourceType::Ssls => {
                let ssl = json_to_resource::<config::SSL>(data)
                    .map_err(|e| vec![format!("Invalid JSON: {e}")])?;
                ssl.validate().map_err(|e| vec![e.to_string()])?;
                Ok(())
            }
        }
    }

    /// Validate dependencies between resources
    fn validate_dependencies(
        resource_type: ResourceType,
        _resource_id: &str,
        data: &[u8],
    ) -> Result<Vec<ValidationWarning>, ValidationError> {
        let mut warnings = Vec::new();

        match resource_type {
            ResourceType::Services => {
                let service =
                    json_to_resource::<config::Service>(data).map_err(|e| ValidationError {
                        field: "service".to_string(),
                        message: format!("Failed to parse service: {e}"),
                        error_type: ValidationErrorType::InvalidFormat,
                    })?;

                // Check if referenced upstream exists
                if let Some(ref upstream_id) = service.upstream_id {
                    if UPSTREAM_MAP.get(upstream_id).is_none() {
                        return Err(ValidationError {
                            field: "upstream_id".to_string(),
                            message: format!("Referenced upstream '{upstream_id}' does not exist"),
                            error_type: ValidationErrorType::MissingDependency,
                        });
                    }
                }
            }
            ResourceType::Routes => {
                let route =
                    json_to_resource::<config::Route>(data).map_err(|e| ValidationError {
                        field: "route".to_string(),
                        message: format!("Failed to parse route: {e}"),
                        error_type: ValidationErrorType::InvalidFormat,
                    })?;

                // Check if referenced upstream exists
                if let Some(ref upstream_id) = route.upstream_id {
                    if UPSTREAM_MAP.get(upstream_id).is_none() {
                        return Err(ValidationError {
                            field: "upstream_id".to_string(),
                            message: format!("Referenced upstream '{upstream_id}' does not exist"),
                            error_type: ValidationErrorType::MissingDependency,
                        });
                    }
                }

                // Check if referenced service exists
                if let Some(ref service_id) = route.service_id {
                    if SERVICE_MAP.get(service_id).is_none() {
                        return Err(ValidationError {
                            field: "service_id".to_string(),
                            message: format!("Referenced service '{service_id}' does not exist"),
                            error_type: ValidationErrorType::MissingDependency,
                        });
                    }
                }

                // Warn if both upstream_id and service_id are specified
                if route.upstream_id.is_some() && route.service_id.is_some() {
                    warnings.push(ValidationWarning {
                        field: "upstream_id,service_id".to_string(),
                        message: "Both upstream_id and service_id are specified. upstream_id will take precedence.".to_string(),
                    });
                }
            }
            ResourceType::McpServices => {
                let mcp_service =
                    json_to_resource::<config::MCPService>(data).map_err(|e| ValidationError {
                        field: "mcp_service".to_string(),
                        message: format!("Failed to parse MCP service: {e}"),
                        error_type: ValidationErrorType::InvalidFormat,
                    })?;

                // Check if referenced upstream exists
                if let Some(ref upstream_id) = mcp_service.upstream_id {
                    if UPSTREAM_MAP.get(upstream_id).is_none() {
                        return Err(ValidationError {
                            field: "upstream_id".to_string(),
                            message: format!("Referenced upstream '{upstream_id}' does not exist"),
                            error_type: ValidationErrorType::MissingDependency,
                        });
                    }
                }
            }
            _ => {
                // No dependencies for other resource types
            }
        }

        Ok(warnings)
    }

    /// Check for circular dependencies
    fn check_circular_dependencies(
        _resource_type: ResourceType,
        _resource_id: &str,
        _data: &[u8],
    ) -> Result<(), ValidationError> {
        // TODO: Implement circular dependency detection
        // This would involve building a dependency graph and checking for cycles
        Ok(())
    }

    /// Validate deletion of a resource (check if it's referenced by other resources)
    pub fn validate_deletion(resource_type: ResourceType, resource_id: &str) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check if the resource exists
        let exists = match resource_type {
            ResourceType::Upstreams => UPSTREAM_MAP.get(resource_id).is_some(),
            ResourceType::Services => SERVICE_MAP.get(resource_id).is_some(),
            ResourceType::GlobalRules => GLOBAL_RULE_MAP.get(resource_id).is_some(),
            ResourceType::Routes => ROUTE_MAP.get(resource_id).is_some(),
            ResourceType::McpServices => MCP_SERVICE_MAP.get(resource_id).is_some(),
            ResourceType::Ssls => SSL_MAP.get(resource_id).is_some(),
        };

        if !exists {
            warnings.push(ValidationWarning {
                field: "id".to_string(),
                message: format!("Resource '{resource_id}' does not exist"),
            });
            return ValidationResult {
                valid: true,
                errors,
                warnings,
            };
        }

        // Check for dependencies based on resource type
        match resource_type {
            ResourceType::Upstreams => {
                let mut dependents = Vec::new();

                // Check services
                for service in SERVICE_MAP.iter() {
                    if service.inner.upstream_id.as_deref() == Some(resource_id) {
                        dependents.push(format!("service:{}", service.id()));
                    }
                }

                // Check routes
                for route in ROUTE_MAP.iter() {
                    if route.inner.upstream_id.as_deref() == Some(resource_id) {
                        dependents.push(format!("route:{}", route.id()));
                    }
                }

                // Check MCP services
                for mcp_service in MCP_SERVICE_MAP.iter() {
                    if mcp_service.inner.upstream_id.as_deref() == Some(resource_id) {
                        dependents.push(format!("mcp_service:{}", mcp_service.id()));
                    }
                }

                if !dependents.is_empty() {
                    errors.push(ValidationError {
                        field: "dependencies".to_string(),
                        message: format!(
                            "Cannot delete upstream '{}' as it is referenced by: {}",
                            resource_id,
                            dependents.join(", ")
                        ),
                        error_type: ValidationErrorType::ConstraintViolation,
                    });
                }
            }
            ResourceType::Services => {
                let mut dependents = Vec::new();

                // Check routes
                for route in ROUTE_MAP.iter() {
                    if route.inner.service_id.as_deref() == Some(resource_id) {
                        dependents.push(format!("route:{}", route.id()));
                    }
                }

                if !dependents.is_empty() {
                    errors.push(ValidationError {
                        field: "dependencies".to_string(),
                        message: format!(
                            "Cannot delete service '{}' as it is referenced by: {}",
                            resource_id,
                            dependents.join(", ")
                        ),
                        error_type: ValidationErrorType::ConstraintViolation,
                    });
                }
            }
            _ => {
                // Other resource types don't have dependents
            }
        }

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    /// Get dependency graph for all resources
    pub fn get_dependency_graph() -> HashMap<String, Vec<String>> {
        let mut graph = HashMap::new();

        // Add upstreams
        for upstream in UPSTREAM_MAP.iter() {
            graph.insert(format!("upstream:{}", upstream.id()), Vec::new());
        }

        // Add services and their dependencies
        for service in SERVICE_MAP.iter() {
            let mut deps = Vec::new();
            if let Some(ref upstream_id) = service.inner.upstream_id {
                deps.push(format!("upstream:{upstream_id}"));
            }
            graph.insert(format!("service:{}", service.id()), deps);
        }

        // Add routes and their dependencies
        for route in ROUTE_MAP.iter() {
            let mut deps = Vec::new();
            if let Some(ref upstream_id) = route.inner.upstream_id {
                deps.push(format!("upstream:{upstream_id}"));
            }
            if let Some(ref service_id) = route.inner.service_id {
                deps.push(format!("service:{service_id}"));
            }
            graph.insert(format!("route:{}", route.id()), deps);
        }

        // Add MCP services and their dependencies
        for mcp_service in MCP_SERVICE_MAP.iter() {
            let mut deps = Vec::new();
            if let Some(ref upstream_id) = mcp_service.inner.upstream_id {
                deps.push(format!("upstream:{upstream_id}"));
            }
            graph.insert(format!("mcp_service:{}", mcp_service.id()), deps);
        }

        // Add global rules
        for rule in GLOBAL_RULE_MAP.iter() {
            graph.insert(format!("global_rule:{}", rule.id()), Vec::new());
        }

        // Add SSLs
        for ssl in SSL_MAP.iter() {
            graph.insert(format!("ssl:{}", ssl.id()), Vec::new());
        }

        graph
    }

    /// Check if a batch operation is valid
    pub fn validate_batch_operations(
        operations: &[super::resource_types::ResourceOperation],
    ) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check for duplicate operations on the same resource
        let mut seen = HashMap::new();
        for (idx, op) in operations.iter().enumerate() {
            let key = (op.resource_type, &op.resource_id);
            if let Some(prev_idx) = seen.insert(key, idx) {
                warnings.push(ValidationWarning {
                    field: format!("operations[{idx}]"),
                    message: format!(
                        "Duplicate operation on {}:{} (previous at index {prev_idx})",
                        op.resource_type, op.resource_id
                    ),
                });
            }
        }

        // Validate operation order (dependencies should be created before dependents)
        // This is a simplified version - a full implementation would need topological sorting
        let mut creates_upstreams = HashSet::new();
        let mut creates_services = HashSet::new();

        for op in operations {
            if let super::resource_types::OperationType::Create = op.operation_type {
                match op.resource_type {
                    ResourceType::Upstreams => {
                        creates_upstreams.insert(&op.resource_id);
                    }
                    ResourceType::Services => {
                        creates_services.insert(&op.resource_id);
                        // Check if service references an upstream that will be created
                        if let Some(data) = &op.data {
                            if let Ok(service) =
                                serde_json::from_value::<config::Service>(data.clone())
                            {
                                if let Some(ref upstream_id) = service.upstream_id {
                                    if !creates_upstreams.contains(upstream_id)
                                        && UPSTREAM_MAP.get(upstream_id).is_none()
                                    {
                                        errors.push(ValidationError {
                                            field: format!("operations[{}].data.upstream_id", operations.iter().position(|x| x.resource_id == op.resource_id).unwrap()),
                                            message: format!("Service references upstream '{}' which doesn't exist and isn't being created", upstream_id),
                                            error_type: ValidationErrorType::MissingDependency,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    ResourceType::Routes => {
                        // Similar dependency checking for routes
                        if let Some(data) = &op.data {
                            if let Ok(route) = serde_json::from_value::<config::Route>(data.clone())
                            {
                                if let Some(ref upstream_id) = route.upstream_id {
                                    if !creates_upstreams.contains(upstream_id)
                                        && UPSTREAM_MAP.get(upstream_id).is_none()
                                    {
                                        errors.push(ValidationError {
                                            field: format!("operations[{}].data.upstream_id", operations.iter().position(|x| x.resource_id == op.resource_id).unwrap()),
                                            message: format!("Route references upstream '{}' which doesn't exist and isn't being created", upstream_id),
                                            error_type: ValidationErrorType::MissingDependency,
                                        });
                                    }
                                }
                                if let Some(ref service_id) = route.service_id {
                                    if !creates_services.contains(service_id)
                                        && SERVICE_MAP.get(service_id).is_none()
                                    {
                                        errors.push(ValidationError {
                                            field: format!("operations[{}].data.service_id", operations.iter().position(|x| x.resource_id == op.resource_id).unwrap()),
                                            message: format!("Route references service '{}' which doesn't exist and isn't being created", service_id),
                                            error_type: ValidationErrorType::MissingDependency,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Upstream;
    use std::collections::HashMap;

    #[test]
    fn test_validate_upstream() {
        let mut nodes: HashMap<String, u32> = HashMap::new();
        nodes.insert("127.0.0.1:8080".to_string(), 1);
        let upstream = Upstream {
            id: "test".to_string(),
            nodes,
            ..Default::default()
        };

        let data = serde_json::to_vec(&upstream).unwrap();
        let result = ResourceValidator::validate_resource(ResourceType::Upstreams, "test", &data);

        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_empty_upstream_nodes() {
        let upstream = Upstream {
            id: "test".to_string(),
            nodes: HashMap::new(),
            ..Default::default()
        };

        let data = serde_json::to_vec(&upstream).unwrap();
        let result = ResourceValidator::validate_resource(ResourceType::Upstreams, "test", &data);

        assert!(!result.valid);
        assert!(!result.errors.is_empty());
    }
}
