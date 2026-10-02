//! E110, E111: text whose contrast with what lies behind it is below WCAG's: 4.5:1 for
//! body text (E110), 3:1 for display text (E111).
//!
//! - **Behind it** is the state at rest with its text taken away, painted by the
//!   client's painter ([`Backdrop`]): the surface, shapes, images, shaders, and charts,
//!   at the opacity they have. A shader is judged at rest and at the end of the state's
//!   hold, where it has moved most.
//! - Each run of text is judged over the pixels under its glyphs, its own color laid on
//!   each at its alpha times its node's opacity. A run fails when more than 2% of them
//!   are below the line: noise in a background may dip under it, a band of it may not.
//! - **Display** is WCAG's large text: 24 px or more, or 18.67 px at weight 700 or more,
//!   on a screen whose shorter side is 1080 px (a 1920 × 1080 canvas unit is a pixel).
//!   Everything else is body.
//! - Text in a node and in a table's cells is judged; a chart's labels are not yet.

use super::{Cx, Laid};
use crate::EngineError;
use crate::cascade;
use crate::layout::Grid;
use crate::sample::Content;
use crate::text::TextLayout;
use scaena_core::displaylist::{Color, Op};
use scaena_core::lint::{Backdrop, Finding, Pixels, Severity};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// The share of a run's pixels that may fall below the line.
const SPARE: f64 = 0.02;

/// Pixels on the canvas's shorter side when painting what text sits on.
const SIDE: f32 = 540.0;

