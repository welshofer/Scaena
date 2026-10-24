//! `Engine::frame(request) -> DisplayList`: the engine's one public entry point.
//!
//! A frame draws the theme surface, then one layer per visible node in paint order
//! (`z`, then scene-graph order). Each snapshot is laid out once into a [`Scene`]
//! (text on the theme grid, charts compiled to marks); a frame inside a transition
//! samples the two scenes through a [`Transition`] and lays nothing out (SPEC §5).
//! Node types that arrive later return `NotImplemented` naming their PLAN task,
//! rather than drawing a placeholder a golden would freeze.

use crate::EngineError;
use crate::cascade;
use crate::charts::{self, Ctx};
use crate::containers::{self, Placement};
use crate::data::DataFiles;
use crate::fonts::BundleFonts;
use crate::images::{BundleImages, ImageNode};
use crate::layout::{AlignX, AlignY, BaselineGrid, Grid};
use crate::motion;
use crate::sample::{Content, Place, Policy, Scene, SceneNode, Timing, Transition};
use crate::shaders::ShaderNode;
use crate::shapes::ShapeNode;
use crate::tables;
use crate::text::{GRID_EPSILON, ListMark, Span, TextAlign, TextEngine, TextLayout, TextSpec};
use crate::theme::{Numeric, TextBox, Theme, Wrap};
use scaena_core::displaylist::{Color, DisplayList, Rect};
use scaena_core::document::{NodeType, Props};
use scaena_core::model::Format;
use scaena_core::model::nodes::TextFit;
use scaena_core::model::theme::Snap;
use scaena_core::model::values::ListItem;
use scaena_core::model::values::{SplitUnit, Transform};
use scaena_core::pose::Pose;
use scaena_core::timeline::{Motion, Timeline};
use scaena_core::{Deck, Snapshot};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct FrameRequest<'a> {
    pub deck: &'a Deck,
    pub theme: &'a Theme,
    /// The bundle's data files, which charts read (SPEC §3.10).
    pub data: &'a DataFiles,
    pub state: &'a str,
    /// Milliseconds since the start of the transition into `state`: into the state's
    /// cue, its transition and then its motions. At or past its span (`f64::INFINITY`
    /// always is), the state at rest.
    pub t_ms: f64,
    /// The format to lay the deck out in (SPEC §3.4): one of its `formats`, as it writes
    /// them (`"9:16"`), or `None` for its own canvas.
    pub format: Option<&'a str>,
}

