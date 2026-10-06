//! A bundle's files, and what in its deck uses each (PLAN 2.59): its images, fonts, and data.
//! Each says what names it (an image node's `src` or a beat's evidence, a face of the deck's
//! fonts or a family of its theme, a data source) and the nodes drawn from it, each with the
//! states that show it so. A file nothing names says so: taken out, the deck draws and reads
//! the same.
//!
//! What a node is drawn from is read from the deck as each state resolves it, its overrides
//! last, before anything is laid out:
//! - an image node, from the file its `src` names;
//! - a chart or a table, from the file of the source its `data` names;
//! - a text, from the fonts of the families it is set in: its role's (its own, or its slot's),
//!   or the family its style, or a run's, names; its italic face where it, or a run, is set
//!   italic, and the upright one otherwise; and the families each falls back to. A chart's and
//!   a table's text, likewise, in the theme's roles for them. A face no character takes is
//!   counted all the same.
//!
//! Images are the PNG, JPEG, GIF, and WebP files, and any file an image node names; fonts, the
//! font files (`.ttf`, `.otf`, `.woff`, `.woff2`); data, the CSV, TSV, and JSON files under
//! `data/`, and any file a source names. The deck, its theme, its manifest, its history, and
//! what else the bundle holds (a license, a note on where its images came from, a skill) are
//! none of them.

use crate::document::{Deck, NodeType, Props};
use crate::model::theme::Theme;
use crate::tracking::{merge_props, resolve_states};
use indexmap::{IndexMap, IndexSet};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

/// What a file of a bundle is, as an editor lists them: images, then fonts, then data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Image,
    Font,
    Data,
}

/// One of a bundle's images, fonts, or data files, and what uses it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct BundleFile {
    /// Its path in the bundle.
    pub path: String,
    #[serde(rename = "type")]
    pub kind: Kind,
    /// Its size, in bytes.
    pub bytes: u64,
    /// What in the deck or its theme names it. None, where nothing does: it may be taken out.
    pub named: Vec<Named>,
    /// The nodes drawn from it, in the deck's order, each with the states that show it so.
    pub used: Vec<Used>,
}

/// What names a file.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "by", rename_all = "lowercase")]
pub enum Named {
    /// An image node's `src`: its own, a state's, or the deck's overrides'.
    Node { node: String },
    /// A beat cites it as evidence (SPEC §3.11).
    Evidence { beat: String },
    /// A face the deck's `fonts` list: its family, and its style where it sets one.
    Font {
        family: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        style: Option<String>,
    },
    /// A family of the theme (`typography.families`, by its key): its face, or its italic.
    Theme { family: String },
    /// A data source reads it.
    Source { source: String },
}

/// A node drawn from a file, and the states that show it so, in the deck's order.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Used {
    pub node: String,
    pub states: Vec<String>,
}

/// The font files, by extension, as `scaena_store` places them.
const FONTS: [&str; 4] = ["ttf", "otf", "woff", "woff2"];
/// The data files, by extension, under `data/`.
const DATA: [&str; 3] = ["csv", "tsv", "json"];
/// The images, by extension, anywhere.
const IMAGES: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

fn extension(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    name.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).filter(|e| !e.is_empty())
}

/// What `path` is among `deck`'s files, if one of its images, fonts, or data.
pub fn kind_of(deck: &Deck, path: &str) -> Option<Kind> {
    let ext = extension(path);
    let ext = ext.as_deref();
    if deck.data.values().any(|d| d.source.as_str() == Some(path)) {
        return Some(Kind::Data);
    }
    if ext.is_some_and(|e| FONTS.contains(&e)) {
        return Some(Kind::Font);
    }
    if ext.is_some_and(|e| IMAGES.contains(&e)) || deck.image_files().iter().any(|p| p == path) {
        return Some(Kind::Image);
    }
    if path.starts_with("data/") && ext.is_some_and(|e| DATA.contains(&e)) {
        return Some(Kind::Data);
    }
    None
}

