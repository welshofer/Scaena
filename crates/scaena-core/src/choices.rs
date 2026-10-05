//! What an inspector offers for a node in a state (ADR-0013, PLAN 2.33): each property it
//! edits, with the theme's names for it or what the deck's schema allows; the value the state
//! shows; and where that value lives, which is what a choice changes. `choose` (SPEC §7.3)
//! writes a choice there.
//!
//! - **What it takes.** A role, a family, a color, a preset, a palette, or a length is one
//!   of the theme's names, in its order (`Theme::names`). A word, a number, or a flag is
//!   what the schema generated from the model allows.
//! - **A value written out** where the theme has names is lint W300's: a color, a text size,
//!   a length in canvas units. It is legal only in the deck's `overrides`, where `choose`
//!   writes it, and where it counts as an override.
//!
//! A state has its own (PLAN 2.36, [`state_choices`]): its layout, its transition, its hold,
//! and its notes, which `set_state` writes.

use crate::document::{Deck, NodeType, Props};
use crate::lint::literal;
use crate::model::check::Checker;
use crate::model::theme::{Theme, Vocabulary};
use crate::tracking::{Lives, layout_lives, layout_takers, lives, merge_props, resolve_states};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Map, Value};

/// What an inspector offers for one node as one state shows it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Choices {
    pub node: String,
    #[serde(rename = "type")]
    pub node_type: NodeType,
    pub state: String,
    /// Each property the inspector edits: the node type's own, then those every node has.
    pub fields: Vec<Field>,
}

/// What an inspector offers for a state itself (PLAN 2.36).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct StateChoices {
    pub state: String,
    /// Its layout, each key of its transition, its hold, and its notes: what `set_state`
    /// names. A state that sets no key of its transition cuts in.
    pub fields: Vec<Field>,
}

/// One property an inspector edits.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Field {
    /// A property, or one key of an object property (`style/color`): what `choose` names.
    pub prop: String,
    /// What it takes.
    pub takes: Takes,
    /// The value the state shows, as the deck sets it: tracked (SPEC §2.2), then the deck's
    /// `overrides`. Absent where nothing sets it, and the theme's shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// Where that value lives: where a choice is written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lives: Option<Where>,
    /// The value is written out where the theme has names: an override in the deck's
    /// `overrides`, lint W300's anywhere else.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub literal: bool,
}

/// Where a value a state shows lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Where {
    /// The deck's `overrides`, in every state: an override.
    Overrides,
    /// A state's delta, by the state's id: the state shown, or one it tracks from. For a
    /// state's own property, the state that sets it.
    State(String),
    /// The node's own properties.
    Node,
}

/// What a property takes.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Takes {
    /// One of the theme's names of a kind (`of`), in its order. With `overrides`, a value
    /// written out too (a color, a length in canvas units), which goes in the deck's
    /// `overrides`.
    Name {
        of: Vocabulary,
        names: Vec<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        overrides: bool,
    },
    /// One of these words.
    Word { words: Vec<String> },
    /// A number: at least `min`, or more than `above`; at most `max`; whole with `whole`.
    /// With `overrides`, every value is written out (a text size), and goes in the deck's
    /// `overrides`.
    Number {
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        above: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        whole: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        overrides: bool,
    },
    /// Yes or no.
    Flag,
    /// Words for people, as they are written: a state's notes.
    Text,
}

/// Where a property's values come from.
#[derive(Debug, Clone, Copy)]
enum Source {
    /// The theme's names of a kind; with `true`, a value written out too, an override.
    Names(Vocabulary, bool),
    /// What the deck's schema allows; with `true`, every value is written out, an override.
    Schema(bool),
}

use Source::{Names, Schema};
use Vocabulary as V;

/// What an inspector edits on every node.
const EVERY: [(&str, Source); 3] =
    [("opacity", Schema(false)), ("enter", Names(V::MotionPreset, false)), ("exit", Names(V::MotionPreset, false))];

