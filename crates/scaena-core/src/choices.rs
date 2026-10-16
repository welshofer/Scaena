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

use crate::data::{self, SourceFiles};
use crate::document::{Deck, NodeType, Props};
use crate::lint::literal;
use crate::looks::{casts, rounds, see_through};
use crate::model::check::Checker;
use crate::model::nodes::TextFit;
use crate::model::theme::{Theme, Vocabulary};
use crate::model::values::SplitUnit;
use crate::tracking::{Lives, layout_lives, layout_takers, lives_shown, merge_props, resolve_states};
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
    /// The node as a person names it, by what it shows (PLAN 2.99, [`crate::words::object_in`]):
    /// what the session words its answers with for an editor, absent elsewhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub said: Option<String>,
    /// Its kind as a person names it, `Text`, `Chart`, or a shape's, `Star` ([`crate::words::kind_of`]),
    /// as `said` is; `Icon` for a filled path on a view (PLAN 2.104, [`crate::inserts::is_icon`]),
    /// which the icon picker replaces (PLAN G.23), and `Drawing` for the pen's, stroked (PLAN 2.114),
    /// which it does not.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub noun: Option<String>,
    /// An image's frame as a person says it, where its theme frames pictures (PLAN T.3a): the
    /// theme's, which no image property changes, so an inspector shows it and offers nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub framed: Option<String>,
    /// Where a fetched picture came from, as an editor's Map section heads it (ADR-0030): "Map
    /// from Geoapify, fetched 15 October 2026".
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub fetched: Option<String>,
}

/// What an inspector offers for a state itself (PLAN 2.36).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct StateChoices {
    pub state: String,
    /// Its layout, each key of its transition, its hold, and its notes: what `set_state`
    /// names. A state that sets no key of its transition cuts in.
    pub fields: Vec<Field>,
    /// The state as its slide's number says it, `Slide 3, step 2` (PLAN 2.99,
    /// [`crate::words::slide`]): what the session words its answers with for an editor.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub said: Option<String>,
    /// The slide's number and footer (ADR-0028): what the deck shows on its slides, and what
    /// this slide says of them.
    pub furniture: FurnitureChoice,
}

/// What an inspector offers of a slide's number and footer (ADR-0028): the deck's, which
/// `furnish` sets, and the slide's own, which `set_state`'s `furniture` writes on its first state.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct FurnitureChoice {
    /// Whether the theme places a number or a footer at all: none shows where it places neither.
    pub placed: bool,
    /// Whether the deck numbers its slides.
    pub number: bool,
    /// The words at the foot of its slides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<String>,
    /// The slide's first state, where the slide says whether it shows them.
    pub head: String,
    /// What the slide says: shown, hidden, or as its layout has them (none).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slide: Option<bool>,
    /// Whether the slide shows them, as it stands.
    pub shown: bool,
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
    /// What the theme gives a text's look where the deck sets none, which the state then shows:
    /// its role's family, weight, size, italic, case, tracking, or color; how it fits: as its
    /// slot fits it (theme format 0.13), else shrunk, for a shape's words and for code (PLAN
    /// 2.131); where a shape's words stand across it, in its middle (PLAN 2.108);
    /// how a connector runs and a line's ends are drawn where it says none (PLAN 2.128);
    /// and for characters selected, the text's own look where their run sets none, its role and
    /// emphasis too. What an inspector shows in place of a blank. Absent for any other property.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<Value>,
    /// Its label and its values as a person says them (PLAN 2.99, [`crate::words::field`]):
    /// what the session words its answers with for an editor, absent elsewhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub words: Option<crate::words::FieldWords>,
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
    /// Fractions of an image, from 0 to 1, one for each of `names` (PLAN 2.45): a point's `x`
    /// and `y`, or a part's `x`, `y`, `w`, and `h`.
    Fractions { names: Vec<String> },
}

/// Where a property's values come from.
#[derive(Debug, Clone, Copy)]
enum Source {
    /// The theme's names of a kind; with `true`, a value written out too, an override.
    Names(Vocabulary, bool),
    /// What the deck's schema allows; with `true`, every value is written out, an override.
    Schema(bool),
    /// The deck's data sources, `@name` (PLAN 2.41).
    Data,
    /// The columns of the table the node reads that its channel can read (PLAN 2.41,
    /// [`data::readable`]).
    Columns,
    /// Fractions of the image, one for each name (PLAN 2.45).
    Fractions(&'static [&'static str]),
}

use Source::{Columns, Data, Fractions, Names, Schema};
use Vocabulary as V;

/// What an inspector edits on every node: how far it is turned is its `transform`'s
/// `rotate`, in degrees, clockwise (PLAN 2.51); and what a reader hears of it (PLAN 2.56,
/// SPEC §3.12), its description (`alt`) and its part in the story (`semantic`).
const EVERY: [(&str, Source); 7] = [
    ("opacity", Schema(false)),
    ("transform/rotate", Schema(false)),
    ("enter", Names(V::MotionPreset, false)),
    ("emphasis", Names(V::MotionPreset, false)),
    ("exit", Names(V::MotionPreset, false)),
    ("alt", Schema(false)),
    ("semantic", Schema(false)),
];

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
            // Where its lines stand across its box: left, centered, or right (PLAN 3.25).
            ("align/x", Schema(false)),
            ("fit", Schema(false)),
            ("wrap", Schema(false)),
            ("maxLines", Schema(false)),
        ],
        NodeType::Shape => &[
            ("fill", Names(V::Color, true)),
            ("stroke/paint", Names(V::Color, true)),
            ("stroke/width", Names(V::Stroke, true)),
            ("radius", Names(V::Radius, true)),
            // How a connector runs, and what a line's ends are drawn with (PLAN 2.128).
            ("connect/route", Schema(false)),
            ("ends/start", Schema(false)),
            ("ends/end", Schema(false)),
            // The shadow it casts (PLAN 2.115).
            ("shadow", Names(V::Shadow, false)),
        ],
        // Where an image's focal point is and what part of it shows (PLAN 2.45).
        NodeType::Image => &[
            ("fit", Schema(false)),
            ("focal", Fractions(&["x", "y"])),
            ("crop", Fractions(&["x", "y", "w", "h"])),
            ("radius", Names(V::Radius, true)),
            // The shape it shows inside (PLAN 2.113).
            ("mask", Schema(false)),
            // Its border around what shows (PLAN 2.118), as a shape's outline.
            ("stroke/paint", Names(V::Color, true)),
            ("stroke/width", Names(V::Stroke, true)),
            ("shadow", Names(V::Shadow, false)),
            // Its light and color (PLAN 2.117), each from -1 to 1, as Adjust Image offers them.
            ("adjust/exposure", Schema(false)),
            ("adjust/contrast", Schema(false)),
            ("adjust/saturation", Schema(false)),
            ("adjust/warmth", Schema(false)),
            // A pin's label and color, where it has one, and where a fetched picture's credit
            // stands (ADR-0030).
            ("pin/label", Schema(false)),
            ("pin/color", Names(V::Color, false)),
            ("credit", Schema(false)),
        ],
        NodeType::Shader => &[("preset", Names(V::ShaderPreset, false)), ("palette", Names(V::ShaderPalette, false))],
        // What a chart reads, and how (PLAN 2.41): its source, each channel's field and the
        // type it reads it as, and its key.
        NodeType::Chart => &[
            ("data", Data),
            ("kind", Schema(false)),
            ("orient", Schema(false)),
            ("x/field", Columns),
            ("x/type", Schema(false)),
            ("y/field", Columns),
            ("y/type", Schema(false)),
            ("series/field", Columns),
            ("color/field", Columns),
            ("sizeEncoding/field", Columns),
            ("interval/low", Columns),
            ("interval/high", Columns),
            ("facet/field", Columns),
            ("key", Columns),
            ("labels/show", Schema(false)),
            ("labels/role", Names(V::TextRole, false)),
        ],
        NodeType::Table => &[("data", Data), ("key", Columns), ("header", Schema(false))],
        // A container's panel casts a shadow (PLAN 2.115).
        NodeType::Stack => {
            &[("axis", Schema(false)), ("gap", Names(V::Space, true)), ("shadow", Names(V::Shadow, false))]
        }
        NodeType::Grid => &[("gap", Names(V::Space, true)), ("shadow", Names(V::Shadow, false))],
        NodeType::Frame => &[("shadow", Names(V::Shadow, false))],
        NodeType::Group => &[],
    }
}

