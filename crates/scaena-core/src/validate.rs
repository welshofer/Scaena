//! Validation (SPEC §7.5 E-codes that need no layout), PLAN 1.2.
//!
//! [`validate_bundle`] is what `scaena validate` runs: the deck and its theme against
//! their generated schemas ([`crate::model::check`]), then what a schema cannot say.
//! [`validate`] is the semantic part on a parsed deck alone: ids and references.

use crate::data::{self, ColumnType, DataError, Datum, Table};
use crate::document::{Deck, MAX_NESTING, NodeType, Props};
use crate::format::{DateFormat, NumberFormat};
use crate::ids::is_valid_id;
use crate::lint::{Finding, Severity};
use crate::model::check::{Checker, Kind, Violation};
use crate::model::theme::{Grid, Vocabulary};
use crate::model::values::{Annotation, Duration, Easing, Range};
use crate::model::{Format, Theme};
use crate::tracking::{Snapshot, resolve_states};
use crate::transform;
use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;

/// Validate `deck` and return every problem as an E-finding.
pub fn validate(deck: &Deck) -> Vec<Finding> {
    let mut out = Vec::new();
    let err = |code: &str, msg: String| Finding::new(code, Severity::Error, msg);

    // --- ids -------------------------------------------------------------
    for id in deck.nodes.keys() {
        if !is_valid_id(id) {
            out.push(err("E105", format!("invalid node id `{id}`")).at(format!("/nodes/{id}")).node(id.clone()));
        }
    }
    let mut seen = HashSet::new();
    for (i, s) in deck.states.iter().enumerate() {
        if !is_valid_id(&s.id) {
            out.push(
                err("E105", format!("invalid state id `{}`", s.id)).at(format!("/states/{i}/id")).state(s.id.clone()),
            );
        }
        if !seen.insert(&s.id) {
            out.push(
                err("E105", format!("duplicate state id `{}`", s.id)).at(format!("/states/{i}/id")).state(s.id.clone()),
            );
        }
    }

    // --- references ------------------------------------------------------
    let state_ids: HashSet<&str> = deck.states.iter().map(|s| s.id.as_str()).collect();
    for (i, s) in deck.states.iter().enumerate() {
        for id in s.props.keys() {
            if !deck.nodes.contains_key(id) {
                out.push(
                    err("E102", format!("state `{}` sets props on unknown node `{id}`", s.id))
                        .at(format!("/states/{i}/props/{id}"))
                        .state(s.id.clone())
                        .node(id.clone()),
                );
            }
        }
        for id in &s.remove {
            if !deck.nodes.contains_key(id) {
                out.push(
                    err("E102", format!("state `{}` removes unknown node `{id}`", s.id))
                        .at(format!("/states/{i}/remove"))
                        .state(s.id.clone())
                        .node(id.clone()),
                );
            }
        }
        if let Some(from) = &s.from
            && !state_ids.contains(from.as_str())
        {
            out.push(
                err("E102", format!("state `{}` tracks from unknown state `{from}`", s.id))
                    .at(format!("/states/{i}/from"))
                    .state(s.id.clone()),
            );
        }
        if let Some(slide) = &s.slide
            && !state_ids.contains(slide.as_str())
        {
            out.push(
                err("E102", format!("state `{}` belongs to unknown slide `{slide}`", s.id))
                    .at(format!("/states/{i}/slide"))
                    .state(s.id.clone()),
            );
        }
        for (j, item) in s.choreography.iter().enumerate() {
            for target in choreo_targets(item) {
                if !deck.nodes.contains_key(&target) {
                    out.push(
                        err("E102", format!("choreography in `{}` targets unknown node `{target}`", s.id))
                            .at(format!("/states/{i}/choreography/{j}"))
                            .state(s.id.clone())
                            .node(target),
                    );
                }
            }
        }
    }
    for id in deck.overrides.keys() {
        if !deck.nodes.contains_key(id) {
            out.push(
                err("E102", format!("overrides for unknown node `{id}`"))
                    .at(format!("/overrides/{id}"))
                    .node(id.clone()),
            );
        }
    }
    // A container named in `at.parent`, wherever a node's `at` is written.
    let placements = deck
        .nodes
        .iter()
        .map(|(id, node)| (id, node.props.get("at"), format!("/nodes/{}/at/parent", esc(id))))
        .chain(deck.states.iter().enumerate().flat_map(|(i, s)| {
            s.props
                .iter()
                .map(move |(id, delta)| (id, delta.get("at"), format!("/states/{i}/props/{}/at/parent", esc(id))))
        }))
        .chain(deck.overrides.iter().map(|(id, o)| (id, o.get("at"), format!("/overrides/{}/at/parent", esc(id)))));
    for (id, at, path) in placements {
        if let Some(Value::String(parent)) = at.and_then(|at| at.get("parent"))
            && !deck.nodes.contains_key(parent)
        {
            out.push(err("E102", format!("node `{id}` is in unknown container `{parent}`")).at(path).node(id.clone()));
        }
    }
    for (id, node) in &deck.nodes {
        if let Some(Value::String(data)) = node.props.get("data") {
            let name = data.trim_start_matches('@');
            if !deck.data.contains_key(name) {
                out.push(
                    err("E102", format!("node `{id}` references unknown data source `{data}`"))
                        .at(format!("/nodes/{id}/data"))
                        .node(id.clone())
                        .hint(format!(
                            "Declare it under /data/{name} or attach it with `scaena patch` / `data_attach`."
                        )),
                );
            }
        }
    }
    // A link to a state the deck does not have (PLAN 2.70), in any text's runs: a node's own, a
    // state's, or the deck's overrides.
    let texts =
        (deck.nodes.iter().map(|(id, n)| (id, &n.props, format!("/nodes/{}", esc(id)))))
            .chain(deck.states.iter().enumerate().flat_map(|(i, s)| {
                s.props.iter().map(move |(id, p)| (id, p, format!("/states/{i}/props/{}", esc(id))))
            }))
            .chain(deck.overrides.iter().map(|(id, o)| (id, o, format!("/overrides/{}", esc(id)))));
    for (id, props, path) in texts {
        let Some(Value::Array(runs)) = props.get("runs") else { continue };
        for (k, run) in runs.iter().enumerate() {
            if let Some(Value::String(to)) = run.get("link").and_then(|l| l.get("state"))
                && !state_ids.contains(to.as_str())
            {
                out.push(
                    err("E102", format!("a link in `{id}` goes to unknown state `{to}`"))
                        .at(format!("{path}/runs/{k}/link/state"))
                        .node(id.clone()),
                );
            }
            // A quote of a data source the deck does not have (ADR-0019).
            if let Some(Value::String(data)) = run.get("quote").and_then(|q| q.get("data"))
                && let Some(name) = data.strip_prefix('@')
                && !deck.data.contains_key(name)
            {
                out.push(
                    err("E102", format!("a quote in `{id}` reads unknown data source `{data}`"))
                        .at(format!("{path}/runs/{k}/quote/data"))
                        .node(id.clone())
                        .hint(format!("Declare it under /data/{name}, or quote one the deck has.")),
                );
            }
        }
    }
    if let Some(spine) = &deck.spine {
        let mut beat_ids = HashSet::new();
        for (si, sec) in spine.sections.iter().enumerate() {
            for (bi, beat) in sec.beats.iter().enumerate() {
                if !beat_ids.insert(&beat.id) {
                    out.push(
                        err("E105", format!("duplicate beat id `{}`", beat.id))
                            .at(format!("/spine/sections/{si}/beats/{bi}/id")),
                    );
                }
                for st in &beat.states {
                    if !state_ids.contains(st.as_str()) {
                        out.push(
                            err("E102", format!("beat `{}` references unknown state `{st}`", beat.id))
                                .at(format!("/spine/sections/{si}/beats/{bi}/states")),
                        );
                    }
                }
            }
        }
    }

    // --- tracking --------------------------------------------------------
    // Unknown-node / unknown-from errors were reported above; only the ordering rule is new here.
    if let Err(e @ crate::tracking::TrackingError::ForwardFrom { .. }) = crate::tracking::resolve_states(deck) {
        out.push(err("E102", e.to_string()));
    }
    out
}

fn choreo_targets(item: &Value) -> Vec<String> {
    let mut v = Vec::new();
    match item.get("target") {
        Some(Value::String(s)) => v.push(s.clone()),
        Some(Value::Array(a)) => v.extend(a.iter().filter_map(|x| x.as_str().map(String::from))),
        _ => {}
    }
    for key in ["sequence", "parallel"] {
        if let Some(Value::Array(items)) = item.get(key) {
            v.extend(items.iter().flat_map(choreo_targets));
        }
    }
    v
}

// --- the bundle ------------------------------------------------------------

/// The files around a deck, as validation sees them; core reads no filesystem itself.
pub trait BundleFiles {
    /// Whether the bundle holds a file at `path`, a path inside the bundle.
    fn exists(&self, path: &str) -> bool;
    /// The file at `path` as text, if the bundle holds it.
    fn read_text(&self, path: &str) -> Option<String>;
}

