//! Code on its card (PLAN 2.136, ADR-0029), as lint judges it with a painter: the card is code's
//! ground, so it lies under its words in contrast's backdrop, and code on none that reads too
//! faintly over what lies behind it is fixed by its card.

use scaena_store::SaveOptions;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const CODE: &str = "../../docs/examples/code.deck.json";

/// The code example, saved where a test may write it, its deck changed by `edit`.
fn code(test: &str, edit: impl FnOnce(&mut Value)) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("code-card-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    let opts = SaveOptions { subset_fonts: false, now: "2026-10-25T00:00:00Z".into(), history: false };
    scaena_ops::open(Path::new(CODE)).unwrap().save(&dir, &opts).unwrap();
    let path = dir.join("deck.json");
    let mut deck: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    edit(&mut deck);
    std::fs::write(&path, serde_json::to_string_pretty(&deck).unwrap()).unwrap();
    dir
}

/// The contrast errors lint finds of `node` in the bundle at `dir`.
fn faint(dir: &Path, node: &str) -> Vec<scaena_core::Finding> {
    let found = scaena_ops::lint::lint(&scaena_ops::open(dir).unwrap()).unwrap().findings;
    found
        .into_iter()
        .filter(|f| matches!(f.code.as_str(), "E110" | "E111") && f.node.as_deref() == Some(node))
        .collect()
}

/// Dusk's code, over a rect in its light ink: its colors are held to the dark surface, and its
/// card, where it stands on one, is between them.
fn over_ink(card: bool) -> impl FnOnce(&mut Value) {
    move |deck: &mut Value| {
        let nodes = deck["nodes"].as_object_mut().unwrap();
        let mut ordered = serde_json::Map::new();
        for (id, node) in std::mem::take(nodes) {
            if id == "snippet" {
                ordered.insert(
                    "ink".into(),
                    json!({ "type": "shape", "kind": "rect", "fill": "onSurface", "at": { "in": "main" } }),
                );
            }
            ordered.insert(id, node);
        }
        *nodes = ordered;
        deck["nodes"]["snippet"]["code"]["card"] = json!(card);
        for state in deck["states"].as_array_mut().unwrap() {
            if state["mode"] != "delta" || state.get("props").is_some_and(|p| p.get("snippet").is_some()) {
                state["props"]["ink"] = json!({});
            }
        }
    }
}

#[test]
fn code_that_does_not_read_where_it_stands_is_put_on_its_card() {
    let bare = code("bare", over_ink(false));
    let found = faint(&bare, "snippet");
    let fix = found.first().and_then(|f| f.fix.clone()).expect("a contrast error on the bare code, fixed by its card");
    let carded =
        |op: &Value| op["path"].as_str().is_some_and(|p| p.ends_with("/code/card")) && op["value"] == json!(true);
    assert!(fix.iter().any(carded), "{fix:?}");
    let b = scaena_ops::open(&bare).unwrap();
    assert!(scaena_ops::patch::patch(&b, &Value::Array(fix), false).unwrap().applied);
    assert_eq!(faint(&bare, "snippet").len(), 0, "on its card, it reads");
    // On its card from the start, the card is what lint judges it on.
    assert_eq!(faint(&code("carded", over_ink(true)), "snippet").len(), 0);
}
