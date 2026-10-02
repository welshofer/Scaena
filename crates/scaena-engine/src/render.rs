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
use crate::data::DataFiles;
use crate::fonts::BundleFonts;
use crate::layout::{AlignX, AlignY, Grid};
use crate::sample::{Content, Policy, Scene, SceneNode, Timing, Transition};
use crate::shaders::ShaderNode;
use crate::text::{Span, TextEngine, TextLayout, TextSpec};
use crate::theme::{Numeric, TextBox, Theme, Wrap};
use scaena_core::displaylist::{Color, DisplayList, Rect, paint_order};
use scaena_core::document::{NodeType, Props};
use scaena_core::{Deck, Snapshot};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct FrameRequest<'a> {
    pub deck: &'a Deck,
    pub theme: &'a Theme,
    /// The bundle's data files, which charts read (SPEC §3.10).
    pub data: &'a DataFiles,
    pub state: &'a str,
    /// Milliseconds since the start of the transition into `state`; at or past its
    /// duration (`f64::INFINITY` always is), the state at rest.
    pub t_ms: f64,
}

#[derive(Debug, Clone)]
pub struct Frame {
    /// In canvas units; `viewport` is the canvas size, and painters scale to their output.
    pub display_list: DisplayList,
    /// The transition into this state, ms; choreography adds to it in PLAN 1.11.
    pub duration_ms: f64,
}

/// The engine for one bundle: its fonts plus reusable layout scratch.
pub struct Engine {
    fonts: BundleFonts,
    text: TextEngine,
}

impl Engine {
    pub fn new(fonts: BundleFonts) -> Self {
        Self { fonts, text: TextEngine::new() }
    }

    /// Render one frame. Deterministic: same inputs → identical display list (SPEC §13).
    /// At rest it lays out the state alone; inside a transition it lays out the state
    /// and the one before it, then samples. To draw many frames of one transition,
    /// build it once with [`Engine::transition`].
    pub fn frame(&mut self, req: &FrameRequest) -> Result<Frame, EngineError> {
        let snapshots = scaena_core::resolve_states(req.deck)?;
        let i = state_index(&snapshots, req.state)?;
        let timing = Timing::parse(req.theme, req.deck.states[i].transition.as_ref())?;
        let display_list = if timing.progress(req.t_ms) >= 1.0 {
            self.scene(req.deck, req.theme, req.data, &snapshots[i])?.draw()
        } else {
            self.transition_at(req.deck, req.theme, req.data, &snapshots, i, timing)?.frame(req.t_ms)
        };
        Ok(Frame { display_list, duration_ms: timing.duration_ms })
    }

    /// The transition into `state`: it and the state before it in the cue list (what
    /// was on screen), each laid out once. [`Transition::frame`] then samples any
    /// time without layout.
    pub fn transition(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        data: &DataFiles,
        state: &str,
    ) -> Result<Transition, EngineError> {
        let snapshots = scaena_core::resolve_states(deck)?;
        let i = state_index(&snapshots, state)?;
        let timing = Timing::parse(theme, deck.states[i].transition.as_ref())?;
        self.transition_at(deck, theme, data, &snapshots, i, timing)
    }

