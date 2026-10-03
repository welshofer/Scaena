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
use crate::text::{GRID_EPSILON, Span, TextAlign, TextEngine, TextLayout, TextSpec};
use crate::theme::{Numeric, TextBox, Theme, Wrap};
use scaena_core::displaylist::{Color, DisplayList, Rect};
use scaena_core::document::{NodeType, Props};
use scaena_core::model::Format;
use scaena_core::model::nodes::TextFit;
use scaena_core::model::theme::Snap;
use scaena_core::model::values::SplitUnit;
use scaena_core::timeline::{Motion, Timeline};
use scaena_core::{Deck, Snapshot};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
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
/// canvas, and the theme with that format's grid and slots. `None`, or a format of the
/// deck's own shape, is the deck as it is. A format the deck does not list is an error.
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
        return Err(EngineError::Layout(format!("format `{name}` is not one of the deck's formats ({listed})")));
    }
    let mut projected = deck.clone();
    (projected.canvas.width, projected.canvas.height) = (canvas[0], canvas[1]);
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
    /// Lay out what a frame refuses (text under `fit: error` that does not fit, a table
    /// whose rows do not), for lint to report: set only while lint runs.
    pub(crate) lenient: bool,
}

impl Engine {
    pub fn new(fonts: BundleFonts) -> Self {
        Self { fonts, images: BundleImages::new(), text: TextEngine::new(), timeline: None, lenient: false }
    }

    /// The bundle's images, which image nodes name.
    pub fn with_images(mut self, images: BundleImages) -> Self {
        self.images = images;
        self
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
        let items = motion::items(deck, theme, state, before, &snapshots[i], timing.matched)?;
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
        for (i, state) in deck.states.iter().enumerate().take(last + 1).skip(states.len()) {
            let timing = Timing::parse(theme, state.transition.as_ref())?;
            let before = i.checked_sub(1).map(|p| &snapshots[p]);
            let items = motion::items(deck, theme, state, before, &snapshots[i], timing.matched)?;
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
            states.push((state.id.clone(), span, state.hold.unwrap_or(0.0)));
        }
        let timeline = Timeline::new(states.iter().take(last + 1).cloned());
        self.timeline = Some((key, states));
        Ok(timeline)
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
        let grid = Grid::from_theme(theme, canvas)?;
        let snap = &cascade::with_overrides(deck, snap);
        let placement = self.place(deck, theme, &grid, snap)?;
        let tree = tree(deck, snap, &placement);
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
                    let chart = charts::compile(&mut cx, props, [rect[2], rect[3]]).map_err(|e| in_node(id, e))?;
                    Content::Chart { cell: rect, chart: Box::new(chart) }
                }
                NodeType::Table => {
                    let lenient = self.lenient;
                    let mut cx =
                        Ctx { text: &mut self.text, fonts: &mut self.fonts, theme, deck, data, colors: &[], lenient };
                    let table = tables::compile(&mut cx, props, [rect[2], rect[3]]).map_err(|e| in_node(id, e))?;
                    Content::Table { cell: rect, table: Box::new(table) }
                }
                NodeType::Shader => {
                    Content::Shader(ShaderNode::resolve(props, theme, rect).map_err(|e| in_node(id, e))?)
                }
                NodeType::Shape => Content::Shape(ShapeNode::resolve(props, theme, rect).map_err(|e| in_node(id, e))?),
                NodeType::Image => {
                    Content::Image(ImageNode::resolve(props, theme, &self.images, rect).map_err(|e| in_node(id, e))?)
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
        Ok(Scene { state: snap.state_id.clone(), canvas, surface: theme_color(theme, "surface")?, nodes, tree })
    }

    /// One text node of a state, laid out and placed: what `frame` draws, and what
    /// layout-level lints (E100 overflow, W200 widows) read.
    pub fn text_layout(&mut self, req: &FrameRequest, node: &str) -> Result<PlacedText, EngineError> {
        let (deck, theme) = project(req.deck, req.theme, req.format)?;
        let (deck, theme) = (deck.as_ref(), theme.as_ref());
        let snapshots = scaena_core::resolve_states(deck)?;
        let snap = &cascade::with_overrides(deck, &snapshots[state_index(&snapshots, req.state)?]);
        if !snap.nodes.contains_key(node) {
            return Err(EngineError::Layout(format!("node `{node}` is not visible in state `{}`", req.state)));
        }
        let grid = Grid::from_theme(theme, canvas(deck))?;
        let placement = self.place(deck, theme, &grid, snap)?;
        self.layout_text_node(deck, theme, grid.baseline, snap, node, placement.boxes[node])
    }