/// Everything `scaena validate` checks (SPEC §7.1), as findings:
/// - the deck against its schema, and its theme against the theme schema (E106; an
///   invalid id is E105);
/// - a key written twice in one object (E105 for an id, E106 for anything else);
/// - what a schema cannot say: references to nodes, states, data, files, and theme names
///   (E102), a delta that sets `type` (E104), an id used twice (E105), and each state,
///   resolved, against its nodes' types (E106).
///
/// `Err` only when `deck_json` is not JSON.
pub fn validate_bundle(deck_json: &str, files: &dyn BundleFiles) -> Result<Vec<Finding>, serde_json::Error> {
    let doc: Value = serde_json::from_str(deck_json)?;
    let mut out = Vec::new();
    for path in repeated_keys(deck_json)? {
        out.push(locate(repeated(&path), &doc, &path));
    }
    for v in Checker::deck().check(&doc) {
        // A delta's `type` is E104's (below).
        if !(v.kind == Kind::Unknown && delta_key(&v.path) == Some("type")) {
            let path = v.path.clone();
            out.push(locate(schema_finding(v), &doc, &path));
        }
    }
    let theme = load_theme(&doc, files, &mut out);
    if let Ok(deck) = Deck::from_json(deck_json) {
        out.extend(validate(&deck));
        out.extend(type_changes(&deck));
        out.extend(missing_files(&deck, theme.as_ref(), files));
        // Tracking that fails (reported above) leaves no states to resolve.
        let snapshots = resolve_states(&deck).ok();
        if let Some(snapshots) = &snapshots {
            out.extend(resolved_types(&deck, snapshots));
            out.extend(containers(&deck, snapshots));
            out.extend(annotations(&deck, snapshots));
            out.extend(projections(&deck, snapshots));
            out.extend(encodings(&deck, snapshots, files));
            out.extend(quotes(&deck, &doc, files));
        }
        out.extend(override_types(&deck));
        out.extend(format_types(&deck));
        out.extend(cues(&deck, theme.as_ref()));
        if let Some(theme) = &theme {
            out.extend(theme.undefined_names());
            out.extend(theme_names(&deck, snapshots.as_deref(), theme));
            out.extend(shader_presets(&deck, snapshots.as_deref().unwrap_or_default(), theme));
        }
        out.extend(grid_cells(&deck, snapshots.as_deref().unwrap_or_default(), theme.as_ref()));
    }
    // Each finding once, and an id problem once per place: the schema and the semantic
    // checks can both see an invalid id, in their own words.
    let mut seen = HashSet::new();
    out.retain(|f| seen.insert((f.code.clone(), f.file.clone(), f.path.clone(), f.message.clone())));
    let mut placed = HashSet::new();
    out.retain(|f| f.code != "E105" || placed.insert((f.file.clone(), f.path.clone())));
    // A value no node type takes fails the schema and its node's type both; the type's
    // check, which comes later, says it in the node's terms.
    let mut typed = HashSet::new();
    out.reverse();
    out.retain(|f| f.code != "E106" || typed.insert((f.file.clone(), f.path.clone())));
    out.reverse();
    Ok(out)
}

/// A schema violation as a finding: an invalid id, or one listed twice, is E105; the rest
/// are E106.
fn schema_finding(v: Violation) -> Finding {
    let code = match (v.def.as_deref(), v.kind) {
        (Some("Id"), _) | (_, Kind::Name) | (Some("IdList"), Kind::Repeated) => "E105",
        _ => "E106",
    };
    let mut finding = Finding::new(code, Severity::Error, v.message).at(v.path.clone());
    if v.path == "/scaena" {
        finding = finding.hint(format!("This build reads deck format {}.", crate::FORMAT_VERSION));
    }
    finding
}

/// A key written twice in one object: a parser keeps the second, so the first is lost.
fn repeated(path: &str) -> Finding {
    let tokens: Vec<String> = tokens(path);
    let key = tokens.last().cloned().unwrap_or_default();
    let id_map = match tokens.as_slice() {
        [map, _] => matches!(map.as_str(), "nodes" | "data" | "overrides"),
        [states, _, props, _] => states == "states" && props == "props",
        _ => false,
    };
    let (code, what) = if id_map { ("E105", "id") } else { ("E106", "key") };
    Finding::new(
        code,
        Severity::Error,
        format!("{what} `{key}` is written twice in one object; only the last one counts"),
    )
    .at(path)
}

/// `finding` with the state and node its `path` into the deck is about.
fn locate(mut finding: Finding, doc: &Value, path: &str) -> Finding {
    match tokens(path).as_slice() {
        [nodes, id, ..] if nodes == "nodes" || nodes == "overrides" => finding = finding.node(id.clone()),
        [states, i, rest @ ..] if states == "states" => {
            if let Some(id) = i.parse::<usize>().ok().and_then(|i| doc["states"][i]["id"].as_str()) {
                finding = finding.state(id);
            }
            if let [props, node, ..] = rest
                && props == "props"
            {
                finding = finding.node(node.clone());
            }
        }
        _ => {}
    }
    finding
}

/// The key a path names inside a state's delta or a node's overrides
/// (`/states/2/props/title/type`, `/overrides/title/type` → `type`).
fn delta_key(path: &str) -> Option<&str> {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        ["", "states", _, "props", _, key] | ["", "overrides", _, key] => Some(key),
        _ => None,
    }
}

/// A JSON pointer's tokens, unescaped.
fn tokens(path: &str) -> Vec<String> {
    path.split('/').skip(1).map(|t| t.replace("~1", "/").replace("~0", "~")).collect()
}

/// `path` extended by one JSON-pointer token.
fn child(path: &str, token: &str) -> String {
    format!("{path}/{}", esc(token))
}

/// The theme, checked against its schema, with its typed view when it has one.
struct LoadedTheme {
    theme: Theme,
    /// The bundle file it came from; `None` when it is inline in the deck.
    file: Option<String>,
    /// Where its paths start: `/theme` inline, the file's root otherwise.
    root: &'static str,
}

impl LoadedTheme {
    /// A finding about the theme, at `path` inside it.
    fn finding(&self, code: &str, message: String, path: &str) -> Finding {
        let finding = Finding::new(code, Severity::Error, message).at(format!("{}{path}", self.root));
        match &self.file {
            Some(file) => finding.file(file.clone()),
            None => finding,
        }
    }

    /// E102: names the theme uses that it does not define.
    fn undefined_names(&self) -> Vec<Finding> {
        let t = &self.theme;
        let mut out = Vec::new();
        let mut need = |defined: bool, what: &str, name: &str, path: String| {
            if !defined {
                out.push(self.finding("E102", format!("{what} `{name}` is not in the theme{}", self.has(what)), &path));
            }
        };
        for (name, color) in &t.tokens.roles {
            need(t.tokens.color.contains_key(color), "color", color, format!("/tokens/roles/{}", esc(name)));
        }
        for (key, family) in &t.typography.families {
            for (i, fallback) in family.fallback.iter().flatten().enumerate() {
                let path = format!("/type/families/{}/fallback/{i}", esc(key));
                need(t.typography.families.contains_key(fallback), "font family", fallback, path);
            }
        }
        for (name, role) in &t.typography.roles {
            let at = format!("/type/roles/{}", esc(name));
            need(t.typography.families.contains_key(&role.family), "font family", &role.family, format!("{at}/family"));
            if let Some(color) = &role.color {
                need(self.color(color), "color", color, format!("{at}/color"));
            }
        }
        for (name, layout) in &t.layouts {
            for (slot, def) in &layout.slots {
                if let Some(role) = &def.role {
                    let path = format!("/layouts/{}/slots/{}/role", esc(name), esc(slot));
                    need(t.typography.roles.contains_key(role), "text role", role, path);
                }
            }
        }
        for (name, preset) in &t.motion.presets {
            let at = format!("/motion/presets/{}", esc(name));
            if let Some(Duration::Named(d)) = &preset.duration {
                need(t.motion.durations.contains_key(d), "duration", d, format!("{at}/duration"));
            }
            if let Some(Easing::Named(e)) = &preset.ease {
                need(t.motion.easings.contains_key(e), "easing", e, format!("{at}/ease"));
            }
            if let Some(s) = &preset.spring {
                need(t.motion.springs.contains_key(s), "spring", s, format!("{at}/spring"));
            }
        }
        if let Some(shaders) = &t.shaders {
            for (name, preset) in shaders.presets.iter().flatten() {
                if let Some(palette) = &preset.palette {
                    let path = format!("/shaders/presets/{}/palette", esc(name));
                    need(self.palette(palette), "shader palette", palette, path);
                }
            }
        }
        if let Some(charts) = &t.charts {
            for (key, rule) in [("axis", &charts.axis), ("gridlines", &charts.gridlines)] {
                let Some(rule) = rule else { continue };
                if let Some(role) = &rule.role {
                    need(t.typography.roles.contains_key(role), "text role", role, format!("/charts/{key}/role"));
                }
                if let Some(color) = &rule.color {
                    need(self.color(color), "color", color, format!("/charts/{key}/color"));
                }
                if let Some(stroke) = &rule.stroke {
                    need(self.stroke(stroke), "stroke", stroke, format!("/charts/{key}/stroke"));
                }
            }
            let legend = charts.legend.as_ref().and_then(|l| l.role.as_ref());
            let roles = [
                ("label", charts.label.as_ref().and_then(|l| l.role.as_ref())),
                ("title", charts.title.as_ref().and_then(|l| l.role.as_ref())),
                ("legend", legend),
            ];
            for (key, role) in roles {
                if let Some(role) = role {
                    need(t.typography.roles.contains_key(role), "text role", role, format!("/charts/{key}/role"));
                }
            }
            if let Some(stroke) = &charts.stroke_width {
                need(self.stroke(stroke), "stroke", stroke, "/charts/strokeWidth".into());
            }
            if let Some(note) = &charts.annotation {
                if let Some(role) = &note.role {
                    need(t.typography.roles.contains_key(role), "text role", role, "/charts/annotation/role".into());
                }
                if let Some(color) = &note.color {
                    need(self.color(color), "color", color, "/charts/annotation/color".into());
                }
                if let Some(stroke) = &note.stroke {
                    need(self.stroke(stroke), "stroke", stroke, "/charts/annotation/stroke".into());
                }
            }
        }
        if let Some(tables) = &t.tables {
            for (key, text) in [("header", &tables.header), ("cell", &tables.cell)] {
                let Some(text) = text else { continue };
                if let Some(role) = &text.role {
                    need(t.typography.roles.contains_key(role), "text role", role, format!("/tables/{key}/role"));
                }
                if let Some(color) = &text.color {
                    need(self.color(color), "color", color, format!("/tables/{key}/color"));
                }
            }
            for (key, rule) in [("rule", &tables.rule), ("rowRule", &tables.row_rule)] {
                let Some(rule) = rule else { continue };
                if let Some(color) = &rule.color {
                    need(self.color(color), "color", color, format!("/tables/{key}/color"));
                }
                if let Some(stroke) = &rule.stroke {
                    need(self.stroke(stroke), "stroke", stroke, format!("/tables/{key}/stroke"));
                }
            }
        }
        out
    }

    /// Whether `color` is a literal, or a color token or role the theme defines.
    fn color(&self, color: &str) -> bool {
        let tokens = &self.theme.tokens;
        let name = color.strip_prefix("color.").unwrap_or(color);
        is_color_literal(color) || tokens.color.contains_key(name) || tokens.roles.contains_key(name)
    }

    fn palette(&self, name: &str) -> bool {
        self.theme.shaders.as_ref().and_then(|s| s.palettes.as_ref()).is_some_and(|p| p.contains_key(name))
    }

    fn shader_preset(&self, name: &str) -> Option<&crate::model::theme::ShaderPreset> {
        self.theme.shaders.as_ref().and_then(|s| s.presets.as_ref()).and_then(|p| p.get(name))
    }

