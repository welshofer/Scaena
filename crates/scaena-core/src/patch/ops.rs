//! The semantic ops (SPEC §7.3), each compiled to RFC 6902 against the deck it applies to.
//! Each checks what it names before it compiles, and refuses what would not do what it
//! says, with what to do instead: an op on a node that is not there, or a change in a state
//! to a node not on screen there, which would make it enter.

use super::{Annotating, JsonOp, Listing, Renamed, SemanticOp, Spot, Timed, esc};
use crate::data;
use crate::document::{Deck, Props};
use crate::ids::is_valid_id;
use crate::lint::literal;
use crate::model::theme::Theme;
use crate::model::values::{Annotation, Duration, ListItem};
use crate::tracking::{Lives, Snapshot, layout_lives, lives, merge_props, other_spelling, resolve_states, tracks_from};
use crate::validate::BundleFiles;
use serde_json::{Map, Value};
use std::ops::Range;

/// What one semantic op compiles to.
pub(super) struct Out {
    pub ops: Vec<JsonOp>,
    pub renamed: Vec<Renamed>,
}

pub(super) fn compile(doc: &Value, op: &SemanticOp, files: &dyn BundleFiles) -> Result<Out, String> {
    let d = Doc(doc);
    let mut renamed = Vec::new();
    let ops = match op {
        SemanticOp::AddNode { id, node, state, props } => {
            d.free("node", id, d.node(id).is_ok())?;
            if props.is_some() && state.is_none() {
                return Err("`props` is the node's delta in the state it enters: name the `state`".into());
            }
            let value = serde_json::to_value(node).map_err(|e| e.to_string())?;
            let mut ops = vec![JsonOp::Add { path: format!("/nodes/{}", esc(id)), value }];
            if let Some(state) = state {
                ops.push(enter(&d, d.state(state)?, id, props.clone().unwrap_or_default()));
            }
            ops
        }
        SemanticOp::RemoveNode { id } => remove_node(&d, id)?,
        SemanticOp::RenameNode { id, to } => {
            renamed.push(Renamed::Node { from: id.clone(), to: to.clone() });
            rename_node(&d, id, to)?
        }
        SemanticOp::ShowNode { node, state, props } => show_node(&d, node, state, props.clone())?,
        SemanticOp::HideNode { node, state } => hide_node(&d, node, state)?,
        SemanticOp::Group { id, nodes, state } => group(&d, id, nodes, state.as_deref())?,
        SemanticOp::Ungroup { group } => ungroup(&d, group)?,
        SemanticOp::SetProp { node, prop, value, state } => {
            d.node(node)?;
            let (name, key) = parse_prop(prop)?;
            if name == "type" {
                return Err("a node's `type` is what it is (E104): remove the node and add another".into());
            }
            let at = d.showing(node, state.as_deref())?;
            set(&d, node, vec![(name, key, value.clone())], at.map(|(i, _)| i))?
        }
        SemanticOp::Place { node, at, state, fork, format, anew } => match format {
            Some(f) => place_in_format(&d, node, at, state.as_deref(), *fork, f, *anew)?,
            None => place_node(&d, node, at, state.as_deref(), *fork)?,
        },
        SemanticOp::SetText { node, text, state } => {
            let kind = d.kind(node)?;
            if kind != "text" {
                return Err(format!("`{node}` is a {kind} node; `set_text` sets a text node's `text`"));
            }
            let at = d.showing(node, state.as_deref())?;
            let shown = match &at {
                Some((_, props)) => props.contains_key("runs"),
                None => d.node(node)?.contains_key("runs"),
            };
            let mut entries = vec![("text".to_string(), None, Value::String(text.clone()))];
            if shown {
                entries.push(("runs".to_string(), None, Value::Null));
            }
            set(&d, node, entries, at.map(|(i, _)| i))?
        }
        SemanticOp::ReplaceText { node, from, to, text, state, fork } => {
            replace_text(&d, node, (*from as usize, *to as usize), text, state.as_deref(), *fork)?
        }
        SemanticOp::StyleText { node, from, to, look, state, fork } => {
            style_text(&d, node, (*from as usize, *to as usize), look, state.as_deref(), *fork)?
        }
        SemanticOp::List { node, from, to, kind, level, by, state, fork } => {
            mark_list(&d, node, (*from as usize, *to as usize), (*kind, *level, *by), state.as_deref(), *fork)?
        }
        SemanticOp::Choose { node, prop, value, state, fork } => match prop.as_str() {
            "data" if !value.is_null() && matches!(d.kind(node)?, "chart" | "table") => {
                choose_data(&d, files, node, value, state.as_deref(), *fork)?
            }
            _ => choose(&d, node, prop, value, state.as_deref(), *fork)?,
        },
        SemanticOp::Annotate { node, index, annotation, state, fork } => {
            annotate(&d, node, *index, annotation.as_ref(), state.as_deref(), *fork)?
        }
        SemanticOp::BindData { node, data, source, state } => {
            let kind = d.kind(node)?;
            if !matches!(kind, "chart" | "table") {
                return Err(format!("`{node}` is a {kind} node; charts and tables read data"));
            }
            let name = data.strip_prefix('@').unwrap_or(data);
            if !is_valid_id(name) {
                return Err(format!("`{data}` is not a data source's id"));
            }
            let mut ops = Vec::new();
            match source {
                Some(source) => {
                    let value = serde_json::to_value(source).map_err(|e| e.to_string())?;
                    ops.push(match d.0.get("data") {
                        Some(Value::Object(_)) => JsonOp::Add { path: format!("/data/{}", esc(name)), value },
                        _ => JsonOp::Add { path: "/data".into(), value: object(name, value) },
                    });
                }
                None if d.0.get("data").and_then(|m| m.get(name)).is_none() => {
                    return Err(format!("the deck has no data source `{name}`: declare it with `source`"));
                }
                None => {}
            }
            let at = d.showing(node, state.as_deref())?;
            let entry = ("data".to_string(), None, Value::String(format!("@{name}")));
            ops.extend(set(&d, node, vec![entry], at.map(|(i, _)| i))?);
            ops
        }
        SemanticOp::ApplyPreset { node, preset, motion, state } => {
            let kind = d.kind(node)?;
            let at = d.showing(node, state.as_deref())?;
            match motion {
                Some(motion) => {
                    if let Ok(theme) = d.theme(files)
                        && !theme.motion.presets.contains_key(preset)
                    {
                        let names = list(theme.motion.presets.keys());
                        return Err(format!("the theme has no motion preset `{preset}`; it has {names}"));
                    }
                    let entry = (motion.key().to_string(), None, Value::String(preset.clone()));
                    set(&d, node, vec![entry], at.map(|(i, _)| i))?
                }
                None => {
                    if kind != "shader" {
                        return Err(format!(
                            "`{node}` is a {kind} node: name the `motion` a motion preset is for (`enter`, `exit`, `emphasis`); only a shader takes a preset whole"
                        ));
                    }
                    let theme = d.theme(files)?;
                    let presets = theme.shaders.as_ref().and_then(|s| s.presets.as_ref());
                    let Some(found) = presets.and_then(|p| p.get(preset)) else {
                        let names = list(presets.into_iter().flat_map(|p| p.keys()));
                        return Err(format!("the theme has no shader preset `{preset}`; it has {names}"));
                    };
                    let kind = serde_json::to_value(found.kind).map_err(|e| e.to_string())?;
                    let current = match &at {
                        Some((_, props)) => props.clone(),
                        None => d.node(node)?.clone().into_iter().collect(),
                    };
                    let mut entries = vec![("preset".to_string(), None, Value::String(preset.clone()))];
                    if current.get("kind") != Some(&kind) {
                        entries.push(("kind".to_string(), None, kind));
                    }
                    for key in ["palette", "params"] {
                        if current.contains_key(key) {
                            entries.push((key.to_string(), None, Value::Null));
                        }
                    }
                    set(&d, node, entries, at.map(|(i, _)| i))?
                }
            }
        }
        SemanticOp::TimeMotion { node, motion, state, delay, duration } => {
            time_motion(&d, files, node, *motion, state, delay.as_ref().map(|d| d.0), duration.as_ref())?
        }
        SemanticOp::AddState { state, after, before, beat } => {
            d.free("state", &state.id, d.state(&state.id).is_ok())?;
            let at = place(&d, after.as_deref(), before.as_deref(), None)?.unwrap_or(d.states().len());
            let value = serde_json::to_value(state).map_err(|e| e.to_string())?;
            let mut ops = vec![JsonOp::Add { path: format!("/states/{at}"), value }];
            if let Some(beat) = beat {
                let (s, b, found) = d.beat(beat)?;
                let id = Value::String(state.id.clone());
                ops.push(match found.get("states") {
                    Some(Value::Array(_)) => {
                        JsonOp::Add { path: format!("/spine/sections/{s}/beats/{b}/states/-"), value: id }
                    }
                    _ => JsonOp::Add {
                        path: format!("/spine/sections/{s}/beats/{b}/states"),
                        value: Value::Array(vec![id]),
                    },
                });
            }
            ops
        }
        SemanticOp::MoveState { id, after, before } => {
            let from = d.state(id)?;
            if after.as_deref() == Some(id) || before.as_deref() == Some(id) {
                return Err(format!("`{id}` cannot move next to itself"));
            }
            let to = place(&d, after.as_deref(), before.as_deref(), Some(from))?
                .ok_or("say where: `after` or `before` a state")?;
            if to == from {
                Vec::new()
            } else {
                vec![JsonOp::Move { from: format!("/states/{from}"), path: format!("/states/{to}") }]
            }
        }
        SemanticOp::RemoveState { id } => remove_state(&d, id)?,
        SemanticOp::RenameState { id, to } => {
            renamed.push(Renamed::State { from: id.clone(), to: to.clone() });
            rename_state(&d, id, to)?
        }
        SemanticOp::SetState { id, prop, value, fork } => set_state(&d, id, prop, value, *fork)?,
        SemanticOp::Retheme { theme } => {
            match theme {
                Value::String(path) if !files.exists(path) => {
                    return Err(format!("`{path}` is not in the bundle; `scaena theme --apply` copies a theme in"));
                }
                Value::String(_) | Value::Object(_) => {}
                _ => return Err("`theme` is the path of a theme file in the bundle, or a theme inline".into()),
            }
            vec![JsonOp::Add { path: "/theme".into(), value: theme.clone() }]
        }
    };
    Ok(Out { ops, renamed })
}

/// The deck being patched, as JSON.
struct Doc<'a>(&'a Value);

impl<'a> Doc<'a> {
    fn nodes(&self) -> impl Iterator<Item = (&'a String, &'a Value)> {
        self.0.get("nodes").and_then(Value::as_object).into_iter().flatten()
    }