/// The deck and theme as they lay out in `format` (SPEC §3.4): the deck on that format's
/// canvas, each node with its layout there (`formats`, ADR-0020) in place of its own, and the
/// theme with that format's grid and slots. `None`, or a format of the deck's own shape, is
/// the deck as it is. A format the deck does not list is an error.
pub fn project<'d>(
    deck: &'d Deck,
    theme: &'d Theme,
    format: Option<&str>,
) -> Result<(Cow<'d, Deck>, Cow<'d, Theme>), EngineError> {
    let Some(name) = format else { return Ok((Cow::Borrowed(deck), Cow::Borrowed(theme))) };
    let listed = || deck.formats.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", ");
    let format = Format::parse(name).ok_or_else(|| {
        let known = Format::ALL.map(|f| format!("`{}`", f.name())).join(", ");
        EngineError::Layout(format!("format `{name}`: expected one of {known}"))
    })?;
    let own = [deck.canvas.width, deck.canvas.height];
    let canvas = format.canvas(own);
    if canvas == own {
        return Ok((Cow::Borrowed(deck), Cow::Borrowed(theme)));
    }
    if !deck.formats.iter().any(|f| f == name) {
        let listed = match deck.formats.is_empty() {
            true => "none".to_string(),
            false => listed(),
        };
        // How to list it, as one JSON Patch op: the list made where the deck has none, else the
        // format added to the end of it.
        let op = match deck.formats.is_empty() {
            true => format!(r#"{{"op": "add", "path": "/formats", "value": ["{name}"]}}"#),
            false => format!(r#"{{"op": "add", "path": "/formats/-", "value": "{name}"}}"#),
        };
        return Err(EngineError::Layout(format!(
            "format `{name}` is not one of the deck's formats ({listed}): list it to lay the deck out there, \
             with the patch `[{op}]`; a layout whose theme gives `{name}` no slots of its own keeps its slots there"
        )));
    }
    let mut projected = deck.clone();
    (projected.canvas.width, projected.canvas.height) = (canvas[0], canvas[1]);
    // Each node's layout in this format takes the place of its own; a state's delta still
    // sets what it sets.
    for node in projected.nodes.values_mut() {
        let there = node.props.get("formats").and_then(|f| f.get(name)).and_then(|l| l.as_object()).cloned();
        for (prop, value) in there.into_iter().flatten() {
            node.props.insert(prop, value);
        }
    }
    Ok((Cow::Owned(projected), Cow::Owned(theme.in_format(format))))
}

#[derive(Debug, Clone)]
pub struct Frame {
    /// In canvas units; `viewport` is the canvas size, and painters scale to their output.
    pub display_list: DisplayList,
    /// The state's span, ms: its transition and every motion of its cue.
    pub duration_ms: f64,
}

/// Each state's `(state, span, hold)` on the global timeline, ms.
type Spans = Vec<(String, f64, f64)>;

/// The engine for one bundle: its fonts and images, plus reusable layout scratch.
pub struct Engine {
    fonts: BundleFonts,
    images: BundleImages,
    text: TextEngine,
    /// The global timeline's states as far as last worked out, and a hash of the deck,
    /// theme, and data they were worked out from.
    timeline: Option<(u64, Spans)>,
    /// Each state's span, ms, by a hash of all it is worked out from ([`span_key`]), where the
    /// engine keeps them ([`Engine::with_timed`]): kept from deck to deck, so a deck an edit
    /// changed in a few states works out only theirs again. An engine made for one deck, as a
    /// render or an export makes one, keeps none: the hashes would cost it and buy nothing.
    spans: Option<HashMap<u64, f64>>,
    /// Lay out what a frame refuses (text under `fit: error` that does not fit, a table
    /// whose rows do not), for lint to report: set only while lint runs.
    pub(crate) lenient: bool,
}

/// What an engine has worked out of the global timeline: each state's span by all it is worked
/// out from ([`Engine::timeline`]). An engine made again from files that read as they did takes
/// it ([`Engine::with_timed`]), and works out again only the states an edit changed.
#[derive(Debug, Clone, Default)]
pub struct Timed(HashMap<u64, f64>);

impl Engine {
    /// What this engine has worked out of the global timeline, for an engine made again from the
    /// same files.
    pub fn timed(&self) -> Timed {
        Timed(self.spans.clone().unwrap_or_default())
    }

    /// This engine, with what an engine made from the same files worked out of the global
    /// timeline ([`Engine::timed`]), keeping each state's span from here on as an editor's does.
    pub fn with_timed(mut self, timed: Timed) -> Self {
        self.spans = Some(timed.0);
        self
    }

    pub fn new(fonts: BundleFonts) -> Self {
        Self {
            fonts,
            images: BundleImages::new(),
            text: TextEngine::new(),
            timeline: None,
            spans: None,
            lenient: false,
        }
    }

    /// The bundle's images, which image nodes name.
    pub fn with_images(mut self, images: BundleImages) -> Self {
        self.images = images;
        self
    }

    /// An image file the bundle has gained, at `path`, so a deck that names it lays out: what
    /// the engine draws now is drawn as before.
    pub fn add_image(&mut self, path: &str, bytes: &[u8]) -> Result<crate::images::ImageInfo, EngineError> {
        self.images.register(path, bytes)
    }

    /// The bundle's fonts, as registered.
    pub fn fonts(&self) -> &BundleFonts {
        &self.fonts
    }

    /// Render one frame. Deterministic: same inputs → identical display list (SPEC §13).
    /// At rest it lays out the state alone; inside its cue it lays out the state and the
    /// one before it, then samples. To draw many frames of one cue, build it once with
    /// [`Engine::transition`]. Shaders keep the global timeline's time
    /// ([`Engine::timeline`]).
    pub fn frame(&mut self, req: &FrameRequest) -> Result<Frame, EngineError> {
        let (deck, theme) = project(req.deck, req.theme, req.format)?;
        let (deck, theme) = (deck.as_ref(), theme.as_ref());
        let snapshots = scaena_core::resolve_states(deck)?;
        let i = state_index(&snapshots, req.state)?;
        let timeline = self.timeline_to(deck, theme, req.data, i)?;
        let slot = &timeline.slots[i];
        let t = req.t_ms;
        let display_list = if t.is_nan() || t >= slot.span {
            // At rest: its shaders at the time it comes to rest, or `t` into its hold.
            let at = if t.is_finite() { t } else { slot.span };
            self.scene(deck, theme, req.data, &snapshots[i])?.draw_at((slot.start + at) / 1000.0)
        } else {
            self.transition_at(deck, theme, req.data, &snapshots, i, slot.start / 1000.0)?.frame(t)
        };
        Ok(Frame { display_list, duration_ms: slot.span })
    }

    /// The cue of `state`: the transition into it from the state before it in the cue
    /// list (what was on screen), each laid out once, and its motions. Its frames
    /// ([`Transition::frame`]) then sample any time without layout.
    pub fn transition(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        data: &DataFiles,
        state: &str,
    ) -> Result<Transition, EngineError> {
        let snapshots = scaena_core::resolve_states(deck)?;
        let i = state_index(&snapshots, state)?;
        let start = self.timeline_to(deck, theme, data, i)?.slots[i].start;
        self.transition_at(deck, theme, data, &snapshots, i, start / 1000.0)
    }

    /// The cue of state `i`, starting `start` seconds into the global timeline.
    fn transition_at(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        data: &DataFiles,
        snapshots: &[Snapshot],
        i: usize,
        start: f64,
    ) -> Result<Transition, EngineError> {
        let state = &deck.states[i];
        let timing = Timing::parse(theme, state.transition.as_ref())?;
        let before = i.checked_sub(1).map(|p| &snapshots[p]);
        let items = motion::items(deck, theme, state, before, &snapshots[i], timing.matched, timing.slide.is_some())?;
        let to = self.scene(deck, theme, data, &snapshots[i])?;
        // What was on screen is drawn while the transition runs, or a motion moves it.
        let from = match before {
            Some(prev) if timing.duration_ms > 0.0 || !items.is_empty() => Some(self.scene(deck, theme, data, prev)?),
            _ => None,
        };
        Transition::new(from, to, timing, &items, start)
    }

    /// The deck's states end to end (SPEC §2.4): each state's span (its transition and
    /// its motions), then its `hold`. A state is laid out only where a motion splits a
    /// node into what layout counts (lines, words, glyphs, a chart's marks); the rest is
    /// timed from the document.
    pub fn timeline(&mut self, deck: &Deck, theme: &Theme, data: &DataFiles) -> Result<Timeline, EngineError> {
        match deck.states.len() {
            0 => Ok(Timeline::default()),
            n => self.timeline_to(deck, theme, data, n - 1),
        }
    }

    /// The global timeline up to and including state `last`: what a frame of it needs,
    /// since a state starts where the ones before it end. Worked out as far as asked and
    /// kept for the deck, theme, and data it was worked out from, so the frames of one
    /// deck work each state out once.
    fn timeline_to(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        data: &DataFiles,
        last: usize,
    ) -> Result<Timeline, EngineError> {
        let key = fingerprint(deck, theme, data);
        let mut states = match self.timeline.take() {
            Some((k, states)) if k == key => states,
            _ => Vec::new(),
        };
        let snapshots = if states.len() <= last { scaena_core::resolve_states(deck)? } else { Vec::new() };
        let keep = self.spans.is_some() && states.len() <= last;
        let drawn = if keep { drawn_from(deck, theme, data) } else { 0 };
        if let Some(spans) = &mut self.spans
            && spans.len() > SPANS_KEPT
        {
            spans.clear();
        }
        for (i, state) in deck.states.iter().enumerate().take(last + 1).skip(states.len()) {
            let key = keep.then(|| span_key(drawn, deck, &snapshots, i));
            if let (Some(key), Some(spans)) = (key, &self.spans)
                && let Some(&span) = spans.get(&key)
            {
                states.push((state.id.clone(), span, state.hold.unwrap_or(0.0)));
                continue;
            }
            let timing = Timing::parse(theme, state.transition.as_ref())?;
            let before = i.checked_sub(1).map(|p| &snapshots[p]);
            let items =
                motion::items(deck, theme, state, before, &snapshots[i], timing.matched, timing.slide.is_some())?;
            let span = if motion::counted_by_layout(&items) {
                self.transition_at(deck, theme, data, &snapshots, i, 0.0)?.span_ms()
            } else {
                // Only a container's children are counted, and the snapshot has them.
                let mut units = |id: &str, _: SplitUnit, m: &Motion| {
                    let snap = if matches!(m, Motion::Exit(_)) { before } else { Some(&snapshots[i]) };
                    snap.map_or(0, |s| children(deck, s, id).len())
                };
                scaena_core::timeline::schedule(timing.clock(), &items, &mut units).span
            };
            if let (Some(key), Some(spans)) = (key, &mut self.spans) {
                spans.insert(key, span);
            }
            states.push((state.id.clone(), span, state.hold.unwrap_or(0.0)));
        }
        let timeline = Timeline::new(states.iter().take(last + 1).cloned());
        self.timeline = Some((key, states));
        Ok(timeline)
    }

    /// `req.state` at rest, laid out in `req.format` (`req.t_ms` aside): the scene its frames
    /// at rest draw, which says what stands where for a client that edits by pointing
    /// ([`crate::geometry`], ADR-0013).
    pub fn at_rest(&mut self, req: &FrameRequest) -> Result<Scene, EngineError> {
        let (deck, theme) = project(req.deck, req.theme, req.format)?;
        let (deck, theme) = (deck.as_ref(), theme.as_ref());
        let snapshots = scaena_core::resolve_states(deck)?;
        let snap = &snapshots[state_index(&snapshots, req.state)?];
        self.scene(deck, theme, req.data, snap)
    }

    /// Where `node` may go in `req.state` at rest, laid out in `req.format` (`req.t_ms`
    /// aside): the theme's grid and slots there, or its container's cells, order, or box
    /// ([`crate::geometry::Targets`], ADR-0013).
    pub fn targets(&mut self, req: &FrameRequest, node: &str) -> Result<crate::geometry::Targets, EngineError> {
        let (deck, theme) = project(req.deck, req.theme, req.format)?;
        let (deck, theme) = (deck.as_ref(), theme.as_ref());
        let snapshots = scaena_core::resolve_states(deck)?;
        let snap = &snapshots[state_index(&snapshots, req.state)?];
        let scene = self.scene(deck, theme, req.data, snap)?;
        let mut t = crate::geometry::targets(deck, theme, &cascade::with_overrides(deck, snap), &scene, node, None)?;
        t.format = req.format.map(String::from);
        Ok(t)
    }

    /// Where `node` may go in `req.state` at rest `into` another container, or onto the
    /// canvas for `None` (PLAN 2.50): as [`Engine::targets`] says it for a node that container
    /// holds, the node's cell the box it stands in now.
    pub fn targets_into(
        &mut self,
        req: &FrameRequest,
        node: &str,
        into: Option<&str>,
    ) -> Result<crate::geometry::Targets, EngineError> {
        let (deck, theme) = project(req.deck, req.theme, req.format)?;
        let (deck, theme) = (deck.as_ref(), theme.as_ref());
        let snapshots = scaena_core::resolve_states(deck)?;
        let snap = &snapshots[state_index(&snapshots, req.state)?];
        let scene = self.scene(deck, theme, req.data, snap)?;
        let mut t =
            crate::geometry::targets(deck, theme, &cascade::with_overrides(deck, snap), &scene, node, Some(into))?;
        t.format = req.format.map(String::from);
        Ok(t)
    }

    /// Where text `node` goes in `req.state` at rest, laid out in `req.format`, to hold its lines
    /// where they run past the foot of its box (PLAN 2.32), as a presentation app's text box grows
    /// as it is typed in: on the theme's grid, its rows grown down to the first that holds them, or
    /// the grid's last. A text that wraps or grows grows so for its lines as set. Code grows first
    /// and shrinks after (ADR-0029): to hold its lines at the largest size its box's width lets it,
    /// its own at most, by its rows on the grid, or, placed by a `rect`, by its height, no further
    /// than the grid's margin. `None` for a text that its box holds, one placed otherwise (in a
    /// slot, in a container), and one whose fit is none of those.
    pub fn grown(&mut self, req: &FrameRequest, node: &str) -> Result<Option<crate::geometry::Target>, EngineError> {
        let (deck, theme) = project(req.deck, req.theme, req.format)?;
        let (deck, theme) = (deck.as_ref(), theme.as_ref());
        let snapshots = scaena_core::resolve_states(deck)?;
        let snap = &snapshots[state_index(&snapshots, req.state)?];
        let Some(props) = snap.nodes.get(node) else { return Ok(None) };
        let placed_by = |key: &str| props.get("at").is_some_and(|at| at.get(key).is_some());
        let by_cells = placed_by("row") && !["in", "rect", "parent"].iter().any(|k| placed_by(k));
        let by_rect = placed_by("rect") && !placed_by("parent");
        if !by_cells && !by_rect {
            return Ok(None);
        }
        let scene = self.scene(deck, theme, req.data, snap)?;
        let placed = scene.nodes.iter().find(|n| n.id == node).and_then(|n| match &n.content {
            Content::Text(placed) => Some(placed),
            _ => None,
        });
        let Some(placed) = placed else { return Ok(None) };
        // What it needs: a text that wraps or grows its lines; code its lines at the largest size
        // its width lets it, which growing down does not change, and no smaller than it shrinks to.
        let need = match (placed.natural, placed.fit) {
            (Some([wide, tall]), TextFit::Shrink) => {
                let across = if wide > 0.0 { (placed.cell[2] / wide).min(1.0) } else { 1.0 };
                tall * across.max(placed.scale)
            }
            (None, TextFit::Wrap | TextFit::Grow) if by_cells => placed.text.height,
            _ => return Ok(None),
        };
        if need <= placed.cell[3] + 0.5 {
            return Ok(None);
        }
        let mut t = crate::geometry::targets(deck, theme, &cascade::with_overrides(deck, snap), &scene, node, None)?;
        t.format = req.format.map(String::from);
        let [x, y, w, h] = t.cell;
        let Some(last) = t.rows.last().copied() else { return Ok(None) };
        if by_rect {
            let tall = need.min(last[1] - y).ceil();
            return Ok((tall > h + 0.5).then(|| t.snap(crate::geometry::Snap::Free, [x, y, w, tall])).flatten());
        }
        let foot = t.rows.iter().find(|r| r[1] >= y + need - 0.5).unwrap_or(&last)[1];
        if foot - y <= h + 0.5 {
            return Ok(None);
        }
        Ok(t.snap(crate::geometry::Snap::Resize, [x, y, w, foot - y]))
    }

    /// Where a node new to `req.state`, named `node`, would go at the root, laid out in
    /// `req.format` (PLAN 2.34): the theme's grid and the template's slots there, its cell
    /// `share` of the canvas's width and height ([`crate::geometry::room`]).
    pub fn room(
        &mut self,
        req: &FrameRequest,
        node: &str,
        share: [f32; 2],
    ) -> Result<crate::geometry::Targets, EngineError> {
        let (deck, theme) = project(req.deck, req.theme, req.format)?;
        let (deck, theme) = (deck.as_ref(), theme.as_ref());
        let snapshots = scaena_core::resolve_states(deck)?;
        let snap = &snapshots[state_index(&snapshots, req.state)?];
        let scene = self.scene(deck, theme, req.data, snap)?;
        let size = [share[0] * scene.canvas[0], share[1] * scene.canvas[1]];
        crate::geometry::room(theme, &cascade::with_overrides(deck, snap), &scene, node, size)
    }

    /// One snapshot, laid out: every visible node in paint order.
    pub fn scene(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        data: &DataFiles,
        snap: &Snapshot,
    ) -> Result<Scene, EngineError> {
        let canvas = canvas(deck);
        let grid = Grid::from_theme(theme, canvas)?.mirrored(scaena_core::direction::mirrored(deck));
        let snap = &cascade::with_overrides(deck, snap);
        let mut placement = self.place(deck, theme, data, &grid, snap)?;
        let mut tree = tree(deck, snap, &placement)?;
        rejoin(theme, snap, &mut placement, &mut tree);
        let mut nodes = Vec::with_capacity(snap.nodes.len());
        // Every state's props, read once, for what each chart colors across the deck.
        let mut every: Option<Vec<Snapshot>> = None;
        for (id, paint) in &placement.order {
            let id = id.as_str();
            let props = &snap.nodes[id];
            let Some(&rect) = placement.boxes.get(id) else { continue };
            let content = match deck.nodes[id].node_type {
                NodeType::Text => Content::Text(self.layout_text_node(deck, theme, grid.baseline, snap, id, rect)?),
                NodeType::Chart => {
                    let foot = self.foot(deck, theme, props, rect).map_err(|e| in_node(id, e))?;
                    let states = match &mut every {
                        Some(states) => states,
                        none => none.insert(scaena_core::resolve_states(deck)?),
                    };
                    let mut colors: Vec<String> = Vec::new();
                    for state in states.iter() {
                        let state = cascade::with_overrides(deck, state);
                        for key in state.nodes.get(id).map(|p| charts::color_keys(deck, data, p)).unwrap_or_default() {
                            if !colors.contains(&key) {
                                colors.push(key);
                            }
                        }
                    }
                    let mut cx = Ctx {
                        text: &mut self.text,
                        fonts: &mut self.fonts,
                        theme,
                        deck,
                        data,
                        colors: &colors,
                        lenient: self.lenient,
                    };
                    let tall = foot.as_ref().map_or(0.0, |f| f.1);
                    let mut chart =
                        charts::compile(&mut cx, props, [rect[2], rect[3] - tall]).map_err(|e| in_node(id, e))?;
                    chart.credit = foot.map(|f| f.0);
                    Content::Chart { cell: rect, chart: Box::new(chart) }
                }
                NodeType::Table => {
                    let foot = self.foot(deck, theme, props, rect).map_err(|e| in_node(id, e))?;
                    let tall = foot.as_ref().map_or(0.0, |f| f.1);
                    let lenient = self.lenient;
                    let mut cx =
                        Ctx { text: &mut self.text, fonts: &mut self.fonts, theme, deck, data, colors: &[], lenient };
                    let mut table =
                        tables::compile(&mut cx, props, [rect[2], rect[3] - tall]).map_err(|e| in_node(id, e))?;
                    table.credit = foot.map(|f| f.0);
                    Content::Table { cell: rect, table: Box::new(table) }
                }
                NodeType::Shader => {
                    Content::Shader(ShaderNode::resolve(props, theme, rect).map_err(|e| in_node(id, e))?)
                }
                NodeType::Shape => {
                    // A connector runs where placement routed it (PLAN 2.109).
                    let routed = placement.joined.get(id).map(|points| {
                        let mut props = props.clone();
                        props.insert("points".into(), serde_json::json!(points));
                        props
                    });
                    let props = routed.as_ref().unwrap_or(props);
                    Content::Shape(ShapeNode::resolve(props, theme, rect).map_err(|e| in_node(id, e))?)
                }
                NodeType::Image => {
                    let image = ImageNode::resolve(props, theme, &self.images, rect).map_err(|e| in_node(id, e))?;
                    Content::Image(self.mark_image(deck, theme, props, image).map_err(|e| in_node(id, e))?)
                }
                // A container draws its `fill` and `stroke` under its children, if it has them.
                NodeType::Stack | NodeType::Grid | NodeType::Frame => {
                    match container_panel(props, theme, rect).map_err(|e| in_node(id, e))? {
                        Some(panel) => Content::Shape(panel),
                        None => continue,
                    }
                }
                NodeType::Group => continue,
            };
            nodes.push(SceneNode {
                id: id.to_string(),
                paint: paint.clone(),
                opacity: placement.opacity(snap, id),
                policy: Policy::parse(props.get("transition")).map_err(|e| in_node(id, e))?,
                content,
            });
        }
        let surface = theme_color(theme, "surface")?;
        let furniture = self.furniture(deck, theme, &grid, snap)?;
        // What each connector joins, which a drag of either end runs it between again (PLAN 2.109).
        let joins = (placement.joined.keys())
            .filter_map(|id| {
                Some((id.clone(), crate::shapes::Joint::of(snap.nodes.get(id.as_str())?, theme, &snap.nodes)?))
            })
            .collect();
        Ok(Scene {
            state: snap.state_id.clone(),
            canvas,
            surface,
            nodes,
            tree,
            tracks: placement.tracks,
            furniture,
            joins,
            mirrored: grid.is_mirrored(),
        })
    }

    /// The credit at the foot of a chart or a table whose data was fetched from a service (ADR-0030,
    /// decision 6), laid out across its box `rect` in the `caption` role, never under 24 cu where
    /// the canvas's shorter side is 1080, each run linked where it links; and how much of the box's
    /// foot it keeps, the gap above it with it. None where its data was fetched from no service.
    fn foot(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        props: &Props,
        rect: Rect,
    ) -> Result<Option<(charts::Foot, f32)>, EngineError> {
        let Some(runs) = scaena_core::fetched::reads(deck, props) else { return Ok(None) };
        let runs: Vec<Value> = runs
            .into_iter()
            .map(|r| match r.link {
                Some(href) => serde_json::json!({ "text": kept(&r.text), "link": { "href": href } }),
                None => serde_json::json!({ "text": kept(&r.text) }),
            })
            .collect();
        if runs.is_empty() {
            return Ok(None);
        }
        let [cw, ch] = canvas(deck);
        let k = cw.min(ch) / 1080.0;
        let mut words = Props::new();
        words.insert("type".into(), "text".into());
        words.insert("runs".into(), Value::Array(runs));
        words.insert("role".into(), "caption".into());
        if cascade::node_role(theme, &words, None)?.size < MIN_CREDIT * k {
            words.insert("style".into(), serde_json::json!({ "size": MIN_CREDIT * k }));
        }
        let spec = text_spec(deck, theme, &words, None)?;
        let gap = (0.5 * spec.role.size).round();
        let text = self.text.layout(&mut self.fonts, theme, &spec, rect[2].max(1.0))?;
        let tall = (text.height + gap).min(rect[3]);
        let origin = [0.0, rect[3] - text.height];
        Ok(Some((charts::Foot { origin, text }, tall)))
    }

    /// What the engine draws over a picture (ADR-0030, decisions 13–14), laid out with it: the
    /// credit its `fetched` record asks, in the `caption` role, never under 24 cu where the canvas's
    /// shorter side is 1080, on a card in the bottom-left corner of what shows, or below the
    /// picture inside its box; and its `pin`, Phosphor's `map-pin` 48 cu tall with its tip on the
    /// point it marks, its label in the `label` role on a card beside it, inside what shows and
    /// clear of the credit. A pin outside what shows draws nothing.
    fn mark_image(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        props: &Props,
        mut image: ImageNode,
    ) -> Result<ImageNode, EngineError> {
        use crate::images::{Card, Marks, PinMark};
        use scaena_core::fetched::{CreditPlace, Fetched, Pin};
        let fetched = match props.get("fetched") {
            Some(v) => Some(
                serde_json::from_value::<Fetched>(v.clone())
                    .map_err(|e| EngineError::Layout(format!("image `fetched`: {e}")))?,
            ),
            None => None,
        };
        let pin = match props.get("pin") {
            Some(v) => Some(
                serde_json::from_value::<Pin>(v.clone())
                    .map_err(|e| EngineError::Layout(format!("image `pin`: {e}")))?,
            ),
            None => None,
        };
        let place = match props.get("credit") {
            Some(v) => serde_json::from_value::<CreditPlace>(v.clone())
                .map_err(|_| EngineError::Layout(format!("image `credit` {v}: inside or below")))?,
            None => CreditPlace::Inside,
        };
        let credit: Vec<Value> = fetched
            .iter()
            .flat_map(|f| &f.credit)
            .filter(|r| !r.text.is_empty())
            .map(|r| match &r.link {
                Some(href) => serde_json::json!({ "text": kept(&r.text), "link": { "href": href } }),
                None => serde_json::json!({ "text": kept(&r.text) }),
            })
            .collect();
        if credit.is_empty() && pin.is_none() {
            return Ok(image);
        }
        // Sizes at 1080p, in proportion on another canvas.
        let [cw, ch] = canvas(deck);
        let k = cw.min(ch) / 1080.0;
        let surface = theme_color(theme, "surface")?;
        let mut marks = Marks::default();
        let [_, _, w, h] = image.rect;
        if !credit.is_empty() {
            let mut words = Props::new();
            words.insert("type".into(), "text".into());
            words.insert("runs".into(), Value::Array(credit));
            words.insert("role".into(), "caption".into());
            let size = cascade::node_role(theme, &words, None)?.size;
            if size < MIN_CREDIT * k {
                words.insert("style".into(), serde_json::json!({ "size": MIN_CREDIT * k }));
            }
            let spec = text_spec(deck, theme, &words, None)?;
            let size = spec.role.size;
            match place {
                CreditPlace::Below => {
                    let text = self.text.layout(&mut self.fonts, theme, &spec, w)?;
                    let gap = (0.5 * size).round();
                    let tall = (text.height + gap).min(h);
                    let y = h - text.height;
                    image = image.keeping(tall);
                    marks.credit = Some(Card {
                        rect: [0.0, y, text.width, text.height],
                        fill: None,
                        radius: 0.0,
                        origin: [0.0, y],
                        text,
                    });
                }
                CreditPlace::Inside => {
                    let [dx, dy, dw, dh] = image.shown();
                    let pad = (0.4 * size).round();
                    // Clear of the credit the service writes in the bottom right.
                    let room = (dw * CREDIT_SHARE - 2.0 * pad).max(size);
                    let text = self.text.layout(&mut self.fonts, theme, &spec, room)?;
                    let (cw, chh) = (text.width + 2.0 * pad, text.height + 2.0 * pad);
                    let rect = [dx, dy + dh - chh, cw, chh];
                    marks.credit = Some(Card {
                        rect,
                        fill: Some(surface),
                        radius: pad / 2.0,
                        origin: [rect[0] + pad, rect[1] + pad],
                        text,
                    });
                }
            }
        }
        if let Some(pin) = pin
            && let Some(tip) = image.spot([pin.at[0] as f32, pin.at[1] as f32])
        {
            let color = theme_color(theme, pin.color.as_deref().unwrap_or("accent"))?;
            let mark = PinMark { tip, height: PIN_HEIGHT * k, color, ring: surface };
            if let Some(label) = pin.label.as_deref().filter(|l| !l.trim().is_empty()) {
                let mut words = Props::new();
                words.insert("type".into(), "text".into());
                words.insert("text".into(), label.into());
                words.insert("role".into(), "label".into());
                let spec = text_spec(deck, theme, &words, None)?;
                let size = spec.role.size;
                let shown = image.shown();
                let pad = (0.4 * size).round();
                let text = self.text.layout(&mut self.fonts, theme, &spec, (shown[2] - 2.0 * pad).max(size))?;
                let (lw, lh) = (text.width + 2.0 * pad, text.height + 2.0 * pad);
                let [hx, hy] = mark.head();
                let gap = mark.width() / 2.0 + pad / 2.0;
                let inside = |r: Rect| {
                    r[0] >= shown[0] - 0.5
                        && r[1] >= shown[1] - 0.5
                        && r[0] + r[2] <= shown[0] + shown[2] + 0.5
                        && r[1] + r[3] <= shown[1] + shown[3] + 0.5
                };
                let clear = |r: Rect| {
                    marks.credit.as_ref().is_none_or(|c| {
                        let c = c.rect;
                        r[0] >= c[0] + c[2] || c[0] >= r[0] + r[2] || r[1] >= c[1] + c[3] || c[1] >= r[1] + r[3]
                    })
                };
                // Beside the pin's head: to its right where it fits, else to its left.
                let y = (hy - lh / 2.0).clamp(shown[1], (shown[1] + shown[3] - lh).max(shown[1]));
                let card =
                    [[hx + gap, y, lw, lh], [hx - gap - lw, y, lw, lh]].into_iter().find(|&r| inside(r) && clear(r));
                marks.label = card.map(|rect| Card {
                    rect,
                    fill: Some(surface),
                    radius: pad / 2.0,
                    origin: [rect[0] + pad, rect[1] + pad],
                    text,
                });
            }
            marks.pin = Some(mark);
        }
        image.marks = Box::new(marks);
        Ok(image)
    }

    /// The slide's number and footer in `snap`, laid out where the theme places them (ADR-0028):
    /// none where the deck shows neither, its theme places neither, or the slide is one that
    /// hides them, by its own word or as the theme leaves them off its layout.
    fn furniture(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        grid: &Grid,
        snap: &Snapshot,
    ) -> Result<Vec<SceneNode>, EngineError> {
        let (Some(wants), Some(places)) = (deck.furniture.as_ref(), theme.furniture.as_ref()) else {
            return Ok(Vec::new());
        };
        let Some(slide) = crate::furniture::slide(deck, &snap.state_id) else { return Ok(Vec::new()) };
        if !slide.shown(places, snap.layout.as_deref()) {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        if wants.number
            && let Some(item) = &places.number
        {
            let words = slide.written(item.pattern.as_deref());
            out.push(self.furnished(deck, theme, grid, crate::furniture::NUMBER, item, &words)?);
        }
        if let (Some(words), Some(item)) = (wants.footer.as_deref(), &places.footer) {
            out.push(self.furnished(deck, theme, grid, crate::furniture::FOOTER, item, words)?);
        }
        Ok(out)
    }

    /// One piece of furniture: `words` set in its role across its columns, its caps centered in
    /// the margin at the canvas's foot or head.
    fn furnished(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        grid: &Grid,
        id: &str,
        item: &scaena_core::model::theme::FurnitureItem,
        words: &str,
    ) -> Result<SceneNode, EngineError> {
        use scaena_core::model::theme::{Across, Edge};
        use scaena_core::model::values::Range;
        let columns = grid.columns();
        let last = columns.len().max(1) as u32;
        let (a, b) = match item.col {
            Range::Index(i) => (i, i),
            Range::Span([a, b]) => (a, b),
        };
        // Held to the grid of the format laid out, which may have fewer columns.
        let a = a.clamp(1, last);
        let b = b.clamp(a, last);
        // Its first column and its last, which stand right to left in a deck that reads so.
        let (first, last) = (columns[(a - 1) as usize], columns[(b - 1) as usize]);
        let (x0, x1) = (first[0].min(last[0]), first[1].max(last[1]));
        let size = canvas(deck);
        let rows = grid.rows();
        let (y0, y1) = match item.edge {
            Edge::Foot => (rows.last().map_or(size[1], |r| r[1]), size[1]),
            Edge::Head => (0.0, rows.first().map_or(0.0, |r| r[0])),
        };
        let cell = [x0, y0, x1 - x0, y1 - y0];
        let mut props = Props::new();
        props.insert("type".into(), "text".into());
        props.insert("text".into(), words.into());
        props.insert("role".into(), item.role.clone().into());
        if let Some(color) = &item.color {
            props.insert("style".into(), serde_json::json!({ "color": color }));
        }
        let mut spec = text_spec(deck, theme, &props, None).map_err(|e| in_node(id, e))?;
        spec.align = match item.align {
            Some(Across::Center) => TextAlign::Center,
            Some(Across::End) => TextAlign::End,
            _ => TextAlign::Start,
        };
        let text = self.text.layout(&mut self.fonts, theme, &spec, cell[2]).map_err(|e| in_node(id, e))?;
        // Its caps centered in the margin's height.
        let (top, bottom) = text.trimmed(TextBox::Cap);
        let y = cell[1] + 0.5 * (cell[3] - (bottom - top)) - top;
        let overflow = text.lines.iter().any(|l| l.width > cell[2] + FIT_EPSILON);
        let placed = PlacedText {
            cell,
            origin: [cell[0], y],
            text,
            scale: 1.0,
            overflow,
            clip: None,
            fit: TextFit::Wrap,
            natural: None,
            code: None,
        };
        Ok(SceneNode {
            id: id.into(),
            paint: Vec::new(),
            opacity: 1.0,
            policy: Policy::Crossfade,
            content: Content::Text(placed),
        })
    }

    /// One text node of a state, laid out and placed: what `frame` draws, and what
    /// layout-level lints (E100 overflow, W200 widows) read.
    pub fn text_layout(&mut self, req: &FrameRequest, node: &str) -> Result<PlacedText, EngineError> {
        Ok(self.text_layouts(req, &[node])?.remove(0))
    }

    /// Text nodes of a state, each laid out and placed as [`Engine::text_layout`] places one,
    /// the state resolved and placed once: what sets several texts alike reads.
    pub fn text_layouts(&mut self, req: &FrameRequest, nodes: &[&str]) -> Result<Vec<PlacedText>, EngineError> {
        let (deck, theme) = project(req.deck, req.theme, req.format)?;
        let (deck, theme) = (deck.as_ref(), theme.as_ref());
        let snapshots = scaena_core::resolve_states(deck)?;
        let snap = &cascade::with_overrides(deck, &snapshots[state_index(&snapshots, req.state)?]);
        if let Some(node) = nodes.iter().find(|n| !snap.nodes.contains_key(**n)) {
            return Err(EngineError::Layout(format!("node `{node}` is not visible in state `{}`", req.state)));
        }
        let grid = Grid::from_theme(theme, canvas(deck))?.mirrored(scaena_core::direction::mirrored(deck));
        let placement = self.place(deck, theme, req.data, &grid, snap)?;
        let mut placed = Vec::with_capacity(nodes.len());
        for node in nodes {
            placed.push(self.layout_text_node(deck, theme, grid.baseline, snap, node, placement.boxes[*node])?);
        }
        Ok(placed)
    }

    /// Every node's box in `snap` (overrides merged), its container, and paint order:
    /// roots on the theme grid, containers' children through `taffy`, text and tables
    /// measured here.
    fn place(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        data: &DataFiles,
        grid: &Grid,
        snap: &Snapshot,
    ) -> Result<Placement, EngineError> {
        let (text, fonts, lenient) = (&mut self.text, &mut self.fonts, self.lenient);
        let mut specs: HashMap<String, (TextSpec, TextBox)> = HashMap::new();
        // A table's size as content, set once whatever taffy asks: its columns across, its
        // rows down, and whether it spans its cell. Its rows are set only until they pass
        // the canvas, which no cell of it outreaches (STAB-4).
        let mut tables: HashMap<String, ([f32; 2], bool)> = HashMap::new();
        let canvas = grid.canvas()[1];
        let mut measure = |id: &str, known: taffy::Size<Option<f32>>, available: taffy::Size<taffy::AvailableSpace>| {
            let props = &snap.nodes[id];
            if deck.nodes[id].node_type == NodeType::Table {
                if !tables.contains_key(id) {
                    let mut cx = Ctx { text: &mut *text, fonts: &mut *fonts, theme, deck, data, colors: &[], lenient };
                    let set = tables::set(&mut cx, props, Some(canvas)).map_err(|e| in_node(id, e))?;
                    tables.insert(id.to_string(), ([set.width(), set.height()], set.stretches()));
                }
                let (content, stretch) = tables[id];
                return Ok(measure_table(content, stretch, known, available));
            }
            if !specs.contains_key(id) {
                let spec = text_spec(deck, theme, props, None).map_err(|e| in_node(id, e))?;
                let trim = typed_prop::<TextBox>(props, "box")?.unwrap_or(spec.role.text_box);
                specs.insert(id.to_string(), (spec, trim));
            }
            let (spec, trim) = &specs[id];
            measure_text(text, fonts, theme, spec, *trim, known, available).map_err(|e| in_node(id, e))
        };
        containers::place(deck, theme, grid, &self.images, snap, &mut measure)
    }

    fn layout_text_node(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        lines: Option<BaselineGrid>,
        snap: &Snapshot,
        id: &str,
        cell: Rect,
    ) -> Result<PlacedText, EngineError> {
        let props = &snap.nodes[id];
        let template = snap.layout.as_deref();
        let at = props.get("at");
        let mut spec = text_spec(deck, theme, props, Grid::slot_role(theme, template, at).as_deref())
            .map_err(|e| in_node(id, e))?;
        // Words in a shape (PLAN 2.108) stand in its middle and shrink to fit it, unless they say
        // otherwise, off the theme's baseline grid, which the shape is not on.
        let in_shape = props
            .get("at")
            .and_then(|at| at.get("parent"))
            .and_then(Value::as_str)
            .is_some_and(|p| deck.nodes.get(p).is_some_and(|n| n.node_type == NodeType::Shape));
        let lines = if in_shape { None } else { lines };
        let start = if in_shape { (AlignX::Center, AlignY::Center) } else { (AlignX::Start, AlignY::Start) };
        let (align_x, align_y) = Grid::alignment_from(theme, template, props, start).map_err(|e| in_node(id, e))?;
        spec.align = match align_x {
            AlignX::Start | AlignX::Stretch => TextAlign::Start,
            AlignX::Center => TextAlign::Center,
            AlignX::End => TextAlign::End,
        };
        let trim = match typed_prop::<TextBox>(props, "box")? {
            Some(trim) => trim,
            None => spec.role.text_box,
        };
        // One fit (SPEC §3.4, ADR-0029): the text's own, else its slot's (theme format 0.13), else
        // a shape's words and code shrink, else it wraps. Lint and the editors read it as set.
        let code = spec.code.is_some();
        // Verse, any text that wraps `none`, breaks only where it is written to (PLAN O.4b).
        let unbroken = !code && spec.wrap.unwrap_or(spec.role.wrap) == Wrap::None;
        let slot_fit = Grid::slot(theme, template, props.get("at")).and_then(|slot| slot.fit);
        let fit = TextFit::effective(typed_prop::<TextFit>(props, "fit")?, slot_fit, in_shape, code);
        let number = |key: &str| props.get(key).and_then(Value::as_f64).map(|v| v as f32);
        let max_lines = props
            .get("maxLines")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .or(spec.role.max_lines.map(|n| n as usize));
        let fits_box = |t: &TextLayout| {
            let (top, bottom) = t.trimmed(trim);
            bottom - top <= cell[3] + FIT_EPSILON && t.lines.iter().all(|l| l.width <= cell[2] + FIT_EPSILON)
        };
        // Code's lines are its own at any size, and verse's: a size fits its box or it does not.
        let fits = |t: &TextLayout| fits_box(t) && (code || unbroken || max_lines.is_none_or(|m| t.lines.len() <= m));
        let mut lay = |scale: f32| -> Result<TextLayout, EngineError> {
            self.text.layout(&mut self.fonts, theme, &spec.scaled(scale), cell[2]).map_err(|e| in_node(id, e))
        };
        let base = lay(1.0)?;
        // Code's size at its own, which its box grows to hold first (`Engine::grown`).
        let natural = code.then(|| {
            let (top, bottom) = base.trimmed(trim);
            [base.width, bottom - top]
        });
        // A shrinking text's own `maxSize`, where it is under its size, is the most it is set at
        // (deck format 0.36): a set of texts side by side held to one size. A role's is not.
        let cap = match fit {
            TextFit::Shrink => number("maxSize").map_or(1.0, |m| (m / spec.role.size).min(1.0)),
            _ => f32::INFINITY,
        };
        let (scale, text) = match fit {
            // Code shrinks no further than its `minSize`, else its role's, else 12 pt at
            // presentation size, and never above its own size; worked out, since no line of it
            // breaks again at another size.
            TextFit::Shrink if code && !fits(&base) => {
                let floor = spec.code.as_ref().map_or(1.0, |c| c.least);
                worked_out(&mut lay, &fits, &base, [cell[2], cell[3]], trim, floor)?
            }
            // Verse shrinks whole where a line runs too wide, worked out as code is, since no
            // line of it breaks again at another size, to its `minSize`, else its role's: W203.
            TextFit::Shrink if unbroken && !fits(&base) => {
                let floor = number("minSize").or(spec.role.min_size).map_or(0.5, |m| m / spec.role.size).min(1.0);
                worked_out(&mut lay, &fits, &base, [cell[2], cell[3]], trim, floor)?
            }
            TextFit::Shrink if !fits(&base) || cap < 1.0 => {
                let floor = number("minSize").or(spec.role.min_size).map_or(0.5, |m| m / spec.role.size).min(1.0);
                largest_fit(&mut lay, &fits, floor.min(cap), cap)?
            }
            TextFit::Grow if fits(&base) => {
                let ceiling = number("maxSize").or(spec.role.max_size).map_or(2.0, |m| m / spec.role.size).max(1.0);
                largest_fit(&mut lay, &fits, 1.0, ceiling)?
            }
            TextFit::Error if !fits_box(&base) && !self.lenient => {
                let (top, bottom) = base.trimmed(trim);
                return Err(EngineError::Layout(format!(
                    "node `{id}`: its text needs {:.0} × {:.0} cu and its box is {:.0} × {:.0} (`fit: error`)",
                    base.width,
                    bottom - top,
                    cell[2],
                    cell[3]
                )));
            }
            _ => (1.0, base),
        };
        // Code and verse, worked out from one layout, held under the cap the same way.
        let (scale, text) = if scale > cap { (cap, lay(cap)?) } else { (scale, text) };
        let overflow = !fits_box(&text);
        let clip = (fit == TextFit::Clip).then_some(cell);
        let top = text_top(cell, align_y, &text, trim);
        let origin = [cell[0], top + to_grid(lines, spec.role.snap, align_y, top, &text)];
        // The paragraphs a state draws (PLAN 2.129): laid out whole, so each keeps its place.
        let text = match props.get("paragraphs") {
            Some(drawn) => {
                let drawn = scaena_core::lists::drawn(Some(drawn), text.paragraph_count());
                text.keeping(|p| drawn.contains(&p))
            }
            None => text,
        };
        let code = spec.code.as_ref().map(|c| crate::carets::CaretCode {
            tab: c.tab,
            from: (props.get("code").and_then(|c| c.get("start")).and_then(Value::as_u64)).map_or(1, |n| n as usize),
            language: c.language.clone(),
        });
        Ok(PlacedText { cell, origin, text, scale, overflow, clip, fit, natural, code })
    }
}

/// Room for float error when text is held to its box, canvas units.
const FIT_EPSILON: f32 = 1.0 / 64.0;

/// How far a text whose role snaps moves onto the baseline grid (SPEC §3.4), from where
/// its alignment put its top: its first baseline, or its first line's cap height (the line
/// top in a font without one), to the next grid line down; or, aligned to the foot of its
/// box (`end`, `baseline`), to the line above. Texts aligned to one line move together.
/// Its lines are already whole grid lines apart.
fn to_grid(lines: Option<BaselineGrid>, snap: Option<Snap>, align: AlignY, top: f32, text: &TextLayout) -> f32 {
    let (Some(lines), Some(snap), Some(first)) = (lines, snap, text.lines.first()) else { return 0.0 };
    let anchor = top
        + match snap {
            Snap::Baseline => first.baseline,
            Snap::Cap => text.trimmed(TextBox::Cap).0,
        };
    let down = lines.next(anchor) - anchor;
    let on = down.abs() <= GRID_EPSILON * lines.pitch;
    match align {
        AlignY::End | AlignY::Baseline if !on => down - lines.pitch,
        _ => down,
    }
}

/// Bisection steps for `fit: shrink` and `grow`: the scale is within `(hi - lo) / 2^12`
/// of the largest that fits.
const FIT_STEPS: u32 = 12;

/// How far a code text's worked-out scale steps down while it still does not fit, as a fraction
/// of its own size (ADR-0029).
const CODE_STEP: f32 = 1.0 / 64.0;

/// The scale at which a code text, or verse (PLAN O.4b), fits `room`, `[width, height]`, and its layout there, worked out
/// from `base`, its layout at its own size: no line of it breaks again at another size, so its
/// width and its height go as its size. The least of the room over each, then down by
/// [`CODE_STEP`] while it does not fit (a font whose shapes change with its size, by its `opsz`,
/// can set it a hair wider), to no less than `floor`, where it may not fit: W203.
fn worked_out(
    lay: &mut impl FnMut(f32) -> Result<TextLayout, EngineError>,
    fits: &impl Fn(&TextLayout) -> bool,
    base: &TextLayout,
    room: [f32; 2],
    trim: TextBox,
    floor: f32,
) -> Result<(f32, TextLayout), EngineError> {
    let (top, bottom) = base.trimmed(trim);
    let ratio = |room: f32, used: f32| if used > 0.0 { room / used } else { 1.0 };
    let mut scale = ratio(room[0], base.width).min(ratio(room[1], bottom - top)).min(1.0).max(floor);
    let mut laid = lay(scale)?;
    while !fits(&laid) && scale > floor {
        scale = (scale - CODE_STEP).max(floor);
        laid = lay(scale)?;
    }
    Ok((scale, laid))
}

/// The largest scale in `lo..=hi` at which the text fits, by bisection, and its layout.
/// `lo` is used when nothing fits; `hi` when everything does.
fn largest_fit(
    lay: &mut impl FnMut(f32) -> Result<TextLayout, EngineError>,
    fits: &impl Fn(&TextLayout) -> bool,
    lo: f32,
    hi: f32,
) -> Result<(f32, TextLayout), EngineError> {
    let top = lay(hi)?;
    if fits(&top) {
        return Ok((hi, top));
    }
    let bottom = lay(lo)?;
    if !fits(&bottom) {
        return Ok((lo, bottom));
    }
    let (mut lo, mut hi, mut best) = (lo, hi, bottom);
    for _ in 0..FIT_STEPS {
        let mid = 0.5 * (lo + hi);
        let laid = lay(mid)?;
        if fits(&laid) {
            (lo, best) = (mid, laid);
        } else {
            hi = mid;
        }
    }
    Ok((lo, best))
}

/// A text node placed on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedText {
    /// The grid cell or slot it was given, canvas units.
    pub cell: Rect,
    /// Where the text's top-left corner (paragraph top, line start) lands, canvas units.
    pub origin: [f32; 2],
    /// Laid out relative to `origin`.
    pub text: TextLayout,
    /// The size `fit: shrink` or `grow` set the text at, as a multiple of its own; 1
    /// otherwise.
    pub scale: f32,
    /// Whether the text is taller or wider than its box as set: E100, or W203 under
    /// `fit: shrink` at its minimum size (lints, PLAN 1.15).
    pub overflow: bool,
    /// What `fit: clip` cuts the text to, canvas units.
    pub clip: Option<Rect>,
    /// How it fits its box (SPEC §3.4, ADR-0029): its own `fit`, else its slot's, else `shrink`
    /// for a shape's words and for code, else `wrap`. What lint and the editors read.
    pub fit: TextFit,
    /// A code text's size at its own, before it shrinks or grows, `[width, height]` as its `box`
    /// trims it: what its box grows to hold first, at the size its width lets it
    /// ([`Engine::grown`]). `None` for any other text.
    pub natural: Option<[f32; 2]>,
    /// Where it is code, what its keys read (PLAN 2.139): its tab stops and its first line's
    /// number.
    pub code: Option<crate::carets::CaretCode>,
}

/// Canvas y of the paragraph top, so the text's anchor sits where its alignment says
/// (SPEC §3.4). `start`, `center`, and `end` align the trimmed box; `cap` and
/// `x-height` put the first line's cap or x-height on the cell's top edge; `baseline`
/// puts the last line's baseline on the cell's bottom edge.
fn text_top(cell: Rect, align: AlignY, text: &TextLayout, trim: TextBox) -> f32 {
    let (top, bottom) = text.trimmed(trim);
    let (cell_top, cell_bottom) = (cell[1], cell[1] + cell[3]);
    let first = text.lines.first();
    let above_baseline = |metric: Option<f32>| first.map_or(0.0, |l| metric.map_or(0.0, |m| l.baseline - m));
    match align {
        AlignY::Start | AlignY::Stretch => cell_top - top,
        AlignY::End => cell_bottom - bottom,
        AlignY::Center => cell_top + 0.5 * (cell[3] - (bottom - top)) - top,
        AlignY::Cap => cell_top - above_baseline(first.and_then(|l| l.cap_height)),
        AlignY::XHeight => cell_top - above_baseline(first.and_then(|l| l.x_height)),
        AlignY::Baseline => cell_bottom - text.lines.last().map_or(0.0, |l| l.baseline),
    }
}

/// Where a text leaf's box ends past its widest line, canvas units: room for the sums of
/// advances to come out a hair wider when the text is laid out again at that width.
const FIT_SLACK: f32 = 1.0 / 64.0;

/// A text node's size in a container (SPEC §3.4): the lines it breaks into at the width it
/// is given, or that `available` allows; its height, the box its `box` trims to. Its
/// narrowest and widest sizes break greedily, which `balance` and `pretty` never change. Code
/// breaks no line and shrinks instead (ADR-0029): it asks for its own size at most and the
/// least it shrinks to at narrowest, and stands as tall as the width it is given sets it.
fn measure_text(
    text: &mut TextEngine,
    fonts: &mut BundleFonts,
    theme: &Theme,
    spec: &TextSpec,
    trim: TextBox,
    known: taffy::Size<Option<f32>>,
    available: taffy::Size<taffy::AvailableSpace>,
) -> Result<taffy::Size<f32>, EngineError> {
    if let (Some(width), Some(height)) = (known.width, known.height) {
        return Ok(taffy::Size { width, height });
    }
    let (width, greedy) = match (known.width, available.width) {
        (Some(w), _) | (None, taffy::AvailableSpace::Definite(w)) => (w, false),
        (None, taffy::AvailableSpace::MinContent) => (0.0, true),
        (None, taffy::AvailableSpace::MaxContent) => (1.0e7, true),
    };
    // Verse is as wide as its longest line at any room (PLAN O.4b).
    let laid = if greedy && spec.wrap.unwrap_or(spec.role.wrap) != Wrap::None {
        let spec = TextSpec { wrap: Some(Wrap::Greedy), ..spec.clone() };
        text.layout(fonts, theme, &spec, width)?
    } else {
        text.layout(fonts, theme, spec, width)?
    };
    let (top, bottom) = laid.trimmed(trim);
    if let Some(code) = &spec.code {
        let [wide, tall] = [laid.width, bottom - top];
        let scale = match (known.width, available.width) {
            (Some(w), _) | (None, taffy::AvailableSpace::Definite(w)) if wide > 0.0 => {
                (w / wide).clamp(code.least, 1.0)
            }
            (None, taffy::AvailableSpace::MinContent) => code.least,
            _ => 1.0,
        };
        return Ok(taffy::Size {
            width: known.width.unwrap_or(wide * scale + FIT_SLACK),
            height: known.height.unwrap_or(tall * scale),
        });
    }
    Ok(taffy::Size {
        width: known.width.unwrap_or(laid.width + FIT_SLACK),
        height: known.height.unwrap_or(bottom - top),
    })
}

/// A table's size in a container (SPEC §3.3, §3.4): its columns across, or, under
/// `tables.stretch`, the width it is offered, and its rows down. It asks for no more than
/// it is offered and needs none of it, so a table short of room takes what it is given
/// and says what to cut, rather than pushing what follows it past its container's end.
fn measure_table(
    content: [f32; 2],
    stretch: bool,
    known: taffy::Size<Option<f32>>,
    available: taffy::Size<taffy::AvailableSpace>,
) -> taffy::Size<f32> {
    use taffy::AvailableSpace::{Definite, MaxContent, MinContent};
    let along = |available, content: f32, stretch: bool| match available {
        Definite(room) if stretch => room,
        Definite(room) => content.min(room),
        MinContent => 0.0,
        MaxContent => content,
    };
    taffy::Size {
        width: known.width.unwrap_or_else(|| along(available.width, content[0], stretch)),
        height: known.height.unwrap_or_else(|| along(available.height, content[1], false)),
    }
}

/// A container's panel: its `fill` and `stroke` as a rectangle with its `radius`, casting its
/// `shadow` (PLAN 2.115).
fn container_panel(props: &Props, theme: &Theme, rect: Rect) -> Result<Option<ShapeNode>, EngineError> {
    if props.get("fill").is_none() && props.get("stroke").is_none() {
        return Ok(None);
    }
    let mut panel = Props::new();
    panel.insert("kind".into(), Value::from("rect"));
    for key in ["fill", "stroke", "radius", "shadow"] {
        if let Some(v) = props.get(key) {
            panel.insert(key.into(), v.clone());
        }
    }
    ShapeNode::resolve(&panel, theme, rect).map(Some)
}

fn state_index(snapshots: &[Snapshot], state: &str) -> Result<usize, EngineError> {
    snapshots.iter().position(|s| s.state_id == state).ok_or_else(|| EngineError::UnknownState(state.to_string()))
}

/// The transition into `state`, without laying anything out (SPEC §3.9).
pub fn timing(deck: &Deck, theme: &Theme, state: &str) -> Result<Timing, EngineError> {
    let s = deck.states.iter().find(|s| s.id == state).ok_or_else(|| EngineError::UnknownState(state.to_string()))?;
    Timing::parse(theme, s.transition.as_ref())
}

fn canvas(deck: &Deck) -> [f32; 2] {
    [deck.canvas.width as f32, deck.canvas.height as f32]
}

/// The nodes of `snap` placed in container `id`, in flow order: `at.index`, then the
/// deck's order.
fn children<'a>(deck: &Deck, snap: &'a Snapshot, id: &str) -> Vec<&'a str> {
    fn parent(props: &Props) -> Option<&str> {
        props.get("at").and_then(|at| at.get("parent")).and_then(Value::as_str)
    }
    let index = |props: &Props| props.get("at").and_then(|at| at.get("index")).and_then(Value::as_u64);
    let order = |kid: &str| deck.nodes.get_index_of(kid).unwrap_or(usize::MAX);
    let mut kids: Vec<(u64, usize, &str)> = (snap.nodes.iter())
        .filter(|(_, props)| parent(props) == Some(id))
        .map(|(kid, props)| (index(props).unwrap_or(0), order(kid), kid.as_str()))
        .collect();
    scaena_core::sort::sort(&mut kids);
    kids.into_iter().map(|(_, _, kid)| kid).collect()
}

