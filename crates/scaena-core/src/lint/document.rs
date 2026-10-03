//! Document-level design and housekeeping rules: what the document says, with the
//! theme's thresholds, before anything is laid out.

use super::{Context, Finding, Rule, Severity};
use crate::document::{NodeType, Props};
use crate::model::states::{ChoreoItem, ChoreoTarget, Targets};
use crate::model::values::{PresetLook, PresetRef};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(W210Density),
        Box::new(W300Literal),
        Box::new(W301Rect),
        Box::new(W302Fixed),
        Box::new(W322Idle),
        Box::new(W401StateNotInBeat),
        Box::new(W410ImageWithoutAlt),
        Box::new(I400NoopState),
        Box::new(I401NodeNeverVisible),
        Box::new(I402OverrideCount),
    ]
}

/// A JSON pointer token: `~` and `/` escaped (RFC 6901).
pub(super) fn token(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// W210: more words on screen than the theme's `density.maxWordsPerState` (40 when it
/// says nothing). A word is what whitespace separates, in every visible text node.
struct W210Density;
impl Rule for W210Density {
    fn code(&self) -> &'static str {
        "W210"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let max = cx.theme.and_then(|t| t.density.as_ref()).and_then(|d| d.max_words_per_state).unwrap_or(40);
        let mut out = Vec::new();
        for (i, snap) in cx.snapshots.iter().enumerate() {
            let words: usize = snap
                .nodes
                .iter()
                .filter(|(id, _)| cx.deck.nodes.get(*id).is_some_and(|n| n.node_type == NodeType::Text))
                .map(|(_, props)| words(props))
                .sum();
            if words > max as usize {
                out.push(
                    Finding::new(
                        self.code(),
                        self.severity(),
                        format!("state `{}` shows {words} words; the theme's density allows {max}", snap.state_id),
                    )
                    .at(format!("/states/{i}"))
                    .state(snap.state_id.clone())
                    .measure(serde_json::json!({ "words": words, "maxWordsPerState": max }))
                    .hint("Split it into builds, or move the detail to the beat's notes."),
                );
            }
        }
        out
    }
}

/// The words a text node shows: its `text`, or its runs' text. A word is what whitespace
/// separates (W210, and the reading W323 sizes a hold by).
pub fn words(props: &Props) -> usize {
    let count = |v: Option<&Value>| v.and_then(Value::as_str).map_or(0, |s| s.split_whitespace().count());
    match props.get("runs").and_then(Value::as_array) {
        Some(runs) => runs.iter().map(|r| count(r.get("text"))).sum(),
        None => count(props.get("text")),
    }
}

/// W300: a pixel or color literal outside `overrides`: a color written out (`#rrggbb`,
/// `oklch(…)`, `oklab(…)`) where a theme color would go, a text `size`, or a length in
/// canvas units where a theme token would go (`radius`, `gap`, `padding`, `inset`, a
/// stroke's `width`, a child's `size`). Placement by `rect` is W301's.
struct W300Literal;
impl Rule for W300Literal {
    fn code(&self) -> &'static str {
        "W300"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let mut out = Vec::new();
        let mut report = |pointer: String, what: String, state: Option<&str>, node: &str| {
            let mut f = Finding::new(self.code(), self.severity(), format!("{what} outside `overrides`"))
                .at(pointer)
                .node(node)
                .hint("Name a theme color, role, or token; a literal belongs in `overrides`, where it counts as one.");
            if let Some(state) = state {
                f = f.state(state);
            }
            out.push(f);
        };
        for (id, node) in &cx.deck.nodes {
            for (pointer, what) in literals(&node.props, &format!("/nodes/{}", token(id))) {
                report(pointer, what, None, id);
            }
        }
        for (i, state) in cx.deck.states.iter().enumerate() {
            for (id, delta) in &state.props {
                for (pointer, what) in literals(delta, &format!("/states/{i}/props/{}", token(id))) {
                    report(pointer, what, Some(&state.id), id);
                }
            }
        }
        out
    }
}