    fn node(&self, id: &str) -> Result<&'a Map<String, Value>, String> {
        self.0.get("nodes").and_then(|n| n.get(id)).and_then(Value::as_object).ok_or_else(|| format!("no node `{id}`"))
    }

    /// A node's type.
    fn kind(&self, id: &str) -> Result<&'a str, String> {
        Ok(self.node(id)?.get("type").and_then(Value::as_str).unwrap_or("untyped"))
    }

    fn states(&self) -> &'a [Value] {
        self.0.get("states").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()
    }

    /// A state's index.
    fn state(&self, id: &str) -> Result<usize, String> {
        self.states().iter().position(|s| id_of(s) == id).ok_or_else(|| format!("no state `{id}`"))
    }

    /// An id for something new: one, and not taken.
    fn free(&self, what: &str, id: &str, taken: bool) -> Result<(), String> {
        if !is_valid_id(id) {
            return Err(format!(
                "`{id}` is not an id: a lowercase letter, then lowercase letters, digits, `-`, and `_`, 64 at most"
            ));
        }
        if taken {
            return Err(format!("there is a {what} `{id}` already"));
        }
        Ok(())
    }

    /// Every state resolved (SPEC §2.2).
    fn snapshots(&self) -> Result<(Deck, Vec<Snapshot>), String> {
        let deck = Deck::from_value(self.0)
            .map_err(|e| format!("the deck must parse for its states to resolve, and it does not: {e}"))?;
        let snapshots = resolve_states(&deck).map_err(|e| e.to_string())?;
        Ok((deck, snapshots))
    }

    /// `node` as state `state` shows it: the state's index and the node's props there. An
    /// error when the node is not on screen there; `None` without a state.
    fn showing(&self, node: &str, state: Option<&str>) -> Result<Option<(usize, Props)>, String> {
        let Some(state) = state else { return Ok(None) };
        let i = self.state(state)?;
        let (_, snapshots) = self.snapshots()?;
        match snapshots[i].nodes.get(node) {
            Some(props) => Ok(Some((i, props.clone()))),
            None => Err(format!(
                "`{node}` is not on screen in `{state}`, and a change there would make it enter: `show_node` makes it enter"
            )),
        }
    }

    /// Each beat of the spine, by its section's index and its own.
    fn beats(&self) -> impl Iterator<Item = (usize, usize, &'a Value)> {
        let sections = self.0.get("spine").and_then(|s| s.get("sections")).and_then(Value::as_array);
        sections.into_iter().flatten().enumerate().flat_map(|(s, section)| {
            section
                .get("beats")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
                .map(move |(b, beat)| (s, b, beat))
        })
    }

    fn beat(&self, id: &str) -> Result<(usize, usize, &'a Value), String> {
        self.beats().find(|(_, _, b)| id_of(b) == id).ok_or_else(|| format!("no beat `{id}`"))
    }

    /// Every `at.parent` the deck writes, wherever a node's `at` is written: the pointer to
    /// it, the node, and its container.
    fn parents(&self) -> Vec<(String, &'a str, &'a str)> {
        fn at<'a>(out: &mut Vec<(String, &'a str, &'a str)>, base: String, node: &'a str, props: &'a Value) {
            if let Some(parent) = props.get("at").and_then(|a| a.get("parent")).and_then(Value::as_str) {
                out.push((format!("{base}/at/parent"), node, parent));
            }
        }
        let mut out = Vec::new();
        for (id, node) in self.nodes() {
            at(&mut out, format!("/nodes/{}", esc(id)), id, node);
        }
        for (i, state) in self.states().iter().enumerate() {
            for (id, delta) in state.get("props").and_then(Value::as_object).into_iter().flatten() {
                at(&mut out, format!("/states/{i}/props/{}", esc(id)), id, delta);
            }
        }
        for (id, props) in self.0.get("overrides").and_then(Value::as_object).into_iter().flatten() {
            at(&mut out, format!("/overrides/{}", esc(id)), id, props);
        }
        out
    }

    /// The deck's theme, read from the bundle.
    fn theme(&self, files: &dyn BundleFiles) -> Result<Theme, String> {
        match self.0.get("theme") {
            Some(Value::String(path)) => {
                let text = files.read_text(path).ok_or_else(|| format!("the theme `{path}` is not in the bundle"))?;
                Theme::from_json(&text).map_err(|e| format!("the theme `{path}` does not parse: {e}"))
            }
            Some(inline @ Value::Object(_)) => {
                Theme::from_value(inline).map_err(|e| format!("the inline theme does not parse: {e}"))
            }
            _ => Err("the deck has no theme".into()),
        }
    }
}

fn id_of(v: &Value) -> &str {
    v.get("id").and_then(Value::as_str).unwrap_or_default()
}

fn object(key: &str, value: Value) -> Value {
    Value::Object(Map::from_iter([(key.to_string(), value)]))
}

/// Ids, each in backticks, for a message.
fn list<S: AsRef<str>>(items: impl IntoIterator<Item = S>) -> String {
    let items: Vec<String> = items.into_iter().map(|s| format!("`{}`", s.as_ref())).collect();
    if items.is_empty() { "none".into() } else { items.join(", ") }
}

/// `prop` as a property's name and, for one key of an object property, that key.
fn parse_prop(prop: &str) -> Result<(String, Option<String>), String> {
    let parts: Vec<&str> = prop.split('/').collect();
    match parts[..] {
        [name] if !name.is_empty() => Ok((name.into(), None)),
        [name, key] if !name.is_empty() && !key.is_empty() => Ok((name.into(), Some(key.into()))),
        _ => Err(format!(
            "`{prop}` is not a property (`kind`) or one key of one (`at/in`): a delta merges objects one level deep (SPEC §2.2), so set a deeper value whole"
        )),
    }
}

/// One change: a property, or a key of one, and its value.
type Entry = (String, Option<String>, Value);

/// Ops that make `entries` the node's: in state `state`'s delta, or in its defaults, where
/// `null` takes a property away, and an object it leaves with nothing goes. A delta keeps
/// `null`, which takes it away from what the node tracks (SPEC §2.2). A text's `text` or
/// `runs` set there takes the other away there, as a delta's does (`other_spelling`).
fn set(d: &Doc, node: &str, entries: Vec<Entry>, state: Option<usize>) -> Result<Vec<JsonOp>, String> {
    let Some(i) = state else {
        let old = d.node(node)?;
        let mut new = old.clone();
        for (name, key, value) in entries {
            spell(&mut new, &name, key.as_deref(), &value);
            match (key, new.get_mut(&name)) {
                (None, _) if value.is_null() => drop(new.shift_remove(&name)),
                (None, _) => drop(new.insert(name, value)),
                (Some(key), Some(Value::Object(map))) if value.is_null() => {
                    map.shift_remove(&key);
                    if map.is_empty() {
                        new.shift_remove(&name);
                    }
                }
                (Some(key), Some(Value::Object(map))) => drop(map.insert(key, value)),
                (Some(_), _) if value.is_null() => {}
                (Some(key), _) => drop(new.insert(name, object(&key, value))),
            }
        }
        return Ok(diff(&format!("/nodes/{}", esc(node)), old, &new));
    };
    let props = d.states()[i].get("props").and_then(Value::as_object);
    let old = props.and_then(|p| p.get(node)).and_then(Value::as_object);
    let mut new = old.cloned().unwrap_or_default();
    for (name, key, value) in entries {
        spell(&mut new, &name, key.as_deref(), &value);
        match (key, new.get_mut(&name)) {
            (None, _) => drop(new.insert(name, value)),
            (Some(key), Some(Value::Object(map))) => drop(map.insert(key, value)),
            (Some(key), _) => drop(new.insert(name, object(&key, value))),
        }
    }
    let base = format!("/states/{i}/props/{}", esc(node));
    Ok(match (props, old) {
        (None, _) => vec![JsonOp::Add { path: format!("/states/{i}/props"), value: object(node, Value::Object(new)) }],
        (Some(_), None) => vec![JsonOp::Add { path: base, value: Value::Object(new) }],
        (Some(_), Some(old)) => diff(&base, old, &new),
    })
}

/// `props` about to be given `value` for `name` (or its `key`): a text's `text` or `runs`
/// takes the other away, as they are one property written two ways (SPEC §2.2).
fn spell(props: &mut Map<String, Value>, name: &str, key: Option<&str>, value: &Value) {
    if let (None, false, Some(other)) = (key, value.is_null(), other_spelling(name)) {
        props.shift_remove(other);
    }
}

/// `place` (ADR-0013): `spot` becomes `node`'s placement where its placement lives, the
/// deck's `overrides`, a state's delta, or the node's own `at`; or, to `fork` it, in
/// `state`'s own delta. Of `at`'s placement keys, those `spot` names are set there and the
/// rest go: a delta or an override takes them away with `null` from what it merges into. A
/// `parent` it names is set there too, and `null` takes the node out onto the canvas.
fn place_node(d: &Doc, node: &str, spot: &Spot, state: Option<&str>, fork: bool) -> Result<Vec<JsonOp>, String> {
    if fork && state.is_none() {
        return Err("`fork` keeps a placement to a state: name it (`state`)".into());
    }
    let own = d.node(node)?;
    if let Some(Some(into)) = &spot.parent {
        let kind = d.kind(into)?;
        if !matches!(kind, "stack" | "grid" | "frame" | "group") {
            return Err(format!(
                "`{into}`, of type `{kind}`, holds nothing: a node goes into a stack, a grid, a frame, or a group"
            ));
        }
        if into == node {
            return Err(format!("`{node}` cannot hold itself"));
        }
    }
    let spot = match serde_json::to_value(spot).map_err(|e| e.to_string())? {
        Value::Object(spot) => spot,
        _ => Map::new(),
    };
    let ways: [&[&str]; 5] = [&["col", "row"], &["in"], &["rect"], &["area"], &["index"]];
    let named: Vec<&[&str]> = ways.into_iter().filter(|keys| keys.iter().any(|k| spot.contains_key(*k))).collect();
    let way = match named[..] {
        [way] => way[0],
        [] => return Err("say where: `at` takes cells (`col`, `row`), `in`, `rect`, `area`, or `index`".into()),
        _ => return Err("`at` is one placement: cells (`col`, `row`), `in`, `rect`, `area`, or `index`".into()),
    };
    // What holds the node where it is shown says how it is placed.
    let shown = d.showing(node, state)?;
    let at_of = |props: &Map<String, Value>| props.get("at").and_then(Value::as_object).cloned().unwrap_or_default();
    let now = match &shown {
        Some((_, props)) => props.get("at").and_then(Value::as_object).cloned().unwrap_or_default(),
        None => at_of(own),
    };
    // What holds it there: the container `spot` names, or none for the canvas; else its own.
    let parent = match spot.get("parent") {
        Some(into) => into.as_str(),
        None => now.get("parent").and_then(Value::as_str),
    };
    let within = parent.map(|p| d.kind(p).unwrap_or("untyped")).filter(|kind| *kind != "group");
    let (fits, how): (&[&str], String) = match (within, parent) {
        (Some("stack"), Some(p)) => {
            (&["index"], format!("in stack `{p}`, which places its children in order: by `index`"))
        }
        (Some("grid"), Some(p)) => {
            (&["col", "area", "index"], format!("in grid `{p}`: by its cells (`col`, `row`), an `area`, or `index`"))
        }
        (Some("frame"), Some(p)) => (&["rect"], format!("in frame `{p}`: by a `rect` from its padding edge")),
        _ => {
            (&["col", "in", "rect"], "on the theme's grid: by cells (`col`, `row`), a slot (`in`), or a `rect`".into())
        }
    };
    if !fits.contains(&way) {
        return Err(format!("`{node}` is placed {how}"));
    }
    // `at` with `spot`'s placement: the other placement keys go, and those `away` names are
    // taken away with `null` from what a delta or an override merges into. A container it
    // names is set; the canvas takes the container away.
    let placed = |mut at: Map<String, Value>, away: &dyn Fn(&str) -> bool| {
        at.retain(|k, _| !Spot::KEYS.contains(&k.as_str()));
        for key in Spot::KEYS {
            match spot.get(key) {
                Some(value) => drop(at.insert(key.to_string(), value.clone())),
                None if away(key) => drop(at.insert(key.to_string(), Value::Null)),
                None => {}
            }
        }
        match spot.get("parent") {
            Some(Value::Null) if !away("parent") => drop(at.shift_remove("parent")),
            Some(into) => drop(at.insert("parent".into(), into.clone())),
            None => {}
        }
        Value::Object(at)
    };
    // The deck's overrides win in every state: a placement they set is changed there, and
    // takes away every other placement the node has, its own or a state's.
    let overridden = d.0.get("overrides").and_then(|o| o.get(node)).and_then(Value::as_object);
    if let Some(over) = overridden.filter(|o| match o.get("at") {
        Some(Value::Object(at)) => Spot::PLACED.iter().any(|k| at.contains_key(*k)),
        Some(Value::Null) => true,
        _ => false,
    }) {
        if fork {
            return Err(format!(
                "the deck's `overrides` place `{node}` in every state (`/overrides/{node}/at`): kept to one state, a placement would not show"
            ));
        }
        let deltas = d.states().iter().filter_map(|s| s.get("props").and_then(|p| p.get(node)));
        let mut ats: Vec<&Value> = deltas.filter_map(|p| p.get("at")).collect();
        ats.extend(own.get("at"));
        let anywhere = |key: &str| ats.iter().any(|at| at.get(key).is_some());
        let mut new = over.clone();
        let old = match over.get("at") {
            Some(Value::Object(at)) => at.clone(),
            // Overrides that took `at` away keep the rest of it away.
            _ => now.keys().map(|k| (k.clone(), Value::Null)).collect(),
        };
        new.insert("at".into(), placed(old, &anywhere));
        return Ok(diff(&format!("/overrides/{}", esc(node)), over, &new));
    }
    let lives = match shown {
        Some((i, _)) => {
            let (deck, snapshots) = d.snapshots()?;
            match if fork { Lives::State(i) } else { lives(&deck, i, node, "at", &Spot::PLACED) } {
                Lives::State(j) => {
                    // What the delta merges into: the node as the state it tracks from shows
                    // it, or, where it enters there, its own.
                    let removed = deck.states[j].remove.iter().any(|id| id == node);
                    let tracked = tracks_from(&deck, j).and_then(|f| snapshots[f].nodes.get(node)).filter(|_| !removed);
                    let under = match tracked {
                        Some(props) => props.get("at").and_then(Value::as_object).cloned().unwrap_or_default(),
                        None => at_of(own),
                    };
                    Some((j, under))
                }
                Lives::Node => None,
            }
        }
        None => None,
    };
    let Some((j, under)) = lives else {
        let at = placed(at_of(own), &|_| false);
        return set(d, node, vec![("at".into(), None, at)], None);
    };
    let delta = d.states()[j].get("props").and_then(|p| p.get(node)).and_then(|p| p.get("at"));
    let at = match delta {
        Some(Value::Object(at)) => at.clone(),
        // A delta that took `at` away keeps the rest of it away.
        _ => under.keys().map(|k| (k.clone(), Value::Null)).collect(),
    };
    let at = placed(at, &|key| under.contains_key(key));
    set(d, node, vec![("at".into(), None, at)], Some(j))
}

