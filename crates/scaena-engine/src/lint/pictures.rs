//! E107: a picture whose file cannot be drawn. A header the engine reads says how large it is, so
//! the slide lays out, but the painter decodes the rest only when it first draws it: a damaged
//! stream, a truncated file. A painter leaves such a picture out, its box empty, and draws the
//! rest of the frame (the canvas, a PNG, a PDF), so nothing fails for it; lint asks the client's
//! painter ([`Backdrop::picture`]) about each picture the deck shows, once, and one that cannot
//! be drawn is a finding at each node that shows it, with its file and the painter's reason.

use super::{Cx, Laid};
use crate::sample::Content;
use scaena_core::lint::{Backdrop, Finding, Severity};
use serde_json::json;
use std::collections::BTreeMap;

/// Each picture `states` show that `backdrop` cannot draw, by its content id (`sha256:…`), with
/// the painter's reason: each asked about once.
pub(super) fn undrawable(states: &[Laid], backdrop: &mut dyn Backdrop) -> BTreeMap<String, String> {
    // Built as it goes, never collected: a map collected from an iterator sorts first, and
    // compiles that sort into the browser's modules (`scaena_core::sort`, PLAN 2.91).
    let mut tried: Vec<&str> = Vec::new();
    let mut bad = BTreeMap::new();
    for state in states {
        for node in &state.scene.nodes {
            let Content::Image(image) = &node.content else { continue };
            let id = image.info().id.as_str();
            if tried.contains(&id) {
                continue;
            }
            tried.push(id);
            if let Err(why) = backdrop.picture(id) {
                bad.insert(id.to_string(), why);
            }
        }
    }
    bad
}

/// E107: each node that shows a picture in `bad`, once, at the first state that shows it, naming
/// its file, why it cannot be drawn, and every state that shows it.
pub(super) fn e107(cx: &Cx, bad: &BTreeMap<String, String>) -> Vec<Finding> {
    let mut found: BTreeMap<String, (usize, String, String, Vec<String>)> = BTreeMap::new();
    for state in cx.states {
        for node in &state.scene.nodes {
            let Content::Image(image) = &node.content else { continue };
            let Some(why) = bad.get(&image.info().id) else { continue };
            let file = (state.snapshot.nodes.get(&node.id))
                .and_then(|props| props.get("src"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let entry = found.entry(node.id.clone()).or_insert_with(|| (state.index, file, why.clone(), Vec::new()));
            entry.3.push(state.snapshot.state_id.clone());
        }
    }
    found
        .into_iter()
        .map(|(node, (first, file, why, states))| {
            cx.finding(
                "E107",
                Severity::Error,
                cx.laid(first),
                format!("`{node}` shows `{file}`, which cannot be drawn: {why}"),
            )
            .at(cx.node_path(&node))
            .node(node.clone())
            .measure(json!({ "file": file, "why": why, "states": states }))
            .hint("The file is damaged or is not the picture it says it is: replace the picture, choosing another in the inspector or dropping one on it.")
        })
        .collect()
}
