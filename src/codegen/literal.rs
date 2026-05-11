//! Typst literal parsing from strings.
//!
//! When Typst values are serialized to JSON, some types lose their type information:
//! - `12pt` becomes `"12pt"` (string)
//! - `90deg` becomes `"90deg"` (string)
//! - `50%` becomes `"50%"` (string)
//! - `#ff0000` becomes `"#ff0000"` (string)
//!
//! This module provides parsers to recover these types from their string representations.

use typst::foundations::Value;
use typst::layout::{Abs, Angle, Em, Length, Ratio};
use typst::visualize::Color;

/// Try to parse a string as a Typst literal value.
///
/// Returns `Some(Value)` if the string matches a known literal pattern,
/// or `None` if it should be treated as a plain string.
///
/// # Supported Literals
///
/// - Length: `12pt`, `1em`, `2cm`, `3mm`, `4in`
/// - Angle: `90deg`, `1.5rad`, `0.25turn`
/// - Ratio: `50%`
/// - Color: `#rgb`, `#rrggbb`, `#rrggbbaa`
/// - Special: `auto`, `none`, `true`, `false`
pub fn parse_typst_literal(s: &str) -> Option<Value> {
    let s = s.trim();

    // Special values
    match s {
        "auto" => return Some(Value::Auto),
        "none" => return Some(Value::None),
        "true" => return Some(Value::Bool(true)),
        "false" => return Some(Value::Bool(false)),
        _ => {}
    }

    // Try each parser in order
    if let Some(length) = parse_length(s) {
        return Some(Value::Length(length));
    }

    if let Some(angle) = parse_angle(s) {
        return Some(Value::Angle(angle));
    }

    if let Some(ratio) = parse_ratio(s) {
        return Some(Value::Ratio(ratio));
    }

    if let Some(color) = parse_color(s) {
        return Some(Value::Color(color));
    }

    None
}

/// Parse a length literal.
///
/// Supports:
/// - Absolute units: `pt`, `mm`, `cm`, `in`
/// - Relative units: `em`
///
/// # Examples
/// ```ignore
/// parse_length("12pt") // Some(Length::from(Abs::pt(12.0)))
/// parse_length("1.5em") // Some(Length::from(Em::new(1.5)))
/// ```
pub fn parse_length(s: &str) -> Option<Length> {
    let s = s.trim();

    // Absolute units (pt, mm, cm, in)
    // Conversion factors to points:
    // - 1pt = 1pt
    // - 1mm = 2.834645669291339pt
    // - 1cm = 28.34645669291339pt
    // - 1in = 72pt
    for (suffix, factor) in [
        ("pt", 1.0),
        ("mm", 2.834_645_669_291_339),
        ("cm", 28.346_456_692_913_39),
        ("in", 72.0),
    ] {
        if let Some(num_str) = s.strip_suffix(suffix)
            && let Ok(n) = num_str.trim().parse::<f64>()
        {
            return Some(Abs::pt(n * factor).into());
        }
    }

    // Relative unit: em
    if let Some(num_str) = s.strip_suffix("em")
        && let Ok(n) = num_str.trim().parse::<f64>()
    {
        return Some(Em::new(n).into());
    }

    None
}

/// Parse an angle literal.
///
/// Supports: `deg`, `rad`, `turn`
///
/// # Examples
/// ```ignore
/// parse_angle("90deg") // Some(Angle::deg(90.0))
/// parse_angle("3.14rad") // Some(Angle::rad(3.14))
/// ```
pub fn parse_angle(s: &str) -> Option<Angle> {
    let s = s.trim();

    if let Some(num_str) = s.strip_suffix("deg")
        && let Ok(n) = num_str.trim().parse::<f64>()
    {
        return Some(Angle::deg(n));
    }

    if let Some(num_str) = s.strip_suffix("rad")
        && let Ok(n) = num_str.trim().parse::<f64>()
    {
        return Some(Angle::rad(n));
    }

    if let Some(num_str) = s.strip_suffix("turn")
        && let Ok(n) = num_str.trim().parse::<f64>()
    {
        // 1 turn = 360 degrees
        return Some(Angle::deg(n * 360.0));
    }

    None
}

/// Parse a ratio literal.
///
/// Supports: `%`
///
/// # Examples
/// ```ignore
/// parse_ratio("50%") // Some(Ratio::new(0.5))
/// parse_ratio("100%") // Some(Ratio::new(1.0))
/// ```
pub fn parse_ratio(s: &str) -> Option<Ratio> {
    let s = s.trim();

    if let Some(num_str) = s.strip_suffix('%')
        && let Ok(n) = num_str.trim().parse::<f64>()
    {
        return Some(Ratio::new(n / 100.0));
    }

    None
}

