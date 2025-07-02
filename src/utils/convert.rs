use std::collections::HashMap;

pub fn convert_openapi_path_to_name(method: &str, path: &str) -> String {
    let method_mapping: HashMap<&str, &str> = [
        ("get", "get"),
        ("post", "create"),
        ("put", "update"),
        ("patch", "patch"),
        ("delete", "delete"),
        ("options", "options"),
        ("head", "head"),
    ]
    .iter()
    .cloned()
    .collect();
    
    let action = method_mapping.get(method.trim().to_lowercase().as_str()).unwrap_or(&method);

    let mut segments = Vec::new();
    for segment in path.trim().split('/').filter(|s| !s.is_empty()) {
        if segment.starts_with('{') && segment.ends_with('}') {
            let param = &segment[1..segment.len() - 1];
            segments.push(format!("by_{param}"));
        } else {
            segments.push(segment.to_string());
        }
    }

    let mut result = action.to_string();
    if !segments.is_empty() {
        result.push('_');
        result.push_str(&segments.join("_"));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_with_path_and_param() {
        assert_eq!(convert_openapi_path_to_name("get", "/users/{id}"), "get_users_by_id");
    }

    #[test]
    fn test_post_with_path() {
        assert_eq!(convert_openapi_path_to_name("post", "/users"), "create_users");
    }

    #[test]
    fn test_put_with_path_and_param() {
        assert_eq!(convert_openapi_path_to_name("put", "/users/{id}"), "update_users_by_id");
    }

    #[test]
    fn test_delete_with_path_and_param() {
        assert_eq!(convert_openapi_path_to_name("delete", "/users/{id}"), "delete_users_by_id");
    }

    #[test]
    fn test_head_with_path() {
        assert_eq!(convert_openapi_path_to_name("head", "/users"), "head_users");
    }

    #[test]
    fn test_patch_with_multiple_segments() {
        assert_eq!(
            convert_openapi_path_to_name("patch", "/users/{id}/edit"),
            "patch_users_by_id_edit"
        );
    }

    #[test]
    fn test_custom_method() {
        assert_eq!(convert_openapi_path_to_name("custom", "/test"), "custom_test");
    }

    #[test]
    fn test_get_with_empty_path() {
        assert_eq!(convert_openapi_path_to_name("get", ""), "get");
    }

    #[test]
    fn test_get_with_slash_only() {
        assert_eq!(convert_openapi_path_to_name("get", "/"), "get");
    }

    #[test]
    fn test_get_with_single_param() {
        assert_eq!(convert_openapi_path_to_name("get", "/{id}"), "get_by_id");
    }
}