//! The clipboard (PLAN 2.37, ADR-0013): what a copy holds of a deck, and the patch a paste
//! makes of it.
//!
//! - **A copy** ([`copying`]) is a node, or several (PLAN 2.42), and each node they hold, as one
//!   state shows them, with the deck's overrides for them, the data sources they read, and the
//!   files those and their images read. It is plain JSON (`application/x-scaena+json`, and as
//!   text), so it goes into this deck, another, or a text editor.
//! - **A paste** ([`pasting`]) adds each under an id new to the deck, held by the copy of what
//!   held it, the copy of the node copied where the pointer last pressed, as Insert places a
//!   node, and the copies of the others copied with it where they stood about it: one patch. A
//!   file or a data source the deck holds otherwise comes in under a name of its own.
//! - **What it names that the deck's theme lacks** (a color, a preset, a motion) is taken out
//!   of the copy, and the finding that says so comes back with the patch: a paste is not
//!   refused for it. A text keeps a role: one the theme lacks gives way to the role every
//!   theme has that was nearest it in size where it was copied from.

use crate::inspect::{copied_layouts, empty_slot, free_id, held};
use crate::{Context, OpsError};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use indexmap::IndexMap;
use scaena_core::Deck;
use scaena_core::lint::{Finding, Severity};
use scaena_core::model::theme::Theme;
use scaena_core::validate::{BundleFiles, validate_bundle};
use scaena_engine::geometry::{Snap, Target, Targets};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// What a copy holds, and what a paste looks for in what it is handed.
pub const KIND: &str = "scaena/clip";
/// The newest version of the clip's shape that a paste reads: 2 is a clip of several nodes
/// (PLAN 2.42). A clip of one node is written as version 1, which a Scaena before 2 pastes.
pub const VERSION: u32 = 2;
/// The media type a clip goes on the clipboard as, beside its text.
pub const MEDIA_TYPE: &str = "application/x-scaena+json";

/// What a copy holds of a deck (PLAN 2.37).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Clip {
    /// `scaena/clip`.
    pub kind: String,
    /// The shape's version: 1.
    pub version: u32,
    /// The theme the nodes name things from, as the deck names it, where it names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// The node copied, the first of `nodes`.
    pub node: String,
    /// Its box in the state it was copied from, `[x, y, width, height]`, each a share of the
    /// canvas's width or height: a paste sizes it so on any canvas.
    pub share: [f32; 4],
    /// The other nodes copied with it (PLAN 2.42), each with its box as `share` is: a paste
    /// places them where they stood about it. Only in a clip of version 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub more: Vec<Also>,
    /// Each node copied, then each node it holds, before what each holds in turn: its type
    /// and its props as the state shows them.
    pub nodes: IndexMap<String, Value>,
    /// The deck's overrides for each, which win in every state.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub overrides: IndexMap<String, Value>,
    /// Each text role they name beyond the six every theme has, with the one of those six
    /// nearest it in size in the theme they were copied from: what a paste sets in a theme
    /// without it.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub roles: IndexMap<String, String>,
    /// The data sources they read, as the deck declares them.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub data: IndexMap<String, Value>,
    /// The bundle's files they read (images, and data sources' files), by path, in base64.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub files: IndexMap<String, String>,
}

/// A node copied with the first (PLAN 2.42).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Also {
    /// The node.
    pub node: String,
    /// Its box in the state it was copied from, as [`Clip::share`] is.
    pub share: [f32; 4],
}

