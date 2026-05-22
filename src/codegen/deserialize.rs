//! JSON → Typst deserialization.

use serde_json::{Map, Value as JsonValue};
use typst::Library;
use typst::comemo::Tracked;
use typst::ecow::EcoVec;
use typst::engine::Engine;
use typst::foundations::SymbolElem;
use typst::foundations::{Arg, Args, CastInfo, Content, Context, Dict, Func, Str, Value};
use typst::syntax::{Span, Spanned};
use typst::text::{SpaceElem, TextElem};

use super::error::ConvertError;
use super::literal::{parse_angle, parse_color, parse_length, parse_ratio, parse_typst_literal};
use super::lookup::{find_element_funcs, find_element_in_scope};

/// Convert JSON to Typst Content.
///
/// Reconstructs a `Content` object from `typst query` JSON output.
///
/// # Special Elements
///
/// These elements require special handling because they are not registered
/// as public element functions in Typst's global scope:
///
/// - `text` - Constructor expects `body: Content`, not `text: String`
/// - `space` - Not registered in global scope
/// - `sequence` - Internal representation, not a public function
/// - `symbol` - Math symbol, not in global scope
pub fn json_to_content(
    engine: &mut Engine,
    context: Tracked<Context>,
    library: &Library,
    json: &JsonValue,
) -> Result<Content, ConvertError> {
    json_to_content_at(engine, context, library, json, "/")
}

pub(super) fn json_to_content_at(
    engine: &mut Engine,
    context: Tracked<Context>,
    library: &Library,
    json: &JsonValue,
    path: &str,
) -> Result<Content, ConvertError> {
    json_to_content_with_ancestors(engine, context, library, json, &[], path)
}