    fn stroke(&self, name: &str) -> bool {
        self.theme.tokens.stroke.as_ref().is_some_and(|s| s.contains_key(name))
    }

    fn data_palette(&self, name: &str) -> bool {
        let data = &self.theme.tokens.data;
        match name {
            "categorical" => true,
            "sequential" => data.sequential.is_some(),
            "diverging" => data.diverging.is_some(),
            _ => false,
        }
    }

    /// What the theme has of the kind `what` names, for a finding about a name it lacks:
    /// `, which has a, b, c`, or `, which has none`. Long lists end in their count.
    fn has(&self, what: &str) -> String {
        let of = match what {
            "text role" => Vocabulary::TextRole,
            "layout" => Vocabulary::Layout,
            "font family" => Vocabulary::FontFamily,
            "color" => Vocabulary::Color,
            "shader palette" => Vocabulary::ShaderPalette,
            "shader preset" => Vocabulary::ShaderPreset,
            "data palette" => Vocabulary::DataPalette,
            "motion preset" => Vocabulary::MotionPreset,
            "duration" => Vocabulary::Duration,
            "easing" => Vocabulary::Easing,
            "spring" => Vocabulary::Spring,
            _ => return String::new(),
        };
        let names = self.theme.names(of);
        const SHOWN: usize = 24;
        match names.len() {
            0 => ", which has none".into(),
            n if n > SHOWN => format!(", which has {}, … ({n} in all)", names[..SHOWN].join(", ")),
            _ => format!(", which has {}", names.join(", ")),
        }
    }
}

/// `#rrggbb[aa]`, `oklch(…)`, or `oklab(…)`: a color written out, not named.
fn is_color_literal(color: &str) -> bool {
    color.starts_with('#') || color.starts_with("oklch(") || color.starts_with("oklab(")
}

/// The deck's theme, checked against the theme schema: a file in the bundle, or inline.
fn load_theme(doc: &Value, files: &dyn BundleFiles, out: &mut Vec<Finding>) -> Option<LoadedTheme> {
    match doc.get("theme")? {
        Value::String(path) => {
            let Some(text) = files.read_text(path) else {
                let message = format!("theme file `{path}` is not in the bundle");
                out.push(Finding::new("E102", Severity::Error, message).at("/theme"));
                return None;
            };
            let value: Value = match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(e) => {
                    out.push(Finding::new("E106", Severity::Error, format!("not JSON: {e}")).at("").file(path.clone()));
                    return None;
                }
            };
            for at in repeated_keys(&text).unwrap_or_default() {
                out.push(repeated(&at).file(path.clone()));
            }
            out.extend(Checker::theme().check(&value).into_iter().map(|v| schema_finding(v).file(path.clone())));
            let theme = Theme::from_json(&text).ok()?;
            Some(LoadedTheme { theme, file: Some(path.clone()), root: "" })
        }
        inline @ Value::Object(_) => {
            for v in Checker::theme().check(inline) {
                let path = format!("/theme{}", v.path);
                out.push(schema_finding(Violation { path, ..v }));
            }
            let theme = Theme::from_value(inline).ok()?;
            Some(LoadedTheme { theme, file: None, root: "/theme" })
        }
        _ => None,
    }
}

/// E104: a node's type never changes, so neither a state's delta nor the node's overrides
/// can set `type`.
fn type_changes(deck: &Deck) -> Vec<Finding> {
    let mut out = Vec::new();
    for (id, over) in &deck.overrides {
        let (Some(node), Some(_)) = (deck.nodes.get(id), over.get("type")) else { continue };
        let is = type_name(node.node_type);
        out.push(
            Finding::new(
                "E104",
                Severity::Error,
                format!("`{id}` is {} node; overrides cannot set `type`", article(&is)),
            )
            .at(child(&format!("/overrides/{}", esc(id)), "type"))
            .node(id.clone()),
        );
    }
    for (i, state) in deck.states.iter().enumerate() {
        for (id, delta) in &state.props {
            let (Some(node), Some(new)) = (deck.nodes.get(id), delta.get("type")) else { continue };
            let is = type_name(node.node_type);
            let message = match new.as_str() {
                Some(new) if new != is => {
                    format!("`{id}` is {} node; a state cannot make it {}", article(&is), article(new))
                }
                _ => format!("`{id}` is {} node; a state cannot set `type`", article(&is)),
            };
            out.push(
                Finding::new("E104", Severity::Error, message)
                    .at(child(&format!("/states/{i}/props/{}", esc(id)), "type"))
                    .state(state.id.clone())
                    .node(id.clone())
                    .hint("A node's type is set once, in `nodes`. To show something else, add a node and `remove` this one."),
            );
        }
    }
    out
}

/// E102: files the deck and its theme name that the bundle does not hold.
fn missing_files(deck: &Deck, theme: Option<&LoadedTheme>, files: &dyn BundleFiles) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut need = |what: &str, file: &str, at: String| {
        if !files.exists(file) {
            out.push(Finding::new("E102", Severity::Error, format!("{what} `{file}` is not in the bundle")).at(at));
        }
    };
    for (i, font) in deck.fonts.iter().enumerate() {
        need("font file", &font.file, format!("/fonts/{i}/file"));
    }
    for (name, data) in &deck.data {
        if let Value::String(file) = &data.source {
            need("data file", file, format!("/data/{}/source", esc(name)));
        }
    }
    for (id, node) in deck.nodes.iter().filter(|(_, n)| n.node_type == NodeType::Image) {
        if let Some(Value::String(src)) = node.props.get("src") {
            need("image", src, format!("/nodes/{}/src", esc(id)));
        }
        for (i, state) in deck.states.iter().enumerate() {
            if let Some(Value::String(src)) = state.props.get(id).and_then(|d| d.get("src")) {
                need("image", src, format!("/states/{i}/props/{}/src", esc(id)));
            }
        }
    }
    if let Some(theme) = theme {
        for (key, family) in &theme.theme.typography.families {
            // The family's own file, and its italic face's (PLAN 2.40).
            let italic = family.italic.as_ref().map(|face| (face.file.as_str(), "/italic/file", Some("italic")));
            for (file, at, style) in std::iter::once((family.file.as_str(), "/file", None)).chain(italic) {
                if !files.exists(file) {
                    let message = format!("font file `{file}` is not in the bundle");
                    out.push(theme.finding("E102", message, &format!("/type/families/{}{at}", esc(key))));
                }
                // Rendering registers the fonts the deck lists, and needs every family's.
                if !deck.fonts.iter().any(|f| f.file == file) {
                    let face = if style.is_some() { "'s italic" } else { "" };
                    let message = format!(
                        "theme family `{key}`{face} is set in `{file}`, which the deck's `fonts` does not list"
                    );
                    let mut font = json!({ "family": family.family, "file": file });
                    if let Some(style) = style {
                        font["style"] = json!(style);
                    }
                    out.push(
                        Finding::new("E102", Severity::Error, message)
                            .at("/fonts")
                            .hint("Rendering registers only the fonts the deck lists.")
                            .fix(vec![json!({ "op": "add", "path": "/fonts/-", "value": font })]),
                    );
                }
            }
        }
    }
    out
}

/// E106: each node with its overrides merged in, against its type. Overrides are a delta
/// with no `type` (as a state's is), so another type's property, or a value this type does
/// not take, shows only once they meet the node; each is reported where the overrides
/// wrote it.
fn override_types(deck: &Deck) -> Vec<Finding> {
    let checker = Checker::deck();
    let defs: HashMap<String, String> = checker.node_types().into_iter().map(|(def, tag)| (tag, def)).collect();
    let mut out = Vec::new();
    for (id, over) in &deck.overrides {
        let Some(node) = deck.nodes.get(id) else { continue };
        let tag = type_name(node.node_type);
        let Some(def) = defs.get(&tag) else { continue };
        let mut props = node.props.clone();
        crate::tracking::merge_props(&mut props, over);
        let mut resolved = Map::new();
        resolved.insert("type".into(), Value::from(tag.as_str()));
        resolved.extend(props.into_iter().filter(|(k, _)| k != "type"));
        for v in checker.check_def(def, &Value::Object(resolved), "") {
            let Some(prop) = tokens(&v.path).into_iter().next() else { continue };
            if prop == "type" || !over.contains_key(&prop) {
                continue;
            }
            let message = match v.kind {
                Kind::Missing => format!("{tag} nodes need `{prop}`; the overrides delete it"),
                _ => v.message,
            };
            out.push(
                Finding::new("E106", Severity::Error, message)
                    .at(format!("/overrides/{}{}", esc(id), v.path))
                    .node(id.clone()),
            );
        }
    }
    out
}

/// A node's layout in its formats (SPEC §3.4, ADR-0020): E102 for a format the deck does not
/// lay out in (one it does not list, or its own canvas's shape, which the node's own props lay
/// out), and E106 for what the node's type does not take there, as its own props would be.
fn format_types(deck: &Deck) -> Vec<Finding> {
    let checker = Checker::deck();
    let defs: HashMap<String, String> = checker.node_types().into_iter().map(|(def, tag)| (tag, def)).collect();
    let own = [deck.canvas.width, deck.canvas.height];
    let mut out = Vec::new();
    // A node's layout in its formats is the node's own: no state, nor the deck's overrides,
    // changes it.
    let elsewhere = deck.states.iter().enumerate().flat_map(|(i, state)| {
        state
            .props
            .iter()
            .map(move |(id, delta)| (format!("/states/{i}/props/{}/formats", esc(id)), id, delta, Some(state)))
    });
    let overrides = deck.overrides.iter().map(|(id, over)| (format!("/overrides/{}/formats", esc(id)), id, over, None));
    for (at, id, delta, state) in elsewhere.chain(overrides) {
        if delta.contains_key("formats") {
            let message = format!("`{id}`'s layout in its formats is the node's own: set `formats` on the node");
            let finding = Finding::new("E106", Severity::Error, message).at(at).node(id.clone());
            out.push(match state {
                Some(state) => finding.state(state.id.clone()),
                None => finding,
            });
        }
    }
    for (id, node) in &deck.nodes {
        let Some(formats) = node.props.get("formats").and_then(Value::as_object) else { continue };
        let tag = type_name(node.node_type);
        for (name, layout) in formats {
            let at = format!("/nodes/{}/formats/{}", esc(id), esc(name));
            let laid = Format::parse(name).filter(|f| f.canvas(own) != own && deck.formats.iter().any(|g| g == name));
            if laid.is_none() {
                let why = match Format::parse(name).is_some_and(|f| f.canvas(own) == own) {
                    true => "the deck's own canvas, which the node's own props lay out".to_string(),
                    false => format!("not one of the deck's formats ({})", deck.formats.join(", ")),
                };
                out.push(
                    Finding::new("E102", Severity::Error, format!("node `{id}` lays out in `{name}`: {why}"))
                        .at(at.clone())
                        .node(id.clone()),
                );
                continue;
            }
            let (Some(def), Some(layout)) = (defs.get(&tag), layout.as_object()) else { continue };
            let mut resolved = Map::new();
            resolved.insert("type".into(), Value::from(tag.as_str()));
            resolved.extend(
                node.props.iter().filter(|(k, _)| *k != "type" && *k != "formats").map(|(k, v)| (k.clone(), v.clone())),
            );
            resolved.extend(layout.iter().map(|(k, v)| (k.clone(), v.clone())));
            for v in checker.check_def(def, &Value::Object(resolved), "") {
                let Some(prop) = tokens(&v.path).into_iter().next() else { continue };
                if layout.contains_key(&prop) {
                    out.push(
                        Finding::new("E106", Severity::Error, v.message).at(format!("{at}{}", v.path)).node(id.clone()),
                    );
                }
            }
        }
    }
    out
}

