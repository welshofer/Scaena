//! A slide's background (PLAN 2.148, 3.32): one of the theme's shader presets filling the canvas
//! behind everything on the slide, or none.
//!
//! - [`offered`] lists what a slide may take: None, then each preset the theme has
//!   (`shaders.presets`), in its order, each by its name in words and its kind, with the patch that
//!   gives the slide it. A slide that shows a chart or a table says why backgrounds stay off it
//!   (`held`, lint W311): data reads against a plain surface.
//! - [`setting`] is that patch: the slide's background, each shader that fills its canvas as its
//!   first state shows it, taken away (removed where no other slide shows it, else hidden from
//!   this slide, the slides after it that showed it showing it again), and the preset's added,
//!   entering in the slide's first state and kept to its slide (PLAN 3.24). Each text on the
//!   slide too faint to read over it is then set in the theme's color that reads there, as a
//!   re-theme sets it (PLAN 2.82), in the same patch, and the background keeps a record of each
//!   (`recolored`, deck format 0.38). The background taken away, by None or another, takes each
//!   color it set away where the text still has it, so the text reads in its theme's own again,
//!   and what then reads too faintly over what is left is set again as above.
//!   Each option's own patch is the slide with it alone, which a preview draws at once; the
//!   editors ask for [`setting`]'s when one is chosen.
//! - [`backgrounding`] checks it as a patch is checked, writing nothing, and [`background`]
//!   writes it.

use crate::lint::{View, Write};
use crate::patch::Patched;
use crate::{Bundle, Context as _, OpsError};
use scaena_core::document::{Deck, NodeType, Props};
use scaena_core::model::theme::{Theme, Vocabulary};
use scaena_core::patch::SemanticOp;
use scaena_core::words;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};

/// What a slide may take as its background.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Backgrounds {
    /// The slide's first state, where a background enters.
    pub first: String,
    /// Why backgrounds stay off this slide, in words: it shows a chart or a table (W311). Each
    /// is offered all the same, and None takes one away.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<String>,
    /// None first, then each preset in the theme's order.
    pub options: Vec<Background>,
}

/// One background a slide may take.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Background {
    /// The theme's preset; none for None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Its kind of shader (SPEC §3.8): `mesh`, `noise`, …; none for None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Its name in words: `Backdrop`, or `None`.
    pub label: String,
    /// Its kind in words: `Mesh Gradient`; none for None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
    /// The slide's background now. A background no preset makes is none of these.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub current: bool,
    /// The patch that shows the slide with this background, as a preview draws it: none for the one
    /// it has. Giving the slide it is [`setting`]'s patch, which sets each text too faint to read
    /// over it in a color that reads there too, and lints the slide to know which.
    pub patch: Vec<Value>,
}

/// The state `state`'s slide begins with: where its background enters.
fn first_of(deck: &Deck, state: &str) -> Result<usize, OpsError> {
    let s = deck.state_index(state).ok_or_else(|| OpsError::unknown_state(deck, state))?;
    let slide = deck.slide_of(&deck.states[s]);
    Ok(deck.states.iter().position(|st| deck.slide_of(st) == slide).unwrap_or(s))
}

/// Whether `props`, as a state shows a node, fill the canvas: placed in it, held by nothing.
fn fills(props: &Props) -> bool {
    let at = props.get("at");
    at.and_then(|a| a.get("in")).and_then(Value::as_str) == Some("canvas") && at.and_then(|a| a.get("parent")).is_none()
}

/// The shaders that fill the canvas in state `first`, its background, lowest first.
fn current(deck: &Deck, snap: &scaena_core::tracking::Snapshot) -> Vec<String> {
    let z = |props: &Props| props.get("z").and_then(Value::as_f64).unwrap_or(0.0);
    let mut found: Vec<(&String, f64)> = (snap.nodes.iter())
        .filter(|(id, props)| deck.nodes.get(*id).is_some_and(|n| n.node_type == NodeType::Shader) && fills(props))
        .map(|(id, props)| (id, z(props)))
        .collect();
    scaena_core::sort::by(&mut found, |a, b| a.1.total_cmp(&b.1));
    found.into_iter().map(|(id, _)| id.clone()).collect()
}

