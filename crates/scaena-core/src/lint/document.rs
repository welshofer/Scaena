//! Document-level design and housekeeping rules: what the document says, with the
//! theme's thresholds, before anything is laid out.

use super::{Context, Finding, Rule, Severity};
use crate::document::{NodeType, Props};
use crate::model::Format;
use crate::model::states::{ChoreoItem, ChoreoTarget, Targets};
use crate::model::theme::{Grid, Snap};
use crate::model::values::{PresetLook, PresetRef};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(W210Density),
        Box::new(W221OffGrid),
        Box::new(W300Literal),
        Box::new(W301Rect),
        Box::new(W302Fixed),
        Box::new(W322Idle),
        Box::new(W401StateNotInBeat),
        Box::new(W410ImageWithoutAlt),
        Box::new(I400NoopState),
        Box::new(I401NodeNeverVisible),
        Box::new(I402OverrideCount),
        Box::new(I403UncoloredLanguage),
        Box::new(I404Haiku),
        Box::new(W204FocusPastLast),
    ]
}

/// A JSON pointer token: `~` and `/` escaped (RFC 6901).
pub(super) fn token(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// W210: more words on screen than the theme's `density.maxWordsPerState` (40 when it
/// says nothing). A word is what whitespace separates, in every visible text node but one that
/// navigates (`semantic: navigation`: a running head, a section's number), which is read past,
/// and code (PLAN 2.131), whose words are read as code, not counted as prose.
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
                .filter(|(id, props)| {
                    cx.deck.nodes.get(*id).is_some_and(|n| {
                        n.node_type == NodeType::Text
                            && n.props.get("semantic").and_then(Value::as_str) != Some("navigation")
                            && !props.get("code").is_some_and(Value::is_object)
                    })
                })
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

/// The words a text node shows: its `text`, or its runs' text read as one, so a word that
/// runs split ("$" "48.2" "M", a quoted figure) is one word. A word is what whitespace separates
/// (W210, and the reading W323 sizes a hold by).
pub fn words(props: &Props) -> usize {
    match props.get("runs").and_then(Value::as_array) {
        Some(runs) => runs.iter().filter_map(|r| r.get("text").and_then(Value::as_str)).collect::<String>(),
        None => props.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
    }
    .split_whitespace()
    .count()
}

/// How far `size × leading` may miss a whole number of grid lines and still count as
/// whole, in grid lines: float error, as the engine allows it (SPEC §3.4).
const GRID_EPSILON: f64 = 1.0e-3;

/// W221: a text role that snaps its baselines to the baseline grid (`snap: baseline`)
/// with a leading, `size × leading`, that is not a whole number of the grid's lines, so
/// the engine sets its lines further apart than it says (SPEC §3.4); or a grid with no
/// baseline for it to snap to. Every grid the deck is laid out on counts: the theme's,
/// and that of each format it lists with a baseline of its own.
struct W221OffGrid;
impl Rule for W221OffGrid {
    fn code(&self) -> &'static str {
        "W221"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let Some(theme) = cx.theme else { return Vec::new() };
        // A theme file's paths start at its root; an inline theme's under `/theme`.
        let (file, root) = match &cx.deck.theme {
            Some(Value::String(path)) => (Some(path.as_str()), ""),
            _ => (None, "/theme"),
        };
        // The theme's grid, and each listed format's own where it is laid out on one with
        // another baseline: the same baseline would say the same again.
        let own = [cx.deck.canvas.width, cx.deck.canvas.height];
        let mut grids: Vec<(Option<&str>, &Grid)> = vec![(None, &theme.grid)];
        for name in &cx.deck.formats {
            let Some(format) = Format::parse(name).filter(|f| f.canvas(own) != own) else { continue };
            if let Some(grid) = theme.formats.as_ref().and_then(|f| f.get(&format)).and_then(|f| f.grid.as_ref())
                && grid.baseline != theme.grid.baseline
            {
                grids.push((Some(name), grid));
            }
        }
        let mut out = Vec::new();
        for (name, role) in &theme.typography.roles {
            if role.snap != Some(Snap::Baseline) {
                continue;
            }
            let at = format!("{root}/type/roles/{}", token(name));
            let leading = role.size * role.leading;
            for (format, grid) in &grids {
                let there = format.map_or(String::new(), |f| format!(" in {f}"));
                let finding = match grid.baseline {
                    None => Finding::new(
                        self.code(),
                        self.severity(),
                        format!("role `{name}` snaps to the baseline grid, and the grid{there} has none"),
                    )
                    .at(format!("{at}/snap"))
                    .hint("Give the grid a `baseline`, or take `snap` off the role."),
                    Some(pitch) => {
                        let lines = leading / pitch;
                        if (lines - lines.round()).abs() <= GRID_EPSILON && lines.round() >= 1.0 {
                            continue;
                        }
                        let set = (lines - GRID_EPSILON).ceil().max(1.0) * pitch;
                        let whole = |n: f64| format!("{}", (n * pitch / role.size * 1e4).round() / 1e4);
                        let below = (lines.floor() >= 1.0).then(|| whole(lines.floor()));
                        let options = match below {
                            Some(below) => format!("{below} or {}", whole(lines.ceil())),
                            None => whole(lines.ceil().max(1.0)),
                        };
                        Finding::new(
                            self.code(),
                            self.severity(),
                            format!(
                                "role `{name}` snaps to the baseline grid{there}, and its leading is {leading:.2} cu, \
                                 {lines:.2} lines of {pitch} cu: its lines are set {set} cu apart"
                            ),
                        )
                        .at(format!("{at}/leading"))
                        .measure(
                            serde_json::json!({ "leading": leading, "baseline": pitch, "lines": lines, "set": set }),
                        )
                        .hint(format!("Make `leading` a whole number of grid lines over the size: {options}."))
                    }
                };
                let finding = match format {
                    Some(f) => finding.format(*f),
                    None => finding,
                };
                out.push(match file {
                    Some(file) => finding.file(file),
                    None => finding,
                });
            }
        }
        out
    }
}