/// E106: each state resolved, each node against its type. A delta carries no `type`, so
/// another type's property, or a value this type does not take, shows only in the
/// resolved state. Each is reported at the delta that wrote it; node defaults were checked
/// with the deck, and a value a later state tracks was reported where it was written.
fn resolved_types(deck: &Deck, snapshots: &[Snapshot]) -> Vec<Finding> {
    let checker = Checker::deck();
    let defs: HashMap<String, String> = checker.node_types().into_iter().map(|(def, tag)| (tag, def)).collect();
    let mut out = Vec::new();
    for (i, (state, snapshot)) in deck.states.iter().zip(snapshots).enumerate() {
        for (id, props) in &snapshot.nodes {
            let (Some(delta), Some(node)) = (state.props.get(id), deck.nodes.get(id)) else { continue };
            let tag = type_name(node.node_type);
            let Some(def) = defs.get(&tag) else { continue };
            let mut resolved = Map::new();
            resolved.insert("type".into(), Value::from(tag.as_str()));
            resolved.extend(props.iter().map(|(k, v)| (k.clone(), v.clone())));
            for v in checker.check_def(def, &Value::Object(resolved), "") {
                let Some(prop) = tokens(&v.path).into_iter().next() else { continue };
                if prop == "type" || !delta.contains_key(&prop) {
                    continue;
                }
                let message = match v.kind {
                    Kind::Missing => format!("{} nodes need `{prop}`; this state deletes it", tag),
                    _ => v.message,
                };
                let path = format!("/states/{i}/props/{}{}", esc(id), v.path);
                out.push(
                    Finding::new("E106", Severity::Error, message).at(path).state(state.id.clone()).node(id.clone()),
                );
            }
        }
    }
    out
}

/// E106: each chart's annotations stand where their kinds can (SPEC §3.7): a rule at one
/// `x` or `y`, a band across two, a callout at an `x`, a highlight on categories or series.
/// Each finding points at the `annotations` that set it in the state: its delta, or the
/// node.
fn annotations(deck: &Deck, snapshots: &[Snapshot]) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (i, (state, snapshot)) in deck.states.iter().zip(snapshots).enumerate() {
        for (id, props) in &snapshot.nodes {
            if deck.nodes.get(id).is_none_or(|n| n.node_type != NodeType::Chart) {
                continue;
            }
            let Some(notes) = props.get("annotations").and_then(Value::as_array) else { continue };
            let base = match state.props.get(id).and_then(|d| d.get("annotations")) {
                Some(_) => format!("/states/{i}/props/{}/annotations", esc(id)),
                None => format!("/nodes/{}/annotations", esc(id)),
            };
            let donut = props.get("kind").and_then(Value::as_str) == Some("donut");
            for (k, note) in notes.iter().enumerate() {
                // One that does not parse breaks the schema, which says so.
                let Ok(note) = serde_json::from_value::<Annotation>(note.clone()) else { continue };
                let Err(message) = note.check(donut) else { continue };
                let path = format!("{base}/{k}");
                if seen.insert((path.clone(), message.clone())) {
                    out.push(
                        Finding::new("E106", Severity::Error, message)
                            .at(path)
                            .state(state.id.clone())
                            .node(id.clone()),
                    );
                }
            }
        }
    }
    out
}

/// E106: what a chart of some kinds alone takes (SPEC §3.7). `projected` marks the rows of a
/// line or an area, and a chart of another kind draws nothing projected; `orient` turns a
/// `bar`'s or a `stackedBar`'s bars, and another kind has none; `interval` gives a `range`'s
/// values their intervals. A `range` spans each category's series or each value's
/// interval, so one with neither draws nothing. Each finding points at what set it in the
/// state.
fn projections(deck: &Deck, snapshots: &[Snapshot]) -> Vec<Finding> {
    const ONLY: [(&str, &[&str], &str); 3] = [
        ("projected", &["line", "area"], "marks the rows of a line or an area; a `{kind}` chart draws none"),
        ("orient", &["bar", "stackedBar"], "turns a bar chart's bars; a `{kind}` chart has none"),
        ("interval", &["range"], "gives a range's values their intervals; a `{kind}` chart has none"),
    ];
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (i, (state, snapshot)) in deck.states.iter().zip(snapshots).enumerate() {
        for (id, props) in &snapshot.nodes {
            if deck.nodes.get(id).is_none_or(|n| n.node_type != NodeType::Chart) {
                continue;
            }
            let kind = props.get("kind").and_then(Value::as_str).unwrap_or_default();
            for (prop, kinds, says) in ONLY {
                if props.get(prop).is_none() || kinds.contains(&kind) {
                    continue;
                }
                let path = match state.props.get(id).and_then(|d| d.get(prop)) {
                    Some(_) => format!("/states/{i}/props/{}/{prop}", esc(id)),
                    None => format!("/nodes/{}/{prop}", esc(id)),
                };
                let message = format!("`{prop}` {}", says.replace("{kind}", kind));
                if seen.insert(path.clone()) {
                    out.push(
                        Finding::new("E106", Severity::Error, message)
                            .at(path)
                            .state(state.id.clone())
                            .node(id.clone()),
                    );
                }
            }
            if kind == "range" && ["series", "color", "interval"].iter().all(|p| props.get(*p).is_none()) {
                let path = match state.props.get(id).and_then(|d| d.get("kind")) {
                    Some(_) => format!("/states/{i}/props/{}/kind", esc(id)),
                    None => format!("/nodes/{}/kind", esc(id)),
                };
                let message = "a `range` spans each category's series, or each value's interval: give it a \
                               `series` or an `interval`";
                if seen.insert(path.clone()) {
                    out.push(
                        Finding::new("E106", Severity::Error, message)
                            .at(path)
                            .state(state.id.clone())
                            .node(id.clone()),
                    );
                }
            }
        }
    }
    out
}

/// Each state's containers (SPEC §3.4, ADR-0008): a node's `at.parent` is a container the
/// state shows (E102) and of a container type (E106), containers do not nest in a loop
/// (E106), and an `at.area` is one of its grid's areas (E102). Each finding points at
/// the `at` that placed the node in the state: its delta, or the node itself.
fn containers(deck: &Deck, snapshots: &[Snapshot]) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, (state, snapshot)) in deck.states.iter().zip(snapshots).enumerate() {
        let parent_of = |id: &str| snapshot.nodes.get(id)?.get("at")?.get("parent")?.as_str();
        let here = |id: &str, key: &str| match state.props.get(id).and_then(|d| d.get("at")) {
            Some(at) if at.get(key).is_some() => format!("/states/{i}/props/{}/at/{key}", esc(id)),
            _ => format!("/nodes/{}/at/{key}", esc(id)),
        };
        let finding = |code: &str, id: &str, key: &str, message: String| {
            Finding::new(code, Severity::Error, message).at(here(id, key)).state(state.id.clone()).node(id.to_string())
        };
        for id in snapshot.nodes.keys() {
            let Some(parent) = parent_of(id) else { continue };
            let Some(container) = deck.nodes.get(parent) else { continue };
            if !snapshot.nodes.contains_key(parent) {
                let message =
                    format!("node `{id}` is in container `{parent}`, which state `{}` does not show", state.id);
                out.push(finding("E102", id, "parent", message));
                continue;
            }
            let kind = container.node_type;
            if !matches!(kind, NodeType::Stack | NodeType::Grid | NodeType::Frame | NodeType::Group) {
                let message = format!(
                    "node `{id}` is placed in `{parent}`, {} node; a container is a stack, grid, frame, or group",
                    article(&type_name(kind))
                );
                out.push(finding("E106", id, "parent", message));
                continue;
            }
            // Up the chain: a loop comes back to `id` within as many steps as there are nodes.
            let mut chain = vec![id.as_str(), parent];
            let rooted = loop {
                let Some(next) = parent_of(chain[chain.len() - 1]) else { break true };
                if next == id {
                    chain.push(next);
                    let message = format!("containers nest in a loop: {}", chain.join(" → "));
                    out.push(finding("E106", id, "parent", message));
                    break false;
                }
                if chain.len() > snapshot.nodes.len() {
                    break false;
                }
                chain.push(next);
            };
            // The first node deeper than containers nest: what is in it is too deep as well.
            if rooted && chain.len() - 1 == MAX_NESTING + 1 {
                let message = format!(
                    "node `{id}` is {} containers deep; containers nest at most {MAX_NESTING} deep",
                    chain.len() - 1
                );
                out.push(finding("E106", id, "parent", message));
            }
            if let Some(area) = snapshot.nodes[id].get("at").and_then(|at| at.get("area")).and_then(Value::as_str) {
                let areas = snapshot.nodes[parent].get("areas").and_then(Value::as_array);
                let named = areas.is_some_and(|rows| {
                    rows.iter().filter_map(Value::as_str).any(|row| row.split_whitespace().any(|cell| cell == area))
                });
                if kind != NodeType::Grid || !named {
                    let message = format!("area `{area}` is not one of the areas of {} `{parent}`", type_name(kind));
                    out.push(finding("E102", id, "area", message));
                }
            }
        }
    }
    out
}

