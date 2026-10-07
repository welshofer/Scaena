//! Inspect a deck (SPEC §7.1): each state's snapshot, tracking applied (SPEC §2.2); through
//! the theme cascade (PLAN 1.6); its cue on the timeline (PLAN 1.14); the rows its charts and
//! tables read; and, for a client that edits by pointing (ADR-0013), what stands where at
//! rest, where a node may go, and what an inspector offers for it. And what changes between
//! two states.

use crate::arrange::{self, Align, Arranged, Asked, Order, Spread};
use crate::lint::{data_files, engine_with};
use crate::{Bundle, Context, OpsError};
use indexmap::IndexMap;
use scaena_core::choices::{Choices, StateChoices, choices, state_choices};
use scaena_core::document::{NodeType, Props};
use scaena_core::inserts::{Insert, Start, inserts};
use scaena_core::layers::{Layer, layers};
use scaena_core::looks::{self, Put};
use scaena_core::model::values::SplitUnit;
use scaena_core::patch::{SemanticOp, Timed};
use scaena_core::timeline::{self, CubicBezier, Look};
use scaena_core::validate::BundleFiles;
use scaena_core::{Deck, Snapshot};
use scaena_engine::cascade;
use scaena_engine::data::{self, DataFiles, Datum};
use scaena_engine::geometry::{By, Snap};
use scaena_engine::layout::Grid;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest, project};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What `inspect` shows of each state besides its snapshot, and in which format.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Views {
    /// Through the theme cascade: each node with the deck's overrides merged in, each text
    /// node's look, and what each node's overrides set.
    #[serde(default)]
    pub resolved: bool,
    /// Its cue: where it falls on the deck's timeline, its transition, and each motion as
    /// placed on its clock.
    #[serde(default)]
    pub timeline: bool,
    /// The rows each chart and table reads, after its `dataTransform`.
    #[serde(default)]
    pub data: bool,
    /// Each visible node's box at rest, canvas units: what a pointer selects and moves
    /// (ADR-0013).
    #[serde(default)]
    pub boxes: bool,
    /// The nodes that draw at this point at rest, `[x, y]` in canvas units, topmost first,
    /// each with the containers it sits in (ADR-0013).
    #[serde(default)]
    pub at: Option<[f32; 2]>,
    /// One of the deck's `formats` (`9:16`) to inspect it in, laid out again with its
    /// template set; the deck's own canvas without it.
    #[serde(default)]
    pub format: Option<String>,
    /// A node shown in the state inspected: where it may go, as a drag snaps it (ADR-0013).
    /// What holds it, its cell, and the tracks, slots, or order it takes.
    #[serde(default)]
    pub targets: Option<String>,
    /// With `targets` and `to`: how the dropped box snaps, and the patch that puts the node
    /// there.
    #[serde(default)]
    pub snap: Option<SnapMode>,
    /// With `snap`: the box a drag left, `[x, y, width, height]` in canvas units: the node's
    /// cell, moved or resized.
    #[serde(default)]
    pub to: Option<[f32; 4]>,
    /// With `snap` or `arrange`: the patch keeps what it changes to the state inspected,
    /// written into its own props wherever it lives now (`place`'s and `choose`'s `fork`).
    #[serde(default)]
    pub fork: bool,
    /// Several nodes shown in the state inspected, children of one container, arranged at
    /// once (PLAN 2.42): with one of `align`, `spread`, `order`, or `by`, where each lands
    /// and the patch that puts them there.
    #[serde(default)]
    pub arrange: Option<Vec<String>>,
    /// With `arrange`: the edge, or the middle, they all take: the one all of them reach
    /// farthest, or the middle of them all; on a grid, snapped to its tracks.
    #[serde(default)]
    pub align: Option<Align>,
    /// With `arrange`: the first and the last stay, and the ones between move so the gaps
    /// between them all are equal, `across` or `down`.
    #[serde(default)]
    pub spread: Option<Spread>,
    /// With `arrange`: each goes `forward` in front of the next of its container's children
    /// that it overlaps, `backward` behind the one before, or to the `front` or the `back` of
    /// them all, by its `z`.
    #[serde(default)]
    pub order: Option<Order>,
    /// With `arrange`, one node: listed just before this other node, as `layers` lists them
    /// (PLAN 2.50): painted just over it, by the `z` that does it, or, in a stack, laid out
    /// just before it. Held by another container, or by none, the node goes there with it,
    /// placed as that container places what it holds.
    #[serde(default)]
    pub before: Option<String>,
    /// With `arrange`, one node: listed just after this other node: painted just under it, or,
    /// in a stack, laid out just after it; into what holds it, as with `before`.
    #[serde(default)]
    pub after: Option<String>,
    /// With `arrange`, one node: into this container, listed first among what it holds,
    /// placed as it places what it holds (PLAN 2.50).
    #[serde(default)]
    pub into: Option<String>,
    /// With `arrange`: moved together `[dx, dy]` canvas units: the first snapped as a drag of
    /// it snaps, the rest as far as it went, each its own way.
    #[serde(default)]
    pub by: Option<[f32; 2]>,
    /// With `by`: off the grid, each to whole canvas units (a `rect`), as Shift drags.
    #[serde(default)]
    pub free: bool,
    /// A node shown in the state inspected: what an inspector offers for it (ADR-0013). Each
    /// property it edits, with the theme's names for it or what the schema allows, the value
    /// the state shows, and where that value lives, which is where `choose` writes.
    #[serde(default)]
    pub choices: Option<String>,
    /// What an inspector offers for the state inspected itself (PLAN 2.36): its layout (the
    /// theme's layouts with a slot for each node placed in one), each key of its transition,
    /// its hold, and its notes, each with the value it has and where it lives, which is where
    /// `set_state` writes.
    #[serde(default)]
    pub state_choices: bool,
    /// The layouts the state inspected may take, best first (PLAN 2.92): each of the theme's
    /// layouts that `state_choices` offers, with what lint finds in the state laid out in it,
    /// fewest errors, then fewest warnings, then the theme's order; the states its patch
    /// changes; and that patch, one `set_state`. Those that draw the state alike, in `format`
    /// or on the deck's canvas, are one, which names the rest (`alike`).
    #[serde(default)]
    pub layouts: bool,
    /// What may be inserted in the state inspected (PLAN 2.34): a text in each of the
    /// theme's roles, each kind of shape, each image in the bundle, a chart and a table of
    /// each data source (PLAN 2.41), and each shader preset, each as `add_node` adds it, with
    /// the box it takes at first.
    #[serde(default)]
    pub inserts: bool,
    /// The state inspected's layers (PLAN 2.50): its nodes nested as their containers and
    /// groups hold them, topmost first, with those that leave in it and those another state
    /// of its slide shows, hidden, each where the nearest state that shows it places it.
    #[serde(default)]
    pub layers: bool,
    /// A node shown in the state inspected: its look (PLAN 2.58), each property of its type's
    /// look with the value the state shows, as the editor's ⌥⌘C picks it up.
    #[serde(default)]
    pub look: Option<String>,
    /// With `look`: nodes shown in the state to put it on, and what that makes: a `choose` for
    /// each property of it a node shows otherwise, written where that node's own value lives
    /// (what `deck_patch` takes), and the nodes that look so already or take none of it, with
    /// why.
    #[serde(default)]
    pub onto: Option<Vec<String>>,
}