/// What an inspector offers for `node` as `state` shows it, in `theme`. A chart's or a
/// table's fields are the columns of the table it reads there, from the bundle's `files`
/// (PLAN 2.41): none is offered where it cannot be read.
pub fn choices(
    deck: &Deck,
    theme: &Theme,
    state: &str,
    node: &str,
    files: &dyn SourceFiles,
) -> Result<Choices, String> {
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
    let table = match node_type {
        NodeType::Chart | NodeType::Table => data::read(deck, files, &shown).ok(),
        _ => None,
    };
    // A picture that may let what is under it show casts no shadow (PLAN 2.115).
    let src = shown.get("src").and_then(Value::as_str);
    let clear = node_type == NodeType::Image && src.and_then(|src| files.bytes(src)).is_some_and(|b| see_through(&b));
    let fields = own(node_type).iter().chain(&EVERY).filter_map(|&(prop, source)| {
        // Corners and a shadow where they draw (PLAN 2.68, 2.113, 2.115): no corners where a mask
        // or the shape's kind takes their place, and no shadow from a line, an outline, a picture
        // under a mask but an oval or that lets what is under it show, or a container with no
        // panel.
        if (prop == "radius" && !rounds(node_type, &shown)) || (prop == "shadow" && !casts(node_type, &shown, clear)) {
            return None;
        }
        // A pin's label and color where it has a pin; where its credit stands where it was
        // fetched (ADR-0030).
        if (prop.starts_with("pin/") && !shown.contains_key("pin"))
            || (prop == "credit" && !shown.contains_key("fetched"))
        {
            return None;
        }
        // A bar chart's bars turn (PLAN 1.29), and a range's values have intervals (PLAN
        // 1.30); no other kind has either (E106).
        let kind = shown.get("kind").and_then(Value::as_str);
        if prop == "orient" && !matches!(kind, Some("bar" | "stackedBar")) {
            return None;
        }
        if prop.starts_with("interval/") && kind != Some("range") {
            return None;
        }
        // A donut colors its slices by category and keys them by it alone: it has no series.
        if matches!(prop, "series/field" | "color/field") && kind == Some("donut") {
            return None;
        }
        // A shape offers what its kind draws (PLAN 2.106, 2.115): corners on a rectangle; a fill
        // on what closes, not a line or an arrow; a shadow under a rectangle or an ellipse.
        if node_type == NodeType::Shape {
            let path = shown.contains_key("path");
            let kind = kind.unwrap_or(if path { "path" } else { "rect" });
            let draws = match prop {
                "radius" => kind == "rect",
                "fill" => !matches!(kind, "line" | "arrow"),
                "shadow" => matches!(kind, "rect" | "ellipse"),
                "ends/start" | "ends/end" => matches!(kind, "line" | "arrow"),
                "connect/route" => shown.contains_key("connect"),
                _ => true,
            };
            if !draws {
                return None;
            }
        }
        // A connector goes where what it joins goes (PLAN 2.109): it turns with neither.
        if prop.starts_with("transform/") && crate::tracking::connects_of(own_props).is_some() {
            return None;
        }
        let takes = match source {
            Data => Takes::Word { words: deck.data.keys().map(|name| format!("@{name}")).collect() },
            Columns => {
                let channel = prop.split_once('/').map_or(prop, |(channel, _)| channel);
                let columns = data::readable(table.as_ref()?, &shown, channel);
                Takes::Word { words: columns.into_iter().map(String::from).collect() }
            }
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
                // An entrance and an exit take the presets that start from a look, and emphasis
                // those that go to one and back (PLAN 2.116): Fade enters, Pulse emphasizes, and
                // Pop, Drop, and Swing do both. Each splits the node only into what it has (E106):
                // Typewriter is a text's, and Grow a chart's.
                if of == V::MotionPreset {
                    let presets = &theme.motion.presets;
                    let emphasis = prop == "emphasis";
                    names.retain(|n| {
                        presets.get(n).is_some_and(|p| {
                            let kind = if emphasis { p.to.is_some() } else { p.from.is_some() };
                            kind && p.split.is_none_or(|split| split.owned(node_type, emphasis))
                        })
                    });
                    // A build by bullet moves a text's paragraphs (PLAN 2.129): a text of two or
                    // more with words offers it.
                    let bullets = node_type == NodeType::Text && crate::builds::paragraphs(&shown).len() >= 2;
                    names.retain(|n| bullets || presets.get(n).and_then(|p| p.split) != Some(SplitUnit::Paragraphs));
                }
                Takes::Name { of, names, overrides }
            }
            Schema(overrides) => match allowed(Checker::deck().property(&tag, prop)?, overrides)? {
                // A text's lines are set from its start where it would stretch.
                Takes::Word { mut words } if prop == "align/x" => {
                    words.retain(|w| w != "stretch");
                    Takes::Word { words }
                }
                // A chart takes the kinds its data makes, and the one it is.
                Takes::Word { mut words } if node_type == NodeType::Chart && prop == "kind" => {
                    if let Some(table) = &table {
                        words.retain(|w| kind == Some(w.as_str()) || makes(w, table, &shown));
                    }
                    Takes::Word { words }
                }
                takes => takes,
            },
            Fractions(names) => Takes::Fractions { names: names.iter().map(|n| n.to_string()).collect() },
        };
        let (name, key) = prop.split_once('/').map_or((prop, None), |(n, k)| (n, Some(k)));
        let value = match key {
            // One keyword aligns both axes (PLAN 3.25).
            Some(_) if name == "align" && shown.get(name).is_some_and(Value::is_string) => shown.get(name),
            Some(key) => shown.get(name).and_then(|v| v.get(key)),
            None => shown.get(name),
        };
        let lives = value.map(|_| lives_at(deck, i, node, snapshots[i].nodes.get(node), over, name, key));
        let literal = value.is_some_and(|v| literal(prop, v));
        let role = shown.get("role").and_then(Value::as_str);
        let layout = snapshots[i].layout.as_deref();
        // A shape's words stand in its middle across it where they say nothing (PLAN 2.108).
        let in_shape = crate::tracking::words_of(deck, own_props).is_some();
        let words = in_shape && prop == "align/x";
        let gives = || {
            role_gives(theme, role, prop)
                .or_else(|| fit_gives(theme, layout, &shown, in_shape, prop))
                .or_else(|| words.then(|| Value::from("center")))
                .or_else(|| aligned(theme, layout, &shown, prop))
        };
        let theme = match node_type {
            NodeType::Text => gives(),
            NodeType::Shape => drawn_with(&shown, prop),
            _ => None,
        };
        Some(Field { prop: prop.into(), takes, value: value.cloned(), lives, literal, theme, words: None })
    });
    let fetched = (node_type == NodeType::Image).then(|| fetched_said(&shown)).flatten();
    Ok(Choices {
        node: node.into(),
        node_type,
        state: state.into(),
        fields: fields.collect(),
        said: None,
        noun: None,
        fetched,
        framed: None,
    })
}