/// E103 and E106: what each run's `quote` reads (ADR-0019): a row, a column, and a cell that are
/// there, and a format and a transform that parse. A source the deck lacks is E102, found with
/// the deck's other references; a source that does not read is reported at the source.
fn quotes(deck: &Deck, doc: &Value, files: &dyn BundleFiles) -> Vec<Finding> {
    let mut out = Vec::new();
    for q in crate::quotes::quoted(doc) {
        let Some(quote) = q.run.get("quote").and_then(|v| serde_json::from_value(v.clone()).ok()) else {
            continue;
        };
        if let Err(Some(p)) = crate::quotes::value(deck, &data::Texts(files), &quote)
            && p.code != "E102"
        {
            out.push(
                Finding::new(p.code, Severity::Error, format!("a quote in `{}`: {}", q.node, p.message))
                    .at(format!("{}/quote/{}", q.path, p.at))
                    .node(q.node),
            );
        }
    }
    out
}

/// E103: what charts read from their data. Each encoding's `field`, and the chart's
/// `key`, is a column of its source that the encoding can read: a `quantitative` channel
/// reads numbers and a `temporal` one dates, and a `format` prints numbers or dates. A
/// source whose values do not fit its schema or its `parse` formats is E103 too, at the
/// source, and so is a key that repeats in the data a chart or a table shows, made as
/// rendering makes it (SPEC §3.3, §3.7): at its `key`, else at what keys it without one,
/// a chart's `x` or a table's first column (its `data`, when it lists no columns). A
/// `format` that does not parse is E106. Each finding points at what set the
/// field in the state: its delta, or the node. A chart with a `dataTransform` reads
/// columns the transform makes, so its fields are checked once transforms run (PLAN 1.9e).
fn encodings(deck: &Deck, snapshots: &[Snapshot], files: &dyn BundleFiles) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut tables: BTreeMap<&str, Table> = BTreeMap::new();
    for name in deck.data.keys() {
        match data::load(deck, &data::Texts(files), name) {
            Ok(table) => {
                tables.insert(name, table);
            }
            Err(DataError::Bad(message)) => {
                out.push(Finding::new("E103", Severity::Error, message).at(format!("/data/{}/source", esc(name))));
            }
            // A missing source or file is E102's.
            Err(_) => {}
        }
    }
    let mut seen = HashSet::new();
    for (i, (state, snapshot)) in deck.states.iter().zip(snapshots).enumerate() {
        for (id, props) in &snapshot.nodes {
            let Some(node) = deck.nodes.get(id) else { continue };
            if !matches!(node.node_type, NodeType::Chart | NodeType::Table) {
                continue;
            }
            let Some(name) = props.get("data").and_then(Value::as_str).and_then(|d| d.strip_prefix('@')) else {
                continue;
            };
            let Some(source) = tables.get(name) else { continue };
            let here = |key: &str, rest: &str| match state.props.get(id).and_then(|d| d.get(key)) {
                Some(_) => format!("/states/{i}/props/{}/{key}{rest}", esc(id)),
                None => format!("/nodes/{}/{key}{rest}", esc(id)),
            };
            let mut found = |code: &str, path: String, message: String| {
                if seen.insert((path.clone(), message.clone())) {
                    out.push(
                        Finding::new(code, Severity::Error, message).at(path).state(state.id.clone()).node(id.clone()),
                    );
                }
            };
            // The table the chart or table reads: the source, through its transform.
            let steps = props.get("dataTransform").and_then(Value::as_array);
            let transformed;
            let table = match steps.map(|steps| transform::apply(source.clone(), steps)) {
                None => source,
                Some(Ok(t)) => {
                    transformed = t;
                    &transformed
                }
                Some(Err(e)) => {
                    let rest: String = e.at.iter().map(|k| format!("/{}", esc(k))).collect();
                    let code = if e.data { "E103" } else { "E106" };
                    found(code, here("dataTransform", &format!("/{}{rest}", e.step)), format!("`@{name}`: {e}"));
                    continue;
                }
            };
            let read =
                if steps.is_some() { format!("`@{name}` after its `dataTransform`") } else { format!("`@{name}`") };
            let column = |field: &str| table.column(field).map(|c| table.types[c]);
            let missing = |field: &str| {
                format!(
                    "{read} has no column `{field}`; it has {}",
                    table.columns.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ")
                )
            };
            let key = props.get("key").and_then(Value::as_str);
            if let Some(key) = key
                && column(key).is_none()
            {
                found("E103", here("key", ""), missing(key));
            }
            // What reads a field: a chart's channels, or a table's columns, each with the
            // key its path starts at, the rest of the path, and its name in a message.
            let readers: Vec<(&str, String, String, &Map<String, Value>)> = match node.node_type {
                NodeType::Chart => ["x", "y", "series", "color", "sizeEncoding", "projected"]
                    .into_iter()
                    .filter_map(|c| {
                        props.get(c).and_then(Value::as_object).map(|e| (c, String::new(), c.to_string(), e))
                    })
                    .collect(),
                _ => (props.get("columns").and_then(Value::as_array).into_iter().flatten().enumerate())
                    .filter_map(|(k, c)| {
                        c.as_object().map(|e| ("columns", format!("/{k}"), format!("columns[{k}]"), e))
                    })
                    .collect(),
            };
            for (key, at, channel, encoding) in readers {
                let channel = channel.as_str();
                let here = |rest: &str| here(key, &format!("{at}{rest}"));
                let Some(field) = encoding.get("field").and_then(Value::as_str) else { continue };
                let Some(kind) = column(field) else {
                    found("E103", here("/field"), missing(field));
                    continue;
                };
                let wants = match encoding.get("type").and_then(Value::as_str) {
                    Some("quantitative") => Some(ColumnType::Number),
                    Some("temporal") => Some(ColumnType::Date),
                    _ => None,
                };
                if let Some(wants) = wants
                    && kind != wants
                {
                    let message = format!(
                        "`{channel}` reads `{field}` as {}, but {read} types it {}; declare it `{}` in the source's schema",
                        encoding["type"].as_str().unwrap_or_default(),
                        article(kind.name()),
                        wants.name()
                    );
                    found("E103", here("/type"), message);
                }
                if let Some(spec) = encoding.get("format").and_then(Value::as_str) {
                    let parsed = match kind {
                        ColumnType::Number => NumberFormat::parse(spec).map(|_| ()),
                        ColumnType::Date => DateFormat::parse(spec).map(|_| ()),
                        _ => {
                            let message = format!(
                                "`{channel}.format` prints numbers and dates; `{field}` is {} column",
                                article(kind.name())
                            );
                            found("E103", here("/format"), message);
                            continue;
                        }
                    };
                    if let Err(e) = parsed {
                        found("E106", here("/format"), e.to_string());
                    }
                }
            }
            // What marks a row projected: a value its column can hold, or, with none, a
            // true one in a boolean column. A column that is not there was reported above.
            if let Some(projected) = props.get("projected").and_then(Value::as_object)
                && let Some(field) = projected.get("field").and_then(Value::as_str)
                && let Some(kind) = column(field)
            {
                let message = match (projected.get("value"), kind) {
                    (None, ColumnType::Boolean) => None,
                    (None, _) => Some((
                        "/field",
                        format!(
                            "`projected` marks a row whose `{field}` is true, but {read} types it {}; say which `value` marks one, or declare it `boolean` in the source's schema",
                            article(kind.name())
                        ),
                    )),
                    (Some(v), _) => {
                        let fits = match kind {
                            ColumnType::Boolean => v.is_boolean(),
                            ColumnType::Number => v.is_number(),
                            ColumnType::String | ColumnType::Date => v.is_string(),
                        };
                        (!fits).then(|| {
                            let what = match v {
                                Value::Bool(_) => "a boolean",
                                Value::Number(_) => "a number",
                                _ => "text",
                            };
                            (
                                "/value",
                                format!(
                                    "`projected.value` is {what}, but {read} types `{field}` {}",
                                    article(kind.name())
                                ),
                            )
                        })
                    }
                };
                if let Some((rest, message)) = message {
                    found("E103", here("projected", rest), message);
                }
            }
            // A range's interval: each end a column of numbers (PLAN 1.30).
            for (end, field) in (props.get("interval").and_then(Value::as_object).into_iter())
                .flat_map(|i| ["low", "high"].map(|end| (end, i.get(end).and_then(Value::as_str))))
            {
                let Some(field) = field else { continue };
                let message = match column(field) {
                    None => missing(field),
                    Some(ColumnType::Number) => continue,
                    Some(kind) => format!(
                        "`interval.{end}` reads `{field}` as numbers, but {read} types it {}; declare it `number` in the source's schema",
                        article(kind.name())
                    ),
                };
                found("E103", here("interval", &format!("/{end}")), message);
            }
            // Keys, which must be unique (SPEC §3.3, §3.7), made as rendering makes them. A
            // column that is not there was reported above.
            let remedy = if key.is_some() { "key it by a field" } else { "give it a `key` field" };
            if node.node_type == NodeType::Table {
                // A row's key is its `key` field, else its first column.
                let (by, path) = match (key, props.get("columns").and_then(Value::as_array)) {
                    (Some(key), _) => (Some(key), here("key", "")),
                    (None, Some(listed)) => {
                        (listed.first().and_then(|c| c.get("field")).and_then(Value::as_str), here("columns", "/0"))
                    }
                    (None, None) => (table.columns.first().map(String::as_str), here("data", "")),
                };
                let Some(c) = by.and_then(|by| table.column(by)) else { continue };
                let repeated = repeats(table.rows.iter().map(|row| row[c].label()));
                if !repeated.is_empty() {
                    let first = if key.is_some() { "" } else { ", its first column" };
                    let message = format!(
                        "row {} in {read}: the table keys each row by `{}`{first}; {remedy} that tells its rows apart",
                        keys_repeat(&repeated),
                        table.columns[c]
                    );
                    found("E103", path, message);
                }
                continue;
            }
            let field = |c: &str| props.get(c).and_then(|e| e.get("field")).and_then(Value::as_str);
            let kind = props.get("kind").and_then(Value::as_str).unwrap_or_default();
            let x = field("x").and_then(|f| table.column(f));
            // A slope compares two states: its x holds two among the rows it draws (PLAN 1.30).
            if kind == "slope"
                && let (Some(x), Some(y)) = (x, field("y").and_then(|f| table.column(f)))
            {
                let states: BTreeSet<String> =
                    table.rows.iter().filter(|row| row[y] != Datum::Null).map(|row| row[x].label()).collect();
                if states.len() != 2 {
                    let message = format!(
                        "a slope compares two states, and `{}` has {} in {read}: filter it to two",
                        table.columns[x],
                        states.len()
                    );
                    found("E103", here("x", "/field"), message);
                }
            }
            // A slope keys its marks by series and state, not by a `key` field.
            let key = key.filter(|_| kind != "slope");
            // A datum's key is its `key` field, else its x, joined with its series (without one,
            // a color field of text) unless that is the key; a donut's are its categories. A row
            // whose `y` is null is a gap, with no key.
            let group = match (field("series"), field("color")) {
                (Some(f), _) => table.column(f).map(Some),
                (None, Some(f)) => table.column(f).map(|c| (table.types[c] != ColumnType::Number).then_some(c)),
                (None, None) => Some(None),
            };
            let base = key.map_or(x, |key| table.column(key));
            if let (Some(base), Some(group), Some(y)) = (base, group, field("y").and_then(|f| table.column(f))) {
                // The series a key joins: none on a donut, nor when it is the key column.
                let joins = |key_col: Option<usize>| group.filter(|&g| kind != "donut" && Some(g) != key_col);
                let keys = |base: usize, series: Option<usize>| {
                    repeats(table.rows.iter().filter(|row| row[y] != Datum::Null).map(|row| match series {
                        Some(s) => format!("{}\u{1f}{}", row[base].label(), row[s].label()),
                        None => row[base].label(),
                    }))
                };
                let by = |base: usize, series: Option<usize>| match series {
                    Some(s) => format!("`{}` and `{}`", table.columns[base], table.columns[s]),
                    None => format!("`{}`", table.columns[base]),
                };
                let own = joins(key.map(|_| base));
                let repeated = keys(base, own);
                if !repeated.is_empty() {
                    // Without its `key`, the chart may tell its data apart by x and series.
                    let fix = match x.filter(|&x| key.is_some() && keys(x, joins(None)).is_empty()) {
                        Some(x) => {
                            let tell = if joins(None).is_some() { "tell" } else { "tells" };
                            format!("drop `key` to key it by {}, which {tell} its rows apart", by(x, joins(None)))
                        }
                        None => format!("{remedy} that tells its rows apart"),
                    };
                    let message = format!(
                        "{} in {read}: the chart keys each datum by {}; {fix}",
                        keys_repeat(&repeated),
                        by(base, own)
                    );
                    found("E103", if key.is_some() { here("key", "") } else { here("x", "/field") }, message);
                }
            }
            // A chart's annotations name its categories (or x values) and series.
            // The series: its field, else a color field of text.
            let series = (field("series").and_then(|f| table.column(f))).or_else(|| {
                field("color").and_then(|f| table.column(f)).filter(|&c| table.types[c] != ColumnType::Number)
            });
            // A continuous x, as the chart compiler reads one (SPEC §3.7).
            let continuous = x.is_some_and(|c| {
                let numeric = matches!(table.types[c], ColumnType::Number | ColumnType::Date);
                matches!(kind, "line" | "area" | "scatter")
                    && match props.get("x").and_then(|x| x.get("type")).and_then(Value::as_str) {
                        Some("quantitative" | "temporal") => true,
                        Some(_) => false,
                        None => numeric && kind == "scatter",
                    }
            });
            let distinct = |c: usize| {
                let mut values: Vec<String> = Vec::new();
                for row in &table.rows {
                    let v = row[c].label();
                    if !values.contains(&v) {
                        values.push(v);
                    }
                }
                values
            };
            let listed = |values: &[String]| {
                let shown: Vec<String> = values.iter().take(8).map(|v| format!("`{v}`")).collect();
                format!("{}{}", shown.join(", "), if values.len() > 8 { ", …" } else { "" })
            };
            let notes = props.get("annotations").and_then(Value::as_array);
            for (k, note) in notes.into_iter().flatten().enumerate() {
                let Ok(note) = serde_json::from_value::<Annotation>(note.clone()) else { continue };
                if note.check(kind == "donut").is_err() {
                    continue;
                }
                let at = |axis: &str| here("annotations", &format!("/{k}/at/{axis}"));
                if let (Some(place), Some(c)) = (&note.at.x, x) {
                    let column = &table.columns[c];
                    let dates = table.types[c] == ColumnType::Date;
                    let categories = distinct(c);
                    for v in place.values() {
                        let message = match continuous {
                            true if v.position(dates).is_none() => format!(
                                "`at.x` `{}` must be {}: the chart's x runs along `{column}`",
                                v.label(),
                                if dates { "a date in ISO 8601 (`2024-03-01`)" } else { "a number" }
                            ),
                            false if !categories.contains(&v.label()) => format!(
                                "`at.x` `{}` is no category of `{column}` in {read}; its categories are {}",
                                v.label(),
                                listed(&categories)
                            ),
                            _ => continue,
                        };
                        found("E103", at("x"), message);
                    }
                }
                if let Some(place) = &note.at.series {
                    let Some(c) = series else {
                        found("E106", at("series"), "`at.series` picks out a series, and the chart has none".into());
                        continue;
                    };
                    let names = distinct(c);
                    for v in place.values().into_iter().filter(|v| !names.contains(&v.label())) {
                        let message = format!(
                            "`at.series` `{}` is no series of `{}` in {read}; its series are {}",
                            v.label(),
                            table.columns[c],
                            listed(&names)
                        );
                        found("E103", at("series"), message);
                    }
                }
            }
        }
    }
    out
}