/// What a copy of `copied`, as `state` shows them, holds (PLAN 2.37, 2.42): each node and each
/// node it holds there, the deck's overrides for them, the data sources they read, and the
/// files those and their images read, from `read`. Each comes with its box at rest in
/// `state`, on a canvas `canvas` wide and high; the first is the node copied, and one that
/// another of them holds comes with that one. A shader set by a preset of `theme`'s names the
/// preset's kind too, so that it draws in a theme without the preset.
pub fn copying(
    deck: &Deck,
    theme: &Theme,
    state: &str,
    copied: &[(&str, [f32; 4])],
    canvas: [f32; 2],
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Clip, OpsError> {
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let snap =
        snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::new(format!("unknown state `{state}`")))?;
    if copied.is_empty() {
        return Err(OpsError::new("nothing is copied"));
    }
    if let Some((node, _)) = copied.iter().find(|(node, _)| !snap.nodes.contains_key(*node)) {
        return Err(OpsError::new(format!("`{node}` is not on screen in `{state}`")));
    }
    let doc = deck.to_value().context("the deck")?;
    // Each node copied, then what it holds, each before what it holds in turn; one that another
    // of them holds comes with that one.
    let under: Vec<Vec<String>> = copied.iter().map(|(node, _)| held(&[snap], node)).collect();
    let mut roots: Vec<(&str, [f32; 4])> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    for (i, &(node, rect)) in copied.iter().enumerate() {
        let within = |j: usize| j != i && under[j].iter().any(|id| id == node) && copied[j].0 != node;
        if roots.iter().any(|(r, _)| *r == node) || (0..copied.len()).any(within) {
            continue;
        }
        roots.push((node, rect));
        ids.extend(under[i].iter().rev().cloned());
    }
    if roots.is_empty() {
        return Err(OpsError::new("each node copied holds another of them: what holds what goes round"));
    }
    let (mut nodes, mut overrides, mut data, mut roles) =
        (IndexMap::new(), IndexMap::new(), IndexMap::new(), IndexMap::new());
    let mut paths: Vec<String> = Vec::new();
    for id in &ids {
        let mut props = Map::new();
        props.insert("type".into(), serde_json::to_value(deck.nodes[id].node_type)?);
        props.extend(snap.nodes[id].iter().map(|(k, v)| (k.clone(), v.clone())));
        if let Some(name) = props.get("data").and_then(Value::as_str).map(|d| d.trim_start_matches('@'))
            && let Some(source) = doc["data"].get(name)
        {
            data.insert(name.to_string(), source.clone());
        }
        if let Some(src) = props.get("src").and_then(Value::as_str) {
            paths.push(src.to_string());
        }
        if !props.contains_key("kind")
            && let Some(kind) = props.get("preset").and_then(Value::as_str).and_then(|p| preset_kind(theme, p))
        {
            props.insert("kind".into(), kind);
        }
        let runs = props.get("runs").and_then(Value::as_array).into_iter().flatten();
        for role in props.get("role").into_iter().chain(runs.filter_map(|r| r.get("role"))).filter_map(Value::as_str) {
            if let Some(near) = stand_in(theme, role) {
                roles.insert(role.to_string(), near);
            }
        }
        if let Some(theirs) = doc["overrides"].get(id) {
            overrides.insert(id.clone(), theirs.clone());
        }
        nodes.insert(id.clone(), Value::Object(props));
    }
    paths.extend(data.values().filter_map(|s| s["source"].as_str()).map(String::from));
    let mut files = IndexMap::new();
    for path in paths {
        if !files.contains_key(&path)
            && let Some(bytes) = read(&path)
        {
            files.insert(path, STANDARD.encode(bytes));
        }
    }
    let share = |r: [f32; 4]| [r[0] / canvas[0], r[1] / canvas[1], r[2] / canvas[0], r[3] / canvas[1]];
    let more: Vec<Also> =
        roots[1..].iter().map(|&(node, rect)| Also { node: node.into(), share: share(rect) }).collect();
    let named = doc["theme"].as_str().map(String::from);
    Ok(Clip {
        kind: KIND.into(),
        version: if more.is_empty() { 1 } else { VERSION },
        theme: named,
        node: roots[0].0.into(),
        share: share(roots[0].1),
        more,
        nodes,
        overrides,
        roles,
        data,
        files,
    })
}