/// Every visible node's place: its container, its box (a group's, its children's
/// together), its children in flow order, and its transform.
fn tree(deck: &Deck, snap: &Snapshot, placement: &Placement) -> Result<HashMap<String, Place>, EngineError> {
    let mut tree: HashMap<String, Place> = HashMap::new();
    for (id, _) in &placement.order {
        let rect = placement.boxes.get(id).copied().unwrap_or([0.0; 4]);
        let parent = placement.parents.get(id).cloned();
        let children = children(deck, snap, id).into_iter().map(String::from).collect();
        let composite = placement.is_group(id).then(|| placement.opacity(snap, id));
        let transform = typed_prop::<Transform>(&snap.nodes[id], "transform").map_err(|e| in_node(id, e))?;
        // A connector runs where what it joins is drawn (PLAN 2.109): it turns with neither.
        let joins = placement.joined.contains_key(id);
        let pose = transform.map(|t| Pose::from(&t)).filter(|p| !p.is_rest() && !joins);
        tree.insert(id.clone(), Place { parent, rect, children, composite, pose });
    }
    // A group's box spans what its members draw, nested groups included.
    for (id, _) in placement.order.iter().rev() {
        if placement.boxes.contains_key(id) {
            continue;
        }
        let boxes = tree[id].children.iter().map(|kid| tree[kid].rect).filter(|r| r[2] > 0.0 || r[3] > 0.0);
        let union = boxes.reduce(|a, b| {
            let (x0, y0) = (a[0].min(b[0]), a[1].min(b[1]));
            let (x1, y1) = ((a[0] + a[2]).max(b[0] + b[2]), (a[1] + a[3]).max(b[1] + b[3]));
            [x0, y0, x1 - x0, y1 - y0]
        });
        tree.get_mut(id).expect("every visible node has a place").rect = union.unwrap_or([0.0; 4]);
    }
    Ok(tree)
}

