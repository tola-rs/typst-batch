//! Edge case and error handling tests.

use serde_json::json;

use crate::codegen::{content_to_json, json_to_content};

use super::common::{TestEnv, assert_typst_roundtrip};

#[test]
fn deeply_nested() {
    let env = TestEnv::new();
    // 5 levels of nesting
    assert_typst_roundtrip(&env, r#"*_#link("https://example.com")[deep nested]_*"#);
}

#[test]
fn invalid_json() {
    let env = TestEnv::new();

    env.run(|engine, context, library| {
        // Missing "func" field
        assert!(json_to_content(engine, context, library, &json!({"text": "hello"})).is_err());

        // Unknown function
        assert!(
            json_to_content(engine, context, library, &json!({"func": "nonexistent"})).is_err()
        );

        // Not an object
        assert!(json_to_content(engine, context, library, &json!("string")).is_err());
        assert!(json_to_content(engine, context, library, &json!(123)).is_err());
        assert!(json_to_content(engine, context, library, &json!(null)).is_err());
    });
}

#[test]
fn strip_nulls_behavior() {
    use typst::foundations::NativeElement;
    use typst::math::RootElem;
    use typst::text::TextElem;

    // RootElem without index should not have "index" field in JSON
    let root = RootElem::new(TextElem::packed("x")).pack();
    let json = content_to_json(&root);

    // Verify "index" is not present (stripped because it's null)
    assert!(json.get("index").is_none(), "index should be stripped");
    assert!(json.get("radicand").is_some(), "radicand should be present");
}