impl Views {
    /// Whether a view needs the state laid out, as a frame lays it out.
    fn laid(&self) -> bool {
        self.boxes || self.at.is_some() || self.targets.is_some() || self.arrange.is_some()
    }
}

/// How a box dropped on a node's targets snaps (ADR-0013).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SnapMode {
    /// As many cells as the node spans, from the track nearest the box's corner.
    Move,
    /// Each edge to the nearest track's.
    Resize,
    /// Into the slot, or the grid container's area, the box covers most.
    Slot,
    /// Where it was dropped, in whole canvas units: a `rect`. On the theme's grid, an
    /// override (W301); in a frame, its own way of placing.
    Free,
    /// Among a stack's children, where the box's middle falls.
    Order,
}

impl SnapMode {
    pub const ALL: [SnapMode; 5] = [SnapMode::Move, SnapMode::Resize, SnapMode::Slot, SnapMode::Free, SnapMode::Order];

    pub fn name(self) -> &'static str {
        match self {
            SnapMode::Move => "move",
            SnapMode::Resize => "resize",
            SnapMode::Slot => "slot",
            SnapMode::Free => "free",
            SnapMode::Order => "order",
        }
    }

    fn engine(self) -> Snap {
        match self {
            SnapMode::Move => Snap::Move,
            SnapMode::Resize => Snap::Resize,
            SnapMode::Slot => Snap::Slot,
            SnapMode::Free => Snap::Free,
            SnapMode::Order => Snap::Order,
        }
    }
}

impl std::str::FromStr for SnapMode {
    type Err = String;

    fn from_str(s: &str) -> Result<SnapMode, String> {
        SnapMode::ALL.into_iter().find(|m| m.name() == s).ok_or_else(|| {
            let names: Vec<&str> = SnapMode::ALL.iter().map(|m| m.name()).collect();
            format!("`{s}` is not a way to snap: {}", names.join(", "))
        })
    }
}

/// One state, inspected.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Inspected {
    #[serde(flatten)]
    pub snapshot: Snapshot,
    /// Each text node's look, through the cascade (`resolved`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub looks: Option<IndexMap<String, TextLook>>,
    /// What each node's overrides set, as pointers into the node: what makes it not
    /// theme-safe (`resolved`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<IndexMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline: Option<Cue>,
    /// The rows each chart and table reads, by node (`data`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<IndexMap<String, Rows>>,
    /// Each visible node's box at rest (`boxes`): those that draw, in paint order, then the
    /// containers and groups that only hold others.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boxes: Option<IndexMap<String, NodeBox>>,
    /// The nodes that draw at the point asked about (`at`), topmost first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hits: Option<Vec<Hit>>,
    /// Where the node asked about may go (`targets`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targets: Option<Targets>,
    /// Where the box dropped at `to` lands (`snap`), and the patch that puts the node there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapped: Option<Snapped>,
    /// Where the nodes `arrange` names land, and the patch that puts them there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arranged: Option<Arranged>,
    /// What an inspector offers for the node asked about (`choices`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<Choices>,
    /// What an inspector offers for the state itself (`state_choices`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_choices: Option<StateChoices>,
    /// The layouts the state may take, best first (`layouts`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layouts: Option<Vec<crate::layouts::Suggestion>>,
    /// What may be inserted in this state (`inserts`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inserts: Option<Vec<Insert>>,
    /// Its layers, what stands on the canvas topmost first (`layers`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layers: Option<Vec<Layer>>,
    /// The look of the node asked about (`look`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look: Option<looks::Look>,
    /// That look put on the nodes `onto` names (`onto`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<Put>,
}

/// Where a node may go in a state at rest (ADR-0013): what a drag shows as guides.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Targets {
    /// What holds it, and so how it is placed: `grid`, the theme's (by cells, a slot, or a
    /// `rect`); `stack` (by order); `cells`, a grid container's (by its cells or areas); or
    /// `frame` (by a `rect` from its padding edge).
    pub by: String,
    /// The container that holds it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The box its placement names now, before its inset, offset, alignment, and size: what
    /// a drag moves.
    pub cell: [f32; 4],
    /// The tracks a placement by cells takes, each `[start, end]`: columns left to right.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<[f32; 2]>,
    /// Rows, top to bottom.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<[f32; 2]>,
    /// The boxes a placement by name takes: the template's slots, then `canvas` and `grid`;
    /// or a grid container's areas.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub slots: IndexMap<String, [f32; 4]>,
    /// A stack's children in their order, this node among them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub flow: Vec<String>,
    /// What a `rect` is measured in: the canvas, or a frame's padding box; else the box of
    /// the container that holds it.
    pub within: [f32; 4],
    /// How a box dropped here snaps (`snap`).
    pub snaps: Vec<SnapMode>,
}

/// Where a dropped box lands.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Snapped {
    /// The box a guide shows there: a cell, a slot, a `rect`, or, in a stack, a line across
    /// it where the node goes.
    pub cell: [f32; 4],
    /// The patch that puts the node there, made in the state inspected: `place` ops (SPEC
    /// §7.3), each written where that placement lives. What `scaena patch` and `deck_patch`
    /// take; empty when the node is there already.
    pub patch: Vec<Value>,
}

/// A visible node's box at rest, canvas units (ADR-0013).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct NodeBox {
    /// `[x, y, width, height]`: its grid cell or slot, the box its container gave it, or a
    /// group's box around its members.
    pub rect: [f32; 4],
    /// The container or group it sits in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Whether it draws anything: a container with no panel, and a group, only hold others.
    pub draws: bool,
    /// Where its `transform`, and those of what holds it, draw it (SPEC §3.3): the map of
    /// canvas points `[a, b, c, d, e, f]` (`x' = a·x + c·y + e`) from `rect` to where it is
    /// drawn. Left out where nothing turns, scales, leans, or moves it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<[f32; 6]>,
}

/// A node that draws at the point asked about.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Hit {
    pub node: String,
    /// Its box: `[x, y, width, height]`.
    pub rect: [f32; 4],
    /// As a box's `transform`: where it is drawn from `rect`, where something moves it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<[f32; 6]>,
    /// The containers and groups it sits in, innermost first.
    pub containers: Vec<String>,
    /// For a text, where a caret put at the point stands: how many characters of its text
    /// (Unicode scalar values) come before it, as `replace_text` counts them (ADR-0013).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

/// A text node's look as the cascade resolved it (SPEC §3.6).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct TextLook {
    /// The role it is set in: its own, or its slot's.
    pub role: String,
    /// The family's name, as its font names it.
    pub family: String,
    pub size: f64,
    pub weight: f64,
    pub leading: f64,
    pub tracking: f64,
    /// The color as the cascade names it, and as sRGB.
    pub color: String,
    pub hex: String,
}