/// What an inspector edits on a node of a type, besides what every node has.
fn own(node_type: NodeType) -> &'static [(&'static str, Source)] {
    match node_type {
        NodeType::Text => &[
            ("role", Names(V::TextRole, false)),
            ("style/family", Names(V::FontFamily, false)),
            ("style/weight", Schema(false)),
            ("style/italic", Schema(false)),
            ("style/size", Schema(true)),
            ("style/color", Names(V::Color, true)),
            ("style/case", Schema(false)),
            ("fit", Schema(false)),
            ("wrap", Schema(false)),
            ("maxLines", Schema(false)),
        ],
        NodeType::Shape => &[
            ("fill", Names(V::Color, true)),
            ("stroke/paint", Names(V::Color, true)),
            ("stroke/width", Names(V::Stroke, true)),
            ("radius", Names(V::Radius, true)),
        ],
        NodeType::Image => &[("fit", Schema(false)), ("radius", Names(V::Radius, true))],
        NodeType::Shader => &[("preset", Names(V::ShaderPreset, false)), ("palette", Names(V::ShaderPalette, false))],
        NodeType::Chart => {
            &[("kind", Schema(false)), ("labels/show", Schema(false)), ("labels/role", Names(V::TextRole, false))]
        }
        NodeType::Stack => &[("axis", Schema(false)), ("gap", Names(V::Space, true))],
        NodeType::Grid => &[("gap", Names(V::Space, true))],
        NodeType::Table | NodeType::Frame | NodeType::Group => &[],
    }
}

/// What an inspector offers for `node` as `state` shows it, in `theme`.
pub fn choices(deck: &Deck, theme: &Theme, state: &str, node: &str) -> Result<Choices, String> {
    let i = deck.state_index(state).ok_or_else(|| format!("no state `{state}`"))?;
    let own_props = deck.nodes.get(node).ok_or_else(|| format!("no node `{node}`"))?;
    let snapshots = resolve_states(deck).map_err(|e| e.to_string())?;
    let mut shown = (snapshots[i].nodes.get(node).cloned())
        .ok_or_else(|| format!("`{node}` is not on screen in `{state}`: an inspector edits what a state shows"))?;
    let over = deck.overrides.get(node);
    if let Some(over) = over {
        merge_props(&mut shown, over);
    }
    let node_type = own_props.node_type;
    let tag = serde_json::to_value(node_type).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    let fields = own(node_type).iter().chain(&EVERY).filter_map(|&(prop, source)| {
        let takes = match source {
            Names(of, overrides) => {
                let mut names = theme.names(of);
                // A shader takes the presets of its kind (E106).
                if of == V::ShaderPreset {
                    let presets = theme.shaders.as_ref().and_then(|s| s.presets.as_ref());
                    let kind = shown.get("kind");
                    names.retain(|n| {
                        presets
                            .and_then(|p| p.get(n))
                            .is_some_and(|p| serde_json::to_value(p.kind).ok().as_ref() == kind)
                    });
                }
                Takes::Name { of, names, overrides }
            }
            Schema(overrides) => allowed(Checker::deck().property(&tag, prop)?, overrides)?,
        };
        let (name, key) = prop.split_once('/').map_or((prop, None), |(n, k)| (n, Some(k)));
        let value = match key {
            Some(key) => shown.get(name).and_then(|v| v.get(key)),
            None => shown.get(name),
        };
        let lives = value.map(|_| lives_at(deck, i, node, over, name, key));
        let literal = value.is_some_and(|v| literal(prop, v));
        Some(Field { prop: prop.into(), takes, value: value.cloned(), lives, literal })
    });
    Ok(Choices { node: node.into(), node_type, state: state.into(), fields: fields.collect() })
}