/// The images, fonts, and data of `held`, the bundle's files by their paths with their sizes,
/// in that order and each by its path, with what in `deck` and `theme` names each, and the
/// nodes drawn from it in the states that show them so (PLAN 2.59).
pub fn files(deck: &Deck, theme: &Theme, held: &[(String, u64)]) -> Result<Vec<BundleFile>, String> {
    let drawn = drawn(deck)?;
    let mut out: Vec<BundleFile> = held
        .iter()
        .filter_map(|(path, bytes)| {
            let kind = kind_of(deck, path)?;
            let named = named(deck, theme, path, kind);
            let used = used(deck, theme, &drawn, path, kind);
            Some(BundleFile { path: path.clone(), kind, bytes: *bytes, named, used })
        })
        .collect();
    crate::sort::by(&mut out, |a, b| (a.kind, &a.path).cmp(&(b.kind, &b.path)));
    Ok(out)
}

/// What names `path`, a file of kind `kind`, in `deck` and `theme`.
fn named(deck: &Deck, theme: &Theme, path: &str, kind: Kind) -> Vec<Named> {
    let mut out = Vec::new();
    match kind {
        Kind::Image => {
            let srcs = |props: &Props| props.get("src").and_then(Value::as_str) == Some(path);
            for (id, node) in &deck.nodes {
                let in_state = deck.states.iter().any(|s| s.props.get(id).is_some_and(srcs));
                let overridden = deck.overrides.get(id).is_some_and(srcs);
                if node.node_type == NodeType::Image && (srcs(&node.props) || in_state || overridden) {
                    out.push(Named::Node { node: id.clone() });
                }
            }
            for beat in deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats) {
                if beat.evidence.iter().any(|e| e == path) {
                    out.push(Named::Evidence { beat: beat.id.clone() });
                }
            }
        }
        Kind::Font => {
            for font in deck.fonts.iter().filter(|f| f.file == path) {
                out.push(Named::Font { family: font.family.clone(), style: font.style.clone() });
            }
            for (key, family) in &theme.typography.families {
                if family.file == path || family.italic.as_ref().is_some_and(|f| f.file == path) {
                    out.push(Named::Theme { family: key.clone() });
                }
            }
        }
        Kind::Data => {
            for (name, source) in &deck.data {
                if source.source.as_str() == Some(path) {
                    out.push(Named::Source { source: name.clone() });
                }
            }
        }
    }
    out
}

/// Each state's nodes as they draw there, by state: their props there, the deck's overrides
/// over them, and the state's layout.
struct Drawn {
    state: String,
    layout: Option<String>,
    nodes: IndexMap<String, Props>,
}

fn drawn(deck: &Deck) -> Result<Vec<Drawn>, String> {
    let snapshots = resolve_states(deck).map_err(|e| e.to_string())?;
    Ok(snapshots
        .into_iter()
        .map(|s| {
            let mut nodes = s.nodes;
            for (id, props) in nodes.iter_mut() {
                if let Some(o) = deck.overrides.get(id) {
                    merge_props(props, o);
                }
            }
            Drawn { state: s.state_id, layout: s.layout, nodes }
        })
        .collect())
}

/// The nodes drawn from `path`, each with the states that show it so, in the deck's order.
fn used(deck: &Deck, theme: &Theme, drawn: &[Drawn], path: &str, kind: Kind) -> Vec<Used> {
    // What a node in a state must be drawn from, to be drawn from this file.
    let sources: Vec<String> =
        deck.data.iter().filter(|(_, d)| d.source.as_str() == Some(path)).map(|(n, _)| format!("@{n}")).collect();
    let faces: Vec<(&str, bool)> = match kind {
        Kind::Font => theme_faces(theme, path),
        _ => Vec::new(),
    };
    let mut by: IndexMap<&str, Vec<String>> = IndexMap::new();
    for d in drawn {
        for (id, props) in &d.nodes {
            let Some(node) = deck.nodes.get(id) else { continue };
            let draws = match (kind, node.node_type) {
                (Kind::Image, NodeType::Image) => props.get("src").and_then(Value::as_str) == Some(path),
                (Kind::Data, NodeType::Chart | NodeType::Table) => {
                    props.get("data").and_then(Value::as_str).is_some_and(|d| sources.iter().any(|s| s == d))
                }
                (Kind::Font, t) => {
                    let set = set_in(theme, t, props, d.layout.as_deref());
                    set.iter().any(|(family, italic)| faces.contains(&(family.as_str(), *italic)))
                }
                _ => false,
            };
            if draws {
                by.entry(id.as_str()).or_default().push(d.state.clone());
            }
        }
    }
    let order: Vec<&String> = deck.nodes.keys().collect();
    let mut out: Vec<Used> = by.into_iter().map(|(node, states)| Used { node: node.to_string(), states }).collect();
    crate::sort::by_key(&mut out, |u| order.iter().position(|n| **n == u.node));
    out
}

