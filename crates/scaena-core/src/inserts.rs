//! What an editor may insert (PLAN 2.34, ADR-0013): for each node type, the nodes the theme and
//! the bundle name, each as `add_node` adds it, unplaced, with the id it starts from and the box
//! it takes at first. The page keeps no vocabulary of its own.
//!
//! - **A text** in each of the theme's roles, two of the role's lines tall and half the canvas
//!   wide, its text the role's name.
//! - **A shape** of each kind that needs no points: a rectangle and an ellipse, filled with the
//!   theme's accent (or its first color); a line and an arrow, the theme's rule.
//! - **An image** of each PNG in the bundle.
//! - **A shader** of each preset, filling the canvas, under what is there.

use crate::document::Deck;
use crate::ids::is_valid_id;
use crate::model::theme::{Theme, Vocabulary};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};

/// One thing an editor may insert.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Insert {
    /// What a menu says: the node's type, and the name it is made from.
    pub label: String,
    /// The node `add_node` adds, unplaced.
    pub node: Value,
    /// What its id starts from: a role, a kind, an image's name, a preset.
    pub id: String,
    /// The box it takes at first.
    pub start: Start,
}

/// The box an inserted node takes at first.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Start {
    /// A box about the pointer, each side a fraction of the canvas's, snapped to the theme's
    /// grid as a drop snaps; a text or an image fills instead the template's slot under the
    /// pointer, if nothing fills it.
    Box { w: f32, h: f32 },
    /// A slot it fills whole, and under the nodes there (`z`): a shader fills the canvas.
    Slot(String),
}

/// Everything `deck` may have inserted, `theme` its theme and `files` its bundle's paths.
pub fn inserts(deck: &Deck, theme: &Theme, files: &[String]) -> Vec<Insert> {
    let [w, h] = [deck.canvas.width as f32, deck.canvas.height as f32];
    let mut out = Vec::new();
    for (role, look) in &theme.typography.roles {
        let tall = (2.0 * look.size * look.leading) as f32 / h;
        out.push(Insert {
            label: format!("Text · {role}"),
            node: json!({ "type": "text", "role": role, "text": sentence(role) }),
            id: role.clone(),
            start: Start::Box { w: 0.5, h: tall.clamp(1.0 / 12.0, 0.5) },
        });
    }
    let colors = theme.names(Vocabulary::Color);
    let fill = colors.iter().find(|c| *c == "accent").or(colors.first());
    let square = |side: f32| Start::Box { w: side, h: side * w / h };
    for kind in ["rect", "ellipse"] {
        let Some(fill) = fill else { break };
        let node = json!({ "type": "shape", "kind": kind, "fill": fill });
        out.push(Insert { label: format!("Shape · {kind}"), node, id: kind.into(), start: square(0.25) });
    }
    for kind in ["line", "arrow"] {
        let node = json!({ "type": "shape", "kind": kind });
        out.push(Insert {
            label: format!("Shape · {kind}"),
            node,
            id: kind.into(),
            start: Start::Box { w: 0.25, h: 1.0 / 12.0 },
        });
    }
    for path in files.iter().filter(|p| p.to_ascii_lowercase().ends_with(".png")) {
        let name = path.rsplit('/').next().unwrap_or(path);
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        let node = json!({ "type": "image", "src": path });
        // An image dropped into a bundle is named by its SHA-256 (SPEC §3.1): its first eight
        // digits name it well enough.
        let (label, id) = match stem.len() == 64 && stem.bytes().all(|b| b.is_ascii_hexdigit()) {
            true => (format!("Image · {}…", &stem[..8]), format!("image-{}", stem[..8].to_ascii_lowercase())),
            false => (format!("Image · {name}"), slug(stem, "image")),
        };
        out.push(Insert { label, node, id, start: square(1.0 / 3.0) });
    }
    let presets = theme.shaders.as_ref().and_then(|s| s.presets.as_ref());
    for (name, preset) in presets.into_iter().flatten() {
        let node = json!({ "type": "shader", "kind": preset.kind, "preset": name, "z": -1 });
        out.push(Insert {
            label: format!("Shader · {name}"),
            node,
            id: slug(name, "shader"),
            start: Start::Slot("canvas".into()),
        });
    }
    out
}

/// The first of `base`, `base-2`, `base-3`, … that names no node of `deck`.
pub fn fresh(deck: &Deck, base: &str) -> String {
    (1..)
        .map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") })
        .find(|id| !deck.nodes.contains_key(id))
        .expect("some number is free")
}

/// `name` as an id (SPEC §3.2): lowercase, each run of other characters a `-`, starting with
/// a letter (else `prefix-` first), 64 characters at most.
fn slug(name: &str, prefix: &str) -> String {
    let mut out = String::new();
    for c in name.chars().map(|c| c.to_ascii_lowercase()) {
        match c {
            'a'..='z' | '0'..='9' | '_' => out.push(c),
            _ if !out.is_empty() && !out.ends_with('-') => out.push('-'),
            _ => {}
        }
    }
    let out = out.trim_end_matches('-');
    let out =
        if out.starts_with(|c: char| c.is_ascii_lowercase()) { out.to_string() } else { format!("{prefix}-{out}") };
    let out: String = out.chars().take(56).collect();
    let out = out.trim_end_matches('-').to_string();
    if is_valid_id(&out) { out } else { prefix.to_string() }
}

/// A role's name as a text says it at first: `big-number` as "Big number".
fn sentence(role: &str) -> String {
    let words = role.replace(['-', '_'], " ");
    let mut chars = words.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_becomes_an_id_and_a_role_a_sentence() {
        assert_eq!(slug("Team Photo 2026", "image"), "team-photo-2026");
        assert_eq!(slug("2026 plan", "image"), "image-2026-plan");
        assert_eq!(slug("--", "image"), "image");
        assert!(is_valid_id(&slug(&"x".repeat(200), "image")));
        assert_eq!(sentence("big-number"), "Big number");
        let deck = Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        let theme: Theme = serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        let sha = format!("assets/{}.png", "7130f10a".repeat(8));
        let offered = inserts(&deck, &theme, &[sha.clone(), "assets/Team Photo.png".into()]);
        let images: Vec<(&str, &str)> =
            offered.iter().filter(|i| i.node["type"] == "image").map(|i| (i.label.as_str(), i.id.as_str())).collect();
        assert_eq!(images, [("Image · 7130f10a…", "image-7130f10a"), ("Image · Team Photo.png", "team-photo")]);
        assert_eq!(sentence("title"), "Title");
    }
}