impl From<cascade::Look> for TextLook {
    fn from(l: cascade::Look) -> TextLook {
        let cascade::Look { role, family, size, weight, leading, tracking, color, hex } = l;
        TextLook { role, family, size, weight, leading, tracking, color, hex }
    }
}

/// A state's cue (SPEC §2.4, §3.9), ms: where it falls on the deck's timeline, its
/// transition, and each motion as placed on the state's clock, which starts with the
/// transition.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Cue {
    /// When the transition into it starts.
    pub start: f64,
    /// Its transition and motions, from `start`.
    pub span: f64,
    /// Its dwell at rest before the next state starts.
    pub hold: f64,
    pub transition: Transition,
    pub motions: Vec<Motion>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Transition {
    pub duration: f64,
    pub curve: Curve,
    /// `id`: nodes are matched by id across the cut. `none`: every node leaves and enters.
    #[serde(rename = "match")]
    pub matched: String,
}

/// An easing as its cubic Bézier, or a spring as its constants.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum Curve {
    Ease { ease: [f64; 4] },
    Spring { spring: Spring },
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Spring {
    pub stiffness: f64,
    pub damping: f64,
    pub mass: f64,
}

/// One motion on one node: what it does, over how many units, and when, ms.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Motion {
    pub node: String,
    /// `enter`, `exit`, `emphasis`, or `anim`.
    pub motion: String,
    /// What it moves one at a time: lines, words, glyphs, children, or marks.
    pub split: Option<SplitUnit>,
    pub units: usize,
    pub start: f64,
    /// From one unit's start to the next's.
    pub stagger: f64,
    /// Each unit's.
    pub duration: f64,
    pub end: f64,
    /// When its first unit starts to change and its last comes to rest: an `anim`'s from when
    /// its first key falls.
    pub moving: [f64; 2],
    /// Its `delay` as written, what `time_motion` reads and sets (PLAN 2.44): a choreography
    /// item's own, else its preset call's, and a node's own preset call's. A node's own `anim`
    /// has none: it is when its first key falls.
    pub delay: f64,
    /// Where it is written, a JSON pointer into the deck: the choreography item
    /// (`/states/2/choreography/0`), or the node's own property where it lives
    /// (`/states/1/props/title/enter`, `/nodes/title/exit`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub written: Option<String>,
    pub curve: Curve,
    /// An entrance's look before it: what it changes from rest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<LookChange>,
    /// An exit's look after it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<LookChange>,
    /// An emphasis's look at its peak.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak: Option<LookChange>,
    /// An `anim`'s tracks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracks: Option<Tracks>,
}

/// A look as what it changes from rest.
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct LookChange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translate: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tint: Option<Tint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Tint {
    /// sRGB.
    pub color: String,
    pub amount: f64,
}

/// An `anim`'s tracks, each its keys.
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct Tracks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<Vec<Key<f64>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translate: Option<Vec<Key<[f64; 2]>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<Vec<Key<[f64; 2]>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotate: Option<Vec<Key<f64>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<Vec<Key<f64>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<[f64; 2]>,
}

/// A key: when (ms after its track starts), its value, and the curve from the key before.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Key<T> {
    pub t: f64,
    pub v: T,
    pub curve: Curve,
}

/// The rows a chart or table reads, after its `dataTransform`, cells typed by the source's
/// schema; dates in ISO 8601.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Rows {
    pub source: String,
    pub columns: Vec<String>,
    pub types: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// Each state of the bundle's deck, or the one named, inspected.
pub fn inspect(b: &Bundle, state: Option<&str>, views: Views) -> Result<Vec<Inspected>, OpsError> {
    let snaps = scaena_core::resolve_states(&b.deck).context("tracking")?;
    if !snaps.iter().any(|s| state.is_none_or(|id| s.state_id == id)) {
        return Err(OpsError::new(format!("unknown state `{}`", state.unwrap_or_default())));
    }
    let themed = views.resolved
        || views.timeline
        || views.laid()
        || views.format.is_some()
        || views.choices.is_some()
        || views.state_choices
        || views.layouts
        || views.inserts
        || views.look.is_some();
    let theme = if themed { Some(crate::theme(b)?) } else { None };
    let files = match (
        views.timeline || views.data || views.laid(),
        views.choices.is_some() || views.inserts || views.look.is_some(),
    ) {
        (true, _) => data_files(b)?,
        // What a chart may read is its data's columns: a file the bundle lacks offers none.
        (false, true) => data_files(b).unwrap_or_else(|_| DataFiles::new()),
        (false, false) => DataFiles::new(),
    };
    // A cue on lines, words, or a chart's marks counts them after layout, and boxes are
    // layout's, so both need the engine, with the bundle's fonts and images, as `render` does.
    let mut engine = match (&theme, views.timeline || views.laid()) {
        (Some(theme), true) => Some(engine_with(b, theme, None)?),
        _ => None,
    };
    let mut out = inspect_deck(&b.deck, theme.as_ref(), &files, engine.as_mut(), state, &views)?;
    // What may be inserted is the theme's and the bundle's: the deck's own inspection has no
    // list of the bundle's files.
    if let (true, Some(theme)) = (views.inserts, &theme) {
        let paths = b.files.list().context("the bundle's files")?;
        let offered = inserts(&b.deck, theme, &paths, &files);
        out.iter_mut().for_each(|i| i.inserts = Some(offered.clone()));
    }
    // The layouts a state may take are judged by lint, which reads the bundle's files.
    if let (true, Some(state)) = (views.layouts, state) {
        let suggested = crate::layouts::suggest(b, state, views.format.as_deref())?;
        out.iter_mut().for_each(|i| i.layouts = Some(suggested.clone()));
    }
    Ok(out)
}

