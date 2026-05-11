//! Model element tests: strong, emph, link, heading, grid, table.

use serde_json::json;

use crate::codegen::content_to_json;

use super::common::{TestEnv, assert_typst_roundtrip, compile_typst};

#[test]
fn formatting() {
    let env = TestEnv::new();

    // Test strong (bold)
    assert_typst_roundtrip(&env, "*bold*");

    // Test emph (italic)
    assert_typst_roundtrip(&env, "_italic_");

    // Combined formatting
    assert_typst_roundtrip(&env, "*_bold italic_*");
}

#[test]
fn link() {
    let env = TestEnv::new();

    // Simple link
    assert_typst_roundtrip(&env, r#"#link("https://example.com")[click]"#);

    // Link with nested formatting
    assert_typst_roundtrip(&env, r#"#link("https://example.com")[*bold link*]"#);
}

#[test]
fn heading() {
    let env = TestEnv::new();

    assert_typst_roundtrip(&env, "= Title");
    assert_typst_roundtrip(&env, "== Subtitle");
    assert_typst_roundtrip(&env, "=== Level 3");
}

#[test]
fn grid_basic() {
    let env = TestEnv::new();

    // Debug: print the JSON structure
    let content = compile_typst(
        r#"#grid(
  columns: 2,
  [A], [B],
  [C], [D]
)"#,
    );
    let json = content_to_json(&content);
    println!(
        "Grid JSON: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );

    // Simple grid with cells
    assert_typst_roundtrip(
        &env,
        r#"#grid(
  columns: 2,
  [A], [B],
  [C], [D]
)"#,
    );
}

#[test]
fn table_basic() {
    let env = TestEnv::new();

    // Simple table without columns parameter
    assert_typst_roundtrip(
        &env,
        r#"#table(
  [A], [B]
)"#,
    );
}

#[test]
fn grid_with_explicit_cells() {
    let env = TestEnv::new();

    // Grid with explicit grid.cell
    assert_typst_roundtrip(
        &env,
        r#"#grid(
  grid.cell[A],
  grid.cell[B]
)"#,
    );
}

#[test]
fn table_with_explicit_cells() {
    let env = TestEnv::new();

    // Table with explicit table.cell
    assert_typst_roundtrip(
        &env,
        r#"#table(
  table.cell[A],
  table.cell[B]
)"#,
    );
}

#[test]
fn grid_with_header() {
    let env = TestEnv::new();

    // Grid with header containing cells
    assert_typst_roundtrip(
        &env,
        r#"#grid(
  columns: 2,
  grid.header(
    grid.cell[H1], grid.cell[H2]
  ),
  [A], [B]
)"#,
    );
}

#[test]
fn table_with_header() {
    let env = TestEnv::new();

    // Table with header containing cells
    assert_typst_roundtrip(
        &env,
        r#"#table(
  columns: 2,
  table.header(
    table.cell[H1], table.cell[H2]
  ),
  [A], [B]
)"#,
    );
}

#[test]
fn grid_with_footer() {
    let env = TestEnv::new();

    // Grid with footer
    assert_typst_roundtrip(
        &env,
        r#"#grid(
  columns: 2,
  [A], [B],
  grid.footer(
    grid.cell[F1], grid.cell[F2]
  )
)"#,
    );
}

#[test]
fn table_with_footer() {
    let env = TestEnv::new();

    // Table with footer
    assert_typst_roundtrip(
        &env,
        r#"#table(
  columns: 2,
  [A], [B],
  table.footer(
    table.cell[F1], table.cell[F2]
  )
)"#,
    );
}

#[test]
fn grid_with_hline_vline() {
    let env = TestEnv::new();

    // Grid with hline and vline
    assert_typst_roundtrip(
        &env,
        r#"#grid(
  columns: 2,
  grid.hline(),
  [A], [B],
  grid.hline(),
  [C], [D],
  grid.vline(x: 1)
)"#,
    );
}