/// Convert JSON to Typst Content with ancestor context.
///
/// The ancestors are used for context-aware lookup of sub-elements.
/// For example, when deserializing `grid.header.cell`, we look for `cell`
/// in the ancestors' scopes (grid.header, then grid) to find `grid.cell`.
fn json_to_content_with_ancestors(
    engine: &mut Engine,
    context: Tracked<Context>,
    library: &Library,
    json: &JsonValue,
    ancestors: &[Func],
    path: &str,
) -> Result<Content, ConvertError> {
    let obj = json
        .as_object()
        .ok_or(ConvertError::NotObject(json_type_name(json)))?;
    let func_name = obj
        .get("func")
        .and_then(|v| v.as_str())
        .ok_or(ConvertError::MissingFunc)?;

    // Special elements not in global scope
    match func_name {
        "text" => {
            let text = obj
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or(ConvertError::MissingField("text"))?;
            return Ok(TextElem::packed(text));
        }
        "space" => {
            return Ok(SpaceElem::shared().clone());
        }
        "symbol" => {
            // Math symbol (e.g., x, y, alpha)
            let text = obj
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or(ConvertError::MissingField("text"))?;
            return Ok(SymbolElem::packed(text.chars().next().unwrap_or('?')));
        }
        "sequence" => {
            let children = obj
                .get("children")
                .and_then(|v| v.as_array())
                .ok_or(ConvertError::MissingField("children"))?;
            let contents: Result<Vec<Content>, _> = children
                .iter()
                .enumerate()
                .map(|(index, c)| {
                    json_to_content_with_ancestors(
                        engine,
                        context,
                        library,
                        c,
                        ancestors,
                        &index_path(&field_path(path, "children"), index),
                    )
                })
                .collect();
            return Ok(Content::sequence(contents?));
        }
        "styled" => {
            // Styled element: styles are lost in JSON serialization (".."),
            // so we just return the child content without styles.
            let child = obj
                .get("child")
                .ok_or(ConvertError::MissingField("child"))?;
            return json_to_content_with_ancestors(
                engine,
                context,
                library,
                child,
                ancestors,
                &field_path(path, "child"),
            );
        }
        _ => {}
    }

    // Try to find element in ancestors' scopes (most recent first)
    let func = ancestors
        .iter()
        .rev()
        .find_map(|ancestor| find_element_in_scope(ancestor, func_name))
        // Fall back to global lookup with field-based disambiguation
        .or_else(|| find_best_matching_element(library, func_name, obj))
        .ok_or_else(|| ConvertError::Unsupported {
            path: path.to_string(),
            func: func_name.to_string(),
        })?;

    let args = build_args(&func, obj, engine, context, library, ancestors, path)?;

    func.call(engine, context, args)
        .map_err(|e| ConvertError::CallFailed {
            func: func_name.to_string(),
            reason: e
                .iter()
                .map(|d| d.message.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        })?
        .cast::<Content>()
        .map_err(|_| ConvertError::CallFailed {
            func: func_name.to_string(),
            reason: "result is not Content".to_string(),
        })
}

/// Convert JSON to Typst Value.
///
/// - Objects with "func" field → Content
/// - Objects without "func" → Dict
/// - Arrays → Array
/// - Primitives → corresponding Value variant
pub fn json_to_value(
    engine: &mut Engine,
    context: Tracked<Context>,
    library: &Library,
    json: &JsonValue,
) -> Result<Value, ConvertError> {
    json_to_value_with_ancestors(engine, context, library, json, &[], "/")
}

/// Convert JSON to Typst Value with ancestor context.
///
/// # Type Markers
///
/// Supports explicit type markers using `_typst_type` to disambiguate
/// serialized values that lose type information:
///
/// ```json
/// {"_typst_type": "length", "value": "12pt"}
/// {"_typst_type": "angle", "value": "90deg"}
/// {"_typst_type": "ratio", "value": "50%"}
/// {"_typst_type": "color", "value": "#ff0000"}
/// ```
fn json_to_value_with_ancestors(
    engine: &mut Engine,
    context: Tracked<Context>,
    library: &Library,
    json: &JsonValue,
    ancestors: &[Func],
    path: &str,
) -> Result<Value, ConvertError> {
    match json {
        JsonValue::Null => Ok(Value::None),
        JsonValue::Bool(b) => Ok(Value::Bool(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Int(i))
            } else {
                Ok(Value::Float(
                    n.as_f64().ok_or(ConvertError::ValueConversion)?,
                ))
            }
        }
        JsonValue::String(s) => {
            // Try to parse as Typst literal (length, angle, ratio, color, auto, none)
            if let Some(value) = parse_typst_literal(s) {
                return Ok(value);
            }
            Ok(Value::Str(s.as_str().into()))
        }
        JsonValue::Array(arr) => {
            let items: Result<Vec<Value>, _> = arr
                .iter()
                .enumerate()
                .map(|(index, v)| {
                    json_to_value_with_ancestors(
                        engine,
                        context,
                        library,
                        v,
                        ancestors,
                        &index_path(path, index),
                    )
                })
                .collect();
            Ok(Value::Array(items?.into_iter().collect()))
        }
        JsonValue::Object(obj) => {
            // Check for explicit type marker: {"_typst_type": "...", "value": "..."}
            if let Some(type_tag) = obj.get("_typst_type").and_then(|v| v.as_str()) {
                return parse_typed_value(type_tag, obj);
            }

            // Check for Content marker: {"func": "..."}
            if obj.contains_key("func") {
                let content = json_to_content_with_ancestors(
                    engine, context, library, json, ancestors, path,
                )?;
                Ok(Value::Content(content))
            } else {
                // Regular Dict
                let dict: Result<Dict, _> = obj
                    .iter()
                    .map(|(k, v)| {
                        let value = json_to_value_with_ancestors(
                            engine,
                            context,
                            library,
                            v,
                            ancestors,
                            &field_path(path, k),
                        )?;
                        Ok((Str::from(k.as_str()), value))
                    })
                    .collect();
                Ok(Value::Dict(dict?))
            }
        }
    }
}