/// The faces of the theme's families whose text is set from `path`: each family by its key,
/// and whether that is its italic. A family without an italic sets italic text upright, from
/// its own file.
fn theme_faces<'a>(theme: &'a Theme, path: &str) -> Vec<(&'a str, bool)> {
    let mut out = Vec::new();
    for (key, family) in &theme.typography.families {
        if family.file == path {
            out.push((key.as_str(), false));
            if family.italic.is_none() {
                out.push((key.as_str(), true));
            }
        }
        if family.italic.as_ref().is_some_and(|f| f.file == path) {
            out.push((key.as_str(), true));
        }
    }
    out
}

/// The families, by key, that a node of type `node_type` drawn with `props` sets its text in,
/// each with whether italic, and those they fall back to.
fn set_in(theme: &Theme, node_type: NodeType, props: &Props, layout: Option<&str>) -> IndexSet<(String, bool)> {
    let roles = &theme.typography.roles;
    let mut set: IndexSet<(String, bool)> = IndexSet::new();
    // A role's family and italic, then a style over them.
    let face = |role: Option<&str>, style: Option<&Value>, under: Option<(String, bool)>| -> Option<(String, bool)> {
        let (mut family, mut italic) = match role.and_then(|r| roles.get(r)) {
            Some(r) => (r.family.clone(), r.italic.unwrap_or(false)),
            None => under?,
        };
        if let Some(f) = style.and_then(|s| s.get("family")).and_then(Value::as_str) {
            family = f.to_string();
        }
        if let Some(i) = style.and_then(|s| s.get("italic")).and_then(Value::as_bool) {
            italic = i;
        }
        Some((family, italic))
    };
    match node_type {
        NodeType::Text => {
            let slot = (props.get("at").and_then(|a| a.get("in")).and_then(Value::as_str))
                .and_then(|slot| theme.layouts.get(layout?)?.slots.get(slot)?.role.clone());
            let role = props.get("role").and_then(Value::as_str).or(slot.as_deref()).unwrap_or("body");
            let own = face(Some(role), props.get("style"), None);
            if let Some(own) = own.clone() {
                set.insert(own);
            }
            for run in props.get("runs").and_then(Value::as_array).into_iter().flatten() {
                let run_role = run.get("role").and_then(Value::as_str);
                if let Some(f) = face(run_role, run.get("style"), own.clone()) {
                    set.insert(f);
                }
            }
        }
        NodeType::Chart => {
            let charts = theme.charts.as_ref();
            let tick = charts.and_then(|c| c.axis.as_ref()).and_then(|a| a.role.clone()).unwrap_or("label".into());
            let label = (props.get("labels").and_then(|l| l.get("role")).and_then(Value::as_str).map(String::from))
                .or_else(|| charts.and_then(|c| c.label.as_ref()).and_then(|l| l.role.clone()))
                .unwrap_or("label".into());
            let title = charts.and_then(|c| c.title.as_ref()).and_then(|t| t.role.clone()).unwrap_or(tick.clone());
            let legend = charts.and_then(|c| c.legend.as_ref()).and_then(|t| t.role.clone()).unwrap_or(tick.clone());
            for role in [tick, label, title, legend] {
                set.extend(face(Some(&role), None, None));
            }
        }
        NodeType::Table => {
            let tables = theme.tables.as_ref();
            let header = tables.and_then(|t| t.header.as_ref()).and_then(|h| h.role.clone()).unwrap_or("label".into());
            let cell = tables.and_then(|t| t.cell.as_ref()).and_then(|c| c.role.clone()).unwrap_or("body".into());
            for role in [header, cell] {
                set.extend(face(Some(&role), None, None));
            }
        }
        _ => {}
    }
    // Each family's fallbacks set what it lacks, in the same style.
    let mut i = 0;
    while i < set.len() {
        let (family, italic) = set[i].clone();
        for back in theme.typography.families.get(&family).and_then(|f| f.fallback.as_ref()).into_iter().flatten() {
            set.insert((back.clone(), italic));
        }
        i += 1;
    }
    set
}