/// The text roles every theme has (the theme schema requires them).
const EVERY: [&str; 6] = ["display", "headline", "body", "caption", "label", "numeral"];

/// The role of [`EVERY`] nearest `role` in size in `theme`, as a type scale steps (by ratio),
/// where `role` is not one of them.
fn stand_in(theme: &Theme, role: &str) -> Option<String> {
    if EVERY.contains(&role) {
        return None;
    }
    let roles = &theme.typography.roles;
    let size = roles.get(role)?.size;
    let steps = |other: f64| (other / size).ln().abs();
    let near = EVERY.iter().filter_map(|name| Some((*name, steps(roles.get(*name)?.size))));
    near.min_by(|a, b| a.1.total_cmp(&b.1)).map(|(name, _)| name.to_string())
}

/// The kind of `theme`'s shader preset `name`, as a node names it.
fn preset_kind(theme: &Theme, name: &str) -> Option<Value> {
    let preset = theme.shaders.as_ref()?.presets.as_ref()?.get(name)?;
    serde_json::to_value(preset.kind).ok()
}

/// The clip `text` holds, as a clipboard hands it over; `None` where it holds none (text from
/// anywhere else, which pastes as a text: [`of_text`]). A clip that is damaged, or newer than
/// this Scaena reads, is refused, with why.
pub fn read(text: &str) -> Result<Option<Clip>, OpsError> {
    let Ok(value) = serde_json::from_str::<Value>(text) else { return Ok(None) };
    if value.get("kind").and_then(Value::as_str) != Some(KIND) {
        return Ok(None);
    }
    let clip: Clip = serde_json::from_value(value).map_err(|e| OpsError::new(format!("the clip is damaged: {e}")))?;
    known(&clip)?;
    Ok(Some(clip))
}

/// A clip of `text` as a text in `theme`'s `body` role, which every theme has, in the box
/// Insert gives it (PLAN 2.37): what text from anywhere else pastes as.
pub fn of_text(deck: &Deck, theme: &Theme, text: &str) -> Result<Clip, OpsError> {
    let text = text.trim_end_matches(['\n', '\r']);
    if text.trim().is_empty() {
        return Err(OpsError::new("the clipboard holds nothing to paste"));
    }
    let body = scaena_core::inserts::inserts(deck, theme, &[], &std::collections::BTreeMap::new())
        .into_iter()
        .find(|i| i.node["type"] == "text" && i.node["role"] == "body")
        .ok_or_else(|| OpsError::new("the theme has no `body` role to paste text in"))?;
    let scaena_core::inserts::Start::Box { w, h } = body.start else {
        return Err(OpsError::new("a text is inserted in a box"));
    };
    let mut node = body.node;
    node["text"] = Value::String(text.to_string());
    Ok(Clip {
        kind: KIND.into(),
        version: 1,
        theme: None,
        node: body.id.clone(),
        share: [0.0, 0.0, w, h],
        more: Vec::new(),
        nodes: IndexMap::from([(body.id, node)]),
        overrides: IndexMap::new(),
        roles: IndexMap::new(),
        data: IndexMap::new(),
        files: IndexMap::new(),
    })
}

/// `clip` is a clip, of a version this Scaena reads.
fn known(clip: &Clip) -> Result<(), OpsError> {
    if clip.kind != KIND {
        return Err(OpsError::new(format!("not a Scaena clip: its kind is `{}`, not `{KIND}`", clip.kind)));
    }
    if clip.version > VERSION {
        let newer = format!("the clip is of version {}, newer than this Scaena reads ({VERSION})", clip.version);
        return Err(OpsError::new(newer));
    }
    Ok(())
}