/// W300: a pixel or color literal outside `overrides`: a color written out (`#rrggbb`,
/// `oklch(…)`, `oklab(…)`) where a theme color would go, a text `size`, a length in
/// canvas units where a theme token would go (`radius`, `gap`, `padding`, `inset`, a
/// stroke's `width`, a child's `size`), or a shadow written out where the theme names its
/// shadows (PLAN 2.115). Placement by `rect` is W301's.
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

/// Whether `value`, set as a node's `prop` (a property, or one key of one: `style/size`),
/// is a literal W300 flags outside `overrides`: a color written out where a theme color goes,
/// a text size, a length in canvas units where a theme token goes, or a shadow written out.
pub(crate) fn literal(prop: &str, value: &Value) -> bool {
    let set = match prop.split_once('/') {
        Some((name, key)) => (name, Value::Object(serde_json::Map::from_iter([(key.to_string(), value.clone())]))),
        None => (prop, value.clone()),
    };
    !literals(&Props::from_iter([(set.0.to_string(), set.1)]), "").is_empty()
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
        // A shadow written out (PLAN 2.115) is one literal, whatever it holds.
        if k == "shadow" && v.is_object() {
            out.push((format!("{base}/shadow"), "a shadow written out".into()));
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
/// A node that says where it goes in each of those formats (`formats`, ADR-0020) is placed.
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
        // The formats the deck lays out anew: each one but its own canvas's shape.
        let others = cx.deck.anew();
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for (i, snap) in cx.snapshots.iter().enumerate() {
            for (id, props) in &snap.nodes {
                let Some(at) = props.get("at").and_then(Value::as_object) else { continue };
                // A node placed anew in each of those formats (ADR-0020) is placed for each.
                let anew = |f: &&str| props.get("formats").and_then(|l| l.get(*f)).and_then(|l| l.get("at")).is_some();
                if !others.is_empty() && others.iter().all(anew) {
                    continue;
                }
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
                        .hint("Place it in a slot (`at.in`), and give the slot a place in each format's template set; or place it in each format (`formats`)."),
                    );
                }
            }
        }
        out
    }
}

/// W322: a motion that moves nothing. The engine runs an entrance only on a node that
/// enters in its state, and an exit only on one that leaves (under `match: none`, every
/// node does both; under a slide transition, every node enters and no exit runs, the slide
/// left moving whole); an emphasis or `anim` only on a node on screen; and a look's
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
            // A slide transition moves the slide left whole and matches nothing (PLAN 2.111).
            let slide = state.transition.as_ref().and_then(|t| t.get("style")).is_some();
            let matched = !slide
                && state.transition.as_ref().and_then(|t| t.get("match")).and_then(Value::as_str) != Some("none");
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
                            .or(slide.then_some("an exit under a slide transition, which moves the slide left whole"))
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
                            .hint("Remove the item, or move it to the state where the node enters, leaves, or is on screen.")
                            .fix(untargeted(&state.choreography, i, &pointer, id)),
                        );
                    }
                }
            }
        }
        out
    }
}