    /// Every node's box in `snap` (overrides merged), its container, and paint order:
    /// roots on the theme grid, containers' children through `taffy`, text measured here.
    fn place(&mut self, deck: &Deck, theme: &Theme, grid: &Grid, snap: &Snapshot) -> Result<Placement, EngineError> {
        let (text, fonts) = (&mut self.text, &mut self.fonts);
        let mut specs: HashMap<String, (TextSpec, TextBox)> = HashMap::new();
        let mut measure = |id: &str, known: taffy::Size<Option<f32>>, available: taffy::Size<taffy::AvailableSpace>| {
            if !specs.contains_key(id) {
                let props = &snap.nodes[id];
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
        let (align_x, align_y) = Grid::alignment(theme, template, props).map_err(|e| in_node(id, e))?;
        spec.align = match align_x {
            AlignX::Start | AlignX::Stretch => TextAlign::Start,
            AlignX::Center => TextAlign::Center,
            AlignX::End => TextAlign::End,
        };
        let trim = match typed_prop::<TextBox>(props, "box")? {
            Some(trim) => trim,
            None => spec.role.text_box,
        };
        let fit = typed_prop::<TextFit>(props, "fit")?.unwrap_or(TextFit::Wrap);
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
        let fits = |t: &TextLayout| fits_box(t) && max_lines.is_none_or(|m| t.lines.len() <= m);
        let mut lay = |scale: f32| -> Result<TextLayout, EngineError> {
            self.text.layout(&mut self.fonts, theme, &spec.scaled(scale), cell[2]).map_err(|e| in_node(id, e))
        };
        let base = lay(1.0)?;
        let (scale, text) = match fit {
            TextFit::Shrink if !fits(&base) => {
                let floor = number("minSize").or(spec.role.min_size).map_or(0.5, |m| m / spec.role.size).min(1.0);
                largest_fit(&mut lay, &fits, floor, 1.0)?
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
        if text.synthesized {
            return Err(EngineError::Font(format!(
                "node `{id}`: a run needs faux bold or oblique, which the display list cannot express; \
                 use a weight or style the family provides"
            )));
        }
        let overflow = !fits_box(&text);
        let clip = (fit == TextFit::Clip).then_some(cell);
        let top = text_top(cell, align_y, &text, trim);
        let origin = [cell[0], top + to_grid(lines, spec.role.snap, align_y, top, &text)];
        Ok(PlacedText { cell, origin, text, scale, overflow, clip })
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
/// narrowest and widest sizes break greedily, which `balance` and `pretty` never change.
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
    let laid = if greedy {
        let spec = TextSpec { wrap: Some(Wrap::Greedy), ..spec.clone() };
        text.layout(fonts, theme, &spec, width)?
    } else {
        text.layout(fonts, theme, spec, width)?
    };
    let (top, bottom) = laid.trimmed(trim);
    Ok(taffy::Size {
        width: known.width.unwrap_or(laid.width + FIT_SLACK),
        height: known.height.unwrap_or(bottom - top),
    })
}

/// A container's panel: its `fill` and `stroke` as a rectangle with its `radius`.
fn container_panel(props: &Props, theme: &Theme, rect: Rect) -> Result<Option<ShapeNode>, EngineError> {
    if props.get("fill").is_none() && props.get("stroke").is_none() {
        return Ok(None);
    }
    let mut panel = Props::new();
    panel.insert("kind".into(), Value::from("rect"));
    for key in ["fill", "stroke", "radius"] {
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
    kids.sort();
    kids.into_iter().map(|(_, _, kid)| kid).collect()
}

/// Every visible node's place: its container, its box (a group's, its children's
/// together), and its children in flow order.
fn tree(deck: &Deck, snap: &Snapshot, placement: &Placement) -> HashMap<String, Place> {
    let mut tree: HashMap<String, Place> = HashMap::new();
    for (id, _) in &placement.order {
        let rect = placement.boxes.get(id).copied().unwrap_or([0.0; 4]);
        let parent = placement.parents.get(id).cloned();
        let children = children(deck, snap, id).into_iter().map(String::from).collect();
        let composite = placement.is_group(id).then(|| placement.opacity(snap, id));
        tree.insert(id.clone(), Place { parent, rect, children, composite });
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
    tree
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

fn theme_color(theme: &Theme, name: &str) -> Result<Color, EngineError> {
    theme.color(name)
}

/// A text node through the cascade (SPEC §3.6): its props (state deltas and overrides
/// already merged) set it in its role, or the slot's, refined by its `style`, and each run
/// in its own; `lang` falls back to the deck's.
fn text_spec(deck: &Deck, theme: &Theme, props: &Props, slot_role: Option<&str>) -> Result<TextSpec, EngineError> {
    let str_prop = |key: &str| props.get(key).and_then(Value::as_str);
    let role = cascade::node_role(theme, props, slot_role)?;
    let (role_measure, hanging, optical, hyphenate) =
        (role.measure, role.hanging_punctuation, role.optical_margins, role.hyphenate);
    let line_grid = theme.grid.baseline.filter(|_| role.snap == Some(Snap::Baseline)).map(|pitch| pitch as f32);
    let spans = match props.get("runs").and_then(Value::as_array) {
        Some(runs) => runs
            .iter()
            .map(|run| {
                let text = run.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
                Ok(Span { text, style: cascade::run_role(theme, &role, run)? })
            })
            .collect::<Result<_, EngineError>>()?,
        None => vec![Span { text: str_prop("text").unwrap_or_default().to_string(), style: role.clone() }],
    };
    let features = props
        .get("features")
        .and_then(Value::as_object)
        .map(|f| {
            f.iter()
                .map(|(k, v)| {
                    let value = v.as_bool().map(u16::from).or_else(|| v.as_u64().and_then(|n| u16::try_from(n).ok()));
                    value.map(|v| (k.clone(), v)).ok_or_else(|| EngineError::Layout(format!("feature `{k}`: {v}")))
                })
                .collect::<Result<_, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let axes = props
        .get("axes")
        .and_then(Value::as_object)
        .map(|a| a.iter().filter_map(|(k, v)| Some((k.clone(), v.as_f64()? as f32))).collect())
        .unwrap_or_default();
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
    })
}

fn typed_prop<T: serde::de::DeserializeOwned>(props: &Props, key: &str) -> Result<Option<T>, EngineError> {
    props
        .get(key)
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| EngineError::Layout(format!("`{key}`: {e}")))
}