/// The most states' spans an engine keeps ([`Engine::timeline`]): past it, it lets them all go.
const SPANS_KEPT: usize = 4096;

/// A hash of what every state of `deck` is drawn from, which a state's snapshot does not say: the
/// deck but for its nodes, states, overrides, and spine, the theme, and the data. One part of
/// [`rest_key`], worked out once for a deck.
pub fn drawn_from(deck: &Deck, theme: &Theme, data: &DataFiles) -> u64 {
    let mut doc = serde_json::to_value(deck).unwrap_or_default();
    if let Some(doc) = doc.as_object_mut() {
        for key in ["nodes", "states", "overrides", "spine", "comment"] {
            doc.remove(key);
        }
    }
    let mut h = std::hash::DefaultHasher::new();
    serde_json::to_vec(&doc).unwrap_or_default().hash(&mut h);
    serde_json::to_vec(&**theme).unwrap_or_default().hash(&mut h);
    data.hash(&mut h);
    h.finish()
}

/// A hash of all state `i`'s span is worked out from: what every state is drawn from (`drawn`),
/// the state as written (its transition and choreography), and what it and the state before it
/// show, which its cue moves between: each node as the states write it, which its motions read,
/// the deck's overrides over it, which a cue laid out reads too, and its type.
fn span_key(drawn: u64, deck: &Deck, snapshots: &[Snapshot], i: usize) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    drawn.hash(&mut h);
    serde_json::to_vec(&deck.states[i]).unwrap_or_default().hash(&mut h);
    for snap in [i.checked_sub(1).map(|p| &snapshots[p]), Some(&snapshots[i])] {
        let Some(snap) = snap else {
            0u8.hash(&mut h);
            continue;
        };
        let over: Vec<Option<&Props>> = snap.nodes.keys().map(|id| deck.overrides.get(id)).collect();
        let types: Vec<Option<NodeType>> =
            snap.nodes.keys().map(|id| deck.nodes.get(id).map(|n| n.node_type)).collect();
        serde_json::to_vec(&(&snap.layout, &snap.nodes, &over, &types)).unwrap_or_default().hash(&mut h);
    }
    h.finish()
}

