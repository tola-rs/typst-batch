//! Typst dict/array builder utilities.
//!
//! Helpers for constructing Typst collection literals.

use typst::foundations::{IntoValue, Repr};

/// Builder for Typst dictionary literals.
///
/// # Example
///
/// ```ignore
/// use typst_batch::codegen::{DictBuilder, array};
///
/// let code = DictBuilder::new()
///     .field("url", "/posts/")
///     .field_opt("date", Some("2024-01-15"))
///     .field_raw("tags", array(["rust", "typst"]))
///     .build();
/// ```
#[derive(Default)]
pub struct DictBuilder {
    fields: Vec<String>,
}

impl DictBuilder {
    /// Create an empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a field (value converted via `IntoValue`).
    pub fn field<K, V>(mut self, key: K, value: V) -> Self
    where
        K: AsRef<str>,
        V: IntoValue,
    {
        self.fields
            .push(format!("{}: {}", key.as_ref(), value.into_value().repr()));
        self
    }

    /// Add an optional field (None outputs `none`).
    pub fn field_opt<K, V>(mut self, key: K, value: Option<V>) -> Self
    where
        K: AsRef<str>,
        V: IntoValue,
    {
        let repr = match value {
            Some(v) => v.into_value().repr().to_string(),
            None => "none".to_string(),
        };
        self.fields.push(format!("{}: {}", key.as_ref(), repr));
        self
    }

    /// Add a field with raw Typst code (no conversion).
    pub fn field_raw<K, V>(mut self, key: K, value: V) -> Self
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.fields
            .push(format!("{}: {}", key.as_ref(), value.as_ref()));
        self
    }

    /// Add an optional raw field.
    pub fn field_raw_opt<K, V>(mut self, key: K, value: Option<V>) -> Self
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let repr = match value {
            Some(v) => v.as_ref().to_string(),
            None => "none".to_string(),
        };
        self.fields.push(format!("{}: {}", key.as_ref(), repr));
        self
    }

    /// Build the Typst dictionary literal.
    pub fn build(self) -> String {
        format!("({})", self.fields.join(", "))
    }
}

/// Build a Typst dictionary from entries.
pub fn dict<I, K, V>(entries: I) -> String
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: IntoValue,
{
    let items: Vec<_> = entries
        .into_iter()
        .map(|(k, v)| format!("{}: {}", k.as_ref(), v.into_value().repr()))
        .collect();
    format!("({})", items.join(", "))
}

/// Build a Typst dictionary from raw code entries.
pub fn dict_raw<I, K, V>(entries: I) -> String
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let items: Vec<_> = entries
        .into_iter()
        .map(|(k, v)| format!("{}: {}", k.as_ref(), v.as_ref()))
        .collect();
    format!("({})", items.join(", "))
}

/// Build a Typst dictionary with optional values.
pub fn dict_sparse<I, K, V>(entries: I) -> String
where
    I: IntoIterator<Item = (K, Option<V>)>,
    K: AsRef<str>,
    V: IntoValue,
{
    let items: Vec<_> = entries
        .into_iter()
        .map(|(k, v)| {
            let repr = match v {
                Some(v) => v.into_value().repr().to_string(),
                None => "none".to_string(),
            };
            format!("{}: {}", k.as_ref(), repr)
        })
        .collect();
    format!("({})", items.join(", "))
}

/// Build a Typst array from items.
pub fn array<I, V>(items: I) -> String
where
    I: IntoIterator<Item = V>,
    V: IntoValue,
{
    let items: Vec<_> = items
        .into_iter()
        .map(|v| v.into_value().repr().to_string())
        .collect();
    format_array(items)
}

/// Build a Typst array from raw code items.
pub fn array_raw<I, S>(items: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let items: Vec<_> = items.into_iter().map(|s| s.as_ref().to_string()).collect();
    format_array(items)
}

/// Format items as Typst array literal.
///
/// Handles edge cases:
/// - Empty: `()`
/// - Single: `(item,)` (trailing comma required)
/// - Multiple: `(a, b, c)`
pub fn format_array(items: Vec<String>) -> String {
    match items.len() {
        0 => "()".to_string(),
        1 => format!("({},)", items[0]),
        _ => format!("({})", items.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dict() {
        let code = dict([("url", "/posts/"), ("title", "Hello")]);
        assert!(code.contains("url:"));
        assert!(code.contains("title:"));
    }

    #[test]
    fn test_dict_sparse() {
        let code = dict_sparse([("url", Some("/posts/")), ("date", None::<&str>)]);
        assert!(code.contains("url:"));
        assert!(code.contains("date: none"));
    }

    #[test]
    fn test_array_formatting() {
        assert_eq!(format_array(vec![]), "()");
        assert_eq!(format_array(vec!["a".into()]), "(a,)");
        assert_eq!(format_array(vec!["a".into(), "b".into()]), "(a, b)");
    }

    #[test]
    fn test_dict_builder() {
        let code = DictBuilder::new()
            .field("name", "test")
            .field_opt("value", Some(42i64))
            .field_opt("empty", None::<&str>)
            .build();
        assert!(code.contains("name:"));
        assert!(code.contains("value:"));
        assert!(code.contains("empty: none"));
    }
}