/// Each state of `deck`, or the one named, inspected with the theme, data files, and engine
/// the caller holds: `resolved` and a format need the theme, and `timeline`, `boxes`, and
/// `at` the theme and an engine with the deck's fonts and images. A client that keeps them
/// between edits (the web editor, PLAN 2.3) passes its own; [`inspect`] builds them.
pub fn inspect_deck(
    deck: &Deck,
    theme: Option<&Theme>,
    files: &DataFiles,
    engine: Option<&mut Engine>,
    state: Option<&str>,
    views: &Views,
) -> Result<Vec<Inspected>, OpsError> {
    // Where each motion is written is the deck's as written, in whatever format it is inspected.
    let doc = match views.timeline {
        true => deck.to_value().context("the deck as JSON")?,
        false => Value::Null,
    };
    let projected = match (views.format.as_deref(), theme) {
        (Some(format), Some(theme)) => Some(project(deck, theme, Some(format))?),
        (Some(_), None) => return Err(OpsError::new("inspecting in a format needs the deck's theme")),
        (None, _) => None,
    };
    let (deck, theme) = match &projected {
        Some((deck, theme)) => (deck.as_ref(), Some(theme.as_ref())),
        None => (deck, theme),
    };
    if views.choices.is_some() && state.is_none() {
        return Err(OpsError::new("`choices` names a node in a state: name the state"));
    }
    if views.state_choices && state.is_none() {
        return Err(OpsError::new("`state_choices` says what an inspector offers for a state: name the state"));
    }
    if views.inserts && state.is_none() {
        return Err(OpsError::new("`inserts` says what may be inserted in a state: name the state"));
    }
    if views.layouts && state.is_none() {
        return Err(OpsError::new("`layouts` says what layouts a state may take: name the state"));
    }
    if views.look.is_some() && state.is_none() {
        return Err(OpsError::new("`look` picks up a node's look in a state: name the state"));
    }
    if views.onto.is_some() && views.look.is_none() {
        return Err(OpsError::new("`onto` puts a look on nodes: pick it up with `look`"));
    }
    match (&views.targets, state, views.snap, views.to) {
        (Some(_), None, ..) => return Err(OpsError::new("`targets` names a node in a state: name the state")),
        (None, _, Some(_), _) => return Err(OpsError::new("`snap` snaps a node's box: name it with `targets`")),
        (_, _, Some(_), None) => return Err(OpsError::new("`snap` snaps the box a drag left: give it with `to`")),
        (_, _, None, Some(_)) => {
            return Err(OpsError::new("`to` is a box dropped on a node's targets: say how it snaps with `snap`"));
        }
        (_, _, None, None) if views.fork && views.arrange.is_none() => {
            return Err(OpsError::new(
                "`fork` keeps a snapped or arranged patch to its state: say how it snaps with `snap`, or arrange nodes with `arrange`",
            ));
        }
        _ => {}
    }
    let ways = [
        views.align.is_some(),
        views.spread.is_some(),
        views.order.is_some(),
        views.before.is_some(),
        views.after.is_some(),
        views.into.is_some(),
        views.by.is_some(),
    ];
    match (&views.arrange, state, ways.iter().filter(|w| **w).count()) {
        (Some(_), None, _) => return Err(OpsError::new("`arrange` names nodes in a state: name the state")),
        (Some(_), _, 0) => {
            return Err(OpsError::new(
                "`arrange` arranges nodes one way: give `align`, `spread`, `order`, `before`, `after`, `into`, or `by`",
            ));
        }
        (Some(_), _, 2..) => {
            return Err(OpsError::new(
                "`arrange` arranges nodes one way: give one of `align`, `spread`, `order`, `before`, `after`, `into`, and `by`",
            ));
        }
        (None, _, 1..) => {
            return Err(OpsError::new(
                "`align`, `spread`, `order`, `before`, `after`, `into`, and `by` arrange nodes: name them with `arrange`",
            ));
        }
        _ if views.free && views.by.is_none() => {
            return Err(OpsError::new("`free` moves nodes off the grid: move them with `arrange` and `by`"));
        }
        _ => {}
    }
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let selected: Vec<&Snapshot> = snaps.iter().filter(|s| state.is_none_or(|id| s.state_id == id)).collect();
    if selected.is_empty() {
        return Err(OpsError::new(format!("unknown state `{}`", state.unwrap_or_default())));
    }
    let needs = |what: &str| OpsError::new(format!("inspecting {what} needs the deck's theme"));
    let offers = views.choices.is_some() || views.state_choices || views.look.is_some();
    let theme = match (views.resolved || views.timeline || views.laid() || offers, theme) {
        (true, None) => {
            let what = match (views.resolved, views.timeline, views.laid()) {
                (true, ..) => "resolved values",
                (_, true, _) => "the timeline",
                (_, _, true) => "where nodes stand",
                _ => "what an inspector offers",
            };
            return Err(needs(what));
        }
        (_, theme) => theme,
    };
    let mut engine = match (views.timeline || views.laid(), engine) {
        (true, None) => {
            let what = if views.timeline { "the timeline" } else { "where nodes stand" };
            return Err(OpsError::new(format!("inspecting {what} needs an engine with the deck's fonts")));
        }
        (_, engine) => engine,
    };
    let timeline = match (theme, views.timeline, engine.as_deref_mut()) {
        (Some(theme), true, Some(engine)) => Some(engine.timeline(deck, theme, files)?),
        _ => None,
    };
    let mut out = Vec::new();
    for s in selected {
        let snapshot = if views.resolved { cascade::with_overrides(deck, s) } else { s.clone() };
        let mut inspected = Inspected {
            snapshot,
            looks: None,
            overrides: None,
            timeline: None,
            data: None,
            boxes: None,
            hits: None,
            targets: None,
            snapped: None,
            arranged: None,
            choices: None,
            state_choices: None,
            layouts: None,
            inserts: None,
            layers: None,
            look: None,
            put: None,
        };
        if let (true, Some(theme)) = (views.resolved, theme) {
            let (mut looks, mut overrides) = (IndexMap::new(), IndexMap::new());
            for (id, props) in &inspected.snapshot.nodes {
                if deck.nodes[id].node_type == NodeType::Text {
                    let slot = Grid::slot_role(theme, inspected.snapshot.layout.as_deref(), props.get("at"));
                    let look = cascade::look(theme, props, slot.as_deref()).with_context(|| format!("node `{id}`"))?;
                    looks.insert(id.clone(), TextLook::from(look));
                }
                let over = deck.overridden(id);
                if !over.is_empty() {
                    overrides.insert(id.clone(), over);
                }
            }
            (inspected.looks, inspected.overrides) = (Some(looks), Some(overrides));
        }
        if let (Some(timeline), Some(theme), Some(engine)) = (&timeline, theme, engine.as_deref_mut()) {
            let slot = timeline.slot(&s.state_id).context("a state missing from the timeline")?;
            let cue = engine.transition(deck, theme, files, &s.state_id)?;
            inspected.timeline = Some(cue_of(slot, &cue, &doc, &s.state_id));
        }
        if views.data {
            inspected.data = Some(rows(deck, files, &cascade::with_overrides(deck, s))?);
        }
        if views.layers {
            let at = snaps.iter().position(|o| o.state_id == s.state_id).expect("a state of the deck");
            inspected.layers = Some(layers(deck, &snaps, at));
        }
        if let (Some(node), Some(theme)) = (&views.choices, theme) {
            inspected.choices = Some(choices(deck, theme, &s.state_id, node, files).map_err(OpsError::new)?);
        }
        if let (true, Some(theme)) = (views.state_choices, theme) {
            inspected.state_choices = Some(state_choices(deck, theme, &s.state_id).map_err(OpsError::new)?);
        }
        if let (Some(node), Some(theme)) = (&views.look, theme) {
            let picked = looks::look(deck, theme, &s.state_id, node, files).map_err(OpsError::new)?;
            if let Some(onto) = &views.onto {
                let put = looks::putting(deck, theme, &s.state_id, &picked, onto, files).map_err(OpsError::new)?;
                inspected.put = Some(put);
            }
            inspected.look = Some(picked);
        }
        if let (true, Some(theme), Some(engine)) = (views.laid(), theme, engine.as_deref_mut()) {
            // The deck is in its format already: lay it out as it stands.
            let state = &s.state_id;
            let req = FrameRequest { deck, theme, data: files, state, t_ms: f64::INFINITY, format: None };
            let scene = engine.at_rest(&req)?;
            if views.boxes {
                let boxes = scene.boxes().into_iter().map(|b| {
                    (b.node, NodeBox { rect: b.rect, parent: b.parent, draws: b.draws, transform: b.transform })
                });
                inspected.boxes = Some(boxes.collect());
            }
            if let Some(point) = views.at {
                let hits = scene.hit(point).into_iter().map(|h| {
                    // The engine counts bytes; an agent counts characters.
                    let chars = |at: usize| scene.carets(&h.node).map_or(at, |c| c.text[..at].chars().count());
                    let offset = h.offset.map(chars);
                    Hit { node: h.node, rect: h.rect, transform: h.transform, containers: h.containers, offset }
                });
                inspected.hits = Some(hits.collect());
            }
            if let Some(node) = &views.targets {
                let found = engine.targets(&req, node)?;
                if let (Some(how), Some(to)) = (views.snap, views.to) {
                    let Some(target) = snap(&found, how, to, state, views.fork)? else {
                        let ways: Vec<&str> = Targets::from(found).snaps.iter().map(|m| m.name()).collect();
                        return Err(OpsError::new(format!(
                            "`{node}` does not snap by `{}` where it is held: it snaps by {}",
                            how.name(),
                            ways.join(", ")
                        )));
                    };
                    inspected.snapped = Some(target);
                }
                inspected.targets = Some(Targets::from(found));
            }
            if let Some(nodes) = &views.arrange {
                let found = nodes.iter().map(|n| engine.targets(&req, n)).collect::<Result<Vec<_>, _>>()?;
                let asked = Asked {
                    align: views.align,
                    spread: views.spread,
                    order: views.order,
                    before: views.before.clone(),
                    after: views.after.clone(),
                    into: views.into.clone(),
                    by: views.by,
                    free: views.free,
                };
                let how = asked.how()?;
                let (shown, boxes) = (cascade::with_overrides(deck, s), scene.boxes());
                let mut into = |node: &str, holder: Option<&str>| Ok(engine.targets_into(&req, node, holder)?);
                inspected.arranged = arrange::arrange(deck, &shown, &boxes, nodes, found, how, views.fork, &mut into)?;
            }
        }
        out.push(inspected);
    }
    Ok(out)
}