/// Where a fetched picture came from, said (ADR-0030): "Map from Geoapify, fetched 15 October
/// 2026"; none for a picture with no record that reads.
fn fetched_said(shown: &Props) -> Option<String> {
    let record: crate::fetched::Fetched = serde_json::from_value(shown.get("fetched")?.clone()).ok()?;
    let service = crate::fetched::service(&record.from).map_or(record.from.clone(), |s| s.title.to_string());
    let what = if record.map.is_some() { "Map" } else { "Picture" };
    let day = crate::fetched::date(&record.at).map(crate::words::day)?;
    Some(format!("{what} from {service}, fetched {day}"))
}

/// How a line is drawn where nothing in the deck says (PLAN 2.128): a connector runs straight, a
/// line's ends are bare, and an arrow's last is a filled head. None for any other property.
fn drawn_with(shown: &Props, prop: &str) -> Option<Value> {
    let arrow = shown.get("kind").and_then(Value::as_str) == Some("arrow");
    match prop {
        "connect/route" => Some(Value::from("straight")),
        "ends/start" => Some(Value::from("none")),
        "ends/end" => Some(Value::from(if arrow { "arrow" } else { "none" })),
        _ => None,
    }
}

/// What the theme gives `prop` of a text set in `role` where nothing in the deck sets it: the
/// role's own family, weight, size, italic, case, tracking, or color, as the theme cascade
/// starts a text's look (SPEC §3.6). A role that names no color is set in `onSurface`, and none
/// is upright, as typed, and untracked.
fn role_gives(theme: &Theme, role: Option<&str>, prop: &str) -> Option<Value> {
    let role = theme.typography.roles.get(role?)?;
    Some(match prop {
        "style/family" => Value::from(role.family.clone()),
        "style/weight" => Value::from(role.weight),
        "style/size" => Value::from(role.size),
        "style/italic" => Value::from(role.italic.unwrap_or(false)),
        "style/case" => {
            role.case.as_ref().and_then(|c| serde_json::to_value(c).ok()).unwrap_or_else(|| Value::from("none"))
        }
        "style/tracking" => Value::from(role.tracking.unwrap_or(0.0)),
        "style/color" => Value::from(role.color.clone().unwrap_or_else(|| "onSurface".into())),
        _ => return None,
    })
}

