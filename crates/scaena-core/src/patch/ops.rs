//! The semantic ops (SPEC §7.3), each compiled to RFC 6902 against the deck it applies to.
//! Each checks what it names before it compiles, and refuses what would not do what it
//! says, with what to do instead: an op on a node that is not there, or a change in a state
//! to a node not on screen there, which would make it enter.

use super::{JsonOp, Renamed, SemanticOp, Spot, esc};
use crate::document::{Deck, Props};
use crate::ids::is_valid_id;
use crate::lint::literal;
use crate::model::theme::Theme;
use crate::tracking::{Lives, Snapshot, lives, merge_props, resolve_states, tracks_from};
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
        SemanticOp::SetProp { node, prop, value, state } => {
            d.node(node)?;
            let (name, key) = parse_prop(prop)?;
            if name == "type" {
                return Err("a node's `type` is what it is (E104): remove the node and add another".into());
            }
            let at = d.showing(node, state.as_deref())?;
            set(&d, node, vec![(name, key, value.clone())], at.map(|(i, _)| i))?
        }
        SemanticOp::Place { node, at, state, fork } => place_node(&d, node, at, state.as_deref(), *fork)?,
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
        SemanticOp::Choose { node, prop, value, state, fork } => {
            choose(&d, node, prop, value, state.as_deref(), *fork)?
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
/// `null` takes a property away. A delta keeps `null`, which takes it away from what the
/// node tracks (SPEC §2.2).
fn set(d: &Doc, node: &str, entries: Vec<Entry>, state: Option<usize>) -> Result<Vec<JsonOp>, String> {
    let Some(i) = state else {
        let old = d.node(node)?;
        let mut new = old.clone();
        for (name, key, value) in entries {
            match (key, new.get_mut(&name)) {
                (None, _) if value.is_null() => drop(new.shift_remove(&name)),
                (None, _) => drop(new.insert(name, value)),
                (Some(key), Some(Value::Object(map))) if value.is_null() => drop(map.shift_remove(&key)),
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

/// `place` (ADR-0013): `spot` becomes `node`'s placement where its placement lives, the
/// deck's `overrides`, a state's delta, or the node's own `at`; or, to `fork` it, in
/// `state`'s own delta. Of `at`'s placement keys, those `spot` names are set there and the
/// rest go: a delta or an override takes them away with `null` from what it merges into.
fn place_node(d: &Doc, node: &str, spot: &Spot, state: Option<&str>, fork: bool) -> Result<Vec<JsonOp>, String> {
    if fork && state.is_none() {
        return Err("`fork` keeps a placement to a state: name it (`state`)".into());
    }
    let own = d.node(node)?;
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
    let parent = now.get("parent").and_then(Value::as_str);
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
    // taken away with `null` from what a delta or an override merges into.
    let placed = |mut at: Map<String, Value>, away: &dyn Fn(&str) -> bool| {
        at.retain(|k, _| !Spot::KEYS.contains(&k.as_str()));
        for key in Spot::KEYS {
            match spot.get(key) {
                Some(value) => drop(at.insert(key.to_string(), value.clone())),
                None if away(key) => drop(at.insert(key.to_string(), Value::Null)),
                None => {}
            }
        }
        Value::Object(at)
    };
    // The deck's overrides win in every state: a placement they set is changed there, and
    // takes away every other placement the node has, its own or a state's.
    let overridden = d.0.get("overrides").and_then(|o| o.get(node)).and_then(Value::as_object);
    if let Some(over) = overridden.filter(|o| match o.get("at") {
        Some(Value::Object(at)) => Spot::KEYS.iter().any(|k| at.contains_key(*k)),
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
            match if fork { Lives::State(i) } else { lives(&deck, i, node, "at", &Spot::KEYS) } {
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
    let kind = d.kind(node)?;
    if kind != "text" {
        return Err(format!("`{node}` is a {kind} node; `replace_text` edits a text node's text"));
    }
    if fork && state.is_none() {
        return Err("`fork` keeps an edit to a state: name it (`state`)".into());
    }
    // The text as the state shows it: its own props, or the state's, under the overrides.
    let own = d.node(node)?;
    let shown = d.showing(node, state)?;
    let mut props: Props = match &shown {
        Some((_, props)) => props.clone(),
        None => own.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    };
    let over = d.0.get("overrides").and_then(|o| o.get(node)).and_then(Value::as_object);
    if let Some(over) = over {
        merge_props(&mut props, &over.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    }
    let runs = props.get("runs").and_then(Value::as_array).filter(|runs| !runs.is_empty());
    let prop = if runs.is_some() { "runs" } else { "text" };
    let written: String = match runs {
        Some(runs) => runs.iter().filter_map(|r| r.get("text").and_then(Value::as_str)).collect(),
        None => props.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
    };
    let count = written.chars().count();
    if from > to || to > count {
        return Err(format!(
            "`{node}` reads {count} characters there: `from` and `to` are offsets from 0 to {count}, `from` first"
        ));
    }
    let byte = |chars: usize| written.char_indices().nth(chars).map_or(written.len(), |(i, _)| i);
    let (from, to) = (byte(from), byte(to));
    let value = match runs {
        Some(runs) => Value::Array(edit_runs(runs, from..to, text)),
        None => Value::String(format!("{}{text}{}", &written[..from], &written[to..])),
    };
    // The deck's overrides win in every state: text they set is changed there.
    if let Some(over) = over.filter(|o| o.contains_key(prop)) {
        if fork {
            return Err(format!(
                "the deck's `overrides` set `{node}`'s {prop} in every state (`/overrides/{node}/{prop}`): kept to one state, an edit would not show"
            ));
        }
        let mut new = over.clone();
        new.insert(prop.into(), value);
        return Ok(diff(&format!("/overrides/{}", esc(node)), over, &new));
    }
    let at = match shown {
        Some((i, _)) => {
            let (deck, _) = d.snapshots()?;
            match if fork { Lives::State(i) } else { lives(&deck, i, node, prop, &[]) } {
                Lives::State(j) => Some(j),
                Lives::Node => None,
            }
        }
        None => None,
    };
    set(d, node, vec![(prop.into(), None, value)], at)
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
    let mut out = Vec::with_capacity(runs.len());
    for (k, (run, own)) in runs.iter().zip(&texts).enumerate() {
        let (start, end) = (starts[k], starts[k] + own.len());
        let (cut, kept) = (range.start.clamp(start, end) - start, range.end.clamp(start, end) - start);
        let mut new = format!("{}{}", &own[..cut], &own[kept..]);
        if k == into {
            new.insert_str(cut, text);
        } else if new.is_empty() && !own.is_empty() {
            continue;
        }
        let mut run = run.clone();
        if let Some(fields) = run.as_object_mut() {
            fields.insert("text".into(), Value::String(new));
        }
        out.push(run);
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
    let mut ops = Vec::new();
    for (i, state) in d.states().iter().enumerate() {
        if state.get("props").and_then(|p| p.get(id)).is_some() {
            ops.push(JsonOp::Remove { path: format!("/states/{i}/props/{}", esc(id)) });
        }
        if let Some(Value::Array(exits)) = state.get("remove")
            && exits.iter().any(|x| x.as_str() == Some(id))
        {
            let kept = exits.iter().filter(|x| x.as_str() != Some(id)).cloned().collect();
            ops.push(JsonOp::Replace { path: format!("/states/{i}/remove"), value: Value::Array(kept) });
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
        let kept = exits.iter().filter(|x| x.as_str() != Some(node)).cloned().collect();
        ops.push(JsonOp::Replace { path: format!("/states/{i}/remove"), value: Value::Array(kept) });
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
    ops.push(JsonOp::Remove { path: format!("/states/{i}") });
    Ok(ops)
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