impl From<scaena_engine::geometry::Targets> for Targets {
    fn from(found: scaena_engine::geometry::Targets) -> Targets {
        let snaps = SnapMode::ALL.into_iter().filter(|m| found.snap(m.engine(), found.cell).is_some()).collect();
        let (by, parent) = match found.by {
            By::Grid => ("grid", None),
            By::Stack { parent, .. } => ("stack", Some(parent)),
            By::Cells { parent } => ("cells", Some(parent)),
            By::Frame { parent } => ("frame", Some(parent)),
        };
        Targets {
            by: by.to_string(),
            parent,
            cell: found.cell,
            columns: found.columns,
            rows: found.rows,
            slots: found.slots.into_iter().collect(),
            flow: found.flow.into_iter().map(|(id, ..)| id).collect(),
            within: found.within,
            snaps,
        }
    }
}

/// Where the box `to` lands on `found` when it snaps `how`, with the patch that puts the
/// node there as an edit in `state`, or, to `fork` it, kept to `state`; `None` where nothing
/// places the node that way.
pub fn snap(
    found: &scaena_engine::geometry::Targets,
    how: SnapMode,
    to: [f32; 4],
    state: &str,
    fork: bool,
) -> Result<Option<Snapped>, OpsError> {
    let Some(target) = found.snap(how.engine(), to) else { return Ok(None) };
    let ops = target.ops(Some(state), fork);
    let patch = ops.iter().map(serde_json::to_value).collect::<Result<_, _>>().context("a patch")?;
    Ok(Some(Snapped { cell: target.cell, patch }))
}

/// A node a patch adds (PLAN 2.34): its id, where it lands, and the patch.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Added {
    pub id: String,
    /// Its box once placed, `[x, y, width, height]` in canvas units.
    pub cell: [f32; 4],
    /// `add_node`, the node entering in the state named, then the `place` ops that put it
    /// there.
    pub patch: Vec<Value>,
}

/// The patch that inserts `insert` in `state` as `room`'s node (PLAN 2.34): `add_node`, the
/// node entering there, then `place`. A text or an image fills the template's slot under `at`
/// (canvas units) where no node of the state's is placed in it; anything else, or a text where
/// the slot is filled, takes the box it starts as, about `at`, snapped to the theme's grid as a
/// drop snaps; a shader fills the slot it names. What draws content (a text, an image, a chart,
/// a table) and would overlap what is `crowded` there (`Scene::crowded`, what lint E101 judges
/// it against) goes instead to the place on the grid clear of it whose middle is nearest `at`,
/// and stays about `at` where none is (PLAN 2.79). `room` is the engine's for the new node in
/// `deck` (`Engine::room`), its cell the box `insert` starts as.
pub fn inserting(
    deck: &Deck,
    room: &scaena_engine::geometry::Targets,
    insert: &Insert,
    state: &str,
    at: [f32; 2],
    crowded: &[[f32; 4]],
) -> Result<Added, OpsError> {
    let target = match &insert.start {
        Start::Box { .. } => {
            let kind = insert.node["type"].as_str();
            let content = matches!(kind, Some("text" | "image"));
            let empty = if content { empty_slot(deck, room, state, at)? } else { None };
            match empty {
                Some(rect) => room.snap(Snap::Slot, rect),
                None => {
                    let [_, _, w, h] = room.cell;
                    let about = room.snap(Snap::Move, [at[0] - w / 2.0, at[1] - h / 2.0, w, h]);
                    match kind {
                        Some("text" | "image" | "chart" | "table") => roomy(room, about, crowded, at),
                        _ => about,
                    }
                }
            }
        }
        Start::Slot(name) => {
            let slot = room.slots.iter().find(|(n, _)| n == name);
            let (_, rect) = slot.ok_or_else(|| OpsError::new(format!("the state has no slot `{name}`")))?;
            room.snap(Snap::Slot, *rect)
        }
    };
    added(room, insert.node.clone(), state, target)
}

