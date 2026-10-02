//! `docs/schema/deck.schema.json` and `theme.schema.json` are generated from the typed model
//! (PLAN 1.1): this test fails when the committed files are not what the model generates.
//! After a reviewed change to the model: `SCAENA_BLESS=1 cargo test -p scaena-core --test schemas`
//! (or `just bless`), and review the schema diff with the code.

use scaena_core::model::{deck_schema, print_schema, theme_schema};
use std::path::PathBuf;

#[test]
fn committed_schemas_are_the_generated_ones() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/schema");
    let bless = std::env::var_os("SCAENA_BLESS").is_some();
    let mut stale = Vec::new();
    for (name, schema) in [("deck", deck_schema()), ("theme", theme_schema())] {
        let path = dir.join(format!("{name}.schema.json"));
        let generated = print_schema(&schema);
        if bless {
            std::fs::write(&path, &generated).unwrap();
            continue;
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        if committed != generated {
            let line = committed.lines().zip(generated.lines()).position(|(a, b)| a != b);
            let line = line.unwrap_or(committed.lines().count().min(generated.lines().count())) + 1;
            stale.push(format!("{} (first difference at line {line})", path.display()));
        }
    }
    assert!(
        stale.is_empty(),
        "not what the model generates: {stale:?}. Bless with SCAENA_BLESS=1 and review the diff."
    );
}

#[test]
fn every_definition_is_used() {
    for (name, schema) in [("deck", deck_schema()), ("theme", theme_schema())] {
        let text = schema.to_string();
        for def in schema["$defs"].as_object().unwrap().keys() {
            assert!(text.contains(&format!("\"#/$defs/{def}\"")), "{name}: $defs/{def} is never referenced");
        }
    }
}
