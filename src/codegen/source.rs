//! Typst source code generation from Rust types.
//!
//! Uses typst's internal `Repr` trait for correct escaping.

use typst::foundations::{IntoValue, Repr, Str, Value};

/// Convert a Rust value to Typst source code.
///
/// # Example
///
/// ```ignore
/// use typst_batch::codegen::ToTypst;
///
/// assert_eq!("hello".to_typst(), r#""hello""#);
/// assert_eq!(42i64.to_typst(), "42");
/// assert_eq!(true.to_typst(), "true");
/// ```
pub trait ToTypst {
    /// Generate Typst source code representation.
    fn to_typst(&self) -> String;
}

// ---------------------------------------------------------------------------
// Primitive implementations
// ---------------------------------------------------------------------------

impl ToTypst for str {
    fn to_typst(&self) -> String {
        Str::from(self).repr().to_string()
    }
}

impl ToTypst for &str {
    fn to_typst(&self) -> String {
        (*self).to_typst()
    }
}

impl ToTypst for String {
    fn to_typst(&self) -> String {
        self.as_str().to_typst()
    }
}

impl ToTypst for i64 {
    fn to_typst(&self) -> String {
        Value::Int(*self).repr().to_string()
    }
}

impl ToTypst for f64 {
    fn to_typst(&self) -> String {
        Value::Float(*self).repr().to_string()
    }
}

impl ToTypst for bool {
    fn to_typst(&self) -> String {
        Value::Bool(*self).repr().to_string()
    }
}

impl<T: ToTypst> ToTypst for Option<T> {
    fn to_typst(&self) -> String {
        match self {
            Some(v) => v.to_typst(),
            None => "none".to_string(),
        }
    }
}

impl<T: IntoValue + Clone> ToTypst for [T] {
    fn to_typst(&self) -> String {
        let items: Vec<_> = self
            .iter()
            .cloned()
            .map(|v| v.into_value().repr().to_string())
            .collect();
        super::builder::format_array(items)
    }
}

impl<T: IntoValue + Clone> ToTypst for Vec<T> {
    fn to_typst(&self) -> String {
        self.as_slice().to_typst()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string() {
        assert_eq!("hello".to_typst(), r#""hello""#);
        assert_eq!("with \"quotes\"".to_typst(), r#""with \"quotes\"""#);
    }

    #[test]
    fn test_numbers() {
        assert_eq!(42i64.to_typst(), "42");
        assert_eq!(3.14f64.to_typst(), "3.14");
    }

    #[test]
    fn test_bool() {
        assert_eq!(true.to_typst(), "true");
        assert_eq!(false.to_typst(), "false");
    }

    #[test]
    fn test_option() {
        assert_eq!(Some("hello").to_typst(), r#""hello""#);
        assert_eq!(None::<String>.to_typst(), "none");
    }

    #[test]
    fn test_array() {
        let items: Vec<&str> = vec!["a", "b", "c"];
        assert_eq!(items.to_typst(), r#"("a", "b", "c")"#);
    }

    #[test]
    fn test_array_single() {
        let items: Vec<&str> = vec!["only"];
        assert_eq!(items.to_typst(), r#"("only",)"#);
    }

    #[test]
    fn test_array_empty() {
        let items: Vec<&str> = vec![];
        assert_eq!(items.to_typst(), "()");
    }
}