/// How a text fits where nothing in the deck sets `fit`, as the engine sets it (SPEC §3.4,
/// [`TextFit::effective`]): as the slot of the state's `layout` it is placed in fits it (theme
/// format 0.13), else by shrinking, for a shape's words (`in_shape`) and for code (ADR-0029). None
/// for prose its slot gives no fit, which wraps, and for any other property.
fn fit_gives(theme: &Theme, layout: Option<&str>, shown: &Props, in_shape: bool, prop: &str) -> Option<Value> {
    if prop != "fit" {
        return None;
    }
    let code = shown.get("code").is_some_and(Value::is_object);
    let slot = shown.get("at").and_then(|at| at.get("in")).and_then(Value::as_str);
    let slot = slot.and_then(|slot| theme.layouts.get(layout?)?.slots.get(slot)?.fit);
    if slot.is_none() && !in_shape && !code {
        return None;
    }
    serde_json::to_value(TextFit::effective(None, slot, in_shape, code)).ok()
}

/// Where a text's lines stand across its box where nothing in the deck says (`align/x`), as the
/// engine sets them: as the slot it is placed in in `layout` aligns them, else from their start
/// (PLAN 3.25). A shape's words, which stand in its middle, are said before this is asked. None for
/// any other property.
fn aligned(theme: &Theme, layout: Option<&str>, shown: &Props, prop: &str) -> Option<Value> {
    if prop != "align/x" {
        return None;
    }
    let at = shown.get("at");
    let slot = at.filter(|a| a.get("parent").is_none()).and_then(|a| a.get("in")).and_then(Value::as_str);
    let align = slot.and_then(|s| theme.layouts.get(layout?)?.slots.get(s)?.align.as_ref());
    let x = match align {
        Some(crate::model::values::Align::Both(x)) => Some(*x),
        Some(crate::model::values::Align::Axes(axes)) => axes.x,
        None => None,
    };
    // A text set to stretch is set from its start.
    let x = x.filter(|x| *x != crate::model::values::AlignX::Stretch);
    Some(x.and_then(|x| serde_json::to_value(x).ok()).unwrap_or_else(|| Value::from("start")))
}

/// Whether a chart of `kind`, its other props as `shown` gives them, can be drawn from `table`, as
/// validation (E103) and the chart's layout judge it: a scatter's `x` reads numbers or dates; a
/// donut keys each slice by its `x` (or `key`) alone, so those tell its rows apart; a slope compares
/// two of its `x`'s values; and a range spans the series its `series` or `color` gives, or each
/// value's `interval`.
fn makes(kind: &str, table: &data::Table, shown: &Props) -> bool {
    let field = |c: &str| shown.get(c).and_then(|e| e.get("field")).and_then(Value::as_str);
    let column = |c: &str| field(c).and_then(|f| table.column(f));
    let (Some(x), Some(y)) = (column("x"), column("y")) else { return true };
    let drawn = || table.rows.iter().filter(move |row| row[y] != data::Datum::Null);
    match kind {
        "scatter" => matches!(table.types[x], data::ColumnType::Number | data::ColumnType::Date),
        "donut" => {
            let by = shown.get("key").and_then(Value::as_str).and_then(|k| table.column(k)).unwrap_or(x);
            let panel = column("facet");
            let mut seen = std::collections::HashSet::new();
            drawn().all(|row| seen.insert((panel.map(|p| row[p].label()), row[by].label())))
        }
        "slope" => crate::sort::set(drawn().map(|row| row[x].label())).len() == 2,
        "range" => ["series", "color", "interval"].iter().any(|p| shown.contains_key(*p)),
        _ => true,
    }
}

