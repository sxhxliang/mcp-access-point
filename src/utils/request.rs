
use http::Uri;
use regex::Regex;
use serde_json::{json, Value};
use url::form_urlencoded;
use std::collections::HashMap;
pub fn query_to_map(uri: &Uri) -> HashMap<String, String> {
    let mut map = HashMap::new();
    
    if let Some(query) = uri.query() {
        for pair in query.split('&') {
            let mut kv = pair.splitn(2, '=');
            if let (Some(key), Some(value)) = (kv.next(), kv.next()) {
                map.insert(key.to_string(), value.to_string());
            }
        }
    }
    
    map
}

pub fn replace_dynamic_params(path: &str, params: &Value) -> String {
    let re = Regex::new(r"\{(\w+)\}").unwrap();
    re.replace_all(path, |caps: &regex::Captures<'_>| {
        let key = &caps[1];
        let binding = Value::String("".to_string());
        let value = params.get(key).unwrap_or(&binding);
        // params.get(key).unwrap_or(&"") //
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "".to_string(),
            _ => value.to_string(),
        }
    }).to_string()
}

pub fn flatten_json(prefix: &str, value: &Value, result: &mut HashMap<String, String>) {
    match value {
        Value::Object(obj) => {
            for (key, val) in obj {
                let new_prefix = if prefix.is_empty() {
                    key.to_owned()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_json(&new_prefix, val, result);
            }
        }
        Value::Array(arr) => {
            for (index, val) in arr.iter().enumerate() {
                let new_prefix = format!("{}[{}]", prefix, index);
                flatten_json(&new_prefix, val, result);
            }
        }
        Value::String(s) => {
            result.insert(prefix.to_string(), s.to_string());
        }
        Value::Number(n) => {
            result.insert(prefix.to_string(), n.to_string());
        }
        Value::Bool(b) => {
            result.insert(prefix.to_string(), b.to_string());
        }
        Value::Null => {} //
    }
}

pub fn merge_path_query(path: &str, query: &str) -> String {
    if query.is_empty() {
        return path.to_string();
    }
    if path.contains('?') {
        format!("{}&{}", path, query)
    } else {
        format!("{}?{}", path, query)
    }
}
pub fn json_to_uri_query(value: &Value) -> String {
    let mut flattened_params = HashMap::new();
    flatten_json("", value, &mut flattened_params);
    let mut new_query = "?".to_string();
    new_query.push_str(
        &form_urlencoded::Serializer::new(String::new())
            .extend_pairs(flattened_params.iter())
            .finish()
    );
    new_query
}
#[test]
fn flatten_json_object_with_nested_structure_flattens_correctly() {
    let json_value = json!({
        "name": "John",
        "age": 30,
        "address": {
            "city": "New York",
            "zip": "10001"
        },
        "hobbies": ["reading", "traveling"]
    });

    let mut result = HashMap::new();
    flatten_json("", &json_value, &mut result);

    let expected = HashMap::from([
        ("name".to_string(), "John".to_string()),
        ("age".to_string(), "30".to_string()),
        ("address.city".to_string(), "New York".to_string()),
        ("address.zip".to_string(), "10001".to_string()),
        ("hobbies[0]".to_string(), "reading".to_string()),
        ("hobbies[1]".to_string(), "traveling".to_string()),
    ]);

    assert_eq!(result, expected);
}

#[test]
fn flatten_json_empty_object_returns_empty_map() {
    let json_value = json!({});
    let mut result = HashMap::new();
    flatten_json("", &json_value, &mut result);

    assert!(result.is_empty());
}

#[test]
fn flatten_json_empty_array_returns_empty_map() {
    let json_value = json!([]);
    let mut result = HashMap::new();
    flatten_json("", &json_value, &mut result);

    assert!(result.is_empty());
}

#[test]
fn flatten_json_null_value_ignores_null() {
    let json_value = json!(null);
    let mut result = HashMap::new();
    flatten_json("", &json_value, &mut result);

    assert!(result.is_empty());
}