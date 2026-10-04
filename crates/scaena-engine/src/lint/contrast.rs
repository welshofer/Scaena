//! E110, E111: text whose contrast with what lies behind it is below WCAG's: 4.5:1 for
//! body text (E110), 3:1 for display text (E111).
//!
//! - **Text** is a text node's, a table's cells', and a chart's: its category and value
//!   labels, axis labels, titles, series names, and annotations, in their colors, at the
//!   opacity a highlight dims them to.
//! - **Behind it** is the state at rest with its text taken away, painted by the
//!   client's painter ([`Backdrop`]): the surface, shapes, images, shaders, and charts'
//!   marks, at the opacity they have. A shader is judged at rest and at the end of the
//!   state's hold, where it has moved most.
//! - Each run of text is judged over the pixels under its glyphs, its own color laid on
//!   each at its alpha times its node's opacity. A pixel counts by how much of it the
//!   glyphs cover, painted on their own, so a rule or a mark beside the letters (under a
//!   baseline, past a descender) does not. A run fails when more than 2% of its ink is
//!   below the line: noise in a background may dip under it, a band of it may not. Text
//!   at no opacity is not read, so not judged.
//! - **Display** is WCAG's large text: 24 px or more, or 18.67 px at weight 700 or more,
//!   on a screen whose shorter side is 1080 px (a 1920 × 1080 canvas unit is a pixel).
//!   Everything else is body.
//! - A node is reported once per format, at its worst; a chart once for each kind of
//!   text in it that fails, since each kind is set in its own role.

use super::{Cx, Laid};
use crate::EngineError;
use crate::charts::ChartText;
use crate::sample::Content;
use crate::text::TextLayout;
use scaena_core::displaylist::{Blend, Color, DisplayList, FillRule, Op, Paint, Path};
use scaena_core::lint::{Backdrop, Finding, Pixels, Severity};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// The share of a run's pixels that may fall below the line.
const SPARE: f64 = 0.02;

/// Pixels on the canvas's shorter side when painting what text sits on.
const SIDE: f32 = 540.0;

/// An sRGB channel, 0 to 1, in linear light, as WCAG defines it.
fn lin(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { libm::pow((c + 0.055) / 1.055, 2.4) }
}