    fn transition_at(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        data: &DataFiles,
        snapshots: &[Snapshot],
        i: usize,
        timing: Timing,
    ) -> Result<Transition, EngineError> {
        let to = self.scene(deck, theme, data, &snapshots[i])?;
        let from = match i.checked_sub(1) {
            Some(prev) if timing.duration_ms > 0.0 => Some(self.scene(deck, theme, data, &snapshots[prev])?),
            _ => None,
        };
        Ok(Transition::new(from, to, timing))
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
        let mut nodes = Vec::with_capacity(snap.nodes.len());
        for id in paint_order(snap) {
            let props = &snap.nodes[id];
            let content = match deck.nodes[id].node_type {
                NodeType::Text => Content::Text(self.layout_text_node(deck, theme, &grid, snap, id)?),
                NodeType::Chart => {
                    let cell =
                        grid.place(theme, snap.layout.as_deref(), props.get("at")).map_err(|e| in_node(id, e))?;
                    let mut cx = Ctx { text: &mut self.text, fonts: &mut self.fonts, theme, deck, data };
                    let chart = charts::compile(&mut cx, props, [cell[2], cell[3]]).map_err(|e| in_node(id, e))?;
                    Content::Chart { cell, chart }
                }
                NodeType::Shader => {
                    let rect =
                        grid.place(theme, snap.layout.as_deref(), props.get("at")).map_err(|e| in_node(id, e))?;
                    Content::Shader(ShaderNode::resolve(props, theme, rect).map_err(|e| in_node(id, e))?)
                }
                NodeType::Shape | NodeType::Image => {
                    return Err(EngineError::NotImplemented("shape and image nodes — PLAN 1.7"));
                }
                NodeType::Stack | NodeType::Grid | NodeType::Frame | NodeType::Group => {
                    return Err(EngineError::NotImplemented("container nodes — PLAN 1.7"));
                }
            };
            nodes.push(SceneNode {
                id: id.to_string(),
                z: props.get("z").and_then(Value::as_i64).unwrap_or(0),
                order: deck.nodes.get_index_of(id).expect("snapshot nodes are deck nodes"),
                opacity: props.get("opacity").and_then(Value::as_f64).unwrap_or(1.0) as f32,
                policy: Policy::parse(props.get("transition")).map_err(|e| in_node(id, e))?,
                content,
            });
        }
        Ok(Scene {
            state: snap.state_id.clone(),
            canvas,
            surface: theme_color(theme, "surface")?,
            time: rest_time(deck, theme, &snap.state_id)?,
            nodes,
        })
    }

    /// One text node of a state, laid out and placed: what `frame` draws, and what
    /// layout-level lints (E100 overflow, W200 widows) read.
    pub fn text_layout(&mut self, req: &FrameRequest, node: &str) -> Result<PlacedText, EngineError> {
        let snapshots = scaena_core::resolve_states(req.deck)?;
        let snap = &cascade::with_overrides(req.deck, &snapshots[state_index(&snapshots, req.state)?]);
        if !snap.nodes.contains_key(node) {
            return Err(EngineError::Layout(format!("node `{node}` is not visible in state `{}`", req.state)));
        }
        let grid = Grid::from_theme(req.theme, canvas(req.deck))?;
        self.layout_text_node(req.deck, req.theme, &grid, snap, node)
    }

    fn layout_text_node(
        &mut self,
        deck: &Deck,
        theme: &Theme,
        grid: &Grid,
        snap: &Snapshot,
        id: &str,
    ) -> Result<PlacedText, EngineError> {
        let props = &snap.nodes[id];
        let template = snap.layout.as_deref();
        let at = props.get("at");
        let cell = grid.place(theme, template, at).map_err(|e| in_node(id, e))?;
        let spec = text_spec(deck, theme, props, Grid::slot_role(theme, template, at).as_deref())
            .map_err(|e| in_node(id, e))?;
        let text = self.text.layout(&mut self.fonts, theme, &spec, cell[2]).map_err(|e| in_node(id, e))?;
        if text.synthesized {
            return Err(EngineError::Font(format!(
                "node `{id}`: a run needs faux bold or oblique, which the display list cannot express; \
                 use a weight or style the family provides"
            )));
        }
        let (align_x, align_y) = Grid::alignment(theme, template, props).map_err(|e| in_node(id, e))?;
        if matches!(align_x, AlignX::Center | AlignX::End) {
            return Err(EngineError::NotImplemented("centered and end-aligned text — PLAN 1.8"));
        }
        let trim = match typed_prop::<TextBox>(props, "box")? {
            Some(trim) => trim,
            None => spec.role.text_box,
        };
        let origin = [cell[0], text_top(cell, align_y, &text, trim)];
        Ok(PlacedText { cell, origin, text })
    }
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

/// When `state` comes to rest on the global timeline (SPEC §2.2), in seconds: the
/// transitions of the cue list up to and including its own, end to end. Phase 0 has
/// no holds or choreography, so that is the whole timeline. Shaders read their time
/// from it, so a background drifts on across states instead of starting over.
pub fn rest_time(deck: &Deck, theme: &Theme, state: &str) -> Result<f64, EngineError> {
    let mut ms = 0.0;
    for s in &deck.states {
        ms += Timing::parse(theme, s.transition.as_ref())?.duration_ms;
        if s.id == state {
            return Ok(ms / 1000.0);
        }
    }
    Err(EngineError::UnknownState(state.to_string()))
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