/// Keys whose strings are words for people, never colors.
const PROSE: [&str; 9] = ["text", "alt", "title", "label", "notes", "comment", "format", "parse", "dataTransform"];

/// Keys that take a theme token or a length in canvas units.
const LENGTHS: [&str; 4] = ["radius", "gap", "inset", "padding"];

/// The literals in a node's props, each its pointer and what it is.
fn literals(props: &Props, base: &str) -> Vec<(String, String)> {
    fn walk(v: &Value, path: &mut Vec<String>, base: &str, out: &mut Vec<(String, String)>) {
        let key = |back: usize| path.len().checked_sub(back + 1).map(|i| path[i].clone());
        let (k0, k1) = (key(0), key(1));
        let (k0, k1) = (k0.as_deref(), k1.as_deref());
        let pointer = format!("{base}/{}", path.iter().map(|k| token(k)).collect::<Vec<_>>().join("/"));
        // A node's own length (its `radius`, `gap`, `padding`, or `at.inset`), an item of
        // its `padding` list, its stroke's width, a child's size. A shader's `params` and a
        // chart's settings are not lengths in canvas units.
        let top = path.len() == 1 || (path.len() == 2 && path[0] == "at");
        let length = (top && k0.is_some_and(|k| LENGTHS.contains(&k)))
            || (path.len() == 2 && k0.is_some_and(|k| k.parse::<usize>().is_ok()) && k1 == Some("padding"))
            || (k0 == Some("width") && k1 == Some("stroke"))
            || (matches!(k0, Some("w" | "h" | "minW" | "maxW" | "minH" | "maxH")) && k1 == Some("size"));
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    if PROSE.contains(&k.as_str()) {
                        continue;
                    }
                    path.push(k.clone());
                    walk(x, path, base, out);
                    path.pop();
                }
            }
            Value::Array(a) => {
                for (i, x) in a.iter().enumerate() {
                    path.push(i.to_string());
                    walk(x, path, base, out);
                    path.pop();
                }
            }
            Value::String(s) if is_color_literal(s) => out.push((pointer, format!("literal color `{s}`"))),
            Value::String(s) if length && is_cu(s) => out.push((pointer, format!("literal length `{s}`"))),
            Value::Number(n) if k0 == Some("size") && k1 == Some("style") => {
                out.push((pointer, format!("literal text size {n}")))
            }
            Value::Number(n) if length => out.push((pointer, format!("literal length {n}"))),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (k, v) in props {
        if PROSE.contains(&k.as_str()) {
            continue;
        }
        let mut path = vec![k.clone()];
        if k == "at" {
            // `at.rect` is W301's; the rest of `at` (an `inset`) is still checked.
            if let Value::Object(m) = v {
                for (k2, x) in m.iter().filter(|(k2, _)| *k2 != "rect") {
                    path.push(k2.clone());
                    walk(x, &mut path, base, &mut out);
                    path.pop();
                }
            }
            continue;
        }
        walk(v, &mut path, base, &mut out);
    }
    out
}

/// `#rrggbb`, `#rrggbbaa`, `oklch(…)`, or `oklab(…)`: a color written out.
pub(super) fn is_color_literal(s: &str) -> bool {
    let hex = s.strip_prefix('#').is_some_and(|h| matches!(h.len(), 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit()));
    hex || s.starts_with("oklch(") || s.starts_with("oklab(")
}

/// A length in canvas units written out: `24cu`.
fn is_cu(s: &str) -> bool {
    s.strip_suffix("cu").is_some_and(|n| n.parse::<f64>().is_ok())
}