/// What an inspector edits on characters selected in a text (PLAN 2.38): a run's own, which
/// `style_text` sets. A run takes the theme's names only, so a color has no override.
const CHARACTERS: [(&str, Source); 6] = [
    ("role", Names(V::TextRole, false)),
    ("emphasis", Schema(false)),
    ("style/family", Names(V::FontFamily, false)),
    ("style/weight", Schema(false)),
    ("style/italic", Schema(false)),
    ("style/color", Names(V::Color, false)),
];

/// What an inspector offers for the characters `from` to `to` (Unicode scalar values, as
/// `style_text` counts them) of text `node` as `state` shows it, in `theme` (PLAN 2.38):
/// each look a run of its own takes, which `style_text` sets. A value is the first selected
/// character's run's own, absent where its run sets none and the node's look shows; where
/// it is set, it lives where the text does, which is where `style_text` writes.
pub fn characters(
    deck: &Deck,
    theme: &Theme,
    state: &str,
    node: &str,
    (from, to): (usize, usize),
) -> Result<Choices, String> {
    let i = deck.state_index(state).ok_or_else(|| format!("no state `{state}`"))?;
    let own_props = deck.nodes.get(node).ok_or_else(|| format!("no node `{node}`"))?;
    if own_props.node_type != NodeType::Text {
        return Err(format!("`{node}` is no text: only a text's characters take a look of their own"));
    }
    let snapshots = resolve_states(deck).map_err(|e| e.to_string())?;
    let mut shown = (snapshots[i].nodes.get(node).cloned())
        .ok_or_else(|| format!("`{node}` is not on screen in `{state}`: an inspector edits what a state shows"))?;
    let over = deck.overrides.get(node);
    if let Some(over) = over {
        merge_props(&mut shown, over);
    }
    let runs = shown.get("runs").and_then(Value::as_array).filter(|runs| !runs.is_empty());
    let count = match runs {
        Some(runs) => runs.iter().filter_map(|r| r.get("text")?.as_str()).map(|t| t.chars().count()).sum(),
        None => shown.get("text").and_then(Value::as_str).map_or(0, |t| t.chars().count()),
    };
    if from >= to || to > count {
        return Err(format!(
            "`{node}` reads {count} characters there: select one or more, `from` before `to`, up to {count}"
        ));
    }
    // The run the first selected character is in.
    let mut start = 0;
    let run = runs.into_iter().flatten().find(|r| {
        start += r.get("text").and_then(Value::as_str).map_or(0, |t| t.chars().count());
        start > from
    });
    let prop = if runs.is_some() { "runs" } else { "text" };
    let lives = lives_at(deck, i, node, over, prop, None);
    let fields = CHARACTERS.iter().filter_map(|&(prop, source)| {
        let takes = match source {
            Names(of, overrides) => Takes::Name { of, names: theme.names(of), overrides },
            Schema(overrides) => {
                let (def, key) = prop.split_once('/').map_or(("Run", prop), |(_, key)| ("TextStyle", key));
                allowed(Checker::deck().def_property(def, key)?, overrides)?
            }
        };
        let value = run.and_then(|r| match prop.split_once('/') {
            Some((name, key)) => r.get(name)?.get(key),
            None => r.get(prop),
        });
        let lives = value.map(|_| lives.clone());
        Some(Field { prop: prop.into(), takes, value: value.cloned(), lives, literal: false })
    });
    Ok(Choices { node: node.into(), node_type: NodeType::Text, state: state.into(), fields: fields.collect() })
}

/// What an inspector edits on a state: what `set_state` names. A transition's keys are those
/// of `TransitionSpec`; the rest are `State`'s.
const STATE: [(&str, Source); 7] = [
    ("layout", Names(V::Layout, false)),
    ("transition/duration", Names(V::Duration, false)),
    ("transition/ease", Names(V::Easing, false)),
    ("transition/spring", Names(V::Spring, false)),
    ("transition/match", Schema(false)),
    ("hold", Schema(false)),
    ("notes", Schema(false)),
];

/// What an inspector offers for `state` itself, in `theme` (PLAN 2.36). Its layout tracks
/// (SPEC §2.2): it shows the layout the state takes, and lives where it is set, the state
/// shown or one it tracks from. The layouts offered are those with a slot for each node
/// placed in one, in each state that takes its layout from there (E102's). Its transition,
/// hold, and notes are its own; a transition that is a bare duration shows as its
/// `duration`.
pub fn state_choices(deck: &Deck, theme: &Theme, state: &str) -> Result<StateChoices, String> {
    let i = deck.state_index(state).ok_or_else(|| format!("no state `{state}`"))?;
    let snapshots = resolve_states(deck).map_err(|e| e.to_string())?;
    let own = &deck.states[i];
    let transition: Map<String, Value> = match &own.transition {
        Some(Value::Object(spec)) => spec.clone(),
        Some(duration) => Map::from_iter([("duration".to_string(), duration.clone())]),
        None => Map::new(),
    };
    // The slots the nodes are placed in, in the states a layout chosen here would reach.
    let takers = layout_takers(deck, layout_lives(deck, i).unwrap_or(i));
    let placed: Vec<&str> = (takers.iter().flat_map(|&j| snapshots[j].nodes.values()))
        .filter_map(|props| props.get("at")?.get("in")?.as_str())
        .filter(|slot| !matches!(*slot, "canvas" | "grid"))
        .collect();
    let fits =
        |layout: &String| theme.layouts.get(layout).is_some_and(|l| placed.iter().all(|s| l.slots.contains_key(*s)));
    let fields = STATE.iter().filter_map(|&(prop, source)| {
        let key = prop.strip_prefix("transition/");
        let takes = match source {
            Names(V::Layout, overrides) => Takes::Name {
                of: V::Layout,
                names: theme.names(V::Layout).into_iter().filter(fits).collect(),
                overrides,
            },
            Names(of, overrides) => Takes::Name { of, names: theme.names(of), overrides },
            Schema(overrides) => {
                let (def, name) = key.map_or(("State", prop), |key| ("TransitionSpec", key));
                allowed(Checker::deck().def_property(def, name)?, overrides)?
            }
        };
        let (value, lives) = match (prop, key) {
            ("layout", _) => {
                let set = layout_lives(deck, i).map(|j| Where::State(deck.states[j].id.clone()));
                (snapshots[i].layout.clone().map(Value::String), set)
            }
            (_, key) => {
                let value = match (prop, key) {
                    (_, Some(key)) => transition.get(key).cloned(),
                    ("hold", _) => own.hold.map(Value::from),
                    _ => own.notes.clone().map(Value::String),
                };
                let set = value.as_ref().map(|_| Where::State(state.to_string()));
                (value, set)
            }
        };
        Some(Field { prop: prop.into(), takes, value, lives, literal: false })
    });
    Ok(StateChoices { state: state.into(), fields: fields.collect() })
}

/// What the schema allows, as an inspector offers it: a word, a number, yes or no, or words
/// for people. `None` for anything else.
fn allowed(schema: &Value, overrides: bool) -> Option<Takes> {
    let strings = |values: &[Value]| -> Option<Vec<String>> {
        values.iter().map(|v| v.as_str().or_else(|| v.get("const")?.as_str()).map(String::from)).collect()
    };
    if let Some(words) = schema.get("enum").and_then(Value::as_array) {
        return strings(words).map(|words| Takes::Word { words });
    }
    if let Some(options) = schema.get("oneOf").or_else(|| schema.get("anyOf")).and_then(Value::as_array) {
        return strings(options).map(|words| Takes::Word { words });
    }
    let number = |key: &str| schema.get(key).and_then(Value::as_f64);
    match schema.get("type").and_then(Value::as_str)? {
        "boolean" => Some(Takes::Flag),
        "string" => Some(Takes::Text),
        kind @ ("integer" | "number") => Some(Takes::Number {
            min: number("minimum"),
            above: number("exclusiveMinimum"),
            max: number("maximum"),
            whole: kind == "integer",
            overrides,
        }),
        _ => None,
    }
}

