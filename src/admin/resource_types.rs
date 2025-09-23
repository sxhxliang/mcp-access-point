use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents different resource types managed by the Admin API
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    #[serde(rename = "upstreams")]
    Upstreams,
    #[serde(rename = "services")]
    Services,
    #[serde(rename = "global_rules")]
    GlobalRules,
    #[serde(rename = "routes")]
    Routes,
    #[serde(rename = "mcp_services")]
    McpServices,
    #[serde(rename = "ssls")]
    Ssls,
}

impl ResourceType {
    /// Get all available resource types
    pub fn all() -> &'static [ResourceType] {
        &[
            ResourceType::Upstreams,
            ResourceType::Services,
            ResourceType::GlobalRules,
            ResourceType::Routes,
            ResourceType::McpServices,
            ResourceType::Ssls,
        ]
    }

    /// Get the string representation of the resource type
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceType::Upstreams => "upstreams",
            ResourceType::Services => "services",
            ResourceType::GlobalRules => "global_rules",
            ResourceType::Routes => "routes",
            ResourceType::McpServices => "mcp_services",
            ResourceType::Ssls => "ssls",
        }
    }

    /// Parse string to ResourceType
    pub fn from_str(s: &str) -> Option<ResourceType> {
        match s {
            "upstreams" => Some(ResourceType::Upstreams),
            "services" => Some(ResourceType::Services),
            "global_rules" => Some(ResourceType::GlobalRules),
            "routes" => Some(ResourceType::Routes),
            "mcp_services" => Some(ResourceType::McpServices),
            "ssls" => Some(ResourceType::Ssls),
            _ => None,
        }
    }

    /// Get dependencies for this resource type
    /// Returns a list of resource types that this resource depends on
    pub fn dependencies(&self) -> &'static [ResourceType] {
        match self {
            ResourceType::Upstreams => &[],
            ResourceType::Services => &[ResourceType::Upstreams],
            ResourceType::GlobalRules => &[],
            ResourceType::Routes => &[ResourceType::Upstreams, ResourceType::Services],
            ResourceType::McpServices => &[ResourceType::Upstreams],
            ResourceType::Ssls => &[],
        }
    }

    /// Get dependents for this resource type
    /// Returns a list of resource types that depend on this resource
    pub fn dependents(&self) -> &'static [ResourceType] {
        match self {
            ResourceType::Upstreams => &[
                ResourceType::Services,
                ResourceType::Routes,
                ResourceType::McpServices,
            ],
            ResourceType::Services => &[ResourceType::Routes],
            ResourceType::GlobalRules => &[],
            ResourceType::Routes => &[],
            ResourceType::McpServices => &[],
            ResourceType::Ssls => &[],
        }
    }
}

/// Statistics for a resource type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStats {
    pub resource_type: ResourceType,
    pub count: usize,
    pub last_updated: Option<std::time::SystemTime>,
}

/// Summary of all resource types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSummary {
    pub stats: HashMap<ResourceType, ResourceStats>,
    pub total_resources: usize,
}

/// Represents a resource operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceOperationResult {
    pub success: bool,
    pub message: String,
    pub resource_type: ResourceType,
    pub resource_id: Option<String>,
    pub timestamp: std::time::SystemTime,
}

/// Batch operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationRequest {
    pub operations: Vec<ResourceOperation>,
    pub dry_run: Option<bool>,
}

/// Individual resource operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceOperation {
    pub operation_type: OperationType,
    pub resource_type: ResourceType,
    pub resource_id: String,
    pub data: Option<serde_json::Value>,
}

/// Types of operations that can be performed on resources
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OperationType {
    #[serde(rename = "create")]
    Create,
    #[serde(rename = "update")]
    Update,
    #[serde(rename = "delete")]
    Delete,
    #[serde(rename = "reload")]
    Reload,
}

/// Batch operation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationResponse {
    pub success: bool,
    pub results: Vec<ResourceOperationResult>,
    pub summary: String,
    pub dry_run: bool,
}

/// Resource validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

/// Validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub error_type: ValidationErrorType,
}

/// Validation warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub field: String,
    pub message: String,
}

/// Types of validation errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationErrorType {
    #[serde(rename = "missing_dependency")]
    MissingDependency,
    #[serde(rename = "invalid_format")]
    InvalidFormat,
    #[serde(rename = "constraint_violation")]
    ConstraintViolation,
    #[serde(rename = "circular_dependency")]
    CircularDependency,
}

/// Configuration change event
#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    pub resource_type: ResourceType,
    pub resource_id: String,
    pub operation: OperationType,
    pub timestamp: std::time::SystemTime,
    pub user: Option<String>,
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationType::Create => write!(f, "create"),
            OperationType::Update => write!(f, "update"),
            OperationType::Delete => write!(f, "delete"),
            OperationType::Reload => write!(f, "reload"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_type_from_str() {
        assert_eq!(
            ResourceType::from_str("upstreams"),
            Some(ResourceType::Upstreams)
        );
        assert_eq!(
            ResourceType::from_str("services"),
            Some(ResourceType::Services)
        );
        assert_eq!(ResourceType::from_str("invalid"), None);
    }

    #[test]
    fn test_resource_dependencies() {
        assert_eq!(ResourceType::Upstreams.dependencies(), &[]);
        assert_eq!(
            ResourceType::Services.dependencies(),
            &[ResourceType::Upstreams]
        );
        assert_eq!(
            ResourceType::Routes.dependencies(),
            &[ResourceType::Upstreams, ResourceType::Services]
        );
    }

    #[test]
    fn test_resource_dependents() {
        assert_eq!(
            ResourceType::Upstreams.dependents(),
            &[
                ResourceType::Services,
                ResourceType::Routes,
                ResourceType::McpServices
            ]
        );
        assert_eq!(ResourceType::Services.dependents(), &[ResourceType::Routes]);
    }
}