/// Where a node's resolved `at.<key>` in state `i` was set: the latest state at or before
/// it whose delta sets it, else the node's defaults. `None` when only `overrides` sets it.
fn source(cx: &Context, i: usize, id: &str, key: &str) -> Option<String> {
    let sets = |p: Option<&Props>| p.and_then(|p| p.get("at")).and_then(|at| at.get(key)).is_some();
    for j in (0..=i).rev() {
        if sets(cx.deck.states[j].props.get(id)) {
            return Some(format!("/states/{j}/props/{}/at/{key}", token(id)));
        }
    }
    sets(cx.deck.nodes.get(id).map(|n| &n.props)).then(|| format!("/nodes/{}/at/{key}", token(id)))
}

/// W301: a node placed on the canvas by `rect` in a state laid out by a template (a
/// state with a `layout`): canvas units that no template, theme, or format moves. A
/// `rect` in `overrides` is an override already, and I402 counts it; a container's child
/// is placed by `rect` within its container.
struct W301Rect;
impl Rule for W301Rect {
    fn code(&self) -> &'static str {
        "W301"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for (i, snap) in cx.snapshots.iter().enumerate() {
            let Some(layout) = &snap.layout else { continue };
            for (id, props) in &snap.nodes {
                // A container's child is placed in its container, not on the canvas.
                let at = props.get("at");
                if at.and_then(|at| at.get("rect")).is_none() || at.and_then(|at| at.get("parent")).is_some() {
                    continue;
                }
                let Some(pointer) = source(cx, i, id, "rect") else { continue };
                if seen.insert(pointer.clone()) {
                    out.push(
                        Finding::new(
                            self.code(),
                            self.severity(),
                            format!("node `{id}` is placed by `rect` in state `{}`, which layout `{layout}` lays out", snap.state_id),
                        )
                        .at(pointer)
                        .state(snap.state_id.clone())
                        .node(id.clone())
                        .hint("Place it in one of the layout's slots (`at.in`), or by grid cells; a `rect` belongs in `overrides`."),
                    );
                }
            }
        }
        out
    }
}

/// W302: a deck laid out in other `formats` that places a node by `rect` or by grid
/// cells (`col`/`row`): a slot moves with each format's template set, and those do not.
struct W302Fixed;
impl Rule for W302Fixed {
    fn code(&self) -> &'static str {
        "W302"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        if cx.deck.formats.is_empty() {
            return vec![];
        }
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for (i, snap) in cx.snapshots.iter().enumerate() {
            for (id, props) in &snap.nodes {
                let Some(at) = props.get("at").and_then(Value::as_object) else { continue };
                let key = if at.contains_key("rect") {
                    "rect"
                } else if at.contains_key("in") || at.contains_key("parent") {
                    continue;
                } else if at.contains_key("col") {
                    "col"
                } else if at.contains_key("row") {
                    "row"
                } else {
                    continue;
                };
                let pointer = source(cx, i, id, key).unwrap_or_else(|| format!("/overrides/{}/at/{key}", token(id)));
                if seen.insert(pointer.clone()) {
                    out.push(
                        Finding::new(
                            self.code(),
                            self.severity(),
                            format!(
                                "node `{id}` is placed by `{key}` in a deck laid out in {}: it does not move with the formats' slots",
                                cx.deck.formats.join(", ")
                            ),
                        )
                        .at(pointer)
                        .state(snap.state_id.clone())
                        .node(id.clone())
                        .hint("Place it in a slot (`at.in`), and give the slot a place in each format's template set."),
                    );
                }
            }
        }
        out
    }
}