/// What an inspector edits on characters selected in a text (PLAN 2.38): a run's own, which
/// `style_text` sets. A run takes the theme's names only, so a color has no override.
const CHARACTERS: [(&str, Source); 7] = [
    ("role", Names(V::TextRole, false)),
    ("emphasis", Schema(false)),
    ("style/family", Names(V::FontFamily, false)),
    ("style/weight", Schema(false)),
    ("style/italic", Schema(false)),
    // In em after each character selected, on top of the font's kerning: on a pair's first
    // letter, the space between the two (PLAN 1.38).
    ("style/tracking", Schema(false)),
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
    let lives = lives_at(deck, i, node, snapshots[i].nodes.get(node), over, prop, None);
    let fields = CHARACTERS.iter().filter_map(|&(prop, source)| {
        let takes = match source {
            Names(of, overrides) => Takes::Name { of, names: theme.names(of), overrides },
            Schema(overrides) => {
                let (def, key) = prop.split_once('/').map_or(("Run", prop), |(_, key)| ("TextStyle", key));
                allowed(Checker::deck().def_property(def, key)?, overrides)?
            }
            // A run reads no data and has no image.
            Data | Columns | Fractions(_) => return None,
        };
        let value = run.and_then(|r| match prop.split_once('/') {
            Some((name, key)) => r.get(name)?.get(key),
            None => r.get(prop),
        });
        let lives = value.map(|_| lives.clone());
        // Where its run sets none, the characters show the text's own look (its role and its
        // emphasis too, which a run sets of its own), else what the theme gives the role they
        // are set in: the run's, else the text's.
        let own = match prop.split_once('/') {
            Some((name, key)) => shown.get(name).and_then(|look| look.get(key)).cloned(),
            None => shown.get(prop).cloned(),
        };
        let role = run.and_then(|r| r.get("role")?.as_str()).or_else(|| shown.get("role").and_then(Value::as_str));
        let theme = own.or_else(|| role_gives(theme, role, prop));
        Some(Field { prop: prop.into(), takes, value: value.cloned(), lives, literal: false, theme, words: None })
    });
    Ok(Choices {
        node: node.into(),
        node_type: NodeType::Text,
        state: state.into(),
        fields: fields.collect(),
        said: None,
        noun: None,
        fetched: None,
        framed: None,
    })
}

/// What an inspector edits on a state: what `set_state` names. A transition's keys are those
/// of `TransitionSpec`; the rest are `State`'s.
const STATE: [(&str, Source); 9] = [
    ("layout", Names(V::Layout, false)),
    ("transition/style", Schema(false)),
    ("transition/direction", Schema(false)),
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
            // A state reads no data and has no image.
            Data | Columns | Fractions(_) => return None,
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
        Some(Field { prop: prop.into(), takes, value, lives, literal: false, theme: None, words: None })
    });
    let fields = fields.collect();
    let furniture = furniture_choice(deck, theme, state, snapshots[i].layout.as_deref());
    Ok(StateChoices { state: state.into(), fields, said: None, furniture })
}