/// A hash of everything state `i` at rest is drawn from, at `time` seconds on the global timeline
/// (its shaders' clock), which a client keeps a memo of its drawing by (lift-up PERF-4): what every
/// state is drawn from (`drawn`, [`drawn_from`]), what the state shows (each node as the states
/// write it, the deck's overrides over it, and its type, and its layout), and its slide's number
/// and whether the slide shows it (ADR-0028). Two states that hash alike draw alike; a state
/// edited, or one whose slide number or start moved, hashes anew.
pub fn rest_key(drawn: u64, deck: &Deck, snapshots: &[Snapshot], i: usize, time: f64) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    drawn.hash(&mut h);
    let snap = &snapshots[i];
    let over: Vec<Option<&Props>> = snap.nodes.keys().map(|id| deck.overrides.get(id)).collect();
    let types: Vec<Option<NodeType>> = snap.nodes.keys().map(|id| deck.nodes.get(id).map(|n| n.node_type)).collect();
    serde_json::to_vec(&(&snap.layout, &snap.nodes, &over, &types)).unwrap_or_default().hash(&mut h);
    match crate::furniture::slide(deck, &snap.state_id) {
        Some(slide) => (slide.number, slide.of, slide.shows).hash(&mut h),
        None => 0u8.hash(&mut h),
    }
    time.to_bits().hash(&mut h);
    h.finish()
}