/// The patch that takes `id` out of the motion at `pointer` in state `i`'s `choreography`: out of
/// its targets where it has others, else the motion goes, and a sequence or a parallel it leaves
/// with none goes with it.
fn untargeted(choreography: &[Value], i: usize, pointer: &str, id: &str) -> Vec<Value> {
    let base = format!("/states/{i}/choreography");
    let Some(rest) = pointer.strip_prefix(&base) else { return Vec::new() };
    let root = Value::Array(choreography.to_vec());
    let Some(item) = root.pointer(rest) else { return Vec::new() };
    if let Some(Value::Array(targets)) = item.get("target")
        && targets.len() > 1
        && let Some(k) = targets.iter().position(|t| t.as_str() == Some(id))
    {
        return vec![serde_json::json!({ "op": "remove", "path": format!("{pointer}/target/{k}") })];
    }
    let mut steps: Vec<&str> = rest.split('/').skip(1).collect();
    while steps.len() >= 3 {
        let parent = format!("/{}", steps[..steps.len() - 1].join("/"));
        if root.pointer(&parent).and_then(Value::as_array).is_some_and(|a| a.len() == 1) {
            steps.truncate(steps.len() - 2);
        } else {
            break;
        }
    }
    vec![serde_json::json!({ "op": "remove", "path": format!("{base}/{}", steps.join("/")) })]
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
            // A map reads as the map of the place it was fetched for (ADR-0030).
            .filter(|(_, n)| {
                n.node_type == NodeType::Image
                    && !n.props.contains_key("alt")
                    && n.props.get("fetched").and_then(|f| f.get("map")).is_none()
            })
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

/// I404 (PLAN O.4h, ADR-0030 decision 30): a text in the verse role whose words are three lines,
/// in English or in kana alone, that does not count 5, 7, and 5 (`scaena_core::verse`). One
/// finding per text, at the first state that shows it, its counts in `measure`, and no fix: a
/// poem's words are its author's. A line that cannot be counted, a fourth line, or another
/// language leaves it unjudged.
struct I404Haiku;
impl Rule for I404Haiku {
    fn code(&self) -> &'static str {
        "I404"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let deck_lang = cx.deck.meta.as_ref().and_then(|m| m.lang.as_deref()).unwrap_or("");
        let mut out = Vec::new();
        for (id, node) in &cx.deck.nodes {
            if node.node_type != NodeType::Text {
                continue;
            }
            let Some(snap) = cx.snapshots.iter().find(|s| s.nodes.contains_key(id)) else { continue };
            let props = &snap.nodes[id];
            if role(cx, snap.layout.as_deref(), props).as_deref() != Some("verse") {
                continue;
            }
            let words = crate::reading::words(props);
            let lines: Vec<&str> = words.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            if lines.len() != 3 {
                continue;
            }
            // Kana alone counts as morae, whatever the deck's language; else English counts.
            let kana = lines.iter().all(|l| crate::verse::morae(l).is_some());
            let lang = match props.get("lang").and_then(Value::as_str).unwrap_or(deck_lang) {
                _ if kana => "ja",
                l if l.split(['-', '_']).next().is_some_and(|c| c.eq_ignore_ascii_case("en")) => "en",
                _ => continue,
            };
            let checked = crate::verse::check(&lines.join("\n"), lang);
            if matches!(checked.kept, crate::verse::Kept::Kept | crate::verse::Kept::Uncounted) {
                continue;
            }
            let counts: Vec<String> = checked
                .lines
                .iter()
                .filter_map(|l| l.count)
                .map(|c| if c.low == c.high { c.low.to_string() } else { format!("{}–{}", c.low, c.high) })
                .collect();
            let [a, b, c] = [&counts[0], &counts[1], &counts[2]];
            out.push(
                Finding::new(
                    self.code(),
                    self.severity(),
                    format!("This haiku's lines count {a}, {b}, and {c}, not 5, 7, and 5."),
                )
                .at(format!("/nodes/{id}"))
                .node(id.clone())
                .state(snap.state_id.clone())
                .measure(serde_json::json!({
                    "counts": checked.lines.iter().filter_map(|l| l.count).map(|c| [c.low, c.high]).collect::<Vec<_>>(),
                    "targets": crate::verse::HAIKU,
                    "unit": if lang == "ja" { "morae" } else { "syllables" },
                }))
                .hint("A haiku's lines count 5, 7, and 5. The words are the poet's to change, or to keep."),
            );
        }
        out
    }
}