/// What the deck and the slide `state` is on say of the slide's number and footer.
fn furniture_choice(deck: &Deck, theme: &Theme, state: &str, layout: Option<&str>) -> FurnitureChoice {
    let wants = deck.furniture.clone().unwrap_or_default();
    let places = theme.furniture.as_ref();
    let slide = crate::furniture::slide(deck, state);
    let head = deck.state_index(state).map(|i| deck.slide_of(&deck.states[i]).to_string()).unwrap_or_default();
    let placed = places.is_some_and(|p| p.number.is_some() || p.footer.is_some());
    let shows_any = (wants.number && places.is_some_and(|p| p.number.is_some()))
        || (wants.footer.is_some() && places.is_some_and(|p| p.footer.is_some()));
    let shown = shows_any && slide.zip(places).is_some_and(|(s, p)| s.shown(p, layout));
    FurnitureChoice {
        placed,
        number: wants.number,
        footer: wants.footer,
        head,
        slide: slide.and_then(|s| s.shows),
        shown,
    }
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

/// Where the value of `name` (or its key `key`) that state `i` shows of `node` lives, `shown` the
/// node as the state shows it before the deck's overrides, `over`.
fn lives_at(
    deck: &Deck,
    i: usize,
    node: &str,
    shown: Option<&Props>,
    over: Option<&Props>,
    name: &str,
    key: Option<&str>,
) -> Where {
    let overridden =
        over.and_then(|o| o.get(name)).is_some_and(|v| key.is_none_or(|k| v.is_null() || v.get(k).is_some()));
    if overridden {
        return Where::Overrides;
    }
    match lives_shown(deck, i, node, shown, name, &key.into_iter().collect::<Vec<_>>()) {
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

    /// A fetched map offers its pin's label and color and where its credit stands (ADR-0030), and a
    /// picture with neither offers none of them.
    #[test]
    fn a_map_offers_its_pin_and_its_credit() {
        let theme = Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        let mut doc: Value = serde_json::from_str(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        doc["nodes"]["photo"] = json!({ "type": "image", "src": "assets/p.png", "at": { "in": "canvas" } });
        doc["nodes"]["map"] = json!({
            "type": "image", "src": "assets/m.png", "at": { "in": "canvas" },
            "fetched": { "from": "geoapify", "at": "2026-10-15T20:12:44Z", "asked": "x",
                         "credit": [{ "text": "© OpenStreetMap contributors" }] },
            "pin": { "at": [0.5, 0.5], "label": "Here" }
        });
        doc["states"][0]["props"]["photo"] = json!({});
        doc["states"][0]["props"]["map"] = json!({});
        let deck: Deck = serde_json::from_value(doc).unwrap();
        let files = std::collections::BTreeMap::<String, Vec<u8>>::new();
        let state = deck.states[0].id.clone();
        let props = |node: &str| -> Vec<String> {
            choices(&deck, &theme, &state, node, &files).unwrap().fields.into_iter().map(|f| f.prop).collect()
        };
        let map = props("map");
        for prop in ["pin/label", "pin/color", "credit"] {
            assert!(map.iter().any(|p| p == prop), "{prop}: {map:?}");
        }
        assert!(props("photo").iter().all(|p| !p.starts_with("pin/") && p != "credit"));
        let label = choices(&deck, &theme, &state, "map", &files).unwrap();
        let label = label.fields.iter().find(|f| f.prop == "pin/label").unwrap();
        assert_eq!(label.value, Some(json!("Here")));
        let heading = choices(&deck, &theme, &state, "map", &files).unwrap().fetched;
        assert_eq!(heading.as_deref(), Some("Picture from Geoapify, fetched 15 October 2026"));
    }

    /// Each property an inspector offers, on a node, on characters, or on a state, has a label
    /// of its own in the editors' words (PLAN 2.99): none is shown by its path, or by its last
    /// key alone.
    #[test]
    fn every_property_offered_has_a_label_of_its_own() {
        let labeled = |prop: &str| crate::words::LABELS.iter().any(|(p, _)| *p == prop);
        let offered = TYPES.iter().flat_map(|&t| own(t).iter().chain(&EVERY)).chain(&CHARACTERS).chain(&STATE);
        for (prop, _) in offered {
            assert!(labeled(prop), "{prop} has no label in `words::LABELS`");
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
                    // A source and a column are the data's names, never overrides.
                    Data => (None, json!("@q3")),
                    Columns => (None, json!("region")),
                    // Fractions of an image are none either.
                    Fractions(names) => (None, json!(vec![0.5; names.len()])),
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