/// `place` in `format` (ADR-0020): into the node's own layout there where it has one, or,
/// `anew`, one made for it; else as [`place_node`] places it. A format the deck does not lay
/// out anew (its own canvas's shape, or one it does not list) places as without one, and
/// `anew` there is refused.
fn place_in_format(
    d: &Doc,
    node: &str,
    spot: &Spot,
    state: Option<&str>,
    fork: bool,
    format: &str,
    anew: bool,
) -> Result<Vec<JsonOp>, String> {
    let own = d.node(node)?.clone();
    let canvas = |k: &str| d.0.pointer(&format!("/canvas/{k}")).and_then(Value::as_f64).unwrap_or_default();
    let size = [canvas("width"), canvas("height")];
    let listed = d.0.get("formats").and_then(Value::as_array).is_some_and(|f| f.iter().any(|f| f == format));
    let laid = crate::model::Format::parse(format).is_some_and(|f| f.canvas(size) != size) && listed;
    let entry = own.get("formats").and_then(|f| f.get(format)).and_then(Value::as_object).cloned();
    match (laid, entry, anew) {
        (false, _, true) => {
            Err(format!("`{format}` is not a format the deck lays out anew: a node's layout there would not show"))
        }
        (true, entry, anew) if entry.is_some() || anew => {
            if fork {
                return Err(format!("`{node}`'s layout in `{format}` is the node's own: it is not kept to a state"));
            }
            // Check the placement as the node's own would be: what holds it, and how.
            place_node(d, node, spot, state, false)?;
            let mut entry = entry.unwrap_or_default();
            let mut at = entry
                .get("at")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_else(|| own.get("at").and_then(Value::as_object).cloned().unwrap_or_default());
            let spot = match serde_json::to_value(spot).map_err(|e| e.to_string())? {
                Value::Object(spot) => spot,
                _ => Map::new(),
            };
            at.retain(|k, _| !Spot::KEYS.contains(&k.as_str()));
            for key in Spot::KEYS {
                if let Some(value) = spot.get(key) {
                    at.insert(key.to_string(), value.clone());
                }
            }
            match spot.get("parent") {
                Some(Value::Null) => drop(at.shift_remove("parent")),
                Some(into) => drop(at.insert("parent".into(), into.clone())),
                None => {}
            }
            entry.insert("at".into(), Value::Object(at));
            let mut new = own.clone();
            let formats = new.entry("formats").or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(formats) = formats {
                formats.insert(format.to_string(), Value::Object(entry));
            }
            Ok(diff(&format!("/nodes/{}", esc(node)), &own, &new))
        }
        _ => place_node(d, node, spot, state, fork),
    }
}

/// `replace_text` (ADR-0013): the characters `from..to` of `node`'s text as `state` shows it
/// become `text`, where the text lives, or, to `fork` it, in `state`'s own delta. A text set
/// by runs keeps them: the edit is made in their texts.
fn replace_text(
    d: &Doc,
    node: &str,
    (from, to): (usize, usize),
    text: &str,
    state: Option<&str>,
    fork: bool,
) -> Result<Vec<JsonOp>, String> {
    let shown = Shown::read(d, node, state, fork, "`replace_text` edits a text node's text")?;
    let (from, to) = shown.bytes(node, from, to)?;
    let value = match &shown.runs {
        Some(runs) => Value::Array(edit_runs(runs, from..to, text)),
        None => Value::String(format!("{}{text}{}", &shown.written[..from], &shown.written[to..])),
    };
    let prop = shown.prop();
    let mut entries: Vec<Entry> = vec![(prop.into(), None, value)];
    // Its list kept in step with its paragraphs (ADR-0018), written beside the text.
    if let Some(list) = &shown.list {
        let now = crate::lists::edited(list, &shown.written, from..to, text);
        if now != trimmed(list) {
            entries.push(("list".into(), None, serde_json::to_value(now).map_err(|e| e.to_string())?));
        }
    }
    shown.write(d, node, state, fork, entries, prop)
}

/// `list` without the trailing paragraphs that are no items.
fn trimmed(list: &[Option<ListItem>]) -> Vec<Option<ListItem>> {
    let mut list = list.to_vec();
    while list.last().is_some_and(Option::is_none) {
        list.pop();
    }
    list
}

/// `list` (ADR-0018, PLAN 2.69): the paragraphs characters `from`..`to` of `node`'s text as
/// `state` shows it touch, marked as `how` says, the text's `list` written where it lives.
fn mark_list(
    d: &Doc,
    node: &str,
    (from, to): (usize, usize),
    how: (Option<Listing>, Option<u8>, Option<i32>),
    state: Option<&str>,
    fork: bool,
) -> Result<Vec<JsonOp>, String> {
    use crate::lists::Marking;
    use crate::model::values::ListKind;
    let marking = match how {
        (Some(_), _, Some(_)) => return Err("give a `kind` or move items `by` levels, not both".into()),
        (Some(Listing::Bullet), level, None) => Marking::Kind(Some(ListKind::Bullet), level),
        (Some(Listing::Number), level, None) => Marking::Kind(Some(ListKind::Number), level),
        (Some(Listing::None), _, None) => Marking::Kind(None, None),
        (None, None, Some(by)) => Marking::By(by),
        (None, Some(_), None) => return Err("a `level` goes with a `kind`; `by` moves items a level".into()),
        (None, None, None) => {
            return Err("say how to mark the paragraphs: a `kind` (`bullet`, `number`, `none`) or `by`".into());
        }
        (None, Some(_), Some(_)) => return Err("give a `level` with a `kind`, or move items `by` levels".into()),
    };
    let shown = Shown::read(d, node, state, fork, "`list` makes a text node's paragraphs a list")?;
    let (from, to) = shown.bytes(node, from, to)?;
    let now = crate::lists::marked(shown.list.as_deref().unwrap_or_default(), &shown.written, from..to, marking);
    // No items left: `list` is taken away, unless the node's own would show through.
    let bare = d.node(node)?.get("list").is_none_or(Value::is_null);
    let value = match now.is_empty() && (bare || shown.on_node(d, node, state, fork, "list")?) {
        true => Value::Null,
        false => serde_json::to_value(now).map_err(|e| e.to_string())?,
    };
    shown.write(d, node, state, fork, vec![("list".into(), None, value)], "list")
}

/// `style_text` (ADR-0013, PLAN 2.38): characters `from`..`to` of `node`'s text as `state`
/// shows it take `look`, as runs split at the range's ends and joined where alike, written
/// where the text lives.
fn style_text(
    d: &Doc,
    node: &str,
    (from, to): (usize, usize),
    look: &Props,
    state: Option<&str>,
    fork: bool,
) -> Result<Vec<JsonOp>, String> {
    if look.is_empty() {
        return Err(
            "`look` names nothing to set: a run's `role`, `emphasis`, `lang`, `link`, `quote`, or one key of its `style`".into(),
        );
    }
    for (key, value) in look {
        let (name, sub) = key.split_once('/').map_or((key.as_str(), None), |(n, k)| (n, Some(k)));
        match (name, sub) {
            ("role" | "emphasis" | "lang" | "link", None) => {}
            ("quote", None) if value.is_null() => {}
            ("quote", None) => {
                serde_json::from_value::<crate::model::values::Quote>(value.clone())
                    .map_err(|e| format!("a quote is `{{ data, row?, column, format?, dataTransform? }}`: {e}"))?;
            }
            ("style", Some("size")) => {
                return Err("a run's size comes with a role: choose a role for the characters".into());
            }
            ("style", Some(_)) if !value.is_null() && literal(key, value) => {
                return Err(format!(
                    "{value} is written out where the theme has names: a run takes them (a color token or role), as a value written out is legal only in the deck's `overrides`, which would hold the text in every state"
                ));
            }
            ("style", Some(k)) if STYLE_KEYS.contains(&k) => {}
            _ => {
                return Err(format!(
                    "a run's look is its `role`, `emphasis`, `lang`, `link`, `quote`, or one key of its `style` ({}), not `{key}`",
                    STYLE_KEYS.iter().map(|k| format!("`style/{k}`")).collect::<Vec<_>>().join(", ")
                ));
            }
        }
    }
    let shown = Shown::read(d, node, state, fork, "`style_text` sets the look of a text node's characters")?;
    if from == to {
        return Err(format!("`from` and `to` select no characters of `{node}`: give the look to one or more"));
    }
    let (from, to) = shown.bytes(node, from, to)?;
    let runs = match &shown.runs {
        Some(runs) => runs.clone(),
        None => vec![Value::Object(Map::from_iter([("text".to_string(), Value::String(shown.written.clone()))]))],
    };
    // A quoted figure is styled whole (ADR-0019).
    let (from, to) = whole(&runs, from..to);
    let mut runs = look_runs(split_runs(&runs, &[from, to]), from..to, look);
    // Characters given a quote are one run: the figure it sets.
    if look.get("quote").is_some_and(|q| !q.is_null()) {
        runs = one_run(runs, from..to);
    }
    let runs = join_runs(runs);
    // Runs that all read as the node does are its text again.
    let plain = runs.iter().all(|r| r.as_object().is_some_and(|o| o.len() == 1 && o.contains_key("text")));
    let entry: Entry = if plain {
        let text: String = runs.iter().filter_map(|r| r.get("text").and_then(Value::as_str)).collect();
        ("text".into(), None, Value::String(text))
    } else {
        ("runs".into(), None, Value::Array(runs))
    };
    shown.write(d, node, state, fork, vec![entry], shown.prop())
}

/// The keys of a run's `style` that `style_text` sets: the theme's names, and numbers that
/// are no literal (W300's).
const STYLE_KEYS: [&str; 8] = ["family", "weight", "italic", "leading", "tracking", "opsz", "case", "color"];

/// A text node's text as a state shows it, and where an edit of it is written.
struct Shown {
    /// The deck's overrides for the node.
    over: Option<Map<String, Value>>,
    /// Its runs, if it has any.
    runs: Option<Vec<Value>>,
    /// Its text: its `text`, or its runs' texts end to end.
    written: String,
    /// Its paragraphs as list items (ADR-0018), if it has a `list`.
    list: Option<Vec<Option<ListItem>>>,
}

impl Shown {
    /// `node`'s text as `state` shows it (its own props, or the state's, under the overrides);
    /// refused, saying `what`, for a node that is not a text.
    fn read(d: &Doc, node: &str, state: Option<&str>, fork: bool, what: &str) -> Result<Shown, String> {
        let kind = d.kind(node)?;
        if kind != "text" {
            return Err(format!("`{node}` is a {kind} node; {what}"));
        }
        if fork && state.is_none() {
            return Err("`fork` keeps an edit to a state: name it (`state`)".into());
        }
        let own = d.node(node)?;
        let shown = d.showing(node, state)?;
        let mut props: Props = match &shown {
            Some((_, props)) => props.clone(),
            None => own.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        };
        let over = d.0.get("overrides").and_then(|o| o.get(node)).and_then(Value::as_object).cloned();
        if let Some(over) = &over {
            merge_props(&mut props, &over.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        }
        let runs = props.get("runs").and_then(Value::as_array).filter(|runs| !runs.is_empty()).cloned();
        let list = props.get("list").filter(|l| !l.is_null()).map(|l| serde_json::from_value(l.clone())).transpose();
        let list = list.map_err(|e| format!("`{node}`'s `list`: {e}"))?;
        let written: String = match &runs {
            Some(runs) => runs.iter().filter_map(|r| r.get("text").and_then(Value::as_str)).collect(),
            None => props.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
        };
        Ok(Shown { over, runs, written, list })
    }

    /// The property the text is: `runs`, or `text`.
    fn prop(&self) -> &'static str {
        if self.runs.is_some() { "runs" } else { "text" }
    }