/// Parse a value with explicit type marker.
///
/// Handles objects like: `{"_typst_type": "length", "value": "12pt"}`
fn parse_typed_value(type_tag: &str, obj: &Map<String, JsonValue>) -> Result<Value, ConvertError> {
    let value_str = obj
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or(ConvertError::MissingField("value"))?;

    match type_tag {
        "length" => {
            parse_length(value_str)
                .map(Value::Length)
                .ok_or_else(|| ConvertError::InvalidLiteral {
                    type_name: "length",
                    value: value_str.to_string(),
                })
        }
        "angle" => {
            parse_angle(value_str)
                .map(Value::Angle)
                .ok_or_else(|| ConvertError::InvalidLiteral {
                    type_name: "angle",
                    value: value_str.to_string(),
                })
        }
        "ratio" => {
            parse_ratio(value_str)
                .map(Value::Ratio)
                .ok_or_else(|| ConvertError::InvalidLiteral {
                    type_name: "ratio",
                    value: value_str.to_string(),
                })
        }
        "color" => {
            parse_color(value_str)
                .map(Value::Color)
                .ok_or_else(|| ConvertError::InvalidLiteral {
                    type_name: "color",
                    value: value_str.to_string(),
                })
        }
        "str" | "string" => {
            // Explicit string - do NOT parse as literal
            Ok(Value::Str(value_str.into()))
        }
        _ => Err(ConvertError::UnknownTypeTag(type_tag.to_string())),
    }
}

/// Build arguments from function's parameter info.
///
/// Strategy:
/// - Positional-only parameters (positional=true, named=false) must be passed positionally
///   - For optional positional-only params:
///     - If the param type accepts `none`, pass `None` to maintain order
///     - Otherwise, skip (Typst uses type inference)
/// - Named-only parameters (positional=false, named=true) must be passed by name
/// - Dual parameters (positional=true, named=true) are passed by name to avoid ordering issues
/// - Variadic parameters are expanded into multiple positional arguments
fn build_args(
    func: &Func,
    obj: &Map<String, JsonValue>,
    engine: &mut Engine,
    context: Tracked<Context>,
    library: &Library,
    ancestors: &[Func],
    path: &str,
) -> Result<Args, ConvertError> {
    let span = Span::detached();
    let mut items: EcoVec<Arg> = EcoVec::new();

    let params = func.params().ok_or(ConvertError::ValueConversion)?;

    // Build new ancestors list with current func
    let mut new_ancestors = ancestors.to_vec();
    new_ancestors.push(func.clone());

    // Collect positional-only parameters in order
    let positional_only: Vec<_> = params.iter().filter(|p| p.positional && !p.named).collect();

    // First, handle positional-only parameters (must be in order)
    for param in &positional_only {
        if let Some(value) = obj.get(param.name) {
            let value_path = field_path(path, param.name);
            if param.variadic {
                // Expand variadic into multiple positional args
                if let Some(arr) = value.as_array() {
                    for (index, item) in arr.iter().enumerate() {
                        let typst_value = json_to_value_with_ancestors(
                            engine,
                            context,
                            library,
                            item,
                            &new_ancestors,
                            &index_path(&value_path, index),
                        )?;
                        items.push(Arg {
                            span,
                            name: None,
                            value: Spanned::new(typst_value, span),
                        });
                    }
                } else {
                    let typst_value = json_to_value_with_ancestors(
                        engine,
                        context,
                        library,
                        value,
                        &new_ancestors,
                        &value_path,
                    )?;
                    items.push(Arg {
                        span,
                        name: None,
                        value: Spanned::new(typst_value, span),
                    });
                }
            } else {
                let typst_value = json_to_value_with_ancestors(
                    engine,
                    context,
                    library,
                    value,
                    &new_ancestors,
                    &value_path,
                )?;
                items.push(Arg {
                    span,
                    name: None,
                    value: Spanned::new(typst_value, span),
                });
            }
        } else if !param.required && param_accepts_none(param) {
            // For optional positional-only params that accept `none`,
            // pass None to maintain argument order
            items.push(Arg {
                span,
                name: None,
                value: Spanned::new(Value::None, span),
            });
        }
        // For optional params that don't accept `none`, skip them.
        // Typst uses type inference to match arguments to parameters.
    }

    // Collect names of positional-only params to skip them in named processing
    let positional_only_names: std::collections::HashSet<_> =
        positional_only.iter().map(|p| p.name).collect();

    // Then, handle all other parameters as named arguments
    for (key, value) in obj.iter() {
        if key == "func" || positional_only_names.contains(key.as_str()) {
            continue;
        }

        let value_path = field_path(path, key);

        // Check if this is a variadic parameter
        let param = params.iter().find(|p| p.name == key);
        if let Some(param) = param
            && param.variadic
        {
            // Expand variadic into multiple positional args
            if let Some(arr) = value.as_array() {
                for (index, item) in arr.iter().enumerate() {
                    let typst_value = json_to_value_with_ancestors(
                        engine,
                        context,
                        library,
                        item,
                        &new_ancestors,
                        &index_path(&value_path, index),
                    )?;
                    items.push(Arg {
                        span,
                        name: None,
                        value: Spanned::new(typst_value, span),
                    });
                }
                continue;
            }
        }

        // Regular named argument
        let typst_value = json_to_value_with_ancestors(
            engine,
            context,
            library,
            value,
            &new_ancestors,
            &value_path,
        )?;
        items.push(Arg {
            span,
            name: Some(Str::from(key.as_str())),
            value: Spanned::new(typst_value, span),
        });
    }

    Ok(Args { span, items })
}

