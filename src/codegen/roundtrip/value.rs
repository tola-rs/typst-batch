//! json_to_value tests.

use serde_json::json;
use typst::foundations::Value;

use crate::codegen::json_to_value;

use super::common::TestEnv;

#[test]
fn primitives() {
    let env = TestEnv::new();

    env.run(|engine, context, library| {
        // Null
        let v = json_to_value(engine, context, library, &json!(null)).unwrap();
        assert!(matches!(v, Value::None));

        // Bool
        let v = json_to_value(engine, context, library, &json!(true)).unwrap();
        assert!(matches!(v, Value::Bool(true)));

        // Int
        let v = json_to_value(engine, context, library, &json!(42)).unwrap();
        assert!(matches!(v, Value::Int(42)));

        // Float
        let v = json_to_value(engine, context, library, &json!(3.14)).unwrap();
        if let Value::Float(f) = v {
            assert!((f - 3.14).abs() < 0.001);
        } else {
            panic!("Expected Float");
        }

        // String
        let v = json_to_value(engine, context, library, &json!("hello")).unwrap();
        if let Value::Str(s) = v {
            assert_eq!(s.as_str(), "hello");
        } else {
            panic!("Expected Str");
        }
    });
}

#[test]
fn array() {
    let env = TestEnv::new();

    env.run(|engine, context, library| {
        let v = json_to_value(engine, context, library, &json!([1, 2, 3])).unwrap();
        if let Value::Array(arr) = v {
            assert_eq!(arr.len(), 3);
        } else {
            panic!("Expected Array");
        }
    });
}

#[test]
fn dict() {
    let env = TestEnv::new();

    env.run(|engine, context, library| {
        // Object without "func" becomes Dict
        let v = json_to_value(engine, context, library, &json!({"a": 1, "b": "two"})).unwrap();
        if let Value::Dict(dict) = v {
            assert_eq!(dict.len(), 2);
        } else {
            panic!("Expected Dict");
        }
    });
}

#[test]
fn content() {
    let env = TestEnv::new();

    env.run(|engine, context, library| {
        // Object with "func" becomes Content
        let v = json_to_value(engine, context, library, &json!({"func": "text", "text": "hi"})).unwrap();
        assert!(matches!(v, Value::Content(_)));
    });
}

/// Test that Content can be injected via sys.inputs and used in Typst.
#[test]
fn content_in_sys_inputs() {
    use std::fs;
    use tempfile::TempDir;
    use typst::foundations::{Dict, IntoValue};
    use typst::text::TextElem;
    use typst::foundations::Content;

    use std::sync::Arc;

    use crate::resource::font::FontStore;
    use crate::world::TypstWorld;
    use crate::process::compile::compile_with_world;

    // Create Content: "Hello World"
    let content = Content::sequence([
        TextElem::packed("Hello "),
        TextElem::packed("World"),
    ]);

    // Put Content in sys.inputs
    let mut inputs = Dict::new();
    inputs.insert("greeting".into(), content.into_value());

    // Create temp file
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("test.typ");
    fs::write(&file, r#"
        #let greeting = sys.inputs.greeting
        #greeting
    "#).unwrap();

    // Build world with custom inputs
    let world = TypstWorld::builder(&file, dir.path())
        .with_local_cache()
        .with_fonts(Arc::new(FontStore::new()))
        .with_inputs_dict(inputs)
        .build();

    // Compile
    let result = compile_with_world(&world);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    // Verify output contains "Hello World"
    let html_bytes = result.unwrap().html().expect("HTML export failed");
    let html_str = String::from_utf8_lossy(&html_bytes);
    assert!(
        html_str.contains("Hello") && html_str.contains("World"),
        "Output should contain 'Hello World', got: {}",
        html_str
    );
}

/// Test full flow: json_to_content() → sys.inputs → Typst render
///
/// This tests the dynamic Content injection scenario:
/// - Use json_to_content() to rebuild Content from JSON
/// - Inject Content into sys.inputs
/// - Compile Typst file that uses the injected Content
#[test]
fn json_to_content_then_inject() {
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;
    use serde_json::json;
    use typst::foundations::{Dict, IntoValue};

    use crate::codegen::json_to_content;
    use crate::resource::font::FontStore;
    use crate::world::TypstWorld;
    use crate::process::compile::compile_with_world;

    // Create a temporary World for json_to_content()
    let dir = TempDir::new().unwrap();
    let dummy_file = dir.path().join("dummy.typ");
    fs::write(&dummy_file, "").unwrap();

    let temp_world = TypstWorld::builder(&dummy_file, dir.path())
        .with_local_cache()
        .no_fonts()
        .build();

    // Use TestEnv-style Engine creation to call json_to_content()
    let content = {
        use typst::comemo::Track;
        use typst::engine::{Engine, Route, Sink, Traced};
        use typst::foundations::Context;
        use typst::introspection::Introspector;
        use typst::World;

        let introspector = Introspector::default();
        let traced = Traced::default();
        let mut sink = Sink::new();

        let mut engine = Engine {
            world: (&temp_world as &dyn World).track(),
            introspector: introspector.track(),
            traced: traced.track(),
            sink: sink.track_mut(),
            route: Route::default(),
            routines: &typst::ROUTINES,
        };

        let library = temp_world.library();
        let context = Context::none();

        // JSON representing: "Check out " + link("https://example.com", "this link") + "!"
        let json = json!({
            "func": "sequence",
            "children": [
                {"func": "text", "text": "Check out "},
                {"func": "link", "dest": "https://example.com", "body": {"func": "text", "text": "this link"}},
                {"func": "text", "text": "!"}
            ]
        });

        json_to_content(&mut engine, context.track(), library, &json)
            .expect("json_to_content failed")
    };

    // Put Content in sys.inputs
    let mut inputs = Dict::new();
    inputs.insert("summary".into(), content.into_value());

    // Create actual file to compile
    let real_file = dir.path().join("test.typ");
    fs::write(&real_file, r#"
        #let summary = sys.inputs.summary
        Summary: #summary
    "#).unwrap();

    // Build world with inputs and compile
    let world = TypstWorld::builder(&real_file, dir.path())
        .with_local_cache()
        .with_fonts(Arc::new(FontStore::new()))
        .with_inputs_dict(inputs)
        .build();

    let result = compile_with_world(&world);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    // Verify output
    let html_bytes = result.unwrap().html().expect("HTML export failed");
    let html_str = String::from_utf8_lossy(&html_bytes);

    // Should contain the text and the link
    assert!(
        html_str.contains("Check out") && html_str.contains("this link") && html_str.contains("example.com"),
        "Output should contain the summary with link, got: {}",
        html_str
    );
}
