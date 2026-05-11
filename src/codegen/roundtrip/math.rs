//! Math element tests: equation, frac, root, attach.

use typst::foundations::NativeElement;
use typst::math::{AttachElem, EquationElem, FracElem, RootElem};
use typst::text::TextElem;

use super::common::{TestEnv, assert_content_roundtrip, assert_typst_roundtrip};

#[test]
fn equation() {
    let env = TestEnv::new();
    assert_typst_roundtrip(&env, "$x$");
}

#[test]
fn frac() {
    let env = TestEnv::new();
    assert_typst_roundtrip(&env, "$1/2$");
}

#[test]
fn root() {
    let env = TestEnv::new();
    assert_typst_roundtrip(&env, "$sqrt(x)$");
}

#[test]
fn root_with_index() {
    let env = TestEnv::new();
    assert_typst_roundtrip(&env, "$root(3, x)$");
}

#[test]
fn attach() {
    let env = TestEnv::new();
    assert_typst_roundtrip(&env, "$x^2$");
}

#[test]
fn attach_full() {
    let env = TestEnv::new();
    assert_typst_roundtrip(&env, "$x_1^2$");
}

#[test]
fn from_rust() {
    let env = TestEnv::new();

    assert_content_roundtrip(&env, EquationElem::new(TextElem::packed("x")).pack());

    assert_content_roundtrip(
        &env,
        FracElem::new(TextElem::packed("1"), TextElem::packed("2")).pack(),
    );

    assert_content_roundtrip(&env, RootElem::new(TextElem::packed("x")).pack());

    assert_content_roundtrip(
        &env,
        AttachElem::new(TextElem::packed("x"))
            .with_t(Some(TextElem::packed("2")))
            .pack(),
    );
}