/// Check if a parameter's type accepts `none`.
fn param_accepts_none(param: &typst::foundations::ParamInfo) -> bool {
    let mut accepts_none = false;
    param.input.walk(|info| match info {
        CastInfo::Any => accepts_none = true,
        CastInfo::Type(ty) if ty.short_name() == "none" => accepts_none = true,
        _ => {}
    });
    accepts_none
}

fn json_type_name(json: &JsonValue) -> &'static str {
    match json {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

pub(super) fn field_path(parent: &str, key: &str) -> String {
    child_path(parent, &escape_path_segment(key))
}

pub(super) fn index_path(parent: &str, index: usize) -> String {
    child_path(parent, &index.to_string())
}

fn child_path(parent: &str, segment: &str) -> String {
    if parent.is_empty() || parent == "/" {
        format!("/{segment}")
    } else {
        format!("{parent}/{segment}")
    }
}

fn escape_path_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

/// Find the best matching element function based on JSON fields.
///
/// When multiple elements share the same name (e.g., `list.item`, `enum.item`, `terms.item`),
/// we use the JSON fields to determine which element is the best match.
fn find_best_matching_element(
    library: &Library,
    func_name: &str,
    obj: &Map<String, JsonValue>,
) -> Option<Func> {
    let candidates: Vec<_> = find_element_funcs(library, func_name).collect();

    if candidates.len() <= 1 {
        return candidates.into_iter().next();
    }

    // Score each candidate based on how well its parameters match the JSON fields
    let json_fields: std::collections::HashSet<_> = obj.keys().filter(|k| *k != "func").collect();

    candidates.into_iter().max_by_key(|func| {
        let params = func.params().unwrap_or_default();
        let param_names: std::collections::HashSet<_> = params.iter().map(|p| p.name).collect();

        // Count how many JSON fields match parameter names
        let matches = json_fields
            .iter()
            .filter(|f| param_names.contains(f.as_str()))
            .count();

        // Penalize if there are required params not in JSON
        let missing_required = params
            .iter()
            .filter(|p| p.required && !json_fields.contains(&p.name.to_string()))
            .count();

        // Score: matches - missing_required
        (matches as i32) - (missing_required as i32)
    })
}