/// `about`, where it is clear of `crowded`; else the place on `room`'s grid for a box its size,
/// clear of it, whose middle is nearest `at`; else `about`, where there is none (PLAN 2.79). Two
/// boxes are clear of each other as lint E101 has them: overlapping by 2 cu or less either way.
fn roomy(
    room: &scaena_engine::geometry::Targets,
    about: Option<scaena_engine::geometry::Target>,
    crowded: &[[f32; 4]],
    at: [f32; 2],
) -> Option<scaena_engine::geometry::Target> {
    let about = about?;
    let clear = |c: &[f32; 4]| {
        crowded.iter().all(|o| {
            let w = (c[0] + c[2]).min(o[0] + o[2]) - c[0].max(o[0]);
            let h = (c[1] + c[3]).min(o[1] + o[3]) - c[1].max(o[1]);
            w <= 2.0 || h <= 2.0
        })
    };
    if clear(&about.cell) {
        return Some(about);
    }
    let [_, _, w, h] = about.cell;
    let far = |t: &scaena_engine::geometry::Target| {
        let [x, y, w, h] = t.cell;
        let (dx, dy) = (x + w / 2.0 - at[0], y + h / 2.0 - at[1]);
        dx * dx + dy * dy
    };
    let places = room.columns.iter().flat_map(|c| room.rows.iter().map(move |r| [c[0], r[0], w, h]));
    places
        .filter_map(|p| room.snap(Snap::Move, p))
        .filter(|t| clear(&t.cell))
        .min_by(|a, b| far(a).total_cmp(&far(b)))
        .or(Some(about))
}

/// The patch that draws `insert` in `state` as `room`'s node (PLAN 2.48): `add_node`, the node
/// entering there, then `place`. It takes the box a drag from `from` to `to` (canvas units)
/// covers, each edge snapped to the nearest track's as a resize snaps, or, `free`, where it was
/// drawn, in whole canvas units: a `rect` (W301). A line or an arrow runs across that box from
/// the corner the drag began at to the one it ended at, or across its middle where the drag was
/// nearly level or upright (`across`). What fills a slot, a shader, fills it as [`inserting`]
/// puts it.
pub fn drawing(
    deck: &Deck,
    room: &scaena_engine::geometry::Targets,
    insert: &Insert,
    state: &str,
    [from, to]: [[f32; 2]; 2],
    free: bool,
) -> Result<Added, OpsError> {
    if matches!(insert.start, Start::Slot(_)) {
        return inserting(deck, room, insert, state, from, &[]);
    }
    let drawn = [from[0].min(to[0]), from[1].min(to[1]), (to[0] - from[0]).abs(), (to[1] - from[1]).abs()];
    let target = room.snap(if free { Snap::Free } else { Snap::Resize }, drawn);
    let mut node = insert.node.clone();
    if matches!(node["kind"].as_str(), Some("line" | "arrow"))
        && let Some(points) = across(from, to)
    {
        node["points"] = points;
    }
    added(room, node, state, target)
}

/// The points of a line drawn from `from` to `to`, as fractions of its box: from the corner the
/// drag began at to the one it ended at, or across the middle where it was within 14° of level
/// or upright, so a rule drawn by hand comes out straight. `None` for a line's own way, across
/// the middle from left to right.
fn across(from: [f32; 2], to: [f32; 2]) -> Option<Value> {
    let [dx, dy] = [to[0] - from[0], to[1] - from[1]];
    let ends = |d: f32| if d < 0.0 { (1, 0) } else { (0, 1) };
    if dy.abs() * 4.0 <= dx.abs() {
        return (dx < 0.0).then(|| serde_json::json!([[1, 0.5], [0, 0.5]]));
    }
    let (y0, y1) = ends(dy);
    if dx.abs() * 4.0 <= dy.abs() {
        return Some(serde_json::json!([[0.5, y0], [0.5, y1]]));
    }
    let (x0, x1) = ends(dx);
    Some(serde_json::json!([[x0, y0], [x1, y1]]))
}

/// `add_node` for `node` as `room`'s, entering in `state`, then the `place` ops that put it on
/// `target`.
fn added(
    room: &scaena_engine::geometry::Targets,
    node: Value,
    state: &str,
    target: Option<scaena_engine::geometry::Target>,
) -> Result<Added, OpsError> {
    let target = target.ok_or_else(|| OpsError::new("the theme's grid has no tracks to place it on"))?;
    let node = serde_json::from_value(node).context("an inserted node")?;
    let add = SemanticOp::AddNode { id: room.node.clone(), node, state: Some(state.into()), props: None };
    let ops: Vec<SemanticOp> = std::iter::once(add).chain(target.ops(Some(state), false)).collect();
    let patch = ops.iter().map(serde_json::to_value).collect::<Result<_, _>>().context("a patch")?;
    Ok(Added { id: room.node.clone(), cell: target.cell, patch })
}

/// The smallest of the template's slots under `at` that no node `state` shows is placed in.
pub(crate) fn empty_slot(
    deck: &Deck,
    room: &scaena_engine::geometry::Targets,
    state: &str,
    at: [f32; 2],
) -> Result<Option<[f32; 4]>, OpsError> {
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let snap =
        snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::new(format!("unknown state `{state}`")))?;
    let filled: Vec<&str> = snap
        .nodes
        .values()
        .filter_map(|p| p.get("at").filter(|a| a.get("parent").is_none())?.get("in")?.as_str())
        .collect();
    let under = |&[x, y, w, h]: &[f32; 4]| x <= at[0] && at[0] <= x + w && y <= at[1] && at[1] <= y + h;
    let free = room.slots.iter().filter(|(name, rect)| {
        !matches!(name.as_str(), "canvas" | "grid") && !filled.contains(&name.as_str()) && under(rect)
    });
    Ok(free.map(|(_, rect)| *rect).min_by(|a, b| (a[2] * a[3]).total_cmp(&(b[2] * b[3]))))
}

