//! Builder for `sys.inputs` from JSON.

use std::path::Path;
use std::sync::Arc;

use serde_json::Value as JsonValue;
use typst::World;
use typst::comemo::Track;
use typst::engine::{Engine, Route, Sink, Traced};
use typst::foundations::{Array, Context, Dict, IntoValue, Value};
use typst::introspection::Introspector;

use super::{ConvertError, json_to_content};
use crate::world::TypstWorld;

/// Opaque inputs type for `sys.inputs` injection.
///
/// Created via [`Inputs::from_json()`]. Pass to `with_inputs_obj()`.
#[derive(Clone, Default)]
pub struct Inputs {
    pub(crate) dict: Dict,
}

impl Inputs {
    /// Create empty inputs.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create empty inputs.
    pub fn empty() -> Self {
        Self::new()
    }

    /// Create inputs from JSON value.
    ///
    /// Automatically maps JSON types to Typst types:
    /// - `string` → `Str`
    /// - `number` (integer) → `Int`
    /// - `number` (float) → `Float`
    /// - `boolean` → `Bool`
    /// - `array` → `Array`
    /// - `object` → `Dict`
    /// - `null` → `None`
    ///
    /// # Example
    ///
    /// ```ignore
    /// let base_json = serde_json::json!({
    ///     "title": "My Blog",
    ///     "description": "A personal blog",
    ///     "extra": {
    ///         "author": "Alice",
    ///         "twitter": "@alice"
    ///     }
    /// });
    /// let inputs = Inputs::from_json(&base_json)?;
    /// ```
    pub fn from_json(json: &JsonValue) -> Result<Self, ConvertError> {
        match json {
            JsonValue::Object(_) => {
                let dict = json_to_simple_dict(json)?;
                Ok(Self { dict })
            }
            _ => Err(ConvertError::Other("inputs must be a JSON object".into())),
        }
    }

    /// Create inputs from JSON with Content reconstruction.
    ///
    /// Like [`from_json()`](Self::from_json), but automatically rebuilds
    /// Typst Content from JSON objects containing `{"func": ...}`.
    ///
    /// Use this when your JSON contains serialized Typst Content (e.g.,
    /// metadata extracted from compiled pages).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // JSON with Content field (has "func")
    /// let pages_json = serde_json::json!({
    ///     "pages": [
    ///         {
    ///             "url": "/post/1",
    ///             "summary": {
    ///                 "func": "sequence",
    ///                 "children": [
    ///                     {"func": "text", "text": "Hello "},
    ///                     {"func": "link", "dest": "https://example.com", "body": {...}}
    ///                 ]
    ///             }
    ///         }
    ///     ]
    /// });
    ///
    /// // Content fields are automatically rebuilt
    /// let inputs = Inputs::from_json_with_content(&pages_json, root)?;
    /// ```
    pub fn from_json_with_content(json: &JsonValue, root: &Path) -> Result<Self, ConvertError> {
        match json {
            JsonValue::Object(_) => {
                let converter = ContentConverter::new(root);
                let dict = converter.convert_dict(json)?;
                Ok(Self { dict })
            }
            _ => Err(ConvertError::Other("inputs must be a JSON object".into())),
        }
    }

    /// Merge another Inputs into this one.
    ///
    /// Values from `other` overwrite values in `self` for duplicate keys.
    pub fn merge(&mut self, other: Inputs) {
        for (key, value) in other.dict {
            self.dict.insert(key, value);
        }
    }

    /// Merge a JSON object into this Inputs.
    pub fn merge_json(&mut self, json: &JsonValue) -> Result<(), ConvertError> {
        let other = Self::from_json(json)?;
        self.merge(other);
        Ok(())
    }

    /// Get the underlying Dict.
    pub fn into_dict(self) -> Dict {
        self.dict
    }
}