#[test]
fn table_with_hline_vline() {
    let env = TestEnv::new();

    // Table with hline and vline
    assert_typst_roundtrip(
        &env,
        r#"#table(
  columns: 2,
  table.hline(),
  [A], [B],
  table.hline(),
  [C], [D],
  table.vline(x: 1)
)"#,
    );
}

#[test]
fn cell_type_ambiguity() {
    // grid.cell and table.cell both serialize to "cell"
    // Test: context-aware deserialization resolves the correct cell type

    let env = TestEnv::new();

    // Test grid roundtrip with cell
    let grid_json = json!({
        "func": "grid",
        "children": [
            {"func": "cell", "body": {"func": "text", "text": "A"}}
        ]
    });

    env.run(|engine, context, library| {
        use crate::codegen::json_to_content;

        let restored = json_to_content(engine, context, library, &grid_json).unwrap();
        assert_eq!(restored.elem().name(), "grid");

        // Verify roundtrip produces identical JSON
        let back_json = content_to_json(&restored);
        assert_eq!(grid_json, back_json);
    });

    // Test table roundtrip with cell
    let table_json = json!({
        "func": "table",
        "children": [
            {"func": "cell", "body": {"func": "text", "text": "B"}}
        ]
    });

    env.run(|engine, context, library| {
        use crate::codegen::json_to_content;

        let restored = json_to_content(engine, context, library, &table_json).unwrap();
        assert_eq!(restored.elem().name(), "table");

        // Verify roundtrip produces identical JSON
        let back_json = content_to_json(&restored);
        assert_eq!(table_json, back_json);
    });
}

#[test]
fn nested_sub_elements() {
    // Test: grid.header contains grid.cell (nested sub-elements)
    // The cell inside header should still be grid.cell, not table.cell

    let env = TestEnv::new();

    let grid_with_header = json!({
        "func": "grid",
        "children": [
            {
                "func": "header",
                "children": [
                    {"func": "cell", "body": {"func": "text", "text": "Header Cell"}}
                ]
            },
            {"func": "cell", "body": {"func": "text", "text": "Body Cell"}}
        ]
    });

    env.run(|engine, context, library| {
        use crate::codegen::json_to_content;

        let restored = json_to_content(engine, context, library, &grid_with_header).unwrap();
        assert_eq!(restored.elem().name(), "grid");

        // Verify roundtrip produces identical JSON
        let back_json = content_to_json(&restored);
        assert_eq!(grid_with_header, back_json);
    });

    // Also test table with header
    let table_with_header = json!({
        "func": "table",
        "children": [
            {
                "func": "header",
                "children": [
                    {"func": "cell", "body": {"func": "text", "text": "Header Cell"}}
                ]
            },
            {"func": "cell", "body": {"func": "text", "text": "Body Cell"}}
        ]
    });

    env.run(|engine, context, library| {
        use crate::codegen::json_to_content;

        let restored = json_to_content(engine, context, library, &table_with_header).unwrap();
        assert_eq!(restored.elem().name(), "table");

        // Verify roundtrip produces identical JSON
        let back_json = content_to_json(&restored);
        assert_eq!(table_with_header, back_json);
    });
}

#[test]
fn cell_inside_non_grid_context() {
    // Test: cell inside a non-grid/table context (e.g., strong > cell)
    // This should fall back to global lookup (table.cell)

    let env = TestEnv::new();

    // A cell wrapped in strong - not a typical use case, but tests fallback behavior
    let json = json!({
        "func": "strong",
        "body": {"func": "cell", "body": {"func": "text", "text": "test"}}
    });

    env.run(|engine, context, library| {
        use crate::codegen::json_to_content;

        // This should work - falls back to table.cell (first match in global lookup)
        let result = json_to_content(engine, context, library, &json);
        if let Err(e) = &result {
            println!("Error: {:?}", e);
        }
        assert!(result.is_ok());
    });
}