/// Each connector routed again between where its ends are drawn (PLAN 2.109), where a transform
/// of theirs or of what holds them moves them (SPEC §3.3): their boxes through those, so it meets
/// the edge of what is drawn.
fn rejoin(theme: &Theme, snap: &Snapshot, placement: &mut Placement, tree: &mut HashMap<String, Place>) {
    let joined: Vec<String> = placement.joined.keys().cloned().collect();
    for id in joined {
        let Some(joint) = snap.nodes.get(&id).and_then(|props| crate::shapes::Joint::of(props, theme, &snap.nodes))
        else {
            continue;
        };
        let moved = |end: &str| crate::sample::posed(tree, end, true) != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        if !(moved(&joint.from) || moved(&joint.to)) {
            continue;
        }
        let ends = (crate::sample::drawn_box(tree, &joint.from), crate::sample::drawn_box(tree, &joint.to));
        let (Some(from), Some(to)) = ends else { continue };
        let (rect, points) = joint.route(from, to);
        placement.boxes.insert(id.clone(), rect);
        placement.joined.insert(id.clone(), points);
        if let Some(place) = tree.get_mut(&id) {
            place.rect = rect;
        }
    }
}

/// A hash of everything the global timeline is worked out from.
fn fingerprint(deck: &Deck, theme: &Theme, data: &DataFiles) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    serde_json::to_vec(deck).unwrap_or_default().hash(&mut h);
    serde_json::to_vec(&**theme).unwrap_or_default().hash(&mut h);
    data.hash(&mut h);
    h.finish()
}