/// The patch that copies `node`, as `state` shows it, with each node it holds there, beside it
/// (PLAN 2.34): `add_node` for each, entering there, the copy as `found`'s node and each node it
/// holds under the next id free, held by the copy; then `place`: the span the copy takes on the
/// theme's grid, or a grid container's, moved one span right, down, left, or up. It goes to the
/// first of those clear of the node and of what else draws there in front of the background
/// (`boxes`, what stands where in `state`), or else the first clear of the node, or else the
/// first that moves it at all, the grid's edge holding it in; elsewhere it takes the node's own
/// placement. `found` is where `node` may go, its node the copy's id.
pub fn duplicating(
    deck: &Deck,
    found: &scaena_engine::geometry::Targets,
    node: &str,
    state: &str,
    boxes: &[scaena_engine::geometry::NodeBox],
) -> Result<Added, OpsError> {
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let snap =
        snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::new(format!("unknown state `{state}`")))?;
    if !snap.nodes.contains_key(node) {
        return Err(OpsError::new(format!("`{node}` is not on screen in `{state}`")));
    }
    // The node first, then what it holds, each before what it holds in turn.
    let mut copying = held(&[snap], node);
    copying.reverse();
    let mut taken: Vec<String> = deck.nodes.keys().cloned().chain([found.node.clone()]).collect();
    let mut ids: IndexMap<String, String> = IndexMap::new();
    for id in &copying {
        let copy = if id == node { found.node.clone() } else { free_id(&taken, id) };
        taken.push(copy.clone());
        ids.insert(id.clone(), copy);
    }
    let mut ops = Vec::new();
    for (id, copy) in &ids {
        let mut props = serde_json::Map::new();
        props.insert("type".into(), serde_json::to_value(deck.nodes[id].node_type).context("a node's type")?);
        props.extend(snap.nodes[id].iter().map(|(k, v)| (k.clone(), v.clone())));
        // Held by the copy of what held it.
        let parent = props.get("at").and_then(|a| a.get("parent")).and_then(Value::as_str).and_then(|p| ids.get(p));
        if let (Some(parent), Some(Value::Object(at))) = (parent.cloned(), props.get_mut("at")) {
            at.insert("parent".into(), Value::String(parent));
        }
        copied_layouts(&mut props, deck, &ids, false);
        let node_value = serde_json::from_value(Value::Object(props)).context("a copied node")?;
        ops.push(SemanticOp::AddNode { id: copy.clone(), node: node_value, state: Some(state.into()), props: None });
    }
    let [x, y, w, h] = found.cell;
    let clear_of = |c: &[f32; 4], [bx, by, bw, bh]: [f32; 4]| {
        c[0] >= bx + bw || c[0] + c[2] <= bx || c[1] >= by + bh || c[1] + c[3] <= by
    };
    let z = |id: &str| snap.nodes.get(id).and_then(|p| p.get("z")).and_then(Value::as_f64).unwrap_or(0.0);
    let parent = boxes.iter().find(|b| b.node == node).and_then(|b| b.parent.clone());
    let others: Vec<[f32; 4]> = boxes
        .iter()
        .filter(|b| b.draws && b.parent == parent && !copying.contains(&b.node) && z(&b.node) >= 0.0)
        .map(|b| b.rect)
        .collect();
    let target = match found.by {
        By::Grid | By::Cells { .. } => {
            let tried = [[x + w, y, w, h], [x, y + h, w, h], [x - w, y, w, h], [x, y - h, w, h]]
                .map(|to| found.snap(Snap::Move, to));
            let apart = |c: &[f32; 4]| clear_of(c, found.cell);
            let alone = |c: &[f32; 4]| apart(c) && others.iter().all(|o| clear_of(c, *o));
            let moved = |c: &[f32; 4]| c[..2] != found.cell[..2];
            let first = |keep: &dyn Fn(&[f32; 4]) -> bool| tried.iter().flatten().find(|t| keep(&t.cell)).cloned();
            first(&alone).or_else(|| first(&apart)).or_else(|| first(&moved))
        }
        _ => None,
    };
    ops.extend(target.iter().flat_map(|t| t.ops(Some(state), false)));
    let patch = ops.iter().map(serde_json::to_value).collect::<Result<_, _>>().context("a patch")?;
    Ok(Added { id: found.node.clone(), cell: target.map_or(found.cell, |t| t.cell), patch })
}

/// A copy's layouts in the deck's formats (ADR-0020), from the props it was copied with: only
/// those `deck` lays out anew, each held by the copy of what held it there (`ids`). `placed`,
/// the copy goes where it is put, in every format, so its placements there go too; what else
/// it lays out by there (a container's tracks, a text's lines) stays.
pub(crate) fn copied_layouts(
    props: &mut serde_json::Map<String, Value>,
    deck: &Deck,
    ids: &IndexMap<String, String>,
    placed: bool,
) {
    let Some(Value::Object(formats)) = props.get_mut("formats") else { return };
    let anew = deck.anew();
    formats.retain(|name, _| anew.contains(&name.as_str()));
    for layout in formats.values_mut() {
        let Value::Object(layout) = layout else { continue };
        if placed {
            layout.remove("at");
        } else if let Some(Value::Object(at)) = layout.get_mut("at")
            && let Some(parent) = at.get("parent").and_then(Value::as_str).and_then(|p| ids.get(p))
        {
            at.insert("parent".into(), Value::String(parent.clone()));
        }
    }
    formats.retain(|_, layout| layout.as_object().is_none_or(|l| !l.is_empty()));
    if formats.is_empty() {
        props.remove("formats");
    }
}

/// The first of `base`, `base-2`, `base-3`, … not in `taken`.
pub(crate) fn free_id(taken: &[String], base: &str) -> String {
    (1..)
        .map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") })
        .find(|id| !taken.contains(id))
        .expect("some number is free")
}

/// The patch that deletes `node` from `state` (PLAN 2.34), with each node it holds there: each
/// leaves there and in the states after it (`hide_node`), and one that no state shows then goes
/// from the deck (`remove_node`), so a node inserted and deleted leaves nothing behind. With
/// `everywhere`, each goes from the deck, with each node it holds in any state. A node goes
/// before what holds it. `files` is the bundle's, which the patch is made on.
pub fn deleting(
    deck: &Deck,
    files: &dyn BundleFiles,
    node: &str,
    state: &str,
    everywhere: bool,
) -> Result<Vec<Value>, OpsError> {
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let snap =
        snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::new(format!("unknown state `{state}`")))?;
    if !snap.nodes.contains_key(node) {
        return Err(OpsError::new(format!("`{node}` is not on screen in `{state}`")));
    }
    let holding: Vec<&Snapshot> = if everywhere { snaps.iter().collect() } else { vec![snap] };
    let gone = held(&holding, node);
    let remove = |id: &String| serde_json::json!({ "op": "remove_node", "id": id });
    if everywhere {
        return Ok(gone.iter().map(remove).collect());
    }
    let hide: Vec<Value> =
        gone.iter().map(|id| serde_json::json!({ "op": "hide_node", "node": id, "state": state })).collect();
    let doc = deck.to_value().context("the deck")?;
    let hidden = scaena_core::patch::compile(&doc, &hide, files).map_err(|e| OpsError::new(e.message))?;
    let after = Deck::from_value(&hidden.doc).map_err(OpsError::new)?;
    let after = scaena_core::resolve_states(&after).context("tracking")?;
    let shown = |id: &String| after.iter().any(|s| s.nodes.contains_key(id));
    Ok(gone.iter().zip(hide).map(|(id, hide)| if shown(id) { hide } else { remove(id) }).collect())
}

/// `node` and each node it holds in `snaps`, at any depth: the deepest first, `node` last.
pub(crate) fn held(snaps: &[&Snapshot], node: &str) -> Vec<String> {
    let mut depth: IndexMap<String, usize> = IndexMap::new();
    for snap in snaps {
        let parent = |id: &str| snap.nodes.get(id)?.get("at")?.get("parent")?.as_str().map(String::from);
        for id in snap.nodes.keys() {
            let (mut up, mut steps) = (parent(id), 1);
            while let Some(p) = up.filter(|_| steps <= snap.nodes.len()) {
                if p == node {
                    let d = depth.entry(id.clone()).or_default();
                    *d = (*d).max(steps);
                    break;
                }
                (up, steps) = (parent(&p), steps + 1);
            }
        }
    }
    let mut ids: Vec<(String, usize)> = depth.into_iter().collect();
    scaena_core::sort::by_key(&mut ids, |(_, depth)| std::cmp::Reverse(*depth));
    ids.into_iter().map(|(id, _)| id).chain(std::iter::once(node.to_string())).collect()
}