/// WCAG relative luminance of an sRGB color: what [`Ratios`] reads from its tables.
#[cfg(test)]
fn luminance([r, g, b]: [f64; 3]) -> f64 {
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// WCAG contrast ratio of two sRGB colors: what [`Ratios::over`] gives.
#[cfg(test)]
fn ratio(a: [f64; 3], b: [f64; 3]) -> f64 {
    contrast(luminance(a), luminance(b))
}

fn contrast(la: f64, lb: f64) -> f64 {
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The contrast of text in `color` at `alpha` over each pixel it sits on: the WCAG ratio
/// of the text blended over the pixel and the pixel, from tables. A pixel's channels are
/// bytes, so each channel's linear light, the background's and the blended text's alike,
/// takes one of 256 values, each worked out once by the arithmetic a pixel would take: the
/// same numbers, without two `pow`s for each channel of each pixel.
struct Ratios {
    /// Per channel, the text's linear light over a background byte.
    fg: [[f64; 256]; 3],
    /// A background byte's linear light.
    bg: [f64; 256],
}

impl Ratios {
    fn new(color: [f64; 3], alpha: f64) -> Self {
        let byte = |c: usize| f64::from(c as u8) / 255.0;
        let fg = [0, 1, 2].map(|i| std::array::from_fn(|c| lin(color[i] * alpha + byte(c) * (1.0 - alpha))));
        Ratios { fg, bg: std::array::from_fn(|c| lin(byte(c))) }
    }

    /// The contrast over the pixel `[r, g, b]`.
    fn over(&self, [r, g, b]: [u8; 3]) -> f64 {
        let [r, g, b] = [r, g, b].map(usize::from);
        let fg = 0.2126 * self.fg[0][r] + 0.7152 * self.fg[1][g] + 0.0722 * self.fg[2][b];
        let bg = 0.2126 * self.bg[r] + 0.7152 * self.bg[g] + 0.0722 * self.bg[b];
        contrast(fg, bg)
    }
}

/// What a finding is about: a node, and in a chart, the kind of text.
type Subject = (String, Option<ChartText>);

/// One run of text to judge: where its glyphs are, its color and alpha, and its class;
/// in a chart, what its text says.
struct Run {
    subject: Subject,
    said: Option<String>,
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

/// The runs a state's text draws; the nodes that draw only text (text nodes, tables),
/// whose layers the backdrop leaves out; and the charts, whose glyphs it leaves out.
fn texts(cx: &Cx, state: &Laid) -> (Vec<Run>, BTreeSet<String>, BTreeSet<String>) {
    let screen = to_screen(cx);
    let mut out = Vec::new();
    let (mut ids, mut charts) = (BTreeSet::new(), BTreeSet::new());
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
        let at = |cell: &[f32; 4], o: [f32; 2]| [cell[0] + o[0], cell[1] + o[1]];
        // Each text the node sets: where, at what opacity of its own, and in a chart,
        // what it is for.
        let placed: Vec<(&TextLayout, [f32; 2], f32, Option<ChartText>)> = match &node.content {
            Content::Text(placed) => vec![(&placed.text, placed.origin, 1.0, None)],
            Content::Table { cell, table } => {
                table.header.iter().chain(&table.cells).map(|c| (&c.text, at(cell, c.origin), 1.0, None)).collect()
            }
            Content::Chart { cell, chart } => {
                chart.texts().map(|(part, l)| (&l.text, at(cell, l.origin), l.opacity, Some(part))).collect()
            }
            _ => continue,
        };
        match node.content {
            Content::Chart { .. } => charts.insert(node.id.clone()),
            _ => ids.insert(node.id.clone()),
        };
        for (t, origin, own, part) in placed {
            let said = part.map(|_| t.text.replace('\u{AD}', ""));
            for (rect, color, size) in runs(t, origin) {
                let px = size * screen;
                let [r, g, b, a] = color.0.map(|c| f64::from(c) / 255.0);
                out.push(Run {
                    subject: (node.id.clone(), part),
                    said: said.clone(),
                    rect,
                    color: [r, g, b],
                    alpha: a * f64::from(own) * opacity,
                    display: px >= 24.0 || (px >= 18.67 && t.weight >= 700.0),
                    size: px,
                });
            }
        }
    }
    (out, ids, charts)
}

/// `ops` without the layers of `ids`, and without the glyphs in the layers of `charts`,
/// wherever they sit.
fn strip(ops: &mut Vec<Op>, ids: &BTreeSet<String>, charts: &BTreeSet<String>) {
    ops.retain(|op| !matches!(op, Op::Layer { node: Some(id), .. } if ids.contains(id)));
    for op in ops {
        match op {
            Op::Layer { node: Some(id), ops, .. } if charts.contains(id) => unglyph(ops),
            Op::Layer { ops, .. } => strip(ops, ids, charts),
            _ => {}
        }
    }
}

/// `ops` without their glyphs, at any depth.
fn unglyph(ops: &mut Vec<Op>) {
    ops.retain(|op| !matches!(op, Op::Glyphs { .. }));
    for op in ops {
        if let Op::Layer { ops, .. } = op {
            unglyph(ops);
        }
    }
}

/// The text of `dl` alone, opaque white on black: how much of each pixel its glyphs cover.
fn coverage(dl: &DisplayList) -> DisplayList {
    let mut ink = DisplayList::new(dl.viewport);
    ink.fonts = dl.fonts.clone();
    ink.ops.push(Op::Fill {
        path: Path::rect([0.0, 0.0, dl.viewport[0], dl.viewport[1]]),
        rule: FillRule::NonZero,
        paint: Paint::Solid(Color([0, 0, 0, 255])),
    });
    ink.ops.extend(dl.ops.iter().filter_map(glyphs));
    ink
}

/// `op`'s glyphs alone, opaque white, in layers at full opacity.
fn glyphs(op: &Op) -> Option<Op> {
    match op {
        Op::Glyphs { .. } => {
            let mut white = op.clone();
            if let Op::Glyphs { paint, .. } = &mut white {
                *paint = Paint::Solid(Color([255; 4]));
            }
            Some(white)
        }
        Op::Layer { node, cell, transform, clip, ops, .. } => {
            let ops: Vec<Op> = ops.iter().filter_map(glyphs).collect();
            (!ops.is_empty()).then(|| Op::Layer {
                node: node.clone(),
                cell: *cell,
                transform: *transform,
                opacity: 1.0,
                blend: Blend::Normal,
                clip: clip.clone(),
                ops,
            })
        }
        _ => None,
    }
}

/// How a run reads over `px`: its contrast at the `SPARE` quantile of the pixels under
/// its glyphs, each counted by how much of it `ink` covers (all of each without it),
/// and the background there.
fn judge(run: &Run, ratios: &Ratios, px: &Pixels, ink: Option<&Pixels>, scale: f32) -> Option<(f64, [u8; 3])> {
    let [x, y, w, h] = run.rect.map(|v| v * scale);
    let (x0, y0) = (x.floor().max(0.0) as u32, y.floor().max(0.0) as u32);
    let (x1, y1) = (((x + w).ceil() as u32).min(px.width), ((y + h).ceil() as u32).min(px.height));
    let mut seen: Vec<(f64, f64, [u8; 3])> = Vec::new();
    for py in y0..y1 {
        for pxl in x0..x1 {
            let cover = ink.map_or(1.0, |m| f64::from(m.pixel(pxl, py)[0]) / 255.0);
            if cover <= 0.0 {
                continue;
            }
            let [r, g, b, _] = px.pixel(pxl, py);
            seen.push((ratios.over([r, g, b]), cover, [r, g, b]));
        }
    }
    let total: f64 = seen.iter().map(|s| s.1).sum();
    if total <= 0.0 {
        return None;
    }
    seen.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut sum = 0.0;
    for &(r, cover, bg) in &seen {
        sum += cover;
        if sum > total * SPARE {
            return Some((r, bg));
        }
    }
    seen.last().map(|&(r, _, bg)| (r, bg))
}

/// A node's worst reading in a format, and every state it fails in.
struct Worst {
    ratio: f64,
    /// In a chart, what the text of the worst reading says.
    said: Option<String>,
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
    // Each node (each kind of a chart's text) once per format: its worst state, and
    // every state it fails in.
    let mut worst: BTreeMap<Subject, Worst> = BTreeMap::new();
    // A deck's text comes in few colors: each color and alpha's tables, made once.
    let mut tables: BTreeMap<[u64; 4], Ratios> = BTreeMap::new();
    for state in cx.states {
        let (runs, ids, charts) = texts(cx, state);
        if runs.is_empty() {
            continue;
        }
        let rest = state.slot.start + state.slot.span;
        let shader = state.scene.nodes.iter().any(|n| matches!(n.content, Content::Shader(_)));
        let mut times = vec![rest];
        if shader && state.slot.hold > 0.0 {
            times.push(rest + state.slot.hold);
        }
        let mut failed: BTreeSet<&Subject> = BTreeSet::new();
        // Where the text's glyphs are: the same at every time, painted once.
        let mut ink: Option<Pixels> = None;
        let paint = |backdrop: &mut dyn Backdrop, dl: &DisplayList| {
            backdrop.paint(dl, scale).map_err(|e| EngineError::Layout(format!("painting for contrast: {e}")))
        };
        for t in times {
            let mut dl = state.scene.draw_at(t / 1000.0);
            let text = ink.is_none().then(|| coverage(&dl));
            strip(&mut dl.ops, &ids, &charts);
            // Nothing but the surface behind the text: its color is the background, every
            // pixel alike, and there is nothing to paint.
            let (px, ink) = if dl.ops.len() == 1 {
                // Sized as a painter sizes it, which refuses what it cannot hold.
                let [w, h] = dl.viewport.map(|v| (v * scale).round());
                if ![w, h].iter().all(|v| (1.0..=f32::from(u16::MAX)).contains(v)) {
                    return Err(EngineError::Layout(format!(
                        "painting for contrast: {w}×{h} px is not a raster this painter can make"
                    )));
                }
                let [w, h] = [w, h].map(|v| v as u32);
                let [r, g, b, _] = state.scene.surface.0;
                (Pixels { width: w, height: h, rgba: [r, g, b, 255].repeat((w * h) as usize) }, None)
            } else {
                if let Some(text) = text {
                    ink = Some(paint(backdrop, &text)?);
                }
                (paint(backdrop, &dl)?, ink.as_ref())
            };
            for run in runs.iter().filter(|r| r.alpha > 0.0) {
                let alpha = run.alpha.clamp(0.0, 1.0);
                let key = [run.color[0], run.color[1], run.color[2], alpha].map(f64::to_bits);
                let ratios = tables.entry(key).or_insert_with(|| Ratios::new(run.color, alpha));
                let Some((r, bg)) = judge(run, ratios, &px, ink, scale) else { continue };
                let needs = if run.display { 3.0 } else { 4.5 };
                if r >= needs {
                    continue;
                }
                failed.insert(&run.subject);
                let reading = Worst {
                    ratio: r,
                    said: run.said.clone(),
                    needs,
                    index: state.index,
                    background: bg,
                    text: run.color,
                    size: run.size,
                    states: Vec::new(),
                };
                match worst.get_mut(&run.subject) {
                    Some(w) if r < w.ratio => *w = Worst { states: std::mem::take(&mut w.states), ..reading },
                    Some(_) => {}
                    None => {
                        worst.insert(run.subject.clone(), reading);
                    }
                }
            }
        }
        for subject in failed {
            let states = &mut worst.get_mut(subject).expect("failed nodes are recorded").states;
            if !states.contains(&state.snapshot.state_id) {
                states.push(state.snapshot.state_id.clone());
            }
        }
    }
    let hex = |c: [u8; 3]| format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]);
    Ok(worst
        .into_iter()
        .map(|((node, part), Worst { ratio: r, said, needs, index, background: bg, text: fg, size, states })| {
            let (code, class) = if needs > 3.0 { ("E110", "body") } else { ("E111", "display") };
            let fg = hex(fg.map(|c| (c * 255.0).round() as u8));
            let reads = format!("{r:.2}:1 contrast with what is behind it; {class} text needs {needs}:1");
            let finding = match part {
                None => cx.finding(code, Severity::Error, cx.laid(index), format!("text `{node}` has {reads}")),
                Some(part) => cx.finding(
                    code,
                    Severity::Error,
                    cx.laid(index),
                    format!("chart `{node}`: its {} `{}` has {reads}", part.name(), said.as_deref().unwrap_or_default()),
                ),
            };
            let mut measure = json!({
                "ratio": (r * 100.0).round() / 100.0, "needs": needs, "text": fg, "background": hex(bg),
                "size": size, "states": states,
            });
            let hint = match part {
                None => "Set it in a color role with more contrast against what is behind it, or change what is behind it.",
                Some(part) => {
                    measure["part"] = part.name().into();
                    measure["label"] = said.into();
                    "Set the chart's text in a color that reads against what is behind it (the theme's `charts` roles, its palette for series names, `charts.annotation.dimmed` for text a highlight dims), or change what is behind it."
                }
            };
            finding.at(cx.node_path(&node)).node(node).measure(measure).hint(hint)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Ratios` gives what `judge` worked out for each pixel before it, [`ratio`] of the text
    /// blended over the pixel and the pixel, bit for bit: every byte of each channel, beside
    /// several of the others, for text opaque, clear, and between.
    #[test]
    fn ratios_by_table_are_ratio() {
        let texts =
            [([0.1, 0.5, 0.9], 1.0), ([1.0, 1.0, 1.0], 0.62), ([0.0, 0.0, 0.0], 0.0), ([0.93, 0.27, 0.04], 0.35)];
        let others = [0_u8, 1, 10, 77, 128, 200, 254, 255];
        for (color, alpha) in texts {
            let ratios = Ratios::new(color, alpha);
            for c in 0..=255_u8 {
                for (o, p) in others.iter().flat_map(|&o| others.iter().map(move |&p| (o, p))) {
                    for px in [[c, o, p], [o, c, p], [o, p, c]] {
                        let bg = px.map(|c| f64::from(c) / 255.0);
                        let fg = [0, 1, 2].map(|i| color[i] * alpha + bg[i] * (1.0 - alpha));
                        let want = ratio(fg, bg);
                        assert_eq!(ratios.over(px).to_bits(), want.to_bits(), "{px:?} under {color:?} at {alpha}");
                    }
                }
            }
        }
    }
}
