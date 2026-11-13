//! The schemas validation reads (`model::check`), compiled in as it reads them: each committed
//! schema in `docs/schema`, written without its spaces and without the descriptions of its
//! properties, which no check reads. A definition keeps its own description, which a value that
//! fails it whole is told (SPEC §15: the editor's module carries every byte it compiles in).

use serde_json::Value;
use std::path::Path;

fn main() {
    let out = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    // The crate's folder as Cargo runs this script, never as it compiled it: one target folder
    // shared by several checkouts (worktrees) builds this script once for all of them, and a path
    // compiled in (`env!`) names the first, which may be gone. Cargo reads a relative
    // `rerun-if-changed` from the crate's folder too, so each checkout's own schemas are watched.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let root = Path::new(&manifest).join("../../docs/schema");
    for name in ["deck", "theme", "spine"] {
        let path = root.join(format!("{name}.schema.json"));
        println!("cargo:rerun-if-changed=../../docs/schema/{name}.schema.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut schema: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        // Each definition's own description, kept.
        let kept: Vec<(String, Value)> = (schema.get("$defs").and_then(Value::as_object).into_iter().flatten())
            .filter_map(|(def, body)| Some((def.clone(), body.get("description")?.clone())))
            .collect();
        unannotated(&mut schema);
        for (def, description) in kept {
            if let Some(Value::Object(body)) = schema.get_mut("$defs").and_then(|d| d.get_mut(&def)) {
                body.insert("description".into(), description);
            }
        }
        let written = serde_json::to_string(&schema).expect("a schema writes");
        std::fs::write(Path::new(&out).join(format!("{name}.schema.json")), written).expect("OUT_DIR is writable");
    }
}

/// `schema` without its descriptions, at any depth. A property's name is no annotation: a map of
/// names to schemas (`properties`, `$defs`) keeps each name, and loses each schema's description.
fn unannotated(schema: &mut Value) {
    match schema {
        Value::Object(map) => {
            map.remove("description");
            for (key, value) in map.iter_mut() {
                match (key.as_str(), value) {
                    ("properties" | "$defs" | "patternProperties" | "dependentSchemas", Value::Object(named)) => {
                        named.values_mut().for_each(unannotated)
                    }
                    (_, value) => unannotated(value),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(unannotated),
        _ => {}
    }
}