    /// Character offsets `from`..`to` as byte offsets into the text, or why they are not.
    fn bytes(&self, node: &str, from: usize, to: usize) -> Result<(usize, usize), String> {
        let count = self.written.chars().count();
        if from > to || to > count {
            return Err(format!(
                "`{node}` reads {count} characters there: `from` and `to` are offsets from 0 to {count}, `from` first"
            ));
        }
        let byte = |chars: usize| self.written.char_indices().nth(chars).map_or(self.written.len(), |(i, _)| i);
        Ok((byte(from), byte(to)))
    }

    /// Whether `write` writes `prop` on the node itself: neither the overrides nor a state set it.
    fn on_node(&self, d: &Doc, node: &str, state: Option<&str>, fork: bool, prop: &str) -> Result<bool, String> {
        if self.over.as_ref().is_some_and(|o| o.contains_key(prop)) || fork {
            return Ok(false);
        }
        Ok(match d.showing(node, state)? {
            Some((i, _)) => {
                let (deck, _) = d.snapshots()?;
                matches!(lives(&deck, i, node, prop, &[]), Lives::Node)
            }
            None => true,
        })
    }

    /// Ops that write `entries` where the text lives (`prop`, `text` or `runs`): in the deck's
    /// `overrides` if they set it; else in the latest delta that sets it, from `state` back
    /// along what it tracks, or, to `fork` it, in `state`'s own; else in the node's own.
    fn write(
        &self,
        d: &Doc,
        node: &str,
        state: Option<&str>,
        fork: bool,
        entries: Vec<Entry>,
        prop: &str,
    ) -> Result<Vec<JsonOp>, String> {
        // The deck's overrides win in every state: text they set is changed there.
        if let Some(over) = self.over.as_ref().filter(|o| o.contains_key(prop)) {
            if fork {
                return Err(format!(
                    "the deck's `overrides` set `{node}`'s {prop} in every state (`/overrides/{node}/{prop}`): kept to one state, an edit would not show"
                ));
            }
            let mut new = over.clone();
            for (name, key, value) in entries {
                spell(&mut new, &name, key.as_deref(), &value);
                if value.is_null() {
                    new.shift_remove(&name);
                } else {
                    new.insert(name, value);
                }
            }
            return Ok(diff(&format!("/overrides/{}", esc(node)), over, &new));
        }
        let at = match d.showing(node, state)? {
            Some((i, _)) => {
                let (deck, _) = d.snapshots()?;
                match if fork { Lives::State(i) } else { lives(&deck, i, node, prop, &[]) } {
                    Lives::State(j) => Some(j),
                    Lives::Node => None,
                }
            }
            None => None,
        };
        set(d, node, entries, at)
    }
}

/// `runs` cut at each byte offset of `cuts` into their texts end to end: a run a cut falls
/// inside becomes two, each with its look.
fn split_runs(runs: &[Value], cuts: &[usize]) -> Vec<Value> {
    let mut out = Vec::with_capacity(runs.len() + cuts.len());
    let mut start = 0;
    for run in runs {
        let text = run.get("text").and_then(Value::as_str).unwrap_or_default();
        let end = start + text.len();
        let mut at = 0;
        for cut in cuts.iter().filter(|&&c| start < c && c < end).map(|c| c - start) {
            if cut > at {
                out.push(with_text(run, &text[at..cut]));
                at = cut;
            }
        }
        out.push(with_text(run, &text[at..]));
        start = end;
    }
    out
}

/// `range` (byte offsets into `runs`' texts end to end) widened to take in each quoted run it
/// cuts into: a figure is styled whole (ADR-0019).
fn whole(runs: &[Value], range: Range<usize>) -> (usize, usize) {
    let (mut from, mut to) = (range.start, range.end);
    let mut start = 0;
    for run in runs {
        let end = start + run.get("text").and_then(Value::as_str).map_or(0, str::len);
        if run.get("quote").is_some() && start < to && from < end {
            from = from.min(start);
            to = to.max(end);
        }
        start = end;
    }
    (from, to)
}

/// `runs` with those within `range` (byte offsets) made one, in the first one's look.
fn one_run(runs: Vec<Value>, range: Range<usize>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(runs.len());
    let mut start = 0;
    let mut joined: Option<usize> = None;
    for run in runs {
        let text = run.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
        let inside = start >= range.start && start + text.len() <= range.end && !text.is_empty();
        start += text.len();
        match (inside, joined) {
            (true, Some(k)) => {
                let both = format!("{}{text}", out[k].get("text").and_then(Value::as_str).unwrap_or_default());
                out[k] = with_text(&out[k], &both);
            }
            (true, None) => {
                joined = Some(out.len());
                out.push(run);
            }
            (false, _) => out.push(run),
        }
    }
    out
}

/// `look` set on each run of `runs` that lies within `range` (byte offsets into their texts
/// end to end), its `null`s taken away; a `style` left with nothing goes.
fn look_runs(runs: Vec<Value>, range: Range<usize>, look: &Props) -> Vec<Value> {
    let mut start = 0;
    runs.into_iter()
        .map(|mut run| {
            let len = run.get("text").and_then(Value::as_str).map_or(0, str::len);
            let inside = start >= range.start && start + len <= range.end && len > 0;
            start += len;
            if !inside {
                return run;
            }
            let Some(fields) = run.as_object_mut() else { return run };
            for (key, value) in look {
                match key.split_once('/') {
                    Some((name, sub)) => {
                        let style = fields.entry(name.to_string()).or_insert_with(|| Value::Object(Map::new()));
                        if let Some(style) = style.as_object_mut() {
                            if value.is_null() {
                                style.shift_remove(sub);
                            } else {
                                style.insert(sub.to_string(), value.clone());
                            }
                        }
                        if fields.get(name).and_then(Value::as_object).is_some_and(Map::is_empty) {
                            fields.shift_remove(name);
                        }
                    }
                    None if value.is_null() => drop(fields.shift_remove(key)),
                    None => drop(fields.insert(key.clone(), value.clone())),
                }
            }
            run
        })
        .collect()
}

/// `runs` with each run joined to the one before it where the two read alike, and runs left
/// with no text gone, unless every run is.
fn join_runs(runs: Vec<Value>) -> Vec<Value> {
    fn look(run: &Value) -> Option<Vec<(&String, &Value)>> {
        run.as_object().map(|o| o.iter().filter(|(k, _)| *k != "text").collect())
    }
    let mut out: Vec<Value> = Vec::with_capacity(runs.len());
    for run in runs {
        let text = run.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
        if text.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(last) if look(last) == look(&run) => {
                let joined = format!("{}{text}", last.get("text").and_then(Value::as_str).unwrap_or_default());
                *last = with_text(last, &joined);
            }
            _ => out.push(run),
        }
    }
    if out.is_empty() {
        out.push(Value::Object(Map::from_iter([("text".to_string(), Value::String(String::new()))])));
    }
    out
}

/// `run` with `text` for its text.
fn with_text(run: &Value, text: &str) -> Value {
    let mut run = run.clone();
    if let Some(fields) = run.as_object_mut() {
        fields.insert("text".into(), Value::String(text.to_string()));
    }
    run
}

/// `choose` (ADR-0013): `value` becomes `node`'s `prop` (a property, or one key of one), as an
/// inspector sets it. A literal where the theme has names (W300's) goes in the deck's
/// `overrides`, the only place it is legal, as does any value where the overrides set the
/// property. Else it is written where the value lives: the latest delta that sets it, from
/// `state` back, else the node's own; or, to `fork` it, in `state`'s own delta. `null` takes
/// it away where it lives, so what is under it shows.
fn choose(
    d: &Doc,
    node: &str,
    prop: &str,
    value: &Value,
    state: Option<&str>,
    fork: bool,
) -> Result<Vec<JsonOp>, String> {
    let (name, key) = parse_prop(prop)?;
    d.node(node)?;
    if fork && state.is_none() {
        return Err("`fork` keeps a choice to a state: name it (`state`)".into());
    }
    let over = d.0.get("overrides").and_then(|o| o.get(node)).and_then(Value::as_object);
    let set_over = over.and_then(|o| o.get(&name));
    let overridden = set_over.is_some_and(|v| key.as_ref().is_none_or(|k| v.is_null() || v.get(k).is_some()));
    let written = !value.is_null() && literal(prop, value);
    if overridden || written {
        if !fork {
            return Ok(overriding(d, node, over, &name, key.as_deref(), value));
        }
        let state = state.unwrap_or_default();
        return Err(match overridden {
            true => format!(
                "the deck's `overrides` set `{node}`'s {prop} in every state (`/overrides/{node}/{name}`): kept to `{state}`, a choice would not show"
            ),
            false => format!(
                "{value} is written out where the theme has names: it goes in the deck's `overrides`, which hold in every state, so it cannot be kept to `{state}`; one of the theme's names can"
            ),
        });
    }
    let at = match d.showing(node, state)? {
        Some((i, _)) => {
            let (deck, _) = d.snapshots()?;
            let keys: Vec<&str> = key.iter().map(String::as_str).collect();
            match if fork { Lives::State(i) } else { lives(&deck, i, node, &name, &keys) } {
                Lives::State(j) => Some(j),
                Lives::Node => None,
            }
        }
        None => None,
    };
    match at {
        // In the delta it lives in, taken away is taken out: what the delta merges into shows.
        Some(j) if value.is_null() && !fork => Ok(unset(d, node, j, &name, key.as_deref())),
        _ => set(d, node, vec![(name, key, value.clone())], at),
    }
}

/// `annotate` (PLAN 2.67): one of chart `node`'s annotations added, changed, or taken away,
/// and the chart's `annotations` as `state` shows them (its overrides', where they set them)
/// written where they live, as `choose` writes them. An annotation added or changed must stand
/// where its kind can ([`Annotation::check`]); validation says whether its place is in the
/// data. A list emptied where nothing is under it goes, and elsewhere stays, empty, so that
/// what is under it does not show.
fn annotate(
    d: &Doc,
    node: &str,
    index: Option<u32>,
    annotation: Option<&Option<Annotating>>,
    state: Option<&str>,
    fork: bool,
) -> Result<Vec<JsonOp>, String> {
    let kind = d.kind(node)?;
    if kind != "chart" {
        return Err(format!("`{node}` is a {kind} node; a chart takes annotations"));
    }
    if fork && state.is_none() {
        return Err("`fork` keeps the annotations to a state: name it (`state`)".into());
    }
    let showing = d.showing(node, state)?;
    let shown: Props = match &showing {
        Some((_, props)) => props.clone(),
        None => d.node(node)?.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    };
    let over = d.0.get("overrides").and_then(|o| o.get(node));
    let read = |name: &str| over.and_then(|o| o.get(name)).or_else(|| shown.get(name));
    let mut list: Vec<Value> = match read("annotations") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) => list.clone(),
        Some(_) => return Err(format!("`{node}`'s `annotations` is not a list")),
    };
    let count = list.len();
    let place = |i: u32| -> Result<usize, String> {
        match i as usize {
            i if i < count => Ok(i),
            _ => Err(match count {
                0 => format!("`{node}` has no annotations: add one, without an `index`"),
                1 => format!("`{node}` has one annotation, at `index` 0"),
                n => format!("`{node}` has {n} annotations, at `index` 0 to {}", n - 1),
            }),
        }
    };
    let changed = match (index, annotation) {
        (_, None) => {
            return Err(
                "give `annotation`: one to add; with `index`, what changes in that one, or `null` to take it away"
                    .into(),
            );
        }
        (None, Some(None)) => return Err("`null` takes an annotation away: say which by its `index`".into()),
        (None, Some(Some(given))) => {
            if given.kind.is_none() || given.at.is_none() {
                return Err("an annotation added has a `kind` and an `at` (SPEC §3.7)".into());
            }
            list.push(Value::Object(Map::new()));
            Some(list.len() - 1)
        }
        (Some(i), Some(None)) => {
            list.remove(place(i)?);
            None
        }
        (Some(i), Some(Some(_))) => Some(place(i)?),
    };
    if let (Some(i), Some(Some(given))) = (changed, annotation) {
        let Value::Object(note) = &mut list[i] else { return Err(format!("annotation {i} is not an object")) };
        if let Some(kind) = given.kind {
            note.insert("kind".into(), serde_json::to_value(kind).map_err(|e| e.to_string())?);
        }
        if let Some(at) = &given.at {
            note.insert("at".into(), crate::dsl::whole(&serde_json::to_value(at).map_err(|e| e.to_string())?));
        }
        for (key, value) in [("text", &given.text), ("role", &given.role)] {
            match value {
                None => {}
                Some(None) => drop(note.shift_remove(key)),
                Some(Some(text)) => drop(note.insert(key.into(), Value::String(text.clone()))),
            }
        }
        let checked: Annotation =
            serde_json::from_value(list[i].clone()).map_err(|e| format!("annotation {i}: {e}"))?;
        let donut = read("kind").and_then(Value::as_str) == Some("donut");
        checked.check(donut).map_err(|e| format!("annotation {i}: {e}"))?;
    }
    // Emptied on the node itself, the list goes; in a delta or the overrides, an empty one
    // stays, since taking it away there would show what is under it.
    let on_node = over.and_then(|o| o.get("annotations")).is_none()
        && !fork
        && match &showing {
            Some((i, _)) => matches!(lives(&d.snapshots()?.0, *i, node, "annotations", &[]), Lives::Node),
            None => true,
        };
    let value = if list.is_empty() && on_node { Value::Null } else { Value::Array(list) };
    choose(d, node, "annotations", &value, state, fork)
}