/// What `state`'s slide may take as its background in `theme`, each with its patch.
pub fn offered(deck: &Deck, theme: &Theme, files: &View, state: &str) -> Result<Backgrounds, OpsError> {
    let s = first_of(deck, state)?;
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let slide = deck.slide_of(&deck.states[s]);
    let data = |props: &(&String, &Props)| {
        deck.nodes.get(props.0).is_some_and(|n| matches!(n.node_type, NodeType::Chart | NodeType::Table))
    };
    let held = (deck.states.iter().zip(&snaps))
        .any(|(st, snap)| deck.slide_of(st) == slide && snap.nodes.iter().any(|p| data(&p)))
        .then(|| words::BACKGROUND_HELD.to_string());
    let now = current(deck, &snaps[s]);
    let now_preset = now.first().and_then(|id| snaps[s].nodes[id].get("preset")).and_then(Value::as_str);
    let presets = theme.shaders.as_ref().and_then(|sh| sh.presets.as_ref());
    let mut options = vec![Background {
        preset: None,
        kind: None,
        label: "None".into(),
        said: None,
        current: now.is_empty(),
        patch: if now.is_empty() { Vec::new() } else { placing(deck, theme, files, state, None)? },
    }];
    for (name, preset) in presets.into_iter().flatten() {
        let kind = serde_json::to_value(preset.kind).ok().and_then(|k| k.as_str().map(String::from));
        let current = now_preset == Some(name.as_str());
        options.push(Background {
            preset: Some(name.clone()),
            label: words::named(Vocabulary::ShaderPreset, name, theme),
            said: kind.as_deref().map(words::shader),
            kind,
            current,
            patch: if current { Vec::new() } else { placing(deck, theme, files, state, Some(name))? },
        });
    }
    Ok(Backgrounds { first: deck.states[s].id.clone(), held, options })
}

/// A seed for a slide's background, from its id: the same slide draws the same field.
fn seed(slide: &str) -> u64 {
    slide.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193)) as u64 % 1000
}

/// The patch that gives `state`'s slide `preset` as its background, or none (`None`): each shader
/// that fills its canvas in its first state taken away, then the preset's entering there, kept to
/// the slide. One that shows on another slide too is hidden from this one, and comes back on each
/// slide after it that showed it. Each color a background taken away set (its `recolored`) is
/// taken away where its text still has it on the slide. Each text the slide's states show too faint
/// to read then is set in the theme's color that reads where it stands (E110, E111), as a re-theme
/// sets it (PLAN 2.82), which lints the slide, and the preset's background records it. Refused for
/// a preset the theme does not have.
pub fn setting(
    deck: &Deck,
    theme: &Theme,
    files: &View,
    state: &str,
    preset: Option<&str>,
) -> Result<Vec<Value>, OpsError> {
    let first = first_of(deck, state)?;
    let old = current(deck, &scaena_core::resolve_states(deck).context("tracking")?[first]);
    let mut patch = placing(deck, theme, files, state, preset)?;
    let slide = deck.slide_of(&deck.states[first]);
    let slid: Vec<&str> =
        (deck.states.iter()).filter(|st| deck.slide_of(st) == slide).map(|st| st.id.as_str()).collect();
    patch.extend(uncolored(deck, files, &patch, &old, &slid)?);
    let lint = |deck: &Deck| Ok(crate::lint::lint_states_in(deck, files, &slid)?.findings);
    let made = compiled(deck, files, &patch)?;
    let after = lint(&made)?;
    let done = crate::theme::recoloring(made, &[], &slid, after, lint)?;
    patch.extend(done.ops);
    // The background added keeps a record of each text it set in a color that reads over it.
    let added = (done.deck.nodes.iter())
        .find(|(id, n)| n.node_type == NodeType::Shader && !deck.nodes.contains_key(*id))
        .map(|(id, _)| id.clone());
    if let Some(id) = added {
        let snaps = scaena_core::resolve_states(&done.deck).context("tracking")?;
        let mut record = serde_json::Map::new();
        for f in &done.recolored {
            let (Some(node), Some(at)) = (f.node.as_deref(), f.state.as_deref()) else { continue };
            let snap = done.deck.state_index(at).map(|i| &snaps[i]);
            if let Some(name) = snap.and_then(|snap| color_of(&done.deck, snap, node)) {
                record.insert(node.to_string(), Value::String(name));
            }
        }
        if !record.is_empty() {
            let path = format!("/nodes/{}/recolored", id.replace('~', "~0").replace('/', "~1"));
            patch.push(json!({ "op": "add", "path": path, "value": record }));
        }
    }
    Ok(patch)
}

