//! Complex nested structure tests.

use super::common::{TestEnv, assert_typst_roundtrip};

#[test]
fn nested() {
    let env = TestEnv::new();
    assert_typst_roundtrip(
        &env,
        r#"See #link("https://example.com")[*this*] for details."#,
    );
}

#[test]
fn math_expression() {
    let env = TestEnv::new();
    assert_typst_roundtrip(&env, r#"$x^2 / y^2$"#);
}