/// A chart's or a table's source, chosen (`choose` with `data`, PLAN 2.41): written where its
/// `data` lives, as any choice is, with what the new source cannot serve pointed again there,
/// so that the choice reads. A chart's axis whose field the source lacks, or types otherwise
/// than the axis reads ([`data::readable`]), reads a column it can that no other channel reads,
/// one of the type its field had if there is one; a `series`, a `color`, a size, a projection,
/// or a `key` with no column is
/// taken away, and so is each of a table's `columns` the source lacks (all of them, and it shows
/// every column). An `x` or a `y` with none to read refuses the source.
fn choose_data(
    d: &Doc,
    files: &dyn BundleFiles,
    node: &str,
    value: &Value,
    state: Option<&str>,
    fork: bool,
) -> Result<Vec<JsonOp>, String> {
    let named = value.as_str().and_then(|v| v.strip_prefix('@'));
    let name = named.ok_or_else(|| format!("a data source is named `@name`; {value} is not"))?;
    if fork && state.is_none() {
        return Err("`fork` keeps a choice to a state: name it (`state`)".into());
    }
    let (deck, _) = d.snapshots()?;
    if !deck.data.contains_key(name) {
        let declared = list(deck.data.keys().map(|k| format!("@{k}")));
        return Err(format!("the deck has no data source `@{name}`; it has {declared}"));
    }
    if d.0.get("overrides").and_then(|o| o.get(node)).and_then(|o| o.get("data")).is_some() {
        return Err(format!(
            "the deck's `overrides` set `{node}`'s data in every state (`/overrides/{node}/data`): take it out of them to choose its source"
        ));
    }
    // The node as the state shows it, else as its own props have it.
    let showing = d.showing(node, state)?;
    let shown: Props = match &showing {
        Some((_, props)) => props.clone(),
        None => d.node(node)?.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    };
    let mut reading = shown.clone();
    reading.insert("data".into(), value.clone());
    let table = data::read(&deck, &data::Texts(files), &reading)?;
    let has = |column: &str| table.column(column).is_some();
    let mut entries: Vec<Entry> = vec![("data".into(), None, value.clone())];
    let away = |entries: &mut Vec<Entry>, name: &str| entries.push((name.into(), None, Value::Null));
    if d.kind(node)? == "chart" {
        let channels = ["x", "y", "series", "color", "sizeEncoding"];
        let field = |c: &str| shown.get(c).and_then(|e| e.get("field")).and_then(Value::as_str);
        let reads = |c: &str| data::readable(&table, &shown, c);
        // The type each channel's field had in the source it read.
        let before = data::read(&deck, &data::Texts(files), &shown).ok();
        let was = |c: &str| {
            let (field, before) = (field(c)?, before.as_ref()?);
            before.column(field).map(|i| before.types[i])
        };
        let kind = |column: &str| table.column(column).map(|i| table.types[i]);
        // The columns read on: each channel's that the new source serves as it is. The others
        // are pointed again, the channel that can read the fewest columns first.
        let mut taken: Vec<&str> = channels.iter().filter_map(|&c| field(c).filter(|f| reads(c).contains(f))).collect();
        let mut moving: Vec<&str> =
            channels.into_iter().filter(|&c| field(c).is_some_and(|f| !reads(c).contains(&f))).collect();
        crate::sort::by_key(&mut moving, |&c| reads(c).len());
        for c in moving {
            let free: Vec<&str> = reads(c).into_iter().filter(|column| !taken.contains(column)).collect();
            let like = was(c).and_then(|was| free.iter().copied().find(|&column| kind(column) == Some(was)));
            match (c, like.or(free.first().copied())) {
                ("x" | "y", Some(column)) => {
                    taken.push(column);
                    entries.push((c.into(), Some("field".into()), Value::String(column.into())));
                }
                ("x" | "y", None) => {
                    let declared = shown.get(c).and_then(|e| e.get("type")).and_then(Value::as_str);
                    let how = match (c, declared) {
                        (_, Some(kind)) => format!(" as {kind}"),
                        ("y", None) => " (numbers)".into(),
                        _ => String::new(),
                    };
                    let columns = list(table.columns.iter().map(|c| format!("`{c}`")));
                    return Err(format!(
                        "`@{name}` has no column the chart's `{c}` can read{how} that another channel does not: it has {columns}"
                    ));
                }
                _ => away(&mut entries, c),
            }
        }
        let projected = shown.get("projected").and_then(|p| p.get("field")).and_then(Value::as_str);
        if projected.is_some_and(|f| !has(f)) {
            away(&mut entries, "projected");
        }
    } else if let Some(columns) = shown.get("columns").and_then(Value::as_array) {
        let kept: Vec<Value> =
            (columns.iter()).filter(|c| c.get("field").and_then(Value::as_str).is_some_and(has)).cloned().collect();
        if kept.len() < columns.len() {
            entries.push(("columns".into(), None, if kept.is_empty() { Value::Null } else { Value::Array(kept) }));
        }
    }
    if shown.get("key").and_then(Value::as_str).is_some_and(|k| !has(k)) {
        away(&mut entries, "key");
    }
    let at = match showing.map(|(i, _)| i) {
        Some(i) => match if fork { Lives::State(i) } else { lives(&deck, i, node, "data", &[]) } {
            Lives::State(j) => Some(j),
            Lives::Node => None,
        },
        None => None,
    };
    set(d, node, entries, at)
}

/// Ops that make `value` the `name` (or its key `key`) the deck's `overrides` set for `node`;
/// `null` takes it out of them, and a node's overrides left with nothing go.
fn overriding(
    d: &Doc,
    node: &str,
    over: Option<&Map<String, Value>>,
    name: &str,
    key: Option<&str>,
    value: &Value,
) -> Vec<JsonOp> {
    let mut new = over.cloned().unwrap_or_default();
    match (key, new.get_mut(name)) {
        (None, _) if value.is_null() => drop(new.shift_remove(name)),
        (None, _) => drop(new.insert(name.into(), value.clone())),
        (Some(key), Some(Value::Object(map))) if value.is_null() => {
            map.shift_remove(key);
            if map.is_empty() {
                new.shift_remove(name);
            }
        }
        (Some(key), Some(Value::Object(map))) => drop(map.insert(key.into(), value.clone())),
        (Some(_), _) if value.is_null() => drop(new.shift_remove(name)),
        (Some(key), _) => drop(new.insert(name.into(), object(key, value.clone()))),
    }
    let base = format!("/overrides/{}", esc(node));
    match (d.0.get("overrides").and_then(Value::as_object), over) {
        (_, None) if new.is_empty() => Vec::new(),
        (None, _) => vec![JsonOp::Add { path: "/overrides".into(), value: object(node, Value::Object(new)) }],
        (Some(_), None) => vec![JsonOp::Add { path: base, value: Value::Object(new) }],
        (Some(all), Some(_)) if new.is_empty() && all.len() == 1 => vec![JsonOp::Remove { path: "/overrides".into() }],
        (Some(_), Some(_)) if new.is_empty() => vec![JsonOp::Remove { path: base }],
        (Some(_), Some(old)) => diff(&base, old, &new),
    }
}

/// Ops that take `name` (or its key `key`) out of state `j`'s delta for `node`, so what the
/// delta merges into shows; an object left with nothing goes too.
fn unset(d: &Doc, node: &str, j: usize, name: &str, key: Option<&str>) -> Vec<JsonOp> {
    let delta = d.states()[j].get("props").and_then(|p| p.get(node)).and_then(Value::as_object);
    let Some(old) = delta else { return Vec::new() };
    let mut new = old.clone();
    match (key, new.get_mut(name)) {
        (None, _) => drop(new.shift_remove(name)),
        (Some(key), Some(Value::Object(map))) => {
            map.shift_remove(key);
            if map.is_empty() {
                new.shift_remove(name);
            }
        }
        (Some(_), _) => {}
    }
    diff(&format!("/states/{j}/props/{}", esc(node)), old, &new)
}

/// `runs` with the bytes `range` of their texts, end to end, replaced by `text`: typed into
/// the run the range starts in, the one before where two meet, or the first at the start. A
/// run the edit leaves with no text goes, unless it was typed into.
fn edit_runs(runs: &[Value], range: Range<usize>, text: &str) -> Vec<Value> {
    let texts: Vec<&str> = runs.iter().map(|r| r.get("text").and_then(Value::as_str).unwrap_or_default()).collect();
    let starts: Vec<usize> = texts.iter().scan(0, |at, t| Some(std::mem::replace(at, *at + t.len()))).collect();
    let into = match range.start {
        0 => 0,
        start => (0..texts.len())
            .find(|&k| starts[k] < start && start <= starts[k] + texts[k].len())
            .unwrap_or(texts.len().saturating_sub(1)),
    };
    // What is typed beside a quoted figure, not into it, goes in a run of its own, in the
    // figure's look but quoting nothing (ADR-0019).
    let quoted = |k: usize| runs.get(k).is_some_and(|r| r.get("quote").is_some());
    let beside = (range.is_empty() && !text.is_empty() && quoted(into))
        .then(|| (range.start == starts[into], range.start == starts[into] + texts[into].len()))
        .filter(|(before, after)| *before || *after);
    let plain = |run: &Value| {
        let mut run = run.clone();
        if let Some(fields) = run.as_object_mut() {
            fields.shift_remove("quote");
            fields.shift_remove("link");
            fields.insert("text".into(), Value::String(text.into()));
        }
        run
    };
    let mut out = Vec::with_capacity(runs.len() + 1);
    for (k, (run, own)) in runs.iter().zip(&texts).enumerate() {
        let (start, end) = (starts[k], starts[k] + own.len());
        let (cut, kept) = (range.start.clamp(start, end) - start, range.end.clamp(start, end) - start);
        let mut new = format!("{}{}", &own[..cut], &own[kept..]);
        if k == into {
            match beside {
                Some((true, _)) => out.push(plain(run)),
                Some((_, true)) => {}
                _ => new.insert_str(cut, text),
            }
        }
        if new.is_empty() && !own.is_empty() && !(k == into && beside.is_none()) {
            continue;
        }
        let mut run = run.clone();
        if let Some(fields) = run.as_object_mut() {
            // A figure whose characters are typed over is words now: it quotes nothing.
            if new != *own {
                fields.shift_remove("quote");
            }
            fields.insert("text".into(), Value::String(new));
        }
        out.push(run.clone());
        if k == into && matches!(beside, Some((false, true))) {
            out.push(plain(&run));
        }
    }
    out
}

/// Ops that turn `old`, the object at `base`, into `new`: member by member, and one level
/// into a member both hold as an object.
fn diff(base: &str, old: &Map<String, Value>, new: &Map<String, Value>) -> Vec<JsonOp> {
    let mut out = Vec::new();
    for (key, value) in new {
        let at = format!("{base}/{}", esc(key));
        match (old.get(key), value) {
            (Some(was), _) if was == value => {}
            (Some(Value::Object(was)), Value::Object(now)) => {
                for (k, v) in now.iter().filter(|(k, v)| was.get(*k) != Some(*v)) {
                    out.push(JsonOp::Add { path: format!("{at}/{}", esc(k)), value: v.clone() });
                }
                for k in was.keys().filter(|k| !now.contains_key(*k)) {
                    out.push(JsonOp::Remove { path: format!("{at}/{}", esc(k)) });
                }
            }
            _ => out.push(JsonOp::Add { path: at, value: value.clone() }),
        }
    }
    for key in old.keys().filter(|k| !new.contains_key(*k)) {
        out.push(JsonOp::Remove { path: format!("{base}/{}", esc(key)) });
    }
    out
}

/// The op that has node `id` enter in state `i`, `delta` its delta there.
fn enter(d: &Doc, i: usize, id: &str, delta: Props) -> JsonOp {
    let delta = Value::Object(delta.into_iter().collect());
    match d.states()[i].get("props") {
        Some(Value::Object(_)) => JsonOp::Add { path: format!("/states/{i}/props/{}", esc(id)), value: delta },
        _ => JsonOp::Add { path: format!("/states/{i}/props"), value: object(id, delta) },
    }
}