/// What a paste makes (PLAN 2.37).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Pasted {
    /// The copy of the node copied.
    pub id: String,
    /// Its box once placed, `[x, y, width, height]` in canvas units.
    pub cell: [f32; 4],
    /// The copies of the others copied with it (PLAN 2.42).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<String>,
    /// `add_node` for each copy, entering in the state; the overrides they carry; a
    /// `bind_data` for each source the deck lacks; then the `place` ops that put the copy of
    /// the node copied where the pointer pressed.
    pub patch: Vec<Value>,
    /// The files the copies read that the bundle lacks, by path: each is added before the
    /// patch is made.
    pub files: Vec<String>,
    /// What the copies named that the deck's theme lacks, each taken out of them: the
    /// findings that would have refused the paste.
    pub findings: Vec<Finding>,
    /// The files' bytes, in `files`' order.
    #[serde(skip)]
    pub bytes: Vec<Vec<u8>>,
}

/// The patch that pastes `clip` in `state` (PLAN 2.37): each node it holds under an id new
/// to `deck`, the copy of its node `room`'s, held by the copy of what held it, entering
/// there; the copy of the node copied placed as Insert places a node, a text or an image
/// filling the template's slot under `at` where no node of the state's stands in it, and
/// anything else taking the box it was copied with about `at`, snapped to the theme's grid as
/// a drop snaps. Nodes copied together (PLAN 2.42) keep where they stood about each other,
/// their boxes all about `at` and on the grid where they fit, each snapped to it. `room` is the
/// engine's for the copy in `deck`, its cell the clip's share of the canvas. `files` is the
/// bundle, and `read` its bytes: a file the clip carries that the bundle holds with other
/// bytes, and a data source the deck declares otherwise, come in under names of their own.
/// What the theme lacks is taken out, and said in `findings`.
pub fn pasting(
    deck: &Deck,
    files: &dyn BundleFiles,
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
    room: &Targets,
    clip: &Clip,
    state: &str,
    at: [f32; 2],
) -> Result<Pasted, OpsError> {
    known(clip)?;
    let Some(first) = clip.nodes.get(&clip.node) else {
        return Err(OpsError::new(format!("the clip does not hold `{}`, the node it was copied from", clip.node)));
    };
    // The nodes copied, each with its box.
    let roots: Vec<(&str, [f32; 4])> = std::iter::once((clip.node.as_str(), clip.share))
        .chain(clip.more.iter().map(|m| (m.node.as_str(), m.share)))
        .collect();
    if let Some((node, _)) = roots[1..].iter().find(|(node, _)| !clip.nodes.contains_key(*node)) {
        return Err(OpsError::new(format!("the clip does not hold `{node}`, a node it was copied from")));
    }
    let root = |id: &str| roots.iter().any(|(r, _)| *r == id);
    // Ids: the copy of the node copied is the room's, and each other the first free.
    let mut taken: Vec<String> = deck.nodes.keys().cloned().chain([room.node.clone()]).collect();
    let mut ids: IndexMap<String, String> = IndexMap::new();
    for id in clip.nodes.keys() {
        let copy = if *id == clip.node { room.node.clone() } else { free_id(&taken, id) };
        taken.push(copy.clone());
        ids.insert(id.clone(), copy);
    }

    // Files: each the bundle lacks, and each it holds with other bytes under a path of its own.
    let (mut paths, mut bytes, mut moved) = (Vec::<String>::new(), Vec::<Vec<u8>>::new(), IndexMap::new());
    for (path, encoded) in &clip.files {
        let data =
            STANDARD.decode(encoded).map_err(|e| OpsError::new(format!("the clip's `{path}` is not base64: {e}")))?;
        match read(path) {
            Some(held) if held == data => {}
            Some(_) => {
                let (stem, ext) = path.rsplit_once('.').map_or((path.as_str(), ""), |(s, e)| (s, e));
                let free = (2..)
                    .map(|n| if ext.is_empty() { format!("{stem}-{n}") } else { format!("{stem}-{n}.{ext}") })
                    .find(|p| read(p).is_none() && !paths.contains(p))
                    .expect("some number is free");
                moved.insert(path.clone(), free.clone());
                paths.push(free);
                bytes.push(data);
            }
            None => {
                paths.push(path.clone());
                bytes.push(data);
            }
        }
    }

    // Data sources: each the deck lacks is declared, and one it declares otherwise comes in
    // under a name of its own.
    let doc = deck.to_value().context("the deck")?;
    let (mut named, mut declared) = (IndexMap::new(), Vec::new());
    for (name, source) in &clip.data {
        let mut source = source.clone();
        if let Some(to) = source["source"].as_str().and_then(|file| moved.get(file)) {
            source["source"] = Value::String(to.clone());
        }
        let theirs = doc["data"].get(name);
        let name_in = match theirs {
            Some(theirs) if *theirs == source => name.clone(),
            None => name.clone(),
            Some(_) => (2..)
                .map(|n| format!("{name}-{n}"))
                .find(|n| doc["data"].get(n).is_none() && !named.values().any(|v| v == n))
                .expect("some number is free"),
        };
        if theirs != Some(&source) {
            declared.push((name_in.clone(), source));
        }
        named.insert(name.clone(), name_in);
    }

    // The copies.
    let mut nodes: IndexMap<String, Map<String, Value>> = IndexMap::new();
    for (id, node) in &clip.nodes {
        let mut props =
            node.as_object().cloned().ok_or_else(|| OpsError::new(format!("the clip's `{id}` is not a node")))?;
        if root(id) {
            // Placed below, where the pointer pressed.
            props.remove("at");
        } else if let Some(Value::Object(at)) = props.get_mut("at")
            && let Some(parent) = at.get("parent").and_then(Value::as_str).and_then(|p| ids.get(p))
        {
            at.insert("parent".into(), Value::String(parent.clone()));
        }
        if let Some(name) = props.get("data").and_then(Value::as_str).map(|d| d.trim_start_matches('@'))
            && let Some(to) = named.get(name)
        {
            props.insert("data".into(), Value::String(format!("@{to}")));
        }
        if let Some(to) = props.get("src").and_then(Value::as_str).and_then(|src| moved.get(src)) {
            props.insert("src".into(), Value::String(to.clone()));
        }
        copied_layouts(&mut props, deck, &ids, root(id));
        nodes.insert(ids[id].clone(), props);
    }
    let mut overrides: IndexMap<String, Value> =
        clip.overrides.iter().filter_map(|(id, o)| Some((ids.get(id)?.clone(), o.clone()))).collect();

    // Where the copy of the node copied goes: as Insert places a node; and several, about `at`.
    let targets = if roots.len() == 1 {
        let content = matches!(first["type"].as_str(), Some("text" | "image"));
        let slot = if content { empty_slot(deck, room, state, at)? } else { None };
        let target = match slot {
            Some(rect) => room.snap(Snap::Slot, rect),
            None => {
                let [_, _, w, h] = room.cell;
                room.snap(Snap::Move, [at[0] - w / 2.0, at[1] - h / 2.0, w, h])
            }
        };
        vec![target]
    } else {
        together(room, &roots, &ids, at)
    };
    let targets: Vec<Target> = targets
        .into_iter()
        .collect::<Option<_>>()
        .ok_or_else(|| OpsError::new("the theme's grid has no tracks to place it on"))?;
    let cell = targets[0].cell;
    let placing = targets.iter().flat_map(|t| t.ops(Some(state), false));
    let place: Vec<Value> = placing.map(serde_json::to_value).collect::<Result<_, _>>().context("a patch")?;
    let also: Vec<String> = roots[1..].iter().map(|(node, _)| ids[*node].clone()).collect();

    // What the theme lacks is taken out, one round of findings at a time, until none is left
    // that taking out mends.
    let bundle = With { files, paths: &paths, bytes: &bytes };
    let mut findings = Vec::new();
    for _ in 0..8 {
        let patch = ops(&doc, &nodes, &overrides, &declared, state, &place);
        let compiled = scaena_core::patch::compile(&doc, &patch, &bundle).map_err(|e| OpsError {
            message: e.to_string(),
            plan: None,
            op: Some(e.index),
        })?;
        let found = validate_bundle(&serde_json::to_string(&compiled.doc)?, &bundle)?;
        let mut took = false;
        for f in found.into_iter().filter(|f| f.severity == Severity::Error && f.code == "E102") {
            let Some(path) = &f.path else { continue };
            // A text keeps a role: the one its role gave way to where it was copied from, else
            // `body`, which every theme has.
            let text = path.strip_prefix("/nodes/").and_then(|p| p.strip_suffix("/role")).and_then(|id| {
                let node = nodes.get_mut(id).filter(|n| n.get("type") == Some(&json!("text")))?;
                let role = node.get("role").and_then(Value::as_str)?;
                let near = clip.roles.get(role).map_or("body", String::as_str);
                (near != role).then(|| {
                    let near = near.to_string();
                    node.insert("role".into(), Value::String(near.clone()));
                    near
                })
            });
            if let Some(near) = text {
                took = true;
                findings.push(f.hint(format!("taken out of the paste: `{near}` in its place")));
            } else if taken_out(&mut nodes, "/nodes/", path) || taken_out_value(&mut overrides, "/overrides/", path) {
                took = true;
                findings.push(f.hint("taken out of the paste"));
            }
        }
        if !took {
            return Ok(Pasted { id: room.node.clone(), cell, also, patch, files: paths, findings, bytes });
        }
    }
    Err(OpsError::new("the clip names more that this deck's theme lacks than a paste takes out"))
}

