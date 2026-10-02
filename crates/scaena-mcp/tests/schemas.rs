//! Each tool's input and output schemas are generated from the Rust types (SPEC §7.2) and
//! committed in `docs/schema/mcp/<tool>.json`: this test fails when they are not what the
//! server lists. After a reviewed change: `SCAENA_BLESS=1 cargo test -p scaena-mcp --test schemas`.

use scaena_core::model::print_schema;
use std::path::PathBuf;

#[test]
fn committed_tool_schemas_are_the_generated_ones() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/schema/mcp");
    let bless = std::env::var_os("SCAENA_BLESS").is_some();
    let mut names = Vec::new();
    let mut stale = Vec::new();
    for tool in scaena_mcp::tools() {
        let name = tool.name.to_string();
        let mut v = serde_json::json!({
            "name": name,
            "description": tool.description,
            "inputSchema": tool.input_schema,
        });
        // `deck_render` returns an image, and has none.
        if let Some(output) = &tool.output_schema {
            v["outputSchema"] = serde_json::json!(output);
        }
        let path = dir.join(format!("{name}.json"));
        let generated = print_schema(&v);
        names.push(format!("{name}.json"));
        if bless {
            std::fs::write(&path, &generated).unwrap();
        } else if std::fs::read_to_string(&path).unwrap_or_default() != generated {
            stale.push(path.display().to_string());
        }
    }
    // No schema for a tool the server does not list.
    let mut committed: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    committed.sort();
    names.sort();
    assert_eq!(committed, names, "docs/schema/mcp/ holds a schema for each tool, and no other");
    assert!(stale.is_empty(), "not what the server lists: {stale:?}. Bless with SCAENA_BLESS=1 and review the diff.");
}