/// Ops on the choreography at `base` (`items`) that map each target by `f`: `Some` renames
/// it, `None` drops it. An item left with no target goes, and so does a sequence or a group
/// left with no items. Later items first, so each op's path holds as the ops before it
/// apply.
fn retarget_ops(base: &str, items: &[Value], f: &dyn Fn(&str) -> Option<String>) -> Vec<JsonOp> {
    let mut ops = Vec::new();
    for (j, item) in items.iter().enumerate().rev() {
        let at = format!("{base}/{j}");
        if retarget(std::slice::from_ref(item), f).is_empty() {
            ops.push(JsonOp::Remove { path: at });
            continue;
        }
        let Some(fields) = item.as_object() else { continue };
        match fields.get("target") {
            Some(Value::String(target)) => {
                if let Some(to) = f(target).filter(|to| to != target) {
                    ops.push(JsonOp::Replace { path: format!("{at}/target"), value: Value::String(to) });
                }
            }
            Some(Value::Array(targets)) => {
                for (k, target) in targets.iter().enumerate().rev() {
                    let Some(id) = target.as_str() else { continue };
                    match f(id) {
                        None => ops.push(JsonOp::Remove { path: format!("{at}/target/{k}") }),
                        Some(to) if to != id => {
                            ops.push(JsonOp::Replace { path: format!("{at}/target/{k}"), value: Value::String(to) })
                        }
                        Some(_) => {}
                    }
                }
            }
            _ => {}
        }
        for key in ["sequence", "parallel"] {
            if let Some(Value::Array(inner)) = fields.get(key) {
                ops.extend(retarget_ops(&format!("{at}/{key}"), inner, f));
            }
        }
    }
    ops
}

/// `items`, each target mapped by `f`, as [`retarget_ops`] maps them.
fn retarget(items: &[Value], f: &dyn Fn(&str) -> Option<String>) -> Vec<Value> {
    items
        .iter()
        .filter_map(|item| {
            let mut item = item.clone();
            let Some(fields) = item.as_object_mut() else { return Some(item) };
            match fields.get_mut("target") {
                Some(Value::String(target)) => *target = f(target)?,
                Some(Value::Array(targets)) => {
                    let kept: Vec<Value> = targets
                        .iter()
                        .filter_map(|t| match t.as_str() {
                            Some(id) => f(id).map(Value::String),
                            None => Some(t.clone()),
                        })
                        .collect();
                    if kept.is_empty() {
                        return None;
                    }
                    *targets = kept;
                }
                _ => {}
            }
            for key in ["sequence", "parallel"] {
                if let Some(Value::Array(inner)) = fields.get_mut(key) {
                    let kept = retarget(inner, f);
                    if kept.is_empty() {
                        return None;
                    }
                    *inner = kept;
                }
            }
            Some(item)
        })
        .collect()
}

fn remove_node(d: &Doc, id: &str) -> Result<Vec<JsonOp>, String> {
    d.node(id)?;
    let mut held: Vec<&str> = Vec::new();
    for (_, child, _) in d.parents().into_iter().filter(|(_, n, p)| *p == id && *n != id) {
        if !held.contains(&child) {
            held.push(child);
        }
    }
    if !held.is_empty() {
        return Err(format!("`{id}` holds {}: remove them first, or move them out of it (`at.parent`)", list(held)));
    }
    Ok(removal(d, id))
}

/// The ops that take node `id` out of the deck, with everything that names it: its deltas,
/// its exits, its choreography, and its overrides.
fn removal(d: &Doc, id: &str) -> Vec<JsonOp> {
    let mut ops = Vec::new();
    for (i, state) in d.states().iter().enumerate() {
        if state.get("props").and_then(|p| p.get(id)).is_some() {
            ops.push(JsonOp::Remove { path: format!("/states/{i}/props/{}", esc(id)) });
        }
        if let Some(Value::Array(exits)) = state.get("remove")
            && exits.iter().any(|x| x.as_str() == Some(id))
        {
            ops.push(without_exit(i, exits, id));
        }
        if let Some(Value::Array(items)) = state.get("choreography") {
            ops.extend(retarget_ops(&format!("/states/{i}/choreography"), items, &|t| {
                (t != id).then(|| t.to_string())
            }));
        }
    }
    if d.0.get("overrides").and_then(|o| o.get(id)).is_some() {
        ops.push(JsonOp::Remove { path: format!("/overrides/{}", esc(id)) });
    }
    ops.push(JsonOp::Remove { path: format!("/nodes/{}", esc(id)) });
    ops
}

/// State `i`'s exits, `exits`, without `node`: the list goes when it would be left empty, as
/// a state with no exits is written.
fn without_exit(i: usize, exits: &[Value], node: &str) -> JsonOp {
    let kept: Vec<Value> = exits.iter().filter(|x| x.as_str() != Some(node)).cloned().collect();
    match kept.is_empty() {
        true => JsonOp::Remove { path: format!("/states/{i}/remove") },
        false => JsonOp::Replace { path: format!("/states/{i}/remove"), value: Value::Array(kept) },
    }
}

/// A node's container, as its resolved props (or its own) place it.
fn parent_of(props: &Map<String, Value>) -> Option<&str> {
    props.get("at")?.get("parent")?.as_str()
}