fn in_node(id: &str, e: EngineError) -> EngineError {
    match e {
        EngineError::Layout(m) => EngineError::Layout(format!("node `{id}`: {m}")),
        EngineError::Theme(m) => EngineError::Theme(format!("node `{id}`: {m}")),
        EngineError::Font(m) => EngineError::Font(format!("node `{id}`: {m}")),
        EngineError::Data(m) => EngineError::Data(format!("node `{id}`: {m}")),
        other => other,
    }
}

/// A pin's height where the canvas's shorter side is 1080 (ADR-0030).
const PIN_HEIGHT: f32 = 48.0;

/// The least size a credit is set at where the canvas's shorter side is 1080: 12 pt at
/// presentation size (ADR-0030).
pub const MIN_CREDIT: f32 = 24.0;

/// How much of what shows of a picture a credit inside it may run across: the rest of its foot
/// is the service's own credit's.
const CREDIT_SHARE: f32 = 0.7;

fn theme_color(theme: &Theme, name: &str) -> Result<Color, EngineError> {
    theme.color(name)
}

/// A text node through the cascade (SPEC §3.6): its props (state deltas and overrides
/// already merged) set it in its role, or the slot's, refined by its `style`, and each run
/// in its own; `lang` falls back to the deck's.
fn text_spec(deck: &Deck, theme: &Theme, props: &Props, slot_role: Option<&str>) -> Result<TextSpec, EngineError> {
    let str_prop = |key: &str| props.get(key).and_then(Value::as_str);
    let mut role = cascade::node_role(theme, props, slot_role)?;
    // Code is set as written (ADR-0029): no case, no measure, no hyphens, nothing hung. It is
    // colored by its language in the theme's colors, each held to read on its ground: the
    // surface, which it is drawn on (PLAN 2.132).
    let mut code = crate::code::CodeSpec::of(props, role.size, role.min_size, canvas(deck));
    if let Some(code) = &mut code {
        role.case = None;
        let look = theme.code.as_ref();
        code.tab = look.and_then(|c| c.tab).map_or(crate::code::TAB, usize::from);
        let [r, g, b, _] = theme_color(theme, "surface")?.0;
        code.looks = Some(Box::new(theme.code_looks([r, g, b], &role)?));
    }
    let (role_measure, hanging, optical, hyphenate) =
        (role.measure, role.hanging_punctuation, role.optical_margins, role.hyphenate);
    let line_grid = theme.grid.baseline.filter(|_| role.snap == Some(Snap::Baseline)).map(|pitch| pitch as f32);
    let spans = match props.get("runs").and_then(Value::as_array) {
        Some(runs) => runs
            .iter()
            .map(|run| {
                let text = run.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
                // What the run is set in without its own style: its role's, or the node's look.
                let (base_weight, base_italic) = match run.get("role").and_then(Value::as_str) {
                    Some(name) => theme.text_role(name).map(|r| (r.weight, r.italic))?,
                    None => (role.weight, role.italic),
                };
                let link = run.get("link").and_then(scaena_core::links::target);
                let mut style = cascade::run_role(theme, &role, run)?;
                if code.is_some() {
                    style.case = None;
                }
                Ok(Span {
                    text,
                    style,
                    base_weight,
                    base_italic,
                    link,
                    features: features(run.get("features"))?,
                    axes: axes(run.get("axes")),
                })
            })
            .collect::<Result<_, EngineError>>()?,
        None => vec![Span {
            text: str_prop("text").unwrap_or_default().to_string(),
            style: role.clone(),
            base_weight: role.weight,
            base_italic: role.italic,
            link: None,
            features: BTreeMap::new(),
            axes: BTreeMap::new(),
        }],
    };
    let written: String = spans.iter().map(|s| s.text.as_str()).collect();
    let (list_indent, list_gap, _, _) = theme.typography.lists.clone().unwrap_or_default().at(0);
    let features = features(props.get("features"))?;
    let axes = axes(props.get("axes"));
    if code.is_some() {
        return Ok(TextSpec {
            spans,
            role,
            features,
            axes,
            numeric: typed_prop::<Numeric>(props, "numeric")?,
            wrap: None,
            min_last_line_words: None,
            lang: str_prop("lang").map(String::from).or_else(|| deck.meta.as_ref().and_then(|m| m.lang.clone())),
            align: TextAlign::Start,
            measure: None,
            hanging_punctuation: false,
            optical_margins: false,
            hyphenate: false,
            line_grid,
            items: Vec::new(),
            list_indent: list_indent as f32,
            list_gap: list_gap as f32,
            code,
            mirrored: scaena_core::direction::mirrored(deck),
        });
    }
    let items = list_marks(theme, props, &written)?;
    Ok(TextSpec {
        spans,
        role,
        features,
        axes,
        numeric: typed_prop::<Numeric>(props, "numeric")?,
        wrap: typed_prop::<Wrap>(props, "wrap")?,
        min_last_line_words: props.get("minLastLineWords").and_then(Value::as_u64).map(|n| n as u32),
        lang: str_prop("lang").map(String::from).or_else(|| deck.meta.as_ref().and_then(|m| m.lang.clone())),
        align: TextAlign::Start,
        measure: props.get("measure").and_then(Value::as_f64).map(|m| m as f32).or(role_measure),
        hanging_punctuation: props.get("hangingPunctuation").and_then(Value::as_bool).unwrap_or(hanging),
        optical_margins: props.get("opticalMargins").and_then(Value::as_bool).unwrap_or(optical),
        hyphenate: props.get("hyphenate").and_then(Value::as_bool).unwrap_or(hyphenate),
        line_grid,
        items,
        list_indent: list_indent as f32,
        list_gap: list_gap as f32,
        code: None,
        mirrored: scaena_core::direction::mirrored(deck),
    })
}