/// Where nodes copied together go (PLAN 2.42): their boxes, each `roots`' share of the canvas
/// `room` is on, stand as they stood about each other, all about `at`, and moved onto the grid
/// where they fit on it, so that its edge stops none of them alone; then each is snapped to the
/// grid as a drop snaps, its copy named as `ids` says.
fn together(
    room: &Targets,
    roots: &[(&str, [f32; 4])],
    ids: &IndexMap<String, String>,
    at: [f32; 2],
) -> Vec<Option<Target>> {
    let [x, y, w, h] = room.within;
    let boxes: Vec<[f32; 4]> = roots.iter().map(|(_, s)| [s[0] * w, s[1] * h, s[2] * w, s[3] * h]).collect();
    let low = |i: usize| boxes.iter().map(|b| b[i]).fold(f32::INFINITY, f32::min);
    let high = |i: usize| boxes.iter().map(|b| b[i] + b[i + 2]).fold(f32::NEG_INFINITY, f32::max);
    // The grid's extent, its first track's start to its last's end; the canvas without one.
    let extent = |tracks: &[[f32; 2]], whole: [f32; 2]| match (tracks.first(), tracks.last()) {
        (Some(first), Some(last)) => [first[0], last[1]],
        _ => whole,
    };
    let across = extent(&room.columns, [x, x + w]);
    let down = extent(&room.rows, [y, y + h]);
    // About `at`, and on the grid where all of them fit on it.
    let onto = |from: f32, to: f32, middle: f32, [lo, hi]: [f32; 2]| {
        let span = to - from;
        let start = middle - span / 2.0;
        (if span <= hi - lo { start.clamp(lo, hi - span) } else { start }) - from
    };
    let by = [onto(low(0), high(0), at[0], across), onto(low(1), high(1), at[1], down)];
    roots
        .iter()
        .zip(&boxes)
        .map(|((node, _), b)| {
            let mut targets = room.clone();
            targets.node = ids[*node].clone();
            targets.cell = [room.cell[0], room.cell[1], b[2], b[3]];
            targets.snap(Snap::Move, [b[0] + by[0], b[1] + by[1], b[2], b[3]])
        })
        .collect()
}