/// A new group `id` holding `nodes` where they stand (ADR-0008, ADR-0013): see
/// [`SemanticOp::Group`].
fn group(d: &Doc, id: &str, nodes: &[String], state: Option<&str>) -> Result<Vec<JsonOp>, String> {
    d.free("node", id, d.node(id).is_ok())?;
    let Some(first) = nodes.first() else { return Err("a group holds one node or more: name them in `nodes`".into()) };
    for (i, node) in nodes.iter().enumerate() {
        d.node(node)?;
        if nodes[..i].contains(node) {
            return Err(format!("`{node}` is named twice"));
        }
    }
    // Their container: as `state` shows them, else as their own `at` places them.
    let (deck, snapshots) = d.snapshots()?;
    let shown = state.map(|s| d.state(s)).transpose()?;
    let container = |node: &str| -> Result<Option<String>, String> {
        let props = match shown {
            Some(i) => snapshots[i].nodes.get(node).map(|p| p.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
            None => Some(d.node(node)?.clone()),
        };
        let props = props.ok_or_else(|| format!("`{node}` is not on screen in `{}`", state.unwrap_or_default()))?;
        Ok(parent_of(&props).map(String::from))
    };
    let held = container(first)?;
    for node in &nodes[1..] {
        let theirs = container(node)?;
        if theirs != held {
            let name = |c: &Option<String>| c.as_ref().map_or("the slide".to_string(), |c| format!("`{c}`"));
            return Err(format!(
                "`{first}` is in {} and `{node}` in {}: a group holds what one container holds",
                name(&held),
                name(&theirs)
            ));
        }
    }
    if let Some(outer) = &held
        && d.kind(outer)? != "group"
    {
        return Err(format!(
            "`{first}` is in {} `{outer}`, which places what it holds itself: a group sits on the slide or in a group",
            d.kind(outer)?
        ));
    }

    // The group: in their container, at the highest `z` any of them sets.
    let mut node = Map::from_iter([("type".to_string(), Value::String("group".into()))]);
    if let Some(outer) = &held {
        node.insert("at".into(), object("parent", Value::String(outer.clone())));
    }
    let z = |n: &String| d.node(n).ok().and_then(|n| n.get("z")?.as_i64()).unwrap_or(0);
    let top = nodes.iter().map(z).max().unwrap_or(0);
    if top != 0 {
        node.insert("z".into(), Value::from(top));
    }
    let mut ops = vec![JsonOp::Add { path: format!("/nodes/{}", esc(id)), value: Value::Object(node) }];

    // Each takes the group as its container wherever that one is written; on the slide, in
    // its own `at`.
    match &held {
        Some(outer) => {
            for (path, node, parent) in d.parents() {
                if nodes.iter().any(|n| n == node) && parent == outer {
                    ops.push(JsonOp::Replace { path, value: Value::String(id.into()) });
                }
            }
        }
        None => {
            for node in nodes {
                let at = d.node(node)?.get("at");
                ops.push(match at {
                    Some(Value::Object(_)) => {
                        JsonOp::Add { path: format!("/nodes/{}/at/parent", esc(node)), value: Value::String(id.into()) }
                    }
                    _ => JsonOp::Add {
                        path: format!("/nodes/{}/at", esc(node)),
                        value: object("parent", Value::String(id.into())),
                    },
                });
            }
        }
    }

    // It shows in each state that shows one of them in it, and only there: it enters where
    // the state before does not bring it, and leaves where it would come along with none.
    let mut work = d.0.clone();
    for op in &ops {
        super::one(&mut work, op)?;
    }
    let (_, after) = Doc(&work).snapshots()?;
    let mut shows = vec![false; after.len()];
    for (i, snapshot) in after.iter().enumerate() {
        let needed = nodes.iter().any(|n| {
            snapshot
                .nodes
                .get(n)
                .is_some_and(|p| p.get("at").and_then(|a| a.get("parent")) == Some(&Value::String(id.into())))
        });
        let brought = tracks_from(&deck, i).is_some_and(|j| shows[j]);
        if needed && !brought {
            ops.push(enter(d, i, id, Props::new()));
        } else if !needed && brought {
            let value = Value::String(id.into());
            ops.push(match d.states()[i].get("remove") {
                Some(Value::Array(_)) => JsonOp::Add { path: format!("/states/{i}/remove/-"), value },
                _ => JsonOp::Add { path: format!("/states/{i}/remove"), value: Value::Array(vec![value]) },
            });
        }
        shows[i] = needed;
    }
    Ok(ops)
}

/// A group's children out to its container, and the group gone (ADR-0008, ADR-0013): see
/// [`SemanticOp::Ungroup`].
fn ungroup(d: &Doc, group: &str) -> Result<Vec<JsonOp>, String> {
    let kind = d.kind(group)?;
    if kind != "group" {
        return Err(format!("`{group}` is a {kind} node; `ungroup` takes a group's children out of it"));
    }
    let outer = d.node(group)?.get("at").and_then(|a| a.get("parent")).and_then(Value::as_str);
    let mut ops = Vec::new();
    for (path, node, parent) in d.parents() {
        if parent != group || node == group {
            continue;
        }
        match outer {
            Some(outer) => ops.push(JsonOp::Replace { path, value: Value::String(outer.into()) }),
            None => {
                // Out to the slide: its `parent` goes, and an `at` it leaves empty goes with it.
                let at = path.strip_suffix("/parent").unwrap_or(&path);
                let alone = d.0.pointer(at).and_then(Value::as_object).is_some_and(|a| a.len() == 1);
                ops.push(JsonOp::Remove { path: if alone { at.to_string() } else { path.clone() } });
            }
        }
    }
    ops.extend(removal(d, group));
    Ok(ops)
}

fn rename_node(d: &Doc, id: &str, to: &str) -> Result<Vec<JsonOp>, String> {
    d.node(id)?;
    d.free("node", to, d.node(to).is_ok())?;
    let (old, new) = (esc(id), esc(to));
    let mut ops = vec![JsonOp::Move { from: format!("/nodes/{old}"), path: format!("/nodes/{new}") }];
    for (i, state) in d.states().iter().enumerate() {
        if state.get("props").and_then(|p| p.get(id)).is_some() {
            ops.push(JsonOp::Move {
                from: format!("/states/{i}/props/{old}"),
                path: format!("/states/{i}/props/{new}"),
            });
        }
        for (j, exit) in state.get("remove").and_then(Value::as_array).into_iter().flatten().enumerate() {
            if exit.as_str() == Some(id) {
                ops.push(JsonOp::Replace { path: format!("/states/{i}/remove/{j}"), value: Value::String(to.into()) });
            }
        }
        if let Some(Value::Array(items)) = state.get("choreography") {
            let rename = |t: &str| Some(if t == id { to.to_string() } else { t.to_string() });
            ops.extend(retarget_ops(&format!("/states/{i}/choreography"), items, &rename));
        }
    }
    if d.0.get("overrides").and_then(|o| o.get(id)).is_some() {
        ops.push(JsonOp::Move { from: format!("/overrides/{old}"), path: format!("/overrides/{new}") });
    }
    for (path, _, _) in d.parents().into_iter().filter(|(_, n, p)| *p == id && *n != id) {
        ops.push(JsonOp::Replace { path, value: Value::String(to.into()) });
    }
    Ok(ops)
}

fn show_node(d: &Doc, node: &str, state: &str, props: Option<Props>) -> Result<Vec<JsonOp>, String> {
    d.node(node)?;
    let i = d.state(state)?;
    let (_, snapshots) = d.snapshots()?;
    if snapshots[i].nodes.contains_key(node) {
        return Err(format!("`{node}` is on screen in `{state}` already; `set_prop` changes it there"));
    }
    let exits = d.states()[i].get("remove").and_then(Value::as_array);
    let mut ops = Vec::new();
    // It leaves here: it stays instead, as it is in the state before.
    if let Some(exits) = exits.filter(|e| e.iter().any(|x| x.as_str() == Some(node))) {
        ops.push(without_exit(i, exits, node));
        if let Some(props) = props.filter(|p| !p.is_empty()) {
            ops.push(enter(d, i, node, props));
        }
        return Ok(ops);
    }
    ops.push(enter(d, i, node, props.unwrap_or_default()));
    Ok(ops)
}

fn hide_node(d: &Doc, node: &str, state: &str) -> Result<Vec<JsonOp>, String> {
    d.node(node)?;
    let i = d.state(state)?;
    let (deck, snapshots) = d.snapshots()?;
    if !snapshots[i].nodes.contains_key(node) {
        return Err(format!("`{node}` is not on screen in `{state}`"));
    }
    let mut ops = Vec::new();
    // A state that removes a node and gives it a delta has it enter again, from its
    // defaults: the delta goes too.
    if d.states()[i].get("props").and_then(|p| p.get(node)).is_some() {
        ops.push(JsonOp::Remove { path: format!("/states/{i}/props/{}", esc(node)) });
    }
    // On screen in the state it tracks from: it exits here, unless it does already (and
    // the delta had it enter again). Else it entered here, by that delta, and is gone with it.
    let exits = d.states()[i].get("remove").and_then(Value::as_array);
    let listed = exits.is_some_and(|e| e.iter().any(|x| x.as_str() == Some(node)));
    if !listed && tracks_from(&deck, i).is_some_and(|j| snapshots[j].nodes.contains_key(node)) {
        let value = Value::String(node.into());
        ops.push(match exits {
            Some(_) => JsonOp::Add { path: format!("/states/{i}/remove/-"), value },
            None => JsonOp::Add { path: format!("/states/{i}/remove"), value: Value::Array(vec![value]) },
        });
    }
    Ok(ops)
}

/// Where `after` or `before` puts a state: an index into the states without `moving`, the
/// one being moved. `None` without either.
fn place(d: &Doc, after: Option<&str>, before: Option<&str>, moving: Option<usize>) -> Result<Option<usize>, String> {
    let ids: Vec<&str> =
        d.states().iter().enumerate().filter(|(k, _)| Some(*k) != moving).map(|(_, s)| id_of(s)).collect();
    let find = |id: &str| ids.iter().position(|s| *s == id).ok_or_else(|| format!("no state `{id}`"));
    match (after, before) {
        (Some(_), Some(_)) => Err("say `after` or `before`, not both".into()),
        (Some(after), None) => Ok(Some(find(after)? + 1)),
        (None, Some(before)) => Ok(Some(find(before)?)),
        (None, None) => Ok(None),
    }
}

fn remove_state(d: &Doc, id: &str) -> Result<Vec<JsonOp>, String> {
    let i = d.state(id)?;
    if d.states().len() == 1 {
        return Err("a deck keeps at least one state".into());
    }
    let leaning: Vec<&str> = d
        .states()
        .iter()
        .filter(|s| id_of(s) != id)
        .filter(|s| ["slide", "from"].iter().any(|k| s.get(*k).and_then(Value::as_str) == Some(id)))
        .map(id_of)
        .collect();
    if !leaning.is_empty() {
        return Err(format!(
            "{} build on it or track from it (`slide`, `from`): remove them, or point them elsewhere, first",
            list(leaning)
        ));
    }
    let mut ops = Vec::new();
    for (s, b, beat) in d.beats() {
        if let Some(Value::Array(states)) = beat.get("states")
            && states.iter().any(|x| x.as_str() == Some(id))
        {
            let kept = states.iter().filter(|x| x.as_str() != Some(id)).cloned().collect();
            ops.push(JsonOp::Replace {
                path: format!("/spine/sections/{s}/beats/{b}/states"),
                value: Value::Array(kept),
            });
        }
    }
    // A link to it goes, and its words stay (PLAN 2.70).
    for path in links_to(d, id) {
        ops.push(JsonOp::Remove { path });
    }
    ops.push(JsonOp::Remove { path: format!("/states/{i}") });
    Ok(ops)
}

/// The pointers of every link to state `id` in a text's runs (PLAN 2.70): in a node's own, a
/// state's, or the deck's overrides.
fn links_to(d: &Doc, id: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut scan = |props: Option<&Value>, at: String| {
        for (node, p) in props.and_then(Value::as_object).into_iter().flatten() {
            for (k, run) in p.get("runs").and_then(Value::as_array).into_iter().flatten().enumerate() {
                if run.get("link").and_then(|l| l.get("state")).and_then(Value::as_str) == Some(id) {
                    out.push(format!("{at}/{}/runs/{k}/link", esc(node)));
                }
            }
        }
    };
    scan(d.0.get("nodes"), "/nodes".into());
    for (i, state) in d.states().iter().enumerate() {
        scan(state.get("props"), format!("/states/{i}/props"));
    }
    scan(d.0.get("overrides"), "/overrides".into());
    out
}

/// The keys of a state's transition (SPEC §3.9).
const TRANSITION: [&str; 4] = ["duration", "ease", "spring", "match"];

/// `set_state` (PLAN 2.36): `value` becomes state `id`'s `prop`. A layout is written where it
/// lives: the latest state that sets it, from `id` back along what it tracks, else `id`; or,
/// to `fork` it, `id` itself. The rest are `id`'s own.
fn set_state(d: &Doc, id: &str, prop: &str, value: &Value, fork: bool) -> Result<Vec<JsonOp>, String> {
    let i = d.state(id)?;
    let (name, key) = parse_prop(prop)?;
    let at = match (name.as_str(), key.as_deref()) {
        ("layout", None) if fork => i,
        ("layout", None) => layout_lives(&d.snapshots()?.0, i).unwrap_or(i),
        ("hold" | "notes" | "transition", None) => i,
        ("transition", Some(key)) if TRANSITION.contains(&key) => i,
        _ => {
            let keys = list(TRANSITION.iter().map(|k| format!("transition/{k}")));
            return Err(format!(
                "`{prop}` is not one `set_state` sets: a state's `layout`, `transition` or one of its keys ({keys}), `hold`, or `notes`; `rename_state` and `move_state` change the rest"
            ));
        }
    };
    let state = &d.states()[at];
    let old = state.as_object().ok_or_else(|| format!("state `{}` is not an object", id_of(state)))?;
    let set = match &key {
        Some(key) => transition_with(old.get("transition"), key, value),
        None => (!value.is_null()).then(|| value.clone()),
    };
    let mut new = old.clone();
    match set {
        Some(set) => drop(new.insert(name, set)),
        None => drop(new.shift_remove(&name)),
    }
    Ok(diff(&format!("/states/{at}"), old, &new))
}

/// A state's transition, `old`, with its `key` set to `value`, or taken away with `null`. A
/// bare duration, or none, stays bare while its duration is all it sets, and becomes an
/// object to take another key. One left with nothing is none: the state cuts.
fn transition_with(old: Option<&Value>, key: &str, value: &Value) -> Option<Value> {
    let bare = matches!(old, None | Some(Value::String(_) | Value::Number(_)));
    let has = match old {
        Some(Value::Object(spec)) => spec.contains_key(key),
        Some(_) => bare && key == "duration",
        None => false,
    };
    if value.is_null() && !has {
        return old.cloned();
    }
    if bare && key == "duration" {
        return (!value.is_null()).then(|| value.clone());
    }
    let mut spec = match old {
        Some(Value::Object(spec)) => spec.clone(),
        Some(duration) if bare => Map::from_iter([("duration".to_string(), duration.clone())]),
        _ => Map::new(),
    };
    match value {
        Value::Null => drop(spec.shift_remove(key)),
        _ => drop(spec.insert(key.into(), value.clone())),
    }
    (!spec.is_empty()).then_some(Value::Object(spec))
}

/// `time_motion` (PLAN 2.44): `node`'s `motion` in state `state`'s cue, its `delay` and its
/// `duration` per unit, written where the engine reads them (SPEC §3.9). That is the first
/// item of the state's choreography that moves the node so, depth first. Else it is the node's
/// own: its `enter`, `emphasis`, or `anim` as the state reads it, and its `exit` as the state
/// before it in the cue list reads it, since that is what the node leaves from.
/// - A preset by name becomes a call to take a setting, and a call goes back to its name once it
///   sets nothing else. An item's own setting wins over its call's, so a setting goes where the
///   motion's is now: the item's, else the call's, else the item's.
/// - `anim` tracks keep their keys spaced as they were. A node's own `delay` is when its first
///   key falls, and an item's is the item's `delay`; `duration` is the time from the first key
///   to the last.
/// - A motion on a spring lasts as long as it takes to settle, so a `duration` there is refused.
fn time_motion(
    d: &Doc,
    files: &dyn BundleFiles,
    node: &str,
    motion: Timed,
    state: &str,
    delay: Option<f64>,
    duration: Option<&Duration>,
) -> Result<Vec<JsonOp>, String> {
    d.node(node)?;
    let i = d.state(state)?;
    let key = motion.key();
    if delay.is_none() && duration.is_none() {
        return Err("name the `delay` or the `duration` to set".into());
    }
    if let Some(ms) = delay.or(match duration {
        Some(Duration::Ms(ms)) => Some(*ms),
        _ => None,
    }) && !(ms.is_finite() && ms >= 0.0)
    {
        return Err(format!("{ms} is not a time: a `delay` or a `duration` is 0 ms or more"));
    }
    let theme = d.theme(files).ok();
    if let (Some(Duration::Named(name)), Some(theme)) = (duration, &theme)
        && !theme.motion.durations.contains_key(name)
    {
        let names = list(theme.motion.durations.keys());
        return Err(format!("the theme has no duration `{name}`; it has {names}"));
    }
    let what = format!("`{node}`'s `{key}` in `{state}`");
    let duration_value = duration.map(|d| match d {
        Duration::Ms(ms) => number(*ms),
        Duration::Named(name) => Value::String(name.clone()),
    });
    let (path, old, mut written) = match locate(d, i, node, motion, &what)? {
        Located::Item(path, item) => {
            let mut new = item.clone();
            if motion == Timed::Anim {
                if let Some(delay) = delay {
                    set_or_drop(&mut new, "delay", (delay > 0.0).then(|| number(delay)));
                }
                if let Some(duration) = duration {
                    let span = millis(duration, theme.as_ref())?;
                    let tracks = stretched(item.get("anim").unwrap_or(&Value::Null), span, &what)?;
                    new.insert("anim".into(), tracks);
                }
            } else {
                let call = item.get(key).cloned().unwrap_or(Value::Null);
                if duration.is_some() {
                    sprung(theme.as_ref(), &[Some(item), call.as_object()], &call, &what)?;
                }
                for (field, value) in [("delay", delay.map(number)), ("duration", duration_value)] {
                    let Some(value) = value else { continue };
                    let call = new.get(key).cloned().unwrap_or(Value::Null);
                    let in_call = call.get(field).is_some();
                    if new.contains_key(field) || !in_call {
                        // Nothing under the item's own takes over where a delay of 0 goes.
                        let zero = field == "delay" && value.as_f64() == Some(0.0) && !in_call;
                        set_or_drop(&mut new, field, (!zero).then_some(value));
                    } else {
                        let zero = field == "delay" && value.as_f64() == Some(0.0);
                        new.insert(key.into(), call_with(&call, field, (!zero).then_some(value)));
                    }
                }
            }
            return Ok(diff(&path, item, &new));
        }
        Located::Own(path, old, written) => (path, old, written),
    };
    if motion == Timed::Anim {
        if let Some(duration) = duration {
            written = stretched(&written, millis(duration, theme.as_ref())?, &what)?;
        }
        if let Some(delay) = delay {
            written = shifted(&written, delay, &what)?;
        }
    } else {
        if duration.is_some() {
            sprung(theme.as_ref(), &[written.as_object()], &written, &what)?;
        }
        if let Some(delay) = delay {
            written = call_with(&written, "delay", (delay > 0.0).then(|| number(delay)));
        }
        if let Some(value) = duration_value {
            written = call_with(&written, "duration", Some(value));
        }
    }
    let mut new = old.clone();
    new.insert(key.into(), written);
    Ok(diff(&path, old, &new))
}

/// Where a motion is written (PLAN 2.44).
enum Located<'a> {
    /// An item of the state's choreography: its pointer, and the item.
    Item(String, &'a Map<String, Value>),
    /// The node's own: the pointer to what holds the property where it lives, what holds it,
    /// and the property as the state reads it.
    Own(String, &'a Map<String, Value>, Value),
}

/// Where `node`'s `motion` in state `i`'s cue is written: the first item of the state's
/// choreography that moves the node so, depth first, else the node's own. The own is its
/// `enter`, `emphasis`, or `anim` as the state reads it, and its `exit` as the state before
/// it in the cue list reads it (SPEC §3.9). `what` names the motion in an error.
fn locate<'a>(d: &Doc<'a>, i: usize, node: &str, motion: Timed, what: &str) -> Result<Located<'a>, String> {
    let key = motion.key();
    let items = d.states()[i].get("choreography").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    if let Some((path, item)) = choreographed(items, &format!("/states/{i}/choreography"), node, key) {
        return Ok(Located::Item(path, item));
    }
    let (deck, snapshots) = d.snapshots()?;
    let reads = match motion {
        Timed::Exit => i.checked_sub(1),
        _ => Some(i),
    };
    let own = reads.and_then(|j| snapshots[j].nodes.get(node)).and_then(|p| p.get(key)).filter(|v| !v.is_null());
    let (Some(j), Some(own)) = (reads, own) else {
        let how = match motion {
            Timed::Anim => "",
            _ => ": `apply_preset` gives it one, or the state's `choreography`",
        };
        return Err(format!("{what} is not there to time{how}"));
    };
    let lives = match motion {
        Timed::Enter | Timed::Exit => lives(&deck, j, node, key, &[]),
        // What never tracks is the state's own, or the node's as it enters.
        Timed::Emphasis | Timed::Anim => {
            let delta = deck.states[j].props.get(node).and_then(|p| p.get(key));
            if delta.is_some() { Lives::State(j) } else { Lives::Node }
        }
    };
    let (path, old) = match lives {
        Lives::State(k) => {
            let old = d.states()[k].get("props").and_then(|p| p.get(node)).and_then(Value::as_object);
            (format!("/states/{k}/props/{}", esc(node)), old.ok_or_else(|| format!("{what} is not where it lives"))?)
        }
        Lives::Node => (format!("/nodes/{}", esc(node)), d.node(node)?),
    };
    let written = old.get(key).cloned().unwrap_or_else(|| own.clone());
    Ok(Located::Own(path, old, written))
}

