//! Inspect a deck (SPEC §7.1): each state's snapshot, tracking applied (SPEC §2.2); through
//! the theme cascade (PLAN 1.6); its cue on the timeline (PLAN 1.14); the rows its charts and
//! tables read; and, for a client that edits by pointing (ADR-0013), what stands where at
//! rest and where a node may go. And what changes between two states.

use crate::lint::{data_files, engine_with};
use crate::{Bundle, Context, OpsError};
use indexmap::IndexMap;
use scaena_core::document::{NodeType, Props};
use scaena_core::model::values::SplitUnit;
use scaena_core::timeline::{self, CubicBezier, Look};
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
    /// With `snap`: the patch keeps the placement to the state inspected, written into its
    /// own props wherever it lives now (`place`'s `fork`).
    #[serde(default)]
    pub fork: bool,
}

impl Views {
    /// Whether a view needs the state laid out, as a frame lays it out.
    fn laid(&self) -> bool {
        self.boxes || self.at.is_some() || self.targets.is_some()
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
}

/// A node that draws at the point asked about.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Hit {
    pub node: String,
    /// Its box: `[x, y, width, height]`.
    pub rect: [f32; 4],
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
    let theme = match views.resolved || views.timeline || views.laid() || views.format.is_some() {
        true => Some(crate::theme(b)?),
        false => None,
    };
    let files = if views.timeline || views.data || views.laid() { data_files(b)? } else { DataFiles::new() };
    // A cue on lines, words, or a chart's marks counts them after layout, and boxes are
    // layout's, so both need the engine, with the bundle's fonts and images, as `render` does.
    let mut engine = match (&theme, views.timeline || views.laid()) {
        (Some(theme), true) => Some(engine_with(b, theme, None)?),
        _ => None,
    };
    inspect_deck(&b.deck, theme.as_ref(), &files, engine.as_mut(), state, &views)
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
    let projected = match (views.format.as_deref(), theme) {
        (Some(format), Some(theme)) => Some(project(deck, theme, Some(format))?),
        (Some(_), None) => return Err(OpsError::new("inspecting in a format needs the deck's theme")),
        (None, _) => None,
    };
    let (deck, theme) = match &projected {
        Some((deck, theme)) => (deck.as_ref(), Some(theme.as_ref())),
        None => (deck, theme),
    };
    match (&views.targets, state, views.snap, views.to) {
        (Some(_), None, ..) => return Err(OpsError::new("`targets` names a node in a state: name the state")),
        (None, _, Some(_), _) => return Err(OpsError::new("`snap` snaps a node's box: name it with `targets`")),
        (_, _, Some(_), None) => return Err(OpsError::new("`snap` snaps the box a drag left: give it with `to`")),
        (_, _, None, Some(_)) => {
            return Err(OpsError::new("`to` is a box dropped on a node's targets: say how it snaps with `snap`"));
        }
        (_, _, None, None) if views.fork => {
            return Err(OpsError::new("`fork` keeps a snapped patch to its state: say how it snaps with `snap`"));
        }
        _ => {}
    }
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let selected: Vec<&Snapshot> = snaps.iter().filter(|s| state.is_none_or(|id| s.state_id == id)).collect();
    if selected.is_empty() {
        return Err(OpsError::new(format!("unknown state `{}`", state.unwrap_or_default())));
    }
    let needs = |what: &str| OpsError::new(format!("inspecting {what} needs the deck's theme"));
    let theme = match (views.resolved || views.timeline || views.laid(), theme) {
        (true, None) => {
            let what = match (views.resolved, views.timeline) {
                (true, _) => "resolved values",
                (_, true) => "the timeline",
                _ => "where nodes stand",
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
            inspected.timeline = Some(cue_of(slot, &cue));
        }
        if views.data {
            inspected.data = Some(rows(deck, files, &cascade::with_overrides(deck, s))?);
        }
        if let (true, Some(theme), Some(engine)) = (views.laid(), theme, engine.as_deref_mut()) {
            // The deck is in its format already: lay it out as it stands.
            let state = &s.state_id;
            let req = FrameRequest { deck, theme, data: files, state, t_ms: f64::INFINITY, format: None };
            let scene = engine.at_rest(&req)?;
            if views.boxes {
                let boxes = scene
                    .boxes()
                    .into_iter()
                    .map(|b| (b.node, NodeBox { rect: b.rect, parent: b.parent, draws: b.draws }));
                inspected.boxes = Some(boxes.collect());
            }
            if let Some(point) = views.at {
                let hits = scene.hit(point).into_iter().map(|h| {
                    // The engine counts bytes; an agent counts characters.
                    let chars = |at: usize| scene.carets(&h.node).map_or(at, |c| c.text[..at].chars().count());
                    let offset = h.offset.map(chars);
                    Hit { node: h.node, rect: h.rect, containers: h.containers, offset }
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

fn cue_of(slot: &timeline::Slot, cue: &scaena_engine::sample::Transition) -> Cue {
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
        motions: cue.schedule().cues.iter().map(motion).collect(),
    }
}

fn motion(p: &timeline::Placed) -> Motion {
    let mut m = Motion {
        node: p.node.clone(),
        motion: String::new(),
        split: p.split,
        units: p.units,
        start: ms(p.start),
        stagger: ms(p.stagger),
        duration: ms(p.duration),
        end: ms(p.end()),
        curve: curve(&p.curve),
        from: None,
        to: None,
        peak: None,
        tracks: None,
    };
    match &p.motion {
        timeline::Motion::Enter(l) => (m.motion, m.from) = ("enter".into(), Some(change(l))),
        timeline::Motion::Exit(l) => (m.motion, m.to) = ("exit".into(), Some(change(l))),
        timeline::Motion::Emphasis(l) => (m.motion, m.peak) = ("emphasis".into(), Some(change(l))),
        timeline::Motion::Keys(k) => (m.motion, m.tracks) = ("anim".into(), Some(tracks(k))),
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