/// The paste's patch: each data source declared, each copy added, entering in `state`, their
/// overrides, then `place`.
fn ops(
    doc: &Value,
    nodes: &IndexMap<String, Map<String, Value>>,
    overrides: &IndexMap<String, Value>,
    declared: &[(String, Value)],
    state: &str,
    place: &[Value],
) -> Vec<Value> {
    let mut out = Vec::new();
    if !declared.is_empty() && doc.get("data").is_none() {
        out.push(json!({ "op": "add", "path": "/data", "value": {} }));
    }
    for (name, source) in declared {
        out.push(json!({ "op": "add", "path": format!("/data/{name}"), "value": source }));
    }
    out.extend(nodes.iter().map(|(id, node)| json!({ "op": "add_node", "id": id, "node": node, "state": state })));
    if !overrides.is_empty() && doc.get("overrides").is_none() {
        out.push(json!({ "op": "add", "path": "/overrides", "value": {} }));
    }
    for (id, o) in overrides {
        out.push(json!({ "op": "add", "path": format!("/overrides/{id}"), "value": o }));
    }
    out.extend(place.iter().cloned());
    out
}

/// Take the property `path` names out of the node it points into, in `nodes` under `prefix`:
/// a key of an object, which goes with the object when it was the last.
fn taken_out(nodes: &mut IndexMap<String, Map<String, Value>>, prefix: &str, path: &str) -> bool {
    let Some(rest) = path.strip_prefix(prefix) else { return false };
    let mut parts = rest.split('/').map(|p| p.replace("~1", "/").replace("~0", "~"));
    let Some(id) = parts.next() else { return false };
    let keys: Vec<String> = parts.collect();
    let Some(node) = nodes.get_mut(&id) else { return false };
    let mut value = Value::Object(std::mem::take(node));
    let gone = remove(&mut value, &keys);
    if let Value::Object(map) = value {
        *node = map;
    }
    gone
}