/// The theme color `node`'s text is set in as `snap` shows it, by its name: the deck's overrides',
/// which win over every state, else the state's. None for a color written out, or none at all.
fn color_of(deck: &Deck, snap: &scaena_core::tracking::Snapshot, node: &str) -> Option<String> {
    let color = |props: Option<&Props>| props?.get("style")?.get("color")?.as_str().map(String::from);
    color(deck.overrides.get(node)).or_else(|| color(snap.nodes.get(node)))
}

/// The ops that take away each color a background in `old` set (its `recolored`) where its text
/// still has it in a state of the slide (`slid`), once `patch` is made: a `choose` of no
/// `style/color` there, written where the color lives, so what is under it shows. A color a person
/// chose since stays, and a text no longer in the deck is passed over.
fn uncolored(
    deck: &Deck,
    files: &View,
    patch: &[Value],
    old: &[String],
    slid: &[&str],
) -> Result<Vec<Value>, OpsError> {
    let record: Vec<(&String, &str)> = (old.iter())
        .filter_map(|id| deck.nodes.get(id)?.props.get("recolored")?.as_object())
        .flatten()
        .filter_map(|(text, name)| Some((text, name.as_str()?)))
        .collect();
    let mut ops: Vec<Value> = Vec::new();
    for (text, name) in record {
        // One state at a time, the deck made again after each: states that share where the color
        // is written lose it together.
        for state in slid {
            let made = compiled(deck, files, &[patch, ops.as_slice()].concat())?;
            let Some(i) = made.state_index(state) else { continue };
            let snaps = scaena_core::resolve_states(&made).context("tracking")?;
            if made.nodes.contains_key(text.as_str()) && color_of(&made, &snaps[i], text).as_deref() == Some(name) {
                ops.push(json!({ "op": "choose", "node": text, "state": state, "prop": "style/color", "value": null }));
            }
        }
    }
    Ok(ops)
}

/// `deck` with `patch` applied.
fn compiled(deck: &Deck, files: &View, patch: &[Value]) -> Result<Deck, OpsError> {
    let after = scaena_core::patch::compile(&deck.to_value()?, patch, files).map_err(|e| OpsError {
        message: e.to_string(),
        plan: None,
        op: Some(e.index),
    })?;
    Deck::from_value(&after.doc).map_err(OpsError::new)
}