/// WCAG relative luminance of an sRGB color.
fn luminance([r, g, b]: [f64; 3]) -> f64 {
    let lin = |c: f64| if c <= 0.04045 { c / 12.92 } else { libm::pow((c + 0.055) / 1.055, 2.4) };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

fn ratio(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// One run of text to judge: where its glyphs are, its color and alpha, and its class.
struct Run {
    node: String,
    rect: [f32; 4],
    color: [f64; 3],
    alpha: f64,
    display: bool,
    size: f32,
}

/// Each run of `t` placed at `origin`, as boxes from its glyphs' advances and its line's
/// ascent and descent.
fn runs(t: &TextLayout, origin: [f32; 2]) -> Vec<([f32; 4], Color, f32)> {
    let mut out = Vec::new();
    for run in &t.runs {
        let Some(line) = t.lines.get(run.line) else { continue };
        let xs = run.glyphs.iter().zip(&run.advances).map(|(g, a)| (g.x, g.x + a));
        let (left, right) = xs.fold((f32::INFINITY, f32::NEG_INFINITY), |(l, r), (a, b)| (l.min(a), r.max(b)));
        if right <= left {
            continue;
        }
        let top = line.baseline - line.ascent;
        let rect = [origin[0] + left, origin[1] + top, right - left, line.ascent + line.descent];
        out.push((rect, run.color, run.size));
    }
    out
}

/// The canvas's scale to a screen 1080 px on its shorter side.
fn to_screen(cx: &Cx) -> f32 {
    1080.0 / cx.deck.canvas.width.min(cx.deck.canvas.height) as f32
}

/// The runs a state's text draws, and the ids of the nodes that draw them.
fn texts(cx: &Cx, state: &Laid) -> (Vec<Run>, BTreeSet<String>) {
    let screen = to_screen(cx);
    let mut out = Vec::new();
    let mut ids = BTreeSet::new();
    // A node's opacity times its groups'.
    let composite = |id: &str| {
        let mut alpha = 1.0_f64;
        let mut at = state.scene.tree.get(id).and_then(|p| p.parent.clone());
        while let Some(parent) = at {
            let place = state.scene.tree.get(&parent);
            alpha *= place.and_then(|p| p.composite).map_or(1.0, f64::from);
            at = place.and_then(|p| p.parent.clone());
        }
        alpha
    };
    for node in &state.scene.nodes {
        let opacity = f64::from(node.opacity) * composite(&node.id);
        let (placed, weight): (Vec<(&TextLayout, [f32; 2])>, f64) = match &node.content {
            Content::Text(placed) => {
                let props = &state.snapshot.nodes[&node.id];
                let slot = Grid::slot_role(cx.theme, state.snapshot.layout.as_deref(), props.get("at"));
                let weight = cascade::look(cx.theme, props, slot.as_deref()).map_or(400.0, |l| l.weight);
                (vec![(&placed.text, placed.origin)], weight)
            }
            Content::Table { cell, table } => {
                let at = |o: [f32; 2]| [cell[0] + o[0], cell[1] + o[1]];
                let all = table.header.iter().chain(&table.cells).map(|c| (&c.text, at(c.origin)));
                (all.collect(), 400.0)
            }
            _ => continue,
        };
        ids.insert(node.id.clone());
        for (t, origin) in placed {
            for (rect, color, size) in runs(t, origin) {
                let px = size * screen;
                let [r, g, b, a] = color.0.map(|c| f64::from(c) / 255.0);
                out.push(Run {
                    node: node.id.clone(),
                    rect,
                    color: [r, g, b],
                    alpha: a * opacity,
                    display: px >= 24.0 || (px >= 18.67 && weight >= 700.0),
                    size: px,
                });
            }
        }
    }
    (out, ids)
}

/// `ops` without the layers of `ids`, wherever they sit.
fn strip(ops: &mut Vec<Op>, ids: &BTreeSet<String>) {
    ops.retain(|op| !matches!(op, Op::Layer { node: Some(id), .. } if ids.contains(id)));
    for op in ops {
        if let Op::Layer { ops, .. } = op {
            strip(ops, ids);
        }
    }
}

/// How a run reads over `px`: its contrast at the `SPARE` quantile of its pixels, and the
/// background there.
fn judge(run: &Run, px: &Pixels, scale: f32) -> Option<(f64, [u8; 3])> {
    let [x, y, w, h] = run.rect.map(|v| v * scale);
    let (x0, y0) = (x.floor().max(0.0) as u32, y.floor().max(0.0) as u32);
    let (x1, y1) = (((x + w).ceil() as u32).min(px.width), ((y + h).ceil() as u32).min(px.height));
    let mut seen: Vec<(f64, [u8; 3])> = Vec::new();
    for py in y0..y1 {
        for pxl in x0..x1 {
            let [r, g, b, _] = px.pixel(pxl, py);
            let bg = [r, g, b].map(|c| f64::from(c) / 255.0);
            let a = run.alpha.clamp(0.0, 1.0);
            let fg = [0, 1, 2].map(|i| run.color[i] * a + bg[i] * (1.0 - a));
            seen.push((ratio(fg, bg), [r, g, b]));
        }
    }
    if seen.is_empty() {
        return None;
    }
    seen.sort_by(|a, b| a.0.total_cmp(&b.0));
    Some(seen[((seen.len() as f64 * SPARE).floor() as usize).min(seen.len() - 1)])
}

/// A node's worst reading in a format, and every state it fails in.
struct Worst {
    ratio: f64,
    needs: f64,
    /// The state of the worst reading, by index.
    index: usize,
    background: [u8; 3],
    text: [f64; 3],
    size: f32,
    states: Vec<String>,
}

/// E110 and E111 over every state of the deck as laid out in this format.
pub fn check(cx: &Cx, backdrop: &mut dyn Backdrop) -> Result<Vec<Finding>, EngineError> {
    let scale = SIDE / cx.deck.canvas.width.min(cx.deck.canvas.height) as f32;
    // Each node once per format: its worst state, and every state it fails in.
    let mut worst: BTreeMap<String, Worst> = BTreeMap::new();
    for state in cx.states {
        let (runs, ids) = texts(cx, state);
        if runs.is_empty() {
            continue;
        }
        let rest = state.slot.start + state.slot.span;
        let shader = state.scene.nodes.iter().any(|n| matches!(n.content, Content::Shader(_)));
        let mut times = vec![rest];
        if shader && state.slot.hold > 0.0 {
            times.push(rest + state.slot.hold);
        }
        let mut failed: BTreeSet<&str> = BTreeSet::new();
        for t in times {
            let mut dl = state.scene.draw_at(t / 1000.0);
            strip(&mut dl.ops, &ids);
            // Nothing but the surface behind the text: its color is the background, and
            // there is nothing to paint.
            let px = if dl.ops.len() == 1 {
                let [w, h] = dl.viewport.map(|v| (v * scale).round().max(1.0) as u32);
                let [r, g, b, _] = state.scene.surface.0;
                Pixels { width: w, height: h, rgba: [r, g, b, 255].repeat((w * h) as usize) }
            } else {
                backdrop.paint(&dl, scale).map_err(|e| EngineError::Layout(format!("painting for contrast: {e}")))?
            };
            for run in &runs {
                let Some((r, bg)) = judge(run, &px, scale) else { continue };
                let needs = if run.display { 3.0 } else { 4.5 };
                if r >= needs {
                    continue;
                }
                failed.insert(&run.node);
                let reading = Worst {
                    ratio: r,
                    needs,
                    index: state.index,
                    background: bg,
                    text: run.color,
                    size: run.size,
                    states: Vec::new(),
                };
                match worst.get_mut(&run.node) {
                    Some(w) if r < w.ratio => *w = Worst { states: std::mem::take(&mut w.states), ..reading },
                    Some(_) => {}
                    None => {
                        worst.insert(run.node.clone(), reading);
                    }
                }
            }
        }
        for node in failed {
            let states = &mut worst.get_mut(node).expect("failed nodes are recorded").states;
            if !states.contains(&state.snapshot.state_id) {
                states.push(state.snapshot.state_id.clone());
            }
        }
    }
    let hex = |c: [u8; 3]| format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]);
    Ok(worst
        .into_iter()
        .map(|(node, Worst { ratio: r, needs, index, background: bg, text: fg, size, states })| {
            let (code, class) = if needs > 3.0 { ("E110", "body") } else { ("E111", "display") };
            let fg = hex(fg.map(|c| (c * 255.0).round() as u8));
            cx.finding(
                code,
                Severity::Error,
                &cx.states[index],
                format!("text `{node}` has {r:.2}:1 contrast with what is behind it; {class} text needs {needs}:1"),
            )
            .at(cx.node_path(&node))
            .node(node)
            .measure(json!({
                "ratio": (r * 100.0).round() / 100.0, "needs": needs, "text": fg, "background": hex(bg),
                "size": size, "states": states,
            }))
            .hint("Set it in a color role with more contrast against what is behind it, or change what is behind it.")
        })
        .collect())
}
