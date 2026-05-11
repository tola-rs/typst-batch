//! Typst → JSON serialization.

use serde_json::{Map, Value as JsonValue};
use typst::foundations::{Content, Value};

/// Serialize Content to JSON.
///
/// Note: `null` values are stripped from the output for cleaner JSON.
pub fn content_to_json(content: &Content) -> JsonValue {
    let json = serde_json::to_value(content).unwrap_or(JsonValue::Null);
    strip_nulls(json)
}

/// Serialize Value to JSON.
///
/// Note: `null` values are stripped from the output for cleaner JSON.
pub fn value_to_json(value: &Value) -> JsonValue {
    let json = serde_json::to_value(value).unwrap_or(JsonValue::Null);
    strip_nulls(json)
}

/// Recursively strip `null` values from JSON objects.
fn strip_nulls(json: JsonValue) -> JsonValue {
    match json {
        JsonValue::Object(obj) => {
            let filtered: Map<String, JsonValue> = obj
                .into_iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k, strip_nulls(v)))
                .collect();
            JsonValue::Object(filtered)
        }
        JsonValue::Array(arr) => JsonValue::Array(arr.into_iter().map(strip_nulls).collect()),
        other => other,
    }
}

// =============================================================================
// Content simplification (extract text from Content JSON)
// =============================================================================

/// Simplify JSON by extracting text from Content objects.
///
/// Content objects (with "func" field) are converted to plain text strings.
pub fn json_to_simple_text(json: &JsonValue) -> JsonValue {
    match json {
        JsonValue::Object(obj) => {
            if obj.contains_key("func") {
                // Content: extract text only
                JsonValue::String(extract_content_text(json))
            } else {
                // Regular dict: recurse into values
                let simplified: Map<String, JsonValue> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), json_to_simple_text(v)))
                    .collect();
                JsonValue::Object(simplified)
            }
        }
        JsonValue::Array(arr) => JsonValue::Array(arr.iter().map(json_to_simple_text).collect()),
        other => other.clone(),
    }
}

/// Recursively extract text from Typst Content JSON.
fn extract_content_text(json: &JsonValue) -> String {
    match json {
        JsonValue::Object(obj) => {
            // "text" field (text element)
            if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                return text.to_string();
            }
            // "body" field (link, strong, emph, etc.)
            if let Some(body) = obj.get("body") {
                return extract_content_text(body);
            }
            // "children" array (sequence)
            if let Some(children) = obj.get("children").and_then(|v| v.as_array()) {
                return children.iter().map(extract_content_text).collect();
            }
            // "child" field
            if let Some(child) = obj.get("child") {
                return extract_content_text(child);
            }
            String::new()
        }
        JsonValue::Array(arr) => arr.iter().map(extract_content_text).collect(),
        JsonValue::String(s) => s.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simplify_text_content() {
        let json = json!({"func": "text", "text": "Hello World"});
        assert_eq!(json_to_simple_text(&json), json!("Hello World"));
    }

    #[test]
    fn test_simplify_link_content() {
        let json = json!({
            "func": "link",
            "dest": "/posts/hello",
            "body": {"func": "text", "text": "Click here"}
        });
        assert_eq!(json_to_simple_text(&json), json!("Click here"));
    }

    #[test]
    fn test_simplify_sequence_content() {
        let json = json!({
            "func": "sequence",
            "children": [
                {"func": "text", "text": "Hello "},
                {"func": "strong", "body": {"func": "text", "text": "World"}}
            ]
        });
        assert_eq!(json_to_simple_text(&json), json!("Hello World"));
    }

    #[test]
    fn test_simplify_nested_dict() {
        let json = json!({
            "title": "My Post",
            "summary": {"func": "text", "text": "A summary"},
            "next": {
                "func": "link",
                "dest": "/next",
                "body": {"func": "text", "text": "Next Post"}
            }
        });
        let result = json_to_simple_text(&json);
        assert_eq!(result["title"], "My Post");
        assert_eq!(result["summary"], "A summary");
        assert_eq!(result["next"], "Next Post");
    }

    #[test]
    fn test_simplify_array_with_content() {
        let json = json!([
            {"title": "A", "summary": {"func": "text", "text": "Summary A"}},
            {"title": "B", "summary": {"func": "text", "text": "Summary B"}}
        ]);
        let result = json_to_simple_text(&json);
        assert_eq!(result[0]["summary"], "Summary A");
        assert_eq!(result[1]["summary"], "Summary B");
    }
}