/// [`setting`]'s patch before it sets any text's color: the slide with the background, as a preview
/// shows it.
fn placing(
    deck: &Deck,
    theme: &Theme,
    files: &View,
    state: &str,
    preset: Option<&str>,
) -> Result<Vec<Value>, OpsError> {
    let presets = theme.shaders.as_ref().and_then(|sh| sh.presets.as_ref());
    let made = match preset {
        Some(name) => Some(
            presets
                .and_then(|p| p.get(name))
                .ok_or_else(|| {
                    let known = presets.into_iter().flat_map(|p| p.keys()).map(String::as_str);
                    let near = scaena_core::patch::nearest_in("the theme", "shader presets", name, known);
                    OpsError::new(format!("the theme has no shader preset `{name}`{near}"))
                })?,
        ),
        None => None,
    };
    let s = first_of(deck, state)?;
    let first = deck.states[s].id.clone();
    let slide = deck.slide_of(&deck.states[s]).to_string();
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let old = current(deck, &snaps[s]);
    let elsewhere = |id: &str| {
        (deck.states.iter().zip(&snaps)).any(|(st, snap)| deck.slide_of(st) != slide && snap.nodes.contains_key(id))
    };
    let mut ops: Vec<SemanticOp> = Vec::new();
    for id in &old {
        ops.push(match elsewhere(id) {
            true => SemanticOp::HideNode { node: id.clone(), state: first.clone() },
            false => SemanticOp::RemoveNode { id: id.clone() },
        });
    }
    if let (Some(name), Some(made)) = (preset, made) {
        let base = format!("{slide}-background");
        let id = (1..)
            .map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") })
            .find(|id| !deck.nodes.contains_key(id))
            .expect("some number is free");
        let node = json!({
            "type": "shader", "kind": made.kind, "preset": name, "seed": seed(&slide),
            "alt": "", "semantic": "decoration", "at": { "in": "canvas" }, "z": -100,
        });
        let node: scaena_core::document::Node = serde_json::from_value(node).context("a background")?;
        let leaves = crate::inspect::leaving(deck, &first, &[(id.clone(), node.clone())]);
        ops.push(SemanticOp::AddNode { id, node, state: Some(first.clone()), props: None, after: None });
        ops.extend(leaves);
    }
    let mut patch: Vec<Value> = ops.iter().map(serde_json::to_value).collect::<Result<_, _>>().context("a patch")?;
    // A background hidden from this slide shows again on each slide after it that showed it: on the
    // first state of each run that lost it, which those after it track.
    let hidden: Vec<&String> = old.iter().filter(|id| elsewhere(id)).collect();
    if !hidden.is_empty() {
        for k in s + 1..deck.states.len() {
            let now = scaena_core::resolve_states(&compiled(deck, files, &patch)?).context("tracking")?;
            if deck.slide_of(&deck.states[k]) == slide {
                continue;
            }
            for id in &hidden {
                if snaps[k].nodes.contains_key(*id) && !now[k].nodes.contains_key(*id) {
                    let back =
                        SemanticOp::ShowNode { node: (*id).clone(), state: deck.states[k].id.clone(), props: None };
                    patch.push(serde_json::to_value(back).context("a patch")?);
                }
            }
        }
    }
    Ok(patch)
}

/// What the history says the patch did: `Background set to Backdrop`, or taken away.
fn said(theme: &Theme, preset: Option<&str>) -> String {
    match preset {
        Some(name) => format!("Background set to {}", words::named(Vocabulary::ShaderPreset, name, theme)),
        None => "Background taken away".into(),
    }
}

/// [`background`] with nothing written: the patch checked as a patch is, unless it is refused.
pub fn backgrounding(b: &Bundle, state: &str, preset: Option<&str>) -> Result<(Patched, Option<Write>), OpsError> {
    let theme = crate::theme(b)?;
    let patch = setting(&b.deck, &theme, &View::of(b), state, preset)?;
    crate::patch::patching(b, &Value::Array(patch), Some(&said(&theme, preset)))
}

/// `state`'s slide given `preset` as its background, or none, written into the bundle.
pub fn background(b: &Bundle, state: &str, preset: Option<&str>) -> Result<Patched, OpsError> {
    let (patched, write) = backgrounding(b, state, preset)?;
    if let Some(write) = write {
        crate::lint::write(b, write)?;
    }
    Ok(patched)
}
