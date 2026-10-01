//! `Engine::frame(request) -> DisplayList`: the engine's one public entry point.
//!
//! Phase 0 renders a state at rest: the theme surface, then one layer per visible
//! node in paint order (`z`, then scene-graph order). Text nodes are laid out on the
//! theme grid; other node types return `NotImplemented` naming the PLAN task that
//! adds them, rather than drawing a placeholder a golden would freeze.

use crate::EngineError;
use crate::fonts::BundleFonts;
use crate::layout::Grid;
use crate::text::{Span, TextEngine, TextLayout, TextSpec};
use crate::theme::{Numeric, Theme, Wrap};
use scaena_core::displaylist::{Blend, Color, DisplayList, FillRule, Op, Paint, Path, Rect, paint_order};
use scaena_core::document::{NodeType, Props};
use scaena_core::{Deck, Snapshot};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct FrameRequest<'a> {
    pub deck: &'a Deck,
    pub theme: &'a Theme,
    pub state: &'a str,
    /// Milliseconds since the start of the transition into `state`. Phase 0 renders
    /// every state at rest; sampling arrives with PLAN 0.10 / 1.12.
    pub t_ms: f64,
}

#[derive(Debug, Clone)]
pub struct Frame {
    /// In canvas units; `viewport` is the canvas size, and painters scale to their output.
    pub display_list: DisplayList,
    /// Total transition + choreography duration for this state, ms (PLAN 1.11; 0 until then).
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
    pub fn frame(&mut self, req: &FrameRequest) -> Result<Frame, EngineError> {
        let snapshots = scaena_core::resolve_states(req.deck)?;
        let snap = find_state(&snapshots, req.state)?;
        let canvas = canvas(req.deck);
        let grid = Grid::from_theme(req.theme, canvas)?;

        let mut dl = DisplayList::new(canvas);
        dl.ops.push(Op::Fill {
            path: Path::rect([0.0, 0.0, canvas[0], canvas[1]]),
            rule: FillRule::NonZero,
            paint: Paint::Solid(theme_color(req.theme, "surface")?),
        });
        for id in paint_order(snap) {
            let props = &snap.nodes[id];
            match req.deck.nodes[id].node_type {
                NodeType::Text => {
                    let (rect, text) = self.layout_text_node(req, &grid, snap, id)?;
                    let mut ops = Vec::with_capacity(text.runs.len());
                    for run in text.runs {
                        let font = dl.font(run.font);
                        ops.push(Op::Glyphs {
                            font,
                            size: run.size,
                            coords: run.coords,
                            paint: Paint::Solid(run.color),
                            glyphs: run.glyphs,
                        });
                    }
                    dl.ops.push(Op::Layer {
                        node: Some(id.to_string()),
                        transform: [1.0, 0.0, 0.0, 1.0, rect[0], rect[1]],
                        opacity: props.get("opacity").and_then(Value::as_f64).unwrap_or(1.0) as f32,
                        blend: Blend::Normal,
                        clip: None,
                        ops,
                    });
                }
                NodeType::Chart => {
                    return Err(EngineError::NotImplemented("chart nodes — PLAN 0.10 (bar → line), 1.9"));
                }
                NodeType::Shader => return Err(EngineError::NotImplemented("shader nodes — PLAN 0.11 (mesh), 1.10")),
                NodeType::Shape | NodeType::Image => {
                    return Err(EngineError::NotImplemented("shape and image nodes — PLAN 1.7"));
                }
                NodeType::Stack | NodeType::Grid | NodeType::Frame | NodeType::Group => {
                    return Err(EngineError::NotImplemented("container nodes — PLAN 1.7"));
                }
            }
        }
        Ok(Frame { display_list: dl, duration_ms: 0.0 })
    }

    /// One text node of a state, laid out on the grid: its box (canvas units) and its
    /// text, positioned relative to the box. What `frame` draws, and what layout-level
    /// lints (E100 overflow, W200 widows) read.
    pub fn text_layout(&mut self, req: &FrameRequest, node: &str) -> Result<(Rect, TextLayout), EngineError> {
        let snapshots = scaena_core::resolve_states(req.deck)?;
        let snap = find_state(&snapshots, req.state)?;
        if !snap.nodes.contains_key(node) {
            return Err(EngineError::Layout(format!("node `{node}` is not visible in state `{}`", req.state)));
        }
        let grid = Grid::from_theme(req.theme, canvas(req.deck))?;
        self.layout_text_node(req, &grid, snap, node)
    }

    fn layout_text_node(
        &mut self,
        req: &FrameRequest,
        grid: &Grid,
        snap: &Snapshot,
        id: &str,
    ) -> Result<(Rect, TextLayout), EngineError> {
        let props = &snap.nodes[id];
        let template = snap.layout.as_deref();
        let at = props.get("at");
        let rect = grid.place(req.theme, template, at).map_err(|e| in_node(id, e))?;
        let spec = text_spec(req.deck, props, Grid::slot_role(req.theme, template, at)).map_err(|e| in_node(id, e))?;
        let text = self.text.layout(&mut self.fonts, req.theme, &spec, rect[2]).map_err(|e| in_node(id, e))?;
        if text.synthesized {
            return Err(EngineError::Font(format!(
                "node `{id}`: a run needs faux bold or oblique, which the display list cannot express; \
                 use a weight or style the family provides"
            )));
        }
        Ok((rect, text))
    }
}

fn find_state<'a>(snapshots: &'a [Snapshot], state: &str) -> Result<&'a Snapshot, EngineError> {
    snapshots.iter().find(|s| s.state_id == state).ok_or_else(|| EngineError::UnknownState(state.to_string()))
}

fn canvas(deck: &Deck) -> [f32; 2] {
    [deck.canvas.width as f32, deck.canvas.height as f32]
}

fn in_node(id: &str, e: EngineError) -> EngineError {
    match e {
        EngineError::Layout(m) => EngineError::Layout(format!("node `{id}`: {m}")),
        EngineError::Theme(m) => EngineError::Theme(format!("node `{id}`: {m}")),
        EngineError::Font(m) => EngineError::Font(format!("node `{id}`: {m}")),
        other => other,
    }
}

fn theme_color(theme: &Theme, name: &str) -> Result<Color, EngineError> {
    let hex = theme.color(name).ok_or_else(|| EngineError::Theme(format!("unknown color `{name}`")))?;
    Color::from_hex(hex).map_err(|e| EngineError::Theme(e.to_string()))
}

/// The cascade for a text node, Phase 0 subset: node props (state deltas already
/// merged) over the slot's default role; `lang` falls back to the deck's.
fn text_spec(deck: &Deck, props: &Props, slot_role: Option<String>) -> Result<TextSpec, EngineError> {
    let str_prop = |key: &str| props.get(key).and_then(Value::as_str);
    let role = str_prop("role")
        .map(String::from)
        .or(slot_role)
        .ok_or_else(|| EngineError::Layout("text node has no `role` and its slot gives none".into()))?;
    let spans = match props.get("runs").and_then(Value::as_array) {
        Some(runs) => runs
            .iter()
            .map(|run| Span {
                text: run.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
                role: run.get("role").and_then(Value::as_str).unwrap_or(&role).to_string(),
            })
            .collect(),
        None => vec![Span { text: str_prop("text").unwrap_or_default().to_string(), role: role.clone() }],
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
