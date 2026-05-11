//! Test infrastructure for roundtrip tests.

use std::fs;
use tempfile::TempDir;

use serde_json::Value as JsonValue;
use typst::World;
use typst::comemo::Track;
use typst::engine::{Engine, Route, Sink, Traced};
use typst::foundations::{Content, Context};
use typst::introspection::Introspector;

use crate::Scanner;
use crate::codegen::{content_to_json, json_to_content};
use crate::world::TypstWorld;

/// Test environment providing Engine, Context, and Library.
pub struct TestEnv {
    _dir: TempDir,
    world: TypstWorld,
}

impl TestEnv {
    pub fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.typ");
        fs::write(&file, "").unwrap();

        let world = TypstWorld::builder(&file, dir.path())
            .with_local_cache()
            .no_fonts()
            .build();

        Self { _dir: dir, world }
    }

    pub fn run<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Engine, typst::comemo::Tracked<Context>, &typst::Library) -> R,
    {
        let introspector = Introspector::default();
        let traced = Traced::default();
        let mut sink = Sink::new();

        let mut engine = Engine {
            world: (&self.world as &dyn typst::World).track(),
            introspector: introspector.track(),
            traced: traced.track(),
            sink: sink.track_mut(),
            route: Route::default(),
            routines: &typst::ROUTINES,
        };

        let library = self.world.library();
        let context = Context::none();

        f(&mut engine, context.track(), library)
    }
}

/// Compare JSON values, ignoring key order.
pub fn json_eq(a: &JsonValue, b: &JsonValue) -> bool {
    match (a, b) {
        (JsonValue::Object(a), JsonValue::Object(b)) => {
            let keys_a: std::collections::HashSet<_> = a.keys().collect();
            let keys_b: std::collections::HashSet<_> = b.keys().collect();
            keys_a == keys_b && keys_a.iter().all(|k| json_eq(&a[*k], &b[*k]))
        }
        (JsonValue::Array(a), JsonValue::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| json_eq(x, y))
        }
        _ => a == b,
    }
}

/// Test JSON → Content → JSON roundtrip.
pub fn assert_roundtrip(env: &TestEnv, json: JsonValue) {
    env.run(|engine, context, library| {
        let content =
            json_to_content(engine, context, library, &json).expect("json_to_content failed");
        let result = content_to_json(&content);

        assert!(
            json_eq(&json, &result),
            "Roundtrip mismatch:\nInput:  {}\nOutput: {}",
            serde_json::to_string_pretty(&json).unwrap(),
            serde_json::to_string_pretty(&result).unwrap()
        );
    });
}

/// Test Content → JSON → Content → JSON roundtrip.
pub fn assert_content_roundtrip(env: &TestEnv, content: Content) {
    env.run(|engine, context, library| {
        let json1 = content_to_json(&content);
        let content2 =
            json_to_content(engine, context, library, &json1).expect("json_to_content failed");
        let json2 = content_to_json(&content2);

        assert!(
            json_eq(&json1, &json2),
            "Content roundtrip mismatch:\nFirst:  {}\nSecond: {}",
            serde_json::to_string_pretty(&json1).unwrap(),
            serde_json::to_string_pretty(&json2).unwrap()
        );
    });
}

/// Compile Typst source code and return Content.
pub fn compile_typst(source: &str) -> Content {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("test.typ");
    fs::write(&file, source).unwrap();

    Scanner::new(dir.path())
        .scan(&file)
        .expect("Typst compilation failed")
        .content()
        .clone()
}

/// Test Typst source → Content → JSON → Content → JSON roundtrip.
pub fn assert_typst_roundtrip(env: &TestEnv, source: &str) {
    let content = compile_typst(source);
    assert_content_roundtrip(env, content);
}