/// What repeats among `keys`, each once, in the order it first repeats.
fn repeats(keys: impl Iterator<Item = String>) -> Vec<String> {
    let (mut seen, mut twice) = (HashSet::new(), HashSet::new());
    keys.filter(|key| !seen.insert(key.clone()) && twice.insert(key.clone())).collect()
}

/// Keys that repeat as a finding names them, a key's parts joined as the chart compiler
/// prints them: "key `Q1` repeats", or "keys `Q1 · Core`, `Q2 · Core` repeat".
fn keys_repeat(keys: &[String]) -> String {
    let named: Vec<String> = keys.iter().take(8).map(|k| format!("`{}`", k.replace('\u{1f}', " · "))).collect();
    match keys {
        [_] => format!("key {} repeats", named[0]),
        _ => format!("keys {}{} repeat", named.join(", "), if keys.len() > 8 { ", …" } else { "" }),
    }
}

/// E102: a placement past the theme's grid, which layout cannot make. A node placed by
/// `at.col` or `at.row` beyond the grid's columns or rows, or in a slot that runs past them,
/// in the deck's own format or one it lists, where the grid and the slots can differ (SPEC
/// §3.4). Each finding names the grid's size: a node's at its `at`, a slot's in the theme. A
/// node's range that runs backward is E106. A child of a stack, grid, or frame takes its
/// container's lines, and is not judged here. Without a theme, only the E106 is.
fn grid_cells(deck: &Deck, snapshots: &[Snapshot], theme: Option<&LoadedTheme>) -> Vec<Finding> {
    // The grids the deck is laid out on: its own, then each listed format's whose canvas is
    // not the deck's (a format of the deck's own shape lays out the same).
    let own = [deck.canvas.width, deck.canvas.height];
    let mut grids: Vec<(Option<Format>, &Grid)> = Vec::new();
    if let Some(t) = theme.map(|t| &t.theme) {
        grids.push((None, &t.grid));
        for format in deck.formats.iter().filter_map(|f| Format::parse(f)).filter(|f| f.canvas(own) != own) {
            let there = t.formats.as_ref().and_then(|f| f.get(&format)).and_then(|f| f.grid.as_ref());
            grids.push((Some(format), there.unwrap_or(&t.grid)));
        }
    }
    let past = |format: Option<Format>, axis: &str, n: u64| {
        let there = format.map(|f| format!(" in `{}`", f.name())).unwrap_or_default();
        format!("past the theme's grid{there}, which has {n} {}", if axis == "col" { "columns" } else { "rows" })
    };
    let mut out = Vec::new();
    let mut met = HashSet::new();
    for (i, (state, snapshot)) in deck.states.iter().zip(snapshots).enumerate() {
        for (id, props) in &snapshot.nodes {
            let Some(at) = props.get("at") else { continue };
            // `rect`, else `in`, else `col` and `row` (SPEC §3.4); a child of a stack, grid, or
            // frame takes its container's lines.
            if at.get("rect").is_some() {
                continue;
            }
            let parent = at.get("parent").and_then(Value::as_str);
            if parent.is_some_and(|p| deck.nodes.get(p).is_none_or(|p| p.node_type != NodeType::Group)) {
                continue;
            }
            let located = |f: Finding| f.state(state.id.clone()).node(id.clone());
            if let Some(slot) = at.get("in").and_then(Value::as_str) {
                // A slot of the state's layout, as each format has it. A missing layout or
                // slot is reported above.
                let Some(theme) = theme else { continue };
                let layout = snapshot.layout.as_deref().and_then(|l| theme.theme.layouts.get_key_value(l));
                let Some((name, layout)) = layout else { continue };
                for &(format, grid) in &grids {
                    let moved = format.and_then(|f| layout.formats.as_ref()?.get(&f)?.slots.get(slot));
                    let (def, at) = match (moved, layout.slots.get(slot)) {
                        (Some(def), _) => {
                            let f = format.expect("only a format moves a slot").name();
                            (def, format!("/layouts/{}/formats/{}/slots/{}", esc(name), esc(f), esc(slot)))
                        }
                        (None, Some(def)) => (def, format!("/layouts/{}/slots/{}", esc(name), esc(slot))),
                        (None, None) => continue,
                    };
                    for (axis, range) in [("col", def.col), ("row", def.row)] {
                        let Some(range) = range else { continue };
                        let (a, b) = match range {
                            Range::Index(n) => (u64::from(n), u64::from(n)),
                            Range::Span([a, b]) => (u64::from(a), u64::from(b)),
                        };
                        let n = tracks(grid, axis);
                        let path = format!("{at}/{axis}");
                        if b > n && met.insert((path.clone(), n)) {
                            let message = format!(
                                "slot `{slot}` of layout `{name}` is in {}, {}",
                                cells(axis, a, b),
                                past(format, axis, n)
                            );
                            out.push(located(theme.finding("E102", message, &path)));
                        }
                    }
                }
                continue;
            }
            for axis in ["col", "row"] {
                let Some((a, b)) = at.get(axis).and_then(bounds) else { continue };
                let sets =
                    |props: Option<&Props>| props.and_then(|p| p.get("at")).and_then(|at| at.get(axis)).is_some();
                let path = if sets(state.props.get(id)) {
                    format!("/states/{i}/props/{}/at/{axis}", esc(id))
                } else if sets(Some(&deck.nodes[id].props)) {
                    format!("/nodes/{}/at/{axis}", esc(id))
                } else {
                    format!("/states/{i}/props/{}", esc(id))
                };
                if b < a {
                    if met.insert((path.clone(), 0)) {
                        let message =
                            format!("`{id}` is placed in {}, which run backward: write [{b}, {a}]", cells(axis, a, b));
                        out.push(located(Finding::new("E106", Severity::Error, message).at(path)));
                    }
                    continue;
                }
                for &(format, grid) in &grids {
                    // A node placed anew in a format (ADR-0020) is checked there below.
                    if format.is_some_and(|f| placed_in(&deck.nodes[id].props, f).is_some()) {
                        continue;
                    }
                    let n = tracks(grid, axis);
                    if b > n && met.insert((path.clone(), n)) {
                        let message = format!("`{id}` is placed in {}, {}", cells(axis, a, b), past(format, axis, n));
                        out.push(located(Finding::new("E102", Severity::Error, message).at(path.clone()).hint(
                            "Place it within the grid, or in a slot of the state's layout: a slot moves with the \
                             theme and the format, and cells do not.",
                        )));
                    }
                }
            }
        }
    }
    // Each node's cells in a format it is placed anew in, on that format's grid.
    for (id, node) in &deck.nodes {
        for &(format, grid) in &grids {
            let Some(f) = format else { continue };
            let Some(at) = placed_in(&node.props, f) else { continue };
            for axis in ["col", "row"] {
                let Some((a, b)) = at.get(axis).and_then(bounds) else { continue };
                let n = tracks(grid, axis);
                if b > n {
                    let path = format!("/nodes/{}/formats/{}/at/{axis}", esc(id), esc(f.name()));
                    let message = format!("`{id}` is placed in {}, {}", cells(axis, a, b), past(format, axis, n));
                    out.push(Finding::new("E102", Severity::Error, message).at(path).node(id.clone()));
                }
            }
        }
    }
    out
}

