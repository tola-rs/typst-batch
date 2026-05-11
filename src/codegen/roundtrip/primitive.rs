//! Primitive element tests: text, space, sequence, linebreak, parbreak.

use serde_json::json;
use typst::foundations::{Content, NativeElement};
use typst::model::ParbreakElem;
use typst::text::{LinebreakElem, SpaceElem, TextElem};

use super::common::{TestEnv, assert_content_roundtrip, assert_roundtrip};

#[test]
fn text() {
    let env = TestEnv::new();
    assert_roundtrip(&env, json!({"func": "text", "text": "hello"}));
    assert_roundtrip(&env, json!({"func": "text", "text": ""}));
    assert_roundtrip(&env, json!({"func": "text", "text": "你好 🌍"}));
    assert_roundtrip(&env, json!({"func": "text", "text": "a\"b<c>&d"}));
}

#[test]
fn space() {
    let env = TestEnv::new();
    assert_roundtrip(&env, json!({"func": "space"}));
}

#[test]
fn linebreak() {
    let env = TestEnv::new();
    assert_content_roundtrip(&env, LinebreakElem::new().pack());
}

#[test]
fn parbreak() {
    let env = TestEnv::new();
    assert_content_roundtrip(&env, ParbreakElem::shared().clone());
}

#[test]
fn sequence() {
    let env = TestEnv::new();

    // Empty sequence
    assert_roundtrip(&env, json!({"func": "sequence", "children": []}));

    // Simple sequence
    assert_roundtrip(
        &env,
        json!({
            "func": "sequence",
            "children": [
                {"func": "text", "text": "A"},
                {"func": "space"},
                {"func": "text", "text": "B"}
            ]
        }),
    );

    // Nested sequence
    assert_roundtrip(
        &env,
        json!({
            "func": "sequence",
            "children": [
                {"func": "text", "text": "outer"},
                {
                    "func": "sequence",
                    "children": [
                        {"func": "text", "text": "inner1"},
                        {"func": "text", "text": "inner2"}
                    ]
                }
            ]
        }),
    );
}

#[test]
fn from_rust() {
    let env = TestEnv::new();
    assert_content_roundtrip(&env, TextElem::packed("test"));
    assert_content_roundtrip(&env, SpaceElem::shared().clone());
    assert_content_roundtrip(
        &env,
        Content::sequence([
            TextElem::packed("A"),
            SpaceElem::shared().clone(),
            TextElem::packed("B"),
        ]),
    );
}