/// [`taken_out`] for values held whole, as overrides are.
fn taken_out_value(values: &mut IndexMap<String, Value>, prefix: &str, path: &str) -> bool {
    let Some(rest) = path.strip_prefix(prefix) else { return false };
    let mut parts = rest.split('/').map(|p| p.replace("~1", "/").replace("~0", "~"));
    let Some(id) = parts.next() else { return false };
    let keys: Vec<String> = parts.collect();
    let Some(value) = values.get_mut(&id) else { return false };
    let gone = remove(value, &keys);
    if value.as_object().is_some_and(Map::is_empty) {
        values.shift_remove(&id);
    }
    gone
}

/// Remove what `keys` lead to under `value`; an object left with nothing goes with it.
fn remove(value: &mut Value, keys: &[String]) -> bool {
    match keys {
        [] => false,
        [last] => match value {
            Value::Object(map) => map.shift_remove(last).is_some(),
            Value::Array(items) => {
                last.parse::<usize>().ok().filter(|&i| i < items.len()).map(|i| items.remove(i)).is_some()
            }
            _ => false,
        },
        [key, rest @ ..] => {
            let child = match value {
                Value::Object(map) => map.get_mut(key),
                Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get_mut(i)),
                _ => None,
            };
            let Some(child) = child else { return false };
            let gone = remove(child, rest);
            if gone
                && child.as_object().is_some_and(Map::is_empty)
                && let Value::Object(map) = value
            {
                map.shift_remove(key);
            }
            gone
        }
    }
}

/// The bundle, with the files a paste adds.
struct With<'a> {
    files: &'a dyn BundleFiles,
    paths: &'a [String],
    bytes: &'a [Vec<u8>],
}

impl BundleFiles for With<'_> {
    fn exists(&self, path: &str) -> bool {
        self.paths.iter().any(|p| p == path) || self.files.exists(path)
    }

    fn read_text(&self, path: &str) -> Option<String> {
        match self.paths.iter().position(|p| p == path) {
            Some(i) => String::from_utf8(self.bytes[i].clone()).ok(),
            None => self.files.read_text(path),
        }
    }
}