/// Where `props` place their node anew in `format` (ADR-0020), if they do.
fn placed_in(props: &Props, format: Format) -> Option<&Value> {
    props.get("formats")?.get(format.name())?.get("at")
}

/// A grid's columns or rows (`axis`: `col` or `row`); rows default to 6.
fn tracks(grid: &Grid, axis: &str) -> u64 {
    u64::from(if axis == "col" { grid.columns } else { grid.rows.unwrap_or(6) })
}

/// A grid range's first and last track, as a deck writes it: `n` or `[a, b]`.
fn bounds(range: &Value) -> Option<(u64, u64)> {
    match range {
        Value::Number(n) => n.as_u64().map(|n| (n, n)),
        Value::Array(v) if v.len() == 2 => Some((v[0].as_u64()?, v[1].as_u64()?)),
        _ => None,
    }
}

/// Grid tracks as they read: `row 8`, `columns 1–8`.
fn cells(axis: &str, a: u64, b: u64) -> String {
    let what = if axis == "col" { "column" } else { "row" };
    if a == b { format!("{what} {a}") } else { format!("{what}s {a}–{b}") }
}

/// E102: theme names the deck uses that its theme does not define: text roles, layouts and
/// their slots, motion presets, durations, easings, springs, shader palettes, data palettes,
/// and colors.
fn theme_names(deck: &Deck, snapshots: Option<&[Snapshot]>, theme: &LoadedTheme) -> Vec<Finding> {
    let mut names = Names { theme, out: Vec::new() };
    for (id, node) in &deck.nodes {
        names.props(&node.props, node.node_type, &format!("/nodes/{}", esc(id)), None, id);
    }
    for (i, state) in deck.states.iter().enumerate() {
        let at = format!("/states/{i}");
        if let Some(layout) = &state.layout {
            let defined = theme.theme.layouts.contains_key(layout);
            names.need(defined, "layout", layout, format!("{at}/layout"), Some(&state.id), None);
        }
        if let Some(transition) = &state.transition {
            names.timing(transition, &format!("{at}/transition"), Some(&state.id), None);
        }
        for (j, item) in state.choreography.iter().enumerate() {
            names.choreography(item, &format!("{at}/choreography/{j}"), &state.id);
        }
        for (id, delta) in &state.props {
            if let Some(node) = deck.nodes.get(id) {
                names.props(delta, node.node_type, &format!("{at}/props/{}", esc(id)), Some(&state.id), id);
            }
        }
    }
    for (id, over) in &deck.overrides {
        if let Some(node) = deck.nodes.get(id) {
            names.props(over, node.node_type, &format!("/overrides/{}", esc(id)), None, id);
        }
    }
    // A slot belongs to the layout of the state that shows the node. Each node, slot, and
    // layout is reported once, where it first meets: at the delta that put the node in
    // the slot, else at its default, else at the layout of a state it tracked into.
    let mut met = HashSet::new();
    for (i, (state, snapshot)) in deck.states.iter().zip(snapshots.unwrap_or_default()).enumerate() {
        for (id, props) in &snapshot.nodes {
            let slot_of = |props: Option<&Props>| props?.get("at")?.get("in")?.as_str().map(String::from);
            let Some(slot) = slot_of(Some(props)) else { continue };
            if slot == "canvas" || slot == "grid" {
                continue;
            }
            let message = match snapshot.layout.as_deref().map(|l| (l, theme.theme.layouts.get(l))) {
                None => format!("slot `{slot}` needs a layout, and state `{}` has none", state.id),
                Some((_, None)) => continue, // the layout itself is reported
                Some((layout, Some(def))) if !def.slots.contains_key(&slot) => {
                    let slots: Vec<&str> = def.slots.keys().map(String::as_str).chain(["canvas", "grid"]).collect();
                    format!("slot `{slot}` is not in layout `{layout}`, which has {}", slots.join(", "))
                }
                Some(_) => continue,
            };
            if !met.insert((id.clone(), slot.clone(), snapshot.layout.clone())) {
                continue;
            }
            let path = if slot_of(state.props.get(id)).is_some() {
                format!("/states/{i}/props/{}/at/in", esc(id))
            } else if slot_of(Some(&deck.nodes[id].props)).as_deref() == Some(slot.as_str()) {
                format!("/nodes/{}/at/in", esc(id))
            } else {
                format!("/states/{i}/layout")
            };
            names
                .out
                .push(Finding::new("E102", Severity::Error, message).at(path).state(state.id.clone()).node(id.clone()));
        }
    }
    names.out
}

/// E106: a shader's preset is a preset of its kind (SPEC §3.8), in every state that
/// shows it. Each finding points at the `preset` the state's delta or the node set.
fn shader_presets(deck: &Deck, snapshots: &[Snapshot], theme: &LoadedTheme) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (i, (state, snapshot)) in deck.states.iter().zip(snapshots).enumerate() {
        for (id, props) in &snapshot.nodes {
            if deck.nodes.get(id).is_none_or(|n| n.node_type != NodeType::Shader) {
                continue;
            }
            let (Some(name), Some(kind)) = (props.get("preset").and_then(Value::as_str), props.get("kind")) else {
                continue;
            };
            let Some(preset) = theme.shader_preset(name) else { continue };
            let theirs = serde_json::to_value(preset.kind).unwrap_or_default();
            if theirs == *kind {
                continue;
            }
            let path = match state.props.get(id).and_then(|d| d.get("preset")) {
                Some(_) => format!("/states/{i}/props/{}/preset", esc(id)),
                None => format!("/nodes/{}/preset", esc(id)),
            };
            let message = format!("preset `{name}` is a {} shader, and this one is a {}", plain(&theirs), plain(kind));
            if seen.insert(path.clone()) {
                out.push(
                    Finding::new("E106", Severity::Error, message).at(path).state(state.id.clone()).node(id.clone()),
                );
            }
        }
    }
    out
}

/// E106: a choreography item that names no motion, or more than one, or splits a target
/// into what its type does not have: lines, words, and glyphs are text's, children a
/// container's, marks a chart's (SPEC §3.9). An item's split is its own, else its preset
/// call's, else its theme preset's.
fn cues(deck: &Deck, theme: Option<&LoadedTheme>) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, state) in deck.states.iter().enumerate() {
        for (j, item) in state.choreography.iter().enumerate() {
            cue(deck, theme, item, &format!("/states/{i}/choreography/{j}"), &state.id, &mut out);
        }
    }
    out
}

fn cue(deck: &Deck, theme: Option<&LoadedTheme>, item: &Value, at: &str, state: &str, out: &mut Vec<Finding>) {
    for key in ["sequence", "parallel"] {
        for (k, inner) in item.get(key).and_then(Value::as_array).into_iter().flatten().enumerate() {
            cue(deck, theme, inner, &format!("{at}/{key}/{k}"), state, out);
        }
    }
    if item.get("target").is_none() {
        return;
    }
    let motions: Vec<&str> =
        ["enter", "exit", "emphasis", "anim"].into_iter().filter(|k| item.get(*k).is_some()).collect();
    let [motion] = motions[..] else {
        let message = match motions.len() {
            0 => format!("choreography in `{state}` names no motion: give it `enter`, `exit`, `emphasis`, or `anim`"),
            _ => format!("choreography in `{state}` names {}: give each its own item", motions.join(" and ")),
        };
        out.push(Finding::new("E106", Severity::Error, message).at(at.to_string()).state(state.to_string()));
        return;
    };
    let call = item.get(motion);
    let named = call.and_then(|c| c.as_str().or_else(|| c.get("preset").and_then(Value::as_str)));
    let theirs = || {
        let preset = theme?.theme.motion.presets.get(named?)?;
        serde_json::to_value(preset.split?).ok()?.as_str().map(String::from)
    };
    let (split, path) = match (item.get("split").and_then(Value::as_str), call.and_then(|c| c.get("split"))) {
        (Some(split), _) => (split.to_string(), format!("{at}/split")),
        (None, Some(split)) => (split.as_str().unwrap_or_default().to_string(), format!("{at}/{motion}/split")),
        (None, None) => match theirs() {
            Some(split) => (split, format!("{at}/{motion}")),
            None => return,
        },
    };
    let (fits, takes): (fn(NodeType) -> bool, &str) = match split.as_str() {
        "lines" | "words" | "glyphs" => (|t| t == NodeType::Text, "only text splits into lines, words, and glyphs"),
        "children" => (
            |t| matches!(t, NodeType::Stack | NodeType::Grid | NodeType::Frame | NodeType::Group),
            "only a stack, grid, frame, or group splits into its children",
        ),
        "marks" => (|t| t == NodeType::Chart, "only a chart splits into its marks"),
        _ => return,
    };
    for target in choreo_targets(item) {
        let Some(node) = deck.nodes.get(&target) else { continue };
        if !fits(node.node_type) {
            let kind = type_name(node.node_type);
            let message =
                format!("choreography in `{state}` splits `{target}`, {} node, into {split}: {takes}", article(&kind));
            out.push(
                Finding::new("E106", Severity::Error, message).at(path.clone()).state(state.to_string()).node(target),
            );
        }
    }
}