/// Where a motion is written, as `time_motion` writes it (PLAN 2.44).
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    /// A JSON pointer into the deck: the choreography item (`/states/2/choreography/0`), or
    /// the node's own property where it lives (`/states/1/props/title/enter`,
    /// `/nodes/title/exit`).
    pub pointer: String,
    /// Its `delay` as written, ms: an item's own, else its preset call's, and a node's own
    /// preset call's. A node's own `anim` has none: it is when its first key falls.
    pub delay: f64,
}

/// Where `node`'s `motion` in state `state`'s cue is written, and its delay there: what
/// `time_motion` reads and sets (PLAN 2.44).
pub fn written(doc: &Value, state: &str, node: &str, motion: Timed) -> Result<Written, String> {
    let d = Doc(doc);
    d.node(node)?;
    let i = d.state(state)?;
    let what = format!("`{node}`'s `{}` in `{state}`", motion.key());
    let delay = |v: Option<&Value>| v.and_then(|d| d.get("delay")).and_then(Value::as_f64);
    Ok(match locate(&d, i, node, motion, &what)? {
        Located::Item(pointer, item) => {
            let call = (motion != Timed::Anim).then(|| item.get(motion.key())).flatten();
            Written { delay: item.get("delay").and_then(Value::as_f64).or(delay(call)).unwrap_or(0.0), pointer }
        }
        Located::Own(base, _, value) => Written {
            pointer: format!("{base}/{}", motion.key()),
            delay: match motion {
                Timed::Anim => keys(&value, &what)?.iter().map(|k| k.2).fold(f64::INFINITY, f64::min),
                _ => delay(Some(&value)).unwrap_or(0.0),
            },
        },
    })
}

/// The first item of `items`, a state's choreography at `base`, that moves `node` by `key`,
/// depth first: its pointer and the item.
fn choreographed<'a>(
    items: &'a [Value],
    base: &str,
    node: &str,
    key: &str,
) -> Option<(String, &'a Map<String, Value>)> {
    for (k, item) in items.iter().enumerate() {
        let (path, Some(map)) = (format!("{base}/{k}"), item.as_object()) else { continue };
        for group in ["sequence", "parallel"] {
            let inner = map.get(group).and_then(Value::as_array);
            if let Some(found) = inner.and_then(|inner| choreographed(inner, &format!("{path}/{group}"), node, key)) {
                return Some(found);
            }
        }
        let names = match map.get("target") {
            Some(Value::String(one)) => one == node,
            Some(Value::Array(many)) => many.iter().any(|t| t.as_str() == Some(node)),
            _ => false,
        };
        if names && map.contains_key(key) {
            return Some((path, map));
        }
    }
    None
}

/// Refuses a `duration` for a motion on a spring: the first of `places` (the item, then the
/// call) to name an easing or a spring, else the theme's preset `call` calls.
fn sprung(
    theme: Option<&Theme>,
    places: &[Option<&Map<String, Value>>],
    call: &Value,
    what: &str,
) -> Result<(), String> {
    let named = call.as_str().or_else(|| call.get("preset").and_then(Value::as_str));
    let preset = theme.zip(named).and_then(|(theme, name)| theme.motion.presets.get(name));
    let mut springs = places.iter().flatten().map(|place| (place.contains_key("ease"), place.get("spring").cloned()));
    // At one place, a spring wins over an easing.
    let spring = match springs.find(|(ease, spring)| *ease || spring.is_some()) {
        Some((_, spring)) => spring,
        None => preset.and_then(|p| p.spring.clone().map(Value::String)),
    };
    match spring {
        Some(spring) => {
            let name = spring.as_str().map(|s| format!(" `{s}`")).unwrap_or_default();
            Err(format!(
                "{what} runs on the spring{name}, which lasts as long as it takes to settle: give it an `ease` to time it"
            ))
        }
        None => Ok(()),
    }
}

/// A motion preset, by name or a call with settings, with `field` set to `value` or taken away
/// with `None`. A call that sets nothing but its preset is its name.
fn call_with(call: &Value, field: &str, value: Option<Value>) -> Value {
    let mut map = match call {
        Value::String(name) => Map::from_iter([("preset".to_string(), Value::String(name.clone()))]),
        Value::Object(map) => map.clone(),
        other => return other.clone(),
    };
    set_or_drop(&mut map, field, value);
    match map.get("preset") {
        Some(Value::String(name)) if map.len() == 1 => Value::String(name.clone()),
        _ => Value::Object(map),
    }
}

fn set_or_drop(map: &mut Map<String, Value>, field: &str, value: Option<Value>) {
    match value {
        Some(value) => drop(map.insert(field.into(), value)),
        None => drop(map.shift_remove(field)),
    }
}

/// A time as the deck writes it, ms: whole where it is whole.
fn number(ms: f64) -> Value {
    if ms.fract() == 0.0 && ms.abs() < 1e15 { Value::from(ms as i64) } else { Value::from(ms) }
}

/// A duration in ms, a theme duration read from the theme.
fn millis(duration: &Duration, theme: Option<&Theme>) -> Result<f64, String> {
    match duration {
        Duration::Ms(ms) => Ok(*ms),
        Duration::Named(name) => theme
            .and_then(|t| t.motion.durations.get(name))
            .map(|d| d.0)
            .ok_or_else(|| format!("`{name}` is a theme duration the deck's theme does not give")),
    }
}

/// Each key of `anim` tracks: its track's name, its place there, and its `t`.
fn keys(tracks: &Value, what: &str) -> Result<Vec<(String, usize, f64)>, String> {
    let tracks = tracks.as_object().ok_or_else(|| format!("{what} is not tracks"))?;
    let mut out = Vec::new();
    for (name, keys) in tracks {
        for (k, key) in keys.as_array().into_iter().flatten().enumerate() {
            let t = key.get("t").and_then(Value::as_f64).ok_or_else(|| format!("{what}: a `{name}` key has no `t`"))?;
            out.push((name.clone(), k, t));
        }
    }
    if out.is_empty() {
        return Err(format!("{what} has no keys"));
    }
    Ok(out)
}

/// `tracks` with each key's `t` made `to(t)`, and the earliest key's `t`.
fn retimed(tracks: &Value, what: &str, to: impl Fn(f64, f64, f64) -> f64) -> Result<Value, String> {
    let keys = keys(tracks, what)?;
    let first = keys.iter().map(|k| k.2).fold(f64::INFINITY, f64::min);
    let last = keys.iter().map(|k| k.2).fold(f64::NEG_INFINITY, f64::max);
    let mut out = tracks.clone();
    for (name, k, t) in keys {
        if let Some(key) = out.get_mut(&name).and_then(|track| track.get_mut(k)).and_then(Value::as_object_mut) {
            key.insert("t".into(), number((to(t, first, last) * 1000.0).round() / 1000.0));
        }
    }
    Ok(out)
}

/// `anim` tracks whose first key falls at `delay`, the rest as far after it as they were.
fn shifted(tracks: &Value, delay: f64, what: &str) -> Result<Value, String> {
    retimed(tracks, what, |t, first, _| t - first + delay)
}

/// `anim` tracks whose last key falls `span` after their first, the keys between spaced as
/// they were.
fn stretched(tracks: &Value, span: f64, what: &str) -> Result<Value, String> {
    let keys = keys(tracks, what)?;
    let first = keys.iter().map(|k| k.2).fold(f64::INFINITY, f64::min);
    let last = keys.iter().map(|k| k.2).fold(f64::NEG_INFINITY, f64::max);
    if last - first <= 0.0 {
        return Err(format!(
            "{what} has its keys at {first} ms, all of them: there is no time between them to stretch"
        ));
    }
    retimed(tracks, what, |t, first, last| first + (t - first) * span / (last - first))
}

fn rename_state(d: &Doc, id: &str, to: &str) -> Result<Vec<JsonOp>, String> {
    let i = d.state(id)?;
    d.free("state", to, d.state(to).is_ok())?;
    let to_value = || Value::String(to.into());
    let mut ops = vec![JsonOp::Replace { path: format!("/states/{i}/id"), value: to_value() }];
    for (k, state) in d.states().iter().enumerate() {
        for key in ["slide", "from"] {
            if state.get(key).and_then(Value::as_str) == Some(id) {
                ops.push(JsonOp::Replace { path: format!("/states/{k}/{key}"), value: to_value() });
            }
        }
    }
    for (s, b, beat) in d.beats() {
        for (j, x) in beat.get("states").and_then(Value::as_array).into_iter().flatten().enumerate() {
            if x.as_str() == Some(id) {
                ops.push(JsonOp::Replace {
                    path: format!("/spine/sections/{s}/beats/{b}/states/{j}"),
                    value: to_value(),
                });
            }
        }
    }
    // A link to it goes to it by its new name (PLAN 2.70).
    for path in links_to(d, id) {
        ops.push(JsonOp::Replace { path: format!("{path}/state"), value: to_value() });
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn choreography_ops_land_where_the_rewrite_does() {
        let items = json!([
            { "target": ["a", "b"], "enter": "fade" },
            { "sequence": [{ "target": "a", "enter": "rise" }, { "target": "c", "enter": "fade" }] },
            { "parallel": [{ "target": "a", "exit": "fade" }] },
            { "target": "a", "exit": "fade" },
        ]);
        let list = items.as_array().unwrap();
        let drop_a = |t: &str| (t != "a").then(|| t.to_string());
        let rename_a = |t: &str| Some(if t == "a" { "z".to_string() } else { t.to_string() });
        for f in [&drop_a as &dyn Fn(&str) -> Option<String>, &rename_a] {
            let ops: Vec<Value> =
                retarget_ops("", list, f).iter().map(|op| serde_json::to_value(op).unwrap()).collect();
            let mut doc = items.clone();
            super::super::apply(&mut doc, &ops).unwrap();
            assert_eq!(doc, Value::Array(retarget(list, f)), "{ops:#?}");
        }
        let ops: Vec<Value> =
            retarget_ops("/c", list, &drop_a).iter().map(|op| serde_json::to_value(op).unwrap()).collect();
        assert_eq!(
            ops,
            [
                json!({ "op": "remove", "path": "/c/3" }),
                json!({ "op": "remove", "path": "/c/2" }),
                json!({ "op": "remove", "path": "/c/1/sequence/0" }),
                json!({ "op": "remove", "path": "/c/0/target/0" }),
            ]
        );
    }
}