/// W322: a motion that moves nothing. The engine runs an entrance only on a node that
/// enters in its state, and an exit only on one that leaves (under `match: none`, every
/// node does both); an emphasis or `anim` only on a node on screen; and a look's
/// `progress` draws only the outline a shape strokes. Such a cue still takes its time.
struct W322Idle;
impl Rule for W322Idle {
    fn code(&self) -> &'static str {
        "W322"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let mut out = Vec::new();
        for (i, state) in cx.deck.states.iter().enumerate() {
            let (Some(now), before) = (cx.snapshots.get(i), i.checked_sub(1).and_then(|p| cx.snapshots.get(p))) else {
                continue;
            };
            let matched =
                state.transition.as_ref().and_then(|t| t.get("match")).and_then(Value::as_str) != Some("none");
            let mut items = Vec::new();
            for (k, v) in state.choreography.iter().enumerate() {
                if let Ok(item) = serde_json::from_value::<ChoreoItem>(v.clone()) {
                    flatten(&item, format!("/states/{i}/choreography/{k}"), &mut items);
                }
            }
            for (pointer, t) in items {
                let targets: Vec<&str> = match &t.target {
                    Targets::One(id) => vec![id.0.as_str()],
                    Targets::Many(ids) => ids.0.iter().map(|id| id.0.as_str()).collect(),
                };
                for id in targets {
                    let (is, was) = (now.nodes.contains_key(id), before.is_some_and(|b| b.nodes.contains_key(id)));
                    let idle = if t.enter.is_some() {
                        (!is)
                            .then_some("an entrance on a node that is not on screen")
                            .or((was && matched).then_some("an entrance on a node that was already on screen"))
                    } else if t.exit.is_some() {
                        (!was)
                            .then_some("an exit on a node that was not on screen")
                            .or((is && matched).then_some("an exit on a node that stays on screen"))
                    } else {
                        (!is).then_some("a motion on a node that is not on screen")
                    };
                    let reason = idle.map(str::to_string).or_else(|| {
                        let drawn = drawn_only(cx, &t)?;
                        let props = now.nodes.get(id).or_else(|| before.and_then(|b| b.nodes.get(id)))?;
                        let stroked = cx.deck.nodes.get(id).is_some_and(|n| match n.node_type {
                            NodeType::Shape => props.get("stroke").is_some_and(|s| !s.is_null()),
                            NodeType::Group | NodeType::Stack | NodeType::Grid | NodeType::Frame => true,
                            _ => false,
                        });
                        (!stroked).then(|| format!("{drawn} on a node that strokes no outline"))
                    });
                    if let Some(reason) = reason {
                        out.push(
                            Finding::new(
                                self.code(),
                                self.severity(),
                                format!("state `{}` choreographs {reason} (`{id}`): it moves nothing, and still takes its time", state.id),
                            )
                            .at(pointer.clone())
                            .state(state.id.clone())
                            .node(id)
                            .hint("Remove the item, or move it to the state where the node enters, leaves, or is on screen."),
                        );
                    }
                }
            }
        }
        out
    }
}

/// Every choreography target in `item`, with its pointer.
fn flatten(item: &ChoreoItem, pointer: String, out: &mut Vec<(String, ChoreoTarget)>) {
    match item {
        ChoreoItem::Target(t) => out.push((pointer, (**t).clone())),
        ChoreoItem::Sequence(s) => {
            for (j, i) in s.sequence.iter().enumerate() {
                flatten(i, format!("{pointer}/sequence/{j}"), out);
            }
        }
        ChoreoItem::Parallel(p) => {
            for (j, i) in p.parallel.iter().enumerate() {
                flatten(i, format!("{pointer}/parallel/{j}"), out);
            }
        }
    }
}

/// `"a draw-on"` when all a target's motion does is draw outlines on (a look whose only
/// change is `progress`, or `anim` with only a `progress` track); `None` otherwise.
fn drawn_only(cx: &Context, t: &ChoreoTarget) -> Option<&'static str> {
    if let Some(tracks) = &t.anim {
        return (tracks.0.keys().all(|k| k == "progress") && !tracks.0.is_empty()).then_some("a draw-on");
    }
    let preset = t.enter.as_ref().or(t.exit.as_ref()).or(t.emphasis.as_ref())?;
    let (name, params) = match preset {
        PresetRef::Named(name) => (name.as_str(), None),
        PresetRef::With(call) => (call.preset.as_str(), call.params.as_ref()),
    };
    let p = cx.theme?.motion.presets.get(name)?;
    let look: &PresetLook =
        if t.emphasis.is_some() { p.to.as_ref().or(p.from.as_ref())? } else { p.from.as_ref().or(p.to.as_ref())? };
    let progress = params.and_then(|p| p.progress).or(look.params.as_ref().and_then(|p| p.progress));
    let other = look.opacity.is_some_and(|o| o != 1.0) || look.transform.is_some() || look.color.is_some();
    (progress.is_some_and(|p| p != 1.0) && !other).then_some("a draw-on")
}