/// A text's or a run's OpenType features (`features`): each on, off, or an alternate's index.
fn features(v: Option<&Value>) -> Result<BTreeMap<String, u16>, EngineError> {
    v.and_then(Value::as_object)
        .map(|f| {
            scaena_core::sort::try_map(f.iter().map(|(k, v)| {
                let value = v.as_bool().map(u16::from).or_else(|| v.as_u64().and_then(|n| u16::try_from(n).ok()));
                value.map(|v| (k.clone(), v)).ok_or_else(|| EngineError::Layout(format!("feature `{k}`: {v}")))
            }))
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

/// A text's or a run's variable-font axis values (`axes`), by tag.
fn axes(v: Option<&Value>) -> BTreeMap<String, f32> {
    v.and_then(Value::as_object)
        .map(|a| scaena_core::sort::map(a.iter().filter_map(|(k, v)| Some((k.clone(), v.as_f64()? as f32)))))
        .unwrap_or_default()
}

/// A text's paragraphs as list items (ADR-0018): each item's level and marker, numbered as its
/// list counts, by `written`'s paragraphs.
fn list_marks(theme: &Theme, props: &Props, written: &str) -> Result<Vec<Option<ListMark>>, EngineError> {
    if props.get("list").is_none_or(Value::is_null) {
        return Ok(Vec::new());
    }
    let list = typed_prop::<Vec<Option<ListItem>>>(props, "list")?.unwrap_or_default();
    let lists = theme.typography.lists.clone().unwrap_or_default();
    if list.iter().all(Option::is_none) {
        return Ok(Vec::new());
    }
    let items = scaena_core::lists::items(&list, scaena_core::lists::paragraphs(written).len());
    let numbers = scaena_core::lists::numbers(&items);
    Ok(items
        .iter()
        .zip(numbers)
        .map(|(item, n)| {
            item.map(|item| ListMark {
                kind: item.kind,
                level: item.depth(),
                marker: scaena_core::lists::marker(&lists, item, n),
            })
        })
        .collect())
}

fn typed_prop<T: serde::de::DeserializeOwned>(props: &Props, key: &str) -> Result<Option<T>, EngineError> {
    props
        .get(key)
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| EngineError::Layout(format!("`{key}`: {e}")))
}

/// A credit's words as they are laid out: the space before each separator ` · ` no break, so a
/// line breaks after the dot, never before it, and no line of a credit starts with one.
fn kept(text: &str) -> String {
    text.replace(" \u{b7} ", "\u{a0}\u{b7} ")
}