/// A JSON string as it reads, without its quotes.
fn plain(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}

/// What [`theme_names`] collects as it walks the deck.
struct Names<'a> {
    theme: &'a LoadedTheme,
    out: Vec<Finding>,
}

impl Names<'_> {
    fn need(&mut self, defined: bool, what: &str, name: &str, path: String, state: Option<&str>, node: Option<&str>) {
        if defined {
            return;
        }
        let message = format!("{what} `{name}` is not in the theme{}", self.theme.has(what));
        let mut finding = Finding::new("E102", Severity::Error, message).at(path);
        if let Some(state) = state {
            finding = finding.state(state);
        }
        if let Some(node) = node {
            finding = finding.node(node);
        }
        self.out.push(finding);
    }

    /// A node's properties, whole or a delta, at `at`.
    fn props(&mut self, props: &Props, node_type: NodeType, at: &str, state: Option<&str>, node: &str) {
        let t = &self.theme.theme;
        let roles = |role: &str| t.typography.roles.contains_key(role);
        let node_ = Some(node);
        if node_type == NodeType::Text {
            if let Some(role) = props.get("role").and_then(Value::as_str) {
                self.need(roles(role), "text role", role, format!("{at}/role"), state, node_);
            }
            if let Some(style) = props.get("style") {
                self.style(style, &format!("{at}/style"), state, node_);
            }
            for (i, run) in props.get("runs").and_then(Value::as_array).into_iter().flatten().enumerate() {
                if let Some(role) = run.get("role").and_then(Value::as_str) {
                    self.need(roles(role), "text role", role, format!("{at}/runs/{i}/role"), state, node_);
                }
                if let Some(style) = run.get("style") {
                    self.style(style, &format!("{at}/runs/{i}/style"), state, node_);
                }
            }
        }
        if node_type == NodeType::Chart {
            if let Some(role) = props.get("labels").and_then(|l| l.get("role")).and_then(Value::as_str) {
                self.need(roles(role), "text role", role, format!("{at}/labels/role"), state, node_);
            }
            for (i, note) in props.get("annotations").and_then(Value::as_array).into_iter().flatten().enumerate() {
                if let Some(role) = note.get("role").and_then(Value::as_str) {
                    self.need(roles(role), "text role", role, format!("{at}/annotations/{i}/role"), state, node_);
                }
            }
            for channel in ["x", "y", "series", "color", "sizeEncoding"] {
                if let Some(scale) = props.get(channel).and_then(|c| c.get("scale")).and_then(Value::as_str) {
                    let defined = self.theme.data_palette(scale);
                    self.need(defined, "data palette", scale, format!("{at}/{channel}/scale"), state, node_);
                }
            }
        }
        if node_type == NodeType::Shader
            && let Some(palette) = props.get("palette").and_then(Value::as_str)
        {
            let defined = self.theme.palette(palette);
            self.need(defined, "shader palette", palette, format!("{at}/palette"), state, node_);
        }
        if node_type == NodeType::Shader
            && let Some(preset) = props.get("preset").and_then(Value::as_str)
        {
            let defined = self.theme.shader_preset(preset).is_some();
            self.need(defined, "shader preset", preset, format!("{at}/preset"), state, node_);
        }
        for key in ["enter", "exit", "emphasis"] {
            if let Some(preset) = props.get(key) {
                self.preset(preset, &format!("{at}/{key}"), state, node_);
            }
        }
        if let Some(fill) = props.get("fill") {
            self.paint(fill, &format!("{at}/fill"), state, node_);
        }
        if let Some(paint) = props.get("stroke").and_then(|s| s.get("paint")) {
            self.paint(paint, &format!("{at}/stroke/paint"), state, node_);
        }
    }

    /// A text style's family and color.
    fn style(&mut self, style: &Value, at: &str, state: Option<&str>, node: Option<&str>) {
        if let Some(family) = style.get("family").and_then(Value::as_str) {
            let defined = self.theme.theme.typography.families.contains_key(family);
            self.need(defined, "font family", family, format!("{at}/family"), state, node);
        }
        if let Some(color) = style.get("color").and_then(Value::as_str) {
            let defined = self.theme.color(color);
            self.need(defined, "color", color, format!("{at}/color"), state, node);
        }
    }

    /// A motion preset by name, or called with parameters.
    fn preset(&mut self, preset: &Value, at: &str, state: Option<&str>, node: Option<&str>) {
        let presets = &self.theme.theme.motion.presets;
        match preset {
            Value::String(name) => self.need(presets.contains_key(name), "motion preset", name, at.into(), state, node),
            Value::Object(call) => {
                if let Some(name) = call.get("preset").and_then(Value::as_str) {
                    self.need(presets.contains_key(name), "motion preset", name, format!("{at}/preset"), state, node);
                }
                self.timing(preset, at, state, node);
            }
            _ => {}
        }
    }

    /// Named durations, easings, and springs: a bare duration, or an object with any of
    /// `duration`, `ease`, and `spring`.
    fn timing(&mut self, value: &Value, at: &str, state: Option<&str>, node: Option<&str>) {
        let motion = &self.theme.theme.motion;
        if let Value::String(name) = value {
            return self.need(motion.durations.contains_key(name), "duration", name, at.into(), state, node);
        }
        if let Some(name) = value.get("duration").and_then(Value::as_str) {
            self.need(motion.durations.contains_key(name), "duration", name, format!("{at}/duration"), state, node);
        }
        if let Some(name) = value.get("ease").and_then(Value::as_str) {
            self.need(motion.easings.contains_key(name), "easing", name, format!("{at}/ease"), state, node);
        }
        if let Some(name) = value.get("spring").and_then(Value::as_str) {
            self.need(motion.springs.contains_key(name), "spring", name, format!("{at}/spring"), state, node);
        }
    }

    /// A choreography item, and the groups inside it.
    fn choreography(&mut self, item: &Value, at: &str, state: &str) {
        for key in ["enter", "exit", "emphasis"] {
            if let Some(preset) = item.get(key) {
                self.preset(preset, &format!("{at}/{key}"), Some(state), None);
            }
        }
        self.timing(
            &json!({ "duration": item.get("duration"), "ease": item.get("ease"), "spring": item.get("spring") }),
            at,
            Some(state),
            None,
        );
        for key in ["sequence", "parallel"] {
            for (i, inner) in item.get(key).and_then(Value::as_array).into_iter().flatten().enumerate() {
                self.choreography(inner, &format!("{at}/{key}/{i}"), state);
            }
        }
    }

    /// A paint's colors: a color, `{ "solid" }`, or a gradient's stops.
    fn paint(&mut self, paint: &Value, at: &str, state: Option<&str>, node: Option<&str>) {
        let mut colors: Vec<(String, String)> = Vec::new();
        match paint {
            Value::String(color) => colors.push((color.clone(), at.into())),
            Value::Object(p) => {
                if let Some(color) = p.get("solid").and_then(Value::as_str) {
                    colors.push((color.into(), format!("{at}/solid")));
                }
                let stops = p.get("gradient").and_then(|g| g.get("stops")).and_then(Value::as_array);
                for (i, stop) in stops.into_iter().flatten().enumerate() {
                    if let Some(color) = stop.get("color").and_then(Value::as_str) {
                        colors.push((color.into(), format!("{at}/gradient/stops/{i}/color")));
                    }
                }
            }
            _ => {}
        }
        for (color, path) in colors {
            let defined = self.theme.color(&color);
            self.need(defined, "color", &color, path, state, node);
        }
    }
}

fn type_name(node_type: NodeType) -> String {
    serde_json::to_value(node_type).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

fn article(word: &str) -> String {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) { format!("an {word}") } else { format!("a {word}") }
}

/// A key as one JSON-pointer token.
fn esc(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

// --- repeated keys ------------------------------------------------------------

/// Keys written twice in one JSON object, as JSON pointers to each repeat, in document
/// order. A parser keeps the last value of a repeated key, so a node id written twice would
/// replace the first node without a word.
pub fn repeated_keys(json: &str) -> Result<Vec<String>, serde_json::Error> {
    let mut out = Vec::new();
    let mut deserializer = serde_json::Deserializer::from_str(json);
    Walk { path: String::new(), out: &mut out }.deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(out)
}
/// One JSON value, walked for repeated keys.
struct Walk<'a> {
    path: String,
    out: &'a mut Vec<String>,
}

impl<'de> DeserializeSeed<'de> for Walk<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Walk<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let Walk { path, out } = self;
        let mut i = 0;
        while seq.next_element_seed(Walk { path: format!("{path}/{i}"), out: &mut *out })?.is_some() {
            i += 1;
        }
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let Walk { path, out } = self;
        let mut seen = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            let at = child(&path, &key);
            if !seen.insert(key) {
                out.push(at.clone());
            }
            map.next_value_seed(Walk { path: at, out: &mut *out })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn example() -> Deck {
        Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap()
    }

    #[test]
    fn example_validates() {
        assert!(validate(&example()).is_empty());
    }

    #[test]
    fn unknown_references_are_e102() {
        let mut deck = example();
        deck.states[1].props.insert("ghost".into(), Default::default());
        deck.nodes.get_mut("rev").unwrap().props.insert("data".into(), json!("@nope"));
        let f = validate(&deck);
        assert_eq!(f.iter().filter(|x| x.code == "E102").count(), 2, "{f:#?}");
    }

    #[test]
    fn duplicate_state_ids_are_e105() {
        let mut deck = example();
        let dup = deck.states[0].clone();
        deck.states.push(dup);
        assert!(validate(&deck).iter().any(|f| f.code == "E105"));
    }
}