struct W401StateNotInBeat;
impl Rule for W401StateNotInBeat {
    fn code(&self) -> &'static str {
        "W401"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let Some(spine) = &cx.deck.spine else { return vec![] };
        let referenced: Vec<&String> =
            spine.sections.iter().flat_map(|s| s.beats.iter()).flat_map(|b| b.states.iter()).collect();
        cx.deck.states.iter().enumerate().filter(|(_, s)| !referenced.contains(&&s.id)).map(|(i, s)| {
            Finding::new(self.code(), self.severity(), format!("state `{}` is not referenced by any beat", s.id))
                .at(format!("/states/{i}")).state(s.id.clone())
                .hint("Add it to a beat's `states` so projections (PDF order, podcast, infographic) know where it belongs.")
        }).collect()
    }
}

struct W410ImageWithoutAlt;
impl Rule for W410ImageWithoutAlt {
    fn code(&self) -> &'static str {
        "W410"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        cx.deck
            .nodes
            .iter()
            .filter(|(_, n)| n.node_type == NodeType::Image && !n.props.contains_key("alt"))
            .map(|(id, _)| {
                Finding::new(self.code(), self.severity(), format!("image `{id}` has no `alt` text"))
                    .at(format!("/nodes/{id}"))
                    .node(id.clone())
                    .hint("Describe the image for screen readers and tagged PDF, or set alt to \"\" if decorative.")
            })
            .collect()
    }
}

struct I400NoopState;
impl Rule for I400NoopState {
    fn code(&self) -> &'static str {
        "I400"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        cx.snapshots
            .windows(2)
            .enumerate()
            .filter_map(|(i, w)| {
                let (prev, cur) = (&w[0], &w[1]);
                let st = &cx.deck.states[i + 1];
                let same = prev.nodes == cur.nodes && st.choreography.is_empty() && prev.layout == cur.layout;
                same.then(|| {
                    Finding::new(
                        self.code(),
                        self.severity(),
                        format!("state `{}` is identical to `{}`", cur.state_id, prev.state_id),
                    )
                    .at(format!("/states/{}", i + 1))
                    .state(cur.state_id.clone())
                    .hint("Remove it, or give it a change worth a click.")
                })
            })
            .collect()
    }
}

struct I401NodeNeverVisible;
impl Rule for I401NodeNeverVisible {
    fn code(&self) -> &'static str {
        "I401"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        cx.deck
            .nodes
            .keys()
            .filter(|id| !cx.snapshots.iter().any(|s| s.nodes.contains_key(*id)))
            .map(|id| {
                Finding::new(self.code(), self.severity(), format!("node `{id}` is never visible in any state"))
                    .at(format!("/nodes/{id}"))
                    .node(id.clone())
            })
            .collect()
    }
}

struct I402OverrideCount;
impl Rule for I402OverrideCount {
    fn code(&self) -> &'static str {
        "I402"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        cx.deck
            .overrides
            .iter()
            .filter(|(_, o)| !o.is_empty())
            .map(|(id, _)| {
                let props = cx.deck.overridden(id);
                let named: Vec<String> = props.iter().map(|p| format!("`{}`", &p[1..])).collect();
                let n = props.len();
                Finding::new(
                    self.code(),
                    self.severity(),
                    format!(
                        "node `{id}` has {n} override{} ({}); it is not theme-safe",
                        if n == 1 { "" } else { "s" },
                        named.join(", ")
                    ),
                )
                .at(format!("/overrides/{id}"))
                .node(id.clone())
                .measure(serde_json::json!({ "overrides": props }))
            })
            .collect()
    }
}