/// A text's role as a state shows it: its own, else its slot's in the state's layout.
fn role(cx: &Context, layout: Option<&str>, props: &Props) -> Option<String> {
    if let Some(role) = props.get("role").and_then(Value::as_str) {
        return Some(role.to_string());
    }
    let slot = props.get("at")?.get("in")?.as_str()?;
    cx.theme?.layouts.get(layout?)?.slots.get(slot)?.role.clone()
}

/// I403: a code text in a language no scanner of its own colors (ADR-0029, PLAN 2.132), which the
/// generic scanner colors: its strings, numbers, comments, and brackets, and none of its words.
/// One finding a text and language, where the language is written, in the first state it shows
/// in, with every state it shows so in. The hint names the language Scaena knows whose name, or
/// one of its aliases, is within two letters of it, if one is.
struct I403UncoloredLanguage;
impl Rule for I403UncoloredLanguage {
    fn code(&self) -> &'static str {
        "I403"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        use crate::tracking::{Lives, lives};
        let mut out: Vec<Finding> = Vec::new();
        let mut seen: Vec<(String, String, usize)> = Vec::new();
        for (i, snap) in cx.snapshots.iter().enumerate() {
            for (id, props) in &snap.nodes {
                // The language as the state shows it, the deck's overrides over it, read in place:
                // most texts are no code, and no props are copied to find so.
                let over = cx.deck.overrides.get(id);
                let code = over.and_then(|o| o.get("code")).or_else(|| props.get("code"));
                let language = code.and_then(|c| c.get("language")).and_then(Value::as_str);
                if language.is_none() || cx.deck.nodes.get(id).is_none_or(|n| n.node_type != NodeType::Text) {
                    continue;
                }
                let Some(name) = language.filter(|n| matches!(crate::code::lang(n), crate::code::Lang::Generic)) else {
                    continue;
                };
                if let Some((_, _, at)) = seen.iter().find(|(n, l, _)| n == id && l == name) {
                    if let Some(states) = out[*at].measure.as_mut().and_then(|m| m["states"].as_array_mut()) {
                        states.push(snap.state_id.clone().into());
                    }
                    continue;
                }
                let written = over.and_then(|o| o.get("code")).is_some_and(|c| c.get("language").is_some());
                let at = match (written, lives(cx.deck, i, id, "code", &["language"])) {
                    (true, _) => format!("/overrides/{}", token(id)),
                    (false, Lives::State(j)) => format!("/states/{j}/props/{}", token(id)),
                    (false, Lives::Node) => format!("/nodes/{}", token(id)),
                };
                let hint = match nearest(name) {
                    Some(near) => format!(
                        "Did you mean `{near}`? Name a language Scaena knows to color its words too, or `plain` to \
                         color nothing."
                    ),
                    None => "Name a language Scaena knows to color its words too, or `plain` to color nothing.".into(),
                };
                seen.push((id.clone(), name.to_string(), out.len()));
                out.push(
                    Finding::new(
                        self.code(),
                        self.severity(),
                        format!(
                            "`{id}` is code in `{name}`, which no scanner of its own colors: only its strings, numbers, \
                             comments, and brackets are colored"
                        ),
                    )
                    .at(format!("{at}/code/language"))
                    .state(snap.state_id.clone())
                    .node(id.clone())
                    .measure(serde_json::json!({ "language": name, "states": [snap.state_id] }))
                    .hint(hint),
                );
            }
        }
        out
    }
}