/// Parse a color literal.
///
/// Supports hex colors:
/// - `#rgb` (3 digits)
/// - `#rrggbb` (6 digits)
/// - `#rrggbbaa` (8 digits)
///
/// # Examples
/// ```ignore
/// parse_color("#f00") // Some(Color::from_u8(255, 0, 0, 255))
/// parse_color("#ff0000") // Some(Color::from_u8(255, 0, 0, 255))
/// ```
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();

    if !s.starts_with('#') {
        return None;
    }

    let hex = &s[1..];

    match hex.len() {
        // #rgb → expand to #rrggbb
        3 => {
            let chars: Vec<char> = hex.chars().collect();
            let r = parse_hex_digit(chars[0])? * 17;
            let g = parse_hex_digit(chars[1])? * 17;
            let b = parse_hex_digit(chars[2])? * 17;
            Some(Color::from_u8(r, g, b, 255))
        }
        // #rrggbb
        6 => {
            let r = parse_hex_byte(&hex[0..2])?;
            let g = parse_hex_byte(&hex[2..4])?;
            let b = parse_hex_byte(&hex[4..6])?;
            Some(Color::from_u8(r, g, b, 255))
        }
        // #rrggbbaa
        8 => {
            let r = parse_hex_byte(&hex[0..2])?;
            let g = parse_hex_byte(&hex[2..4])?;
            let b = parse_hex_byte(&hex[4..6])?;
            let a = parse_hex_byte(&hex[6..8])?;
            Some(Color::from_u8(r, g, b, a))
        }
        _ => None,
    }
}

fn parse_hex_digit(c: char) -> Option<u8> {
    match c {
        '0'..='9' => Some(c as u8 - b'0'),
        'a'..='f' => Some(c as u8 - b'a' + 10),
        'A'..='F' => Some(c as u8 - b'A' + 10),
        _ => None,
    }
}

fn parse_hex_byte(s: &str) -> Option<u8> {
    u8::from_str_radix(s, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_length_pt() {
        let length = parse_length("12pt").unwrap();
        assert_eq!(length, Abs::pt(12.0).into());
    }

    #[test]
    fn test_parse_length_em() {
        let length = parse_length("1.5em").unwrap();
        assert_eq!(length, Em::new(1.5).into());
    }

    #[test]
    fn test_parse_length_mm() {
        let length = parse_length("10mm").unwrap();
        // 10mm ≈ 28.35pt
        // Length struct has abs and em fields
        let Length { abs, em: _ } = length;
        assert!((abs.to_pt() - 28.346_456_692_913_39).abs() < 0.001);
    }

    #[test]
    fn test_parse_angle_deg() {
        let angle = parse_angle("90deg").unwrap();
        assert_eq!(angle, Angle::deg(90.0));
    }

    #[test]
    fn test_parse_angle_rad() {
        let angle = parse_angle("3.14159rad").unwrap();
        assert!((angle.to_rad() - 3.14159).abs() < 0.0001);
    }

    #[test]
    fn test_parse_ratio() {
        let ratio = parse_ratio("50%").unwrap();
        assert_eq!(ratio, Ratio::new(0.5));
    }

    #[test]
    fn test_parse_color_short() {
        let color = parse_color("#f00").unwrap();
        assert_eq!(color, Color::from_u8(255, 0, 0, 255));
    }

    #[test]
    fn test_parse_color_long() {
        let color = parse_color("#ff0000").unwrap();
        assert_eq!(color, Color::from_u8(255, 0, 0, 255));
    }

    #[test]
    fn test_parse_color_with_alpha() {
        let color = parse_color("#ff000080").unwrap();
        assert_eq!(color, Color::from_u8(255, 0, 0, 128));
    }

    #[test]
    fn test_parse_special_values() {
        assert_eq!(parse_typst_literal("auto"), Some(Value::Auto));
        assert_eq!(parse_typst_literal("none"), Some(Value::None));
        assert_eq!(parse_typst_literal("true"), Some(Value::Bool(true)));
        assert_eq!(parse_typst_literal("false"), Some(Value::Bool(false)));
    }

    #[test]
    fn test_plain_string() {
        // These should NOT be parsed as literals
        assert!(parse_typst_literal("hello").is_none());
        assert!(parse_typst_literal("12").is_none()); // No unit
        assert!(parse_typst_literal("just some text").is_none());
    }
}