/// Convert JSON value to Typst Value (simple, without Content reconstruction).
///
/// This is a lightweight conversion that maps JSON types directly to Typst types.
/// Objects with `{"func": ...}` are converted to Dict, not Content.
///
/// For Content reconstruction, use [`Inputs::from_json_with_content()`].
pub fn json_to_simple_value(json: &JsonValue) -> Result<Value, ConvertError> {
    match json {
        JsonValue::Null => Ok(Value::None),
        JsonValue::Bool(b) => Ok(b.into_value()),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_value())
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_value())
            } else {
                Err(ConvertError::Other(format!("unsupported number: {n}")))
            }
        }
        JsonValue::String(s) => Ok(s.as_str().into_value()),
        JsonValue::Array(arr) => {
            let items: Result<Vec<_>, _> = arr.iter().map(json_to_simple_value).collect();
            let array: typst::foundations::Array = items?.into_iter().collect();
            Ok(array.into_value())
        }
        JsonValue::Object(_) => {
            let dict = json_to_simple_dict(json)?;
            Ok(dict.into_value())
        }
    }
}

/// Convert JSON object to Typst Dict (simple, without Content reconstruction).
fn json_to_simple_dict(json: &JsonValue) -> Result<Dict, ConvertError> {
    let obj = json
        .as_object()
        .ok_or_else(|| ConvertError::Other("expected JSON object".into()))?;

    let mut dict = Dict::new();
    for (key, value) in obj {
        dict.insert(key.as_str().into(), json_to_simple_value(value)?);
    }
    Ok(dict)
}

// =============================================================================
// ContentConverter - JSON to Value with Content reconstruction
// =============================================================================

/// Converter that rebuilds Content from JSON.
struct ContentConverter {
    world: Arc<TypstWorld>,
}

impl ContentConverter {
    fn new(root: &Path) -> Self {
        // Create a minimal World for Content reconstruction
        // We use a dummy path since we don't actually compile anything
        let dummy_path = root.join("__content_converter_dummy.typ");
        let world = TypstWorld::builder(&dummy_path, root)
            .with_local_cache()
            .no_fonts()
            .build();

        Self {
            world: Arc::new(world),
        }
    }