/// Where the value of `name` (or its key `key`) that state `i` shows of `node` lives.
fn lives_at(deck: &Deck, i: usize, node: &str, over: Option<&Props>, name: &str, key: Option<&str>) -> Where {
    let overridden =
        over.and_then(|o| o.get(name)).is_some_and(|v| key.is_none_or(|k| v.is_null() || v.get(k).is_some()));
    if overridden {
        return Where::Overrides;
    }
    match lives(deck, i, node, name, &key.into_iter().collect::<Vec<_>>()) {
        Lives::State(j) => Where::State(deck.states[j].id.clone()),
        Lives::Node => Where::Node,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TYPES: [NodeType; 10] = [
        NodeType::Text,
        NodeType::Shape,
        NodeType::Image,
        NodeType::Chart,
        NodeType::Table,
        NodeType::Shader,
        NodeType::Stack,
        NodeType::Grid,
        NodeType::Frame,
        NodeType::Group,
    ];

    /// Each property the table offers is one the node type has, and one the schema allows
    /// something an inspector can offer for: none drops out unseen.
    #[test]
    fn every_property_offered_is_one_the_schema_has() {
        for node_type in TYPES {
            let tag = serde_json::to_value(node_type).unwrap();
            let tag = tag.as_str().unwrap();
            for &(prop, source) in own(node_type).iter().chain(&EVERY) {
                let schema = Checker::deck().property(tag, prop);
                assert!(schema.is_some(), "{tag} has no {prop}");
                if let Schema(overrides) = source {
                    assert!(allowed(schema.unwrap(), overrides).is_some(), "{tag}'s {prop}: {schema:?}");
                }
            }
        }
    }

    /// Each property a state offers is one `State` or its transition has, and the schema
    /// allows something an inspector can offer for those it takes from there.
    #[test]
    fn every_state_property_offered_is_one_the_schema_has() {
        for (prop, source) in STATE {
            let (def, name) = prop.strip_prefix("transition/").map_or(("State", prop), |k| ("TransitionSpec", k));
            let schema = Checker::deck().def_property(def, name);
            assert!(schema.is_some(), "{def} has no {name}");
            if let Schema(overrides) = source {
                assert!(allowed(schema.unwrap(), overrides).is_some(), "{prop}: {schema:?}");
            }
        }
    }

    /// Each look characters take is one a run has, and the schema allows something an
    /// inspector can offer for those it takes from there.
    #[test]
    fn every_character_property_offered_is_one_a_run_has() {
        for (prop, source) in CHARACTERS {
            let (def, key) = prop.split_once('/').map_or(("Run", prop), |(_, key)| ("TextStyle", key));
            let schema = Checker::deck().def_property(def, key);
            assert!(schema.is_some(), "{def} has no {key}");
            if let Schema(overrides) = source {
                assert!(allowed(schema.unwrap(), overrides).is_some(), "{prop}: {schema:?}");
            }
        }
    }

    /// The table says which values are overrides as lint W300 does: a value written out there
    /// is one, and a name the theme defines is not.
    #[test]
    fn what_goes_in_the_overrides_is_what_w300_flags() {
        for node_type in TYPES {
            for &(prop, source) in own(node_type).iter().chain(&EVERY) {
                let (written, named) = match source {
                    Names(Vocabulary::Color, overrides) => (overrides.then(|| json!("#102030")), json!("accent")),
                    Names(_, overrides) => (overrides.then(|| json!(12)), json!("space.2")),
                    Schema(overrides) => (overrides.then(|| json!(12)), json!(1)),
                };
                if let Some(written) = written {
                    assert!(literal(prop, &written), "{prop} = {written} is an override");
                }
                if !matches!(source, Schema(true)) {
                    assert!(!literal(prop, &named), "{prop} = {named} is not");
                }
            }
        }
    }
}