/// W204: a code text's lines in focus (PLAN 2.140) that name a line past its last, which nothing
/// draws: the lines it has are set back for a line that is not there. One finding a text and the
/// place its focus is written, in the first state it is so in, with every state it is so in. The
/// fix holds each span to the lines the text has in that state, and takes away a span past them
/// all, and the focus where none is left.
struct W204FocusPastLast;
impl Rule for W204FocusPastLast {
    fn code(&self) -> &'static str {
        "W204"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        use crate::tracking::{Lives, lives};
        let mut out: Vec<Finding> = Vec::new();
        let mut seen: Vec<(String, String, usize)> = Vec::new();
        for (i, snap) in cx.snapshots.iter().enumerate() {
            for (id, props) in &snap.nodes {
                if cx.deck.nodes.get(id).is_none_or(|n| n.node_type != NodeType::Text) {
                    continue;
                }
                // The focus as the state shows it, the deck's overrides over it.
                let over = cx.deck.overrides.get(id).and_then(|o| o.get("code")).and_then(|c| c.get("focus"));
                let shown = props.get("code").and_then(|c| c.get("focus"));
                let Some(spans) = over.or(shown).and_then(Value::as_array) else { continue };
                let lines = crate::lists::paragraphs(&crate::reading::words(props)).len() as u64;
                let ends: Vec<(u64, u64)> = spans.iter().filter_map(span).collect();
                let Some(past) = ends.iter().map(|&(_, b)| b).filter(|&b| b > lines).max() else { continue };
                let at = match (over.is_some(), lives(cx.deck, i, id, "code", &["focus"])) {
                    (true, _) => format!("/overrides/{}", token(id)),
                    (false, Lives::State(j)) => format!("/states/{j}/props/{}", token(id)),
                    (false, Lives::Node) => format!("/nodes/{}", token(id)),
                };
                if let Some((_, _, k)) = seen.iter().find(|(n, a, _)| n == id && *a == at) {
                    if let Some(states) = out[*k].measure.as_mut().and_then(|m| m["states"].as_array_mut()) {
                        states.push(snap.state_id.clone().into());
                    }
                    continue;
                }
                let held: Vec<Value> = (ends.iter())
                    .filter(|&&(a, _)| a <= lines)
                    .map(|&(a, b)| match b.min(lines) {
                        b if b == a => Value::from(a),
                        b => Value::from(vec![a, b]),
                    })
                    .collect();
                let pointer = format!("{at}/code/focus");
                let fix = match held.is_empty() {
                    true => serde_json::json!({ "op": "remove", "path": pointer }),
                    false => serde_json::json!({ "op": "replace", "path": pointer, "value": held }),
                };
                seen.push((id.clone(), at.clone(), out.len()));
                out.push(
                    Finding::new(
                        self.code(),
                        self.severity(),
                        format!("`{id}` has line {past} in focus, past its last, line {lines}: nothing draws it"),
                    )
                    .at(pointer)
                    .state(snap.state_id.clone())
                    .node(id.clone())
                    .measure(serde_json::json!({ "line": past, "lines": lines, "states": [snap.state_id] }))
                    .hint("Name the lines the text has, or take the focus away to show every line.")
                    .fix(vec![fix]),
                );
            }
        }
        out
    }
}

/// A span of a code text's lines in focus as its first and last line, counted from 1: none where
/// it names no line (0) or runs backward (E106).
fn span(v: &Value) -> Option<(u64, u64)> {
    let (a, b) = match v {
        Value::Array(pair) if pair.len() == 2 => (pair[0].as_u64()?, pair[1].as_u64()?),
        one => (one.as_u64()?, one.as_u64()?),
    };
    (a >= 1 && a <= b).then_some((a, b))
}

/// The language Scaena knows by a name or an alias within two letters of `name`, as edits of one
/// letter count them, the nearest, and the first in the table's order of those as near; none where
/// no name is that near, or as near as `name` is long.
fn nearest(name: &str) -> Option<&'static str> {
    let mut best: Option<(usize, &'static str)> = None;
    for language in crate::code::LANGUAGES {
        for known in std::iter::once(&language.name).chain(language.aliases) {
            let d = edits(name.as_bytes(), known.as_bytes());
            if d <= 2 && d < name.len() && best.is_none_or(|(b, _)| d < b) {
                best = Some((d, language.name));
            }
        }
    }
    best.map(|(_, name)| name)
}

/// How many letters must be put in, taken out, or changed to make `a` into `b` (Levenshtein).
fn edits(a: &[u8], b: &[u8]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1).min(row[j] + 1).min(diagonal + usize::from(x != y));
            diagonal = above;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_that_runs_split_is_one_word() {
        let props = |v: Value| -> Props { serde_json::from_value(v).unwrap() };
        let text = props(serde_json::json!({ "text": "ARR is up to $48.2M" }));
        let runs = props(serde_json::json!({ "runs": [
            { "text": "ARR is up to $" },
            { "text": "48.2", "quote": { "data": "@kpi", "column": "arr" } },
            { "text": "M" }
        ] }));
        assert_eq!((words(&text), words(&runs)), (5, 5));
    }
}