    fn convert_value(&self, json: &JsonValue) -> Result<Value, ConvertError> {
        match json {
            JsonValue::Null => Ok(Value::None),
            JsonValue::Bool(b) => Ok(b.into_value()),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(i.into_value())
                } else if let Some(f) = n.as_f64() {
                    Ok(f.into_value())
                } else {
                    Err(ConvertError::Other(format!("unsupported number: {n}")))
                }
            }
            JsonValue::String(s) => Ok(s.as_str().into_value()),
            JsonValue::Array(arr) => {
                let mut result = Array::new();
                for item in arr {
                    result.push(self.convert_value(item)?);
                }
                Ok(result.into_value())
            }
            JsonValue::Object(obj) => {
                // Check if this is Typst Content (has "func" field)
                if obj.contains_key("func") {
                    self.rebuild_content(json)
                } else {
                    Ok(self.convert_dict(json)?.into_value())
                }
            }
        }
    }

    fn convert_dict(&self, json: &JsonValue) -> Result<Dict, ConvertError> {
        let obj = json
            .as_object()
            .ok_or_else(|| ConvertError::Other("expected JSON object".into()))?;

        let mut dict = Dict::new();
        for (key, value) in obj {
            dict.insert(key.as_str().into(), self.convert_value(value)?);
        }
        Ok(dict)
    }

    fn rebuild_content(&self, json: &JsonValue) -> Result<Value, ConvertError> {
        let introspector = Introspector::default();
        let traced = Traced::default();
        let mut sink = Sink::new();

        let mut engine = Engine {
            world: (&*self.world as &dyn World).track(),
            introspector: introspector.track(),
            traced: traced.track(),
            sink: sink.track_mut(),
            route: Route::default(),
            routines: &typst::ROUTINES,
        };

        let library = self.world.library();
        let context = Context::none();

        let content = json_to_content(&mut engine, context.track(), library, json)?;
        Ok(content.into_value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;
    use typst::foundations::Str;

    #[test]
    fn default_inputs_are_empty() {
        assert_eq!(Inputs::default().into_dict().len(), 0);
        assert_eq!(Inputs::new().into_dict().len(), 0);
    }

    #[test]
    fn cloned_inputs_can_be_extended_without_mutating_original() {
        let original = Inputs::from_json(&json!({"title": "Original"})).unwrap();
        let mut cloned = original.clone();

        cloned.merge(Inputs::from_json(&json!({"draft": false})).unwrap());

        assert_eq!(original.into_dict().len(), 1);
        assert_eq!(cloned.into_dict().len(), 2);
    }

    #[test]
    fn test_from_json_simple() {
        let json = json!({
            "title": "My Blog",
            "count": 42,
            "ratio": 3.14,
            "draft": false
        });
        let inputs = Inputs::from_json(&json).unwrap();
        assert_eq!(inputs.dict.len(), 4);
    }

    #[test]
    fn test_from_json_nested() {
        let json = json!({
            "title": "My Blog",
            "extra": {
                "author": "Alice",
                "twitter": "@alice"
            }
        });
        let inputs = Inputs::from_json(&json).unwrap();
        assert_eq!(inputs.dict.len(), 2);

        // Check nested dict
        let extra = inputs.dict.get(&Str::from("extra")).unwrap();
        assert!(extra.clone().cast::<Dict>().is_ok());
    }

    #[test]
    fn test_from_json_array() {
        let json = json!({
            "tags": ["rust", "typst", "blog"]
        });
        let inputs = Inputs::from_json(&json).unwrap();

        let tags = inputs.dict.get(&Str::from("tags")).unwrap();
        let arr = tags.clone().cast::<Array>().unwrap();
        assert_eq!(arr.len(), 3);
    }

    #[test]
    fn test_from_json_null() {
        let json = json!({
            "value": null
        });
        let inputs = Inputs::from_json(&json).unwrap();

        let value = inputs.dict.get(&Str::from("value")).unwrap();
        assert_eq!(*value, Value::None);
    }

    #[test]
    fn test_from_json_not_object() {
        let json = json!("not an object");
        assert!(Inputs::from_json(&json).is_err());
    }

    #[test]
    fn test_from_json_with_content_simple() {
        let dir = TempDir::new().unwrap();

        // JSON without Content fields - should work like from_json
        let json = json!({
            "title": "My Blog",
            "count": 42
        });

        let inputs = Inputs::from_json_with_content(&json, dir.path()).unwrap();
        assert_eq!(inputs.dict.len(), 2);
    }

    #[test]
    fn test_from_json_with_content_rebuilds_content() {
        let dir = TempDir::new().unwrap();

        // JSON with Content field (has "func")
        let json = json!({
            "pages": [
                {
                    "url": "/post/1",
                    "title": "First Post",
                    "summary": {
                        "func": "text",
                        "text": "Hello world"
                    }
                }
            ]
        });

        let inputs = Inputs::from_json_with_content(&json, dir.path()).unwrap();

        // Get pages array
        let pages = inputs.dict.get(&Str::from("pages")).unwrap();
        let pages_arr = pages.clone().cast::<Array>().unwrap();
        assert_eq!(pages_arr.len(), 1);

        // Get first page
        let page = pages_arr.at(0, None).unwrap();
        let page_dict = page.clone().cast::<Dict>().unwrap();

        // summary should be Content, not Dict
        let summary = page_dict.get(&Str::from("summary")).unwrap();
        assert!(
            summary.clone().cast::<Dict>().is_err(),
            "summary should be Content, not Dict"
        );
    }

    #[test]
    fn test_from_json_with_content_complex() {
        let dir = TempDir::new().unwrap();

        // Complex Content with link
        let json = json!({
            "summary": {
                "func": "sequence",
                "children": [
                    {"func": "text", "text": "Check out "},
                    {
                        "func": "link",
                        "dest": "https://example.com",
                        "body": {"func": "text", "text": "this link"}
                    },
                    {"func": "text", "text": "!"}
                ]
            }
        });

        let inputs = Inputs::from_json_with_content(&json, dir.path()).unwrap();

        let summary = inputs.dict.get(&Str::from("summary")).unwrap();
        // Should be Content, not Dict
        assert!(summary.clone().cast::<Dict>().is_err());
    }
}