#[test]
fn list_items() {
    let env = TestEnv::new();

    // Test 1: Explicit #list() - works because item has parent context
    assert_typst_roundtrip(&env, r#"#list([Item 1], [Item 2])"#);

    // Test 2: Explicit #enum() - works because item has parent context
    assert_typst_roundtrip(&env, r#"#enum([First], [Second])"#);

    // Test 3: Explicit #terms() - works because item has parent context
    assert_typst_roundtrip(&env, r#"#terms(terms.item([Term], [Definition]))"#);

    // Test 4: Markdown-style list (- xxx) - also works!
    // The item elements don't have list wrapper, but roundtrip still works
    // because list.item and enum.item have the same JSON structure
    let md_list = compile_typst(
        r#"- Item 1
- Item 2"#,
    );
    let md_json = content_to_json(&md_list);
    env.run(|engine, context, library| {
        use crate::codegen::json_to_content;
        let restored = json_to_content(engine, context, library, &md_json).unwrap();
        let restored_json = content_to_json(&restored);
        assert_eq!(md_json, restored_json);
    });
}

#[test]
fn table_with_math() {
    let env = TestEnv::new();

    // Test: table.cell containing math
    assert_typst_roundtrip(
        &env,
        r#"#table(
  table.cell[$x^2$],
  table.cell[$sqrt(y)$]
)"#,
    );
}

#[test]
fn nested_complex() {
    let env = TestEnv::new();

    // Test: grid with header containing math
    assert_typst_roundtrip(
        &env,
        r#"#grid(
  columns: 2,
  grid.header(
    grid.cell[*Header 1*],
    grid.cell[$alpha$]
  ),
  grid.cell[A],
  grid.cell[$x + y$]
)"#,
    );
}

#[test]
fn show_rules() {
    let env = TestEnv::new();

    // Case 1: Content WITHOUT show rule - works
    println!("=== Without show rule ===");
    assert_typst_roundtrip(&env, r#"= Hello"#);
    println!("OK");

    // Case 2: Content WITH show rule - fails
    println!("\n=== With show rule ===");
    let content = compile_typst(
        r#"#show heading: it => text(red, it.body)

= Hello"#,
    );
    let json = content_to_json(&content);
    println!("JSON:\n{}", serde_json::to_string_pretty(&json).unwrap());

    env.run(|engine, context, library| {
        use crate::codegen::json_to_content;
        match json_to_content(engine, context, library, &json) {
            Ok(_) => println!("Roundtrip succeeded (unexpected)"),
            Err(e) => println!("Roundtrip failed: {:?}", e),
        }
    });
}

#[test]
fn terms() {
    use crate::codegen::lookup::find_element_funcs;

    let env = TestEnv::new();

    // Debug: check what "item" elements exist
    env.run(|_engine, _context, library| {
        let items: Vec<_> = find_element_funcs(library, "item").collect();
        for item in &items {
            println!("Found item: {:?}", item.name());
            if let Some(params) = item.params() {
                for p in params {
                    println!(
                        "  param: {} (positional: {}, required: {})",
                        p.name, p.positional, p.required
                    );
                }
            }
        }
    });

    // Debug: print the JSON structure
    let content = compile_typst(r#"/ Term: Definition"#);
    let json = content_to_json(&content);
    println!(
        "Terms JSON: {}",
        serde_json::to_string_pretty(&json).unwrap()
    );

    // Term list
    assert_typst_roundtrip(
        &env,
        r#"/ Term: Definition
/ Another: Description"#,
    );
}

#[test]
fn complex_nested() {
    let env = TestEnv::new();

    // Heading with formatting
    assert_typst_roundtrip(&env, "= *Bold* Heading");

    // Paragraph with mixed formatting
    assert_typst_roundtrip(&env, "Some *bold* and _italic_ text.");
}

#[test]
fn verify_cell_serialization_name() {
    // Verify that grid.cell serializes to "cell" (not "grid.cell")
    let content = compile_typst(r#"#grid(grid.cell[test])"#);
    let json = content_to_json(&content);

    // Find the cell in the JSON
    let children = json.get("children").and_then(|v| v.as_array());
    assert!(children.is_some());

    let cell = &children.unwrap()[0];
    assert_eq!(cell.get("func").and_then(|v| v.as_str()), Some("cell"));
}