/// State `state`'s cue, each motion with where `doc`, the deck as written, writes it.
fn cue_of(slot: &timeline::Slot, cue: &scaena_engine::sample::Transition, doc: &Value, state: &str) -> Cue {
    let timing = cue.timing();
    Cue {
        start: ms(slot.start),
        span: ms(slot.span),
        hold: ms(slot.hold),
        transition: Transition {
            duration: ms(timing.duration_ms),
            curve: curve(&timing.curve),
            matched: if timing.matched { "id" } else { "none" }.to_string(),
        },
        motions: cue.schedule().cues.iter().map(|p| motion(p, doc, state)).collect(),
    }
}

fn motion(p: &timeline::Placed, doc: &Value, state: &str) -> Motion {
    let mut m = Motion {
        node: p.node.clone(),
        motion: String::new(),
        split: p.split,
        units: p.units,
        start: ms(p.start),
        stagger: ms(p.stagger),
        duration: ms(p.duration),
        end: ms(p.end()),
        moving: [ms(p.start), ms(p.end())],
        delay: 0.0,
        written: None,
        curve: curve(&p.curve),
        from: None,
        to: None,
        peak: None,
        tracks: None,
    };
    let timed = match &p.motion {
        timeline::Motion::Enter(l) => {
            (m.motion, m.from) = ("enter".into(), Some(change(l)));
            Timed::Enter
        }
        timeline::Motion::Exit(l) => {
            (m.motion, m.to) = ("exit".into(), Some(change(l)));
            Timed::Exit
        }
        timeline::Motion::Emphasis(l) => {
            (m.motion, m.peak) = ("emphasis".into(), Some(change(l)));
            Timed::Emphasis
        }
        timeline::Motion::Keys(k) => {
            (m.motion, m.tracks) = ("anim".into(), Some(tracks(k)));
            let first = [k.opacity.first().map(|k| k.t), k.translate.first().map(|k| k.t)]
                .into_iter()
                .chain([k.scale.first().map(|k| k.t), k.rotate.first().map(|k| k.t), k.progress.first().map(|k| k.t)])
                .flatten()
                .fold(f64::INFINITY, f64::min);
            if first.is_finite() {
                m.moving[0] = ms(p.start + first);
            }
            Timed::Anim
        }
    };
    if let Ok(written) = scaena_core::patch::written(doc, state, &p.node, timed) {
        (m.delay, m.written) = (ms(written.delay), Some(written.pointer));
    }
    m
}

fn change(l: &Look) -> LookChange {
    let rest = Look::REST;
    let differs = |a: f64, b: f64| (a != b).then_some(a);
    let pair = |a: [f64; 2], b: [f64; 2]| (a != b).then_some(a);
    LookChange {
        opacity: differs(l.opacity, rest.opacity),
        translate: pair(l.translate, rest.translate),
        scale: pair(l.scale, rest.scale),
        rotate: differs(l.rotate, rest.rotate),
        anchor: pair(l.anchor, rest.anchor),
        tint: l.tint.map(|(color, amount)| Tint { color: color.to_hex(), amount }),
        progress: differs(l.progress, rest.progress),
    }
}

fn tracks(k: &timeline::Keys) -> Tracks {
    fn track<T: Copy>(keys: &[timeline::Key<T>]) -> Option<Vec<Key<T>>> {
        (!keys.is_empty()).then(|| keys.iter().map(|k| Key { t: ms(k.t), v: k.v, curve: curve(&k.curve) }).collect())
    }
    Tracks {
        opacity: track(&k.opacity),
        translate: track(&k.translate),
        scale: track(&k.scale),
        rotate: track(&k.rotate),
        progress: track(&k.progress),
        anchor: (k.anchor != Look::REST.anchor).then_some(k.anchor),
    }
}

fn curve(c: &timeline::Curve) -> Curve {
    match c {
        timeline::Curve::Ease(CubicBezier(x1, y1, x2, y2)) => Curve::Ease { ease: [*x1, *y1, *x2, *y2] },
        timeline::Curve::Spring(s, _) => {
            Curve::Spring { spring: Spring { stiffness: s.stiffness, damping: s.damping, mass: s.mass } }
        }
    }
}

/// The rows each chart and table in a state reads (SPEC §3.10): its source through its
/// `dataTransform`, as the engine reads them.
fn rows(deck: &Deck, files: &DataFiles, snap: &Snapshot) -> Result<IndexMap<String, Rows>, OpsError> {
    let cell = |d: &Datum| match d {
        Datum::Number(n) => serde_json::json!(n),
        Datum::Text(s) => serde_json::json!(s),
        Datum::Bool(b) => serde_json::json!(b),
        Datum::Date(_) => serde_json::json!(d.label()),
        Datum::Null => Value::Null,
    };
    let mut out = IndexMap::new();
    for (id, props) in &snap.nodes {
        if !matches!(deck.nodes[id].node_type, NodeType::Chart | NodeType::Table) {
            continue;
        }
        let Some(source) = props.get("data").and_then(|d| d.as_str()).and_then(|d| d.strip_prefix('@')) else {
            continue;
        };
        let table = data::load(deck, files, source)
            .and_then(|t| data::transform(t, props.get("dataTransform")))
            .with_context(|| format!("node `{id}` in state `{}`", snap.state_id))?;
        out.insert(
            id.clone(),
            Rows {
                source: source.to_string(),
                columns: table.columns.clone(),
                types: table.types.iter().map(|t| t.name().to_string()).collect(),
                rows: table.rows.iter().map(|r| r.iter().map(cell).collect()).collect(),
            },
        );
    }
    Ok(out)
}

/// A time in ms to the microsecond: a spring's settle time is not exact, and reads no
/// better for its last digits.
pub fn ms(x: f64) -> f64 {
    let rounded = (x * 1000.0).round() / 1000.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

/// What changes from one state to another, resolved, by node.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    /// It enters, with these props.
    Enter(Props),
    /// It exits.
    Exit(bool),
    /// It stays, and these props change.
    Change(Props),
}

/// What changes between states `from` and `to`, resolved: `diff`.
pub fn diff(b: &Bundle, from: &str, to: &str) -> Result<IndexMap<String, Change>, OpsError> {
    let snaps = scaena_core::resolve_states(&b.deck).context("tracking")?;
    let find = |id: &str| snaps.iter().find(|s| s.state_id == id).with_context(|| format!("unknown state `{id}`"));
    let (a, z) = (find(from)?, find(to)?);
    let mut changes = IndexMap::new();
    for (id, props) in &z.nodes {
        match a.nodes.get(id) {
            None => drop(changes.insert(id.clone(), Change::Enter(props.clone()))),
            Some(prev) if prev != props => {
                let delta: Props = props
                    .iter()
                    .filter(|(k, v)| prev.get(*k) != Some(*v))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                changes.insert(id.clone(), Change::Change(delta));
            }
            _ => {}
        }
    }
    for id in a.nodes.keys().filter(|id| !z.nodes.contains_key(*id)) {
        changes.insert(id.clone(), Change::Exit(true));
    }
    Ok(changes)
}
