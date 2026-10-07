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
use crate::cascade;
use crate::charts::ChartText;
use crate::data::DataFiles;
use crate::render::{Engine, project};
use crate::sample::{Content, Timing};
use crate::text::TextLayout;
use crate::theme::Theme;
use scaena_core::displaylist::{Blend, Color, DisplayList, FillRule, Op, Paint, Path};
use scaena_core::document::{Deck, NodeType};
use scaena_core::lint::{Backdrop, Finding, Pixels, Severity};
use scaena_core::model::theme::Vocabulary;
use scaena_core::timeline::{Schedule, Slot, Timeline};
use scaena_core::tracking::Snapshot;
use scaena_core::validate::BundleFiles;
use serde_json::{Value, json};
use std::borrow::Cow;
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
    /// What its alpha carries besides its color's: its own opacity, its node's, its groups'.
    opacity: f64,
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
        // Where its transform and its containers' draw it (SPEC §3.3): each run over the
        // pixels around it there, at the size it is drawn.
        let map = state.scene.posed(&node.id, true);
        let moved = map != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let scale = scaena_core::pose::stretch(&map)[1] as f32;
        // Flattened to a line or a point, it sets nothing to read.
        if moved && scaena_core::pose::invert(&map).is_none() {
            continue;
        }
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
                let rect = if moved { scaena_core::pose::bounds(&map, rect) } else { rect };
                let px = size * scale * screen;
                let [r, g, b, a] = color.0.map(|c| f64::from(c) / 255.0);
                let opacity = f64::from(own) * opacity;
                out.push(Run {
                    subject: (node.id.clone(), part),
                    said: said.clone(),
                    rect,
                    color: [r, g, b],
                    alpha: a * opacity,
                    opacity,
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

/// What lies under the glyphs of a run whose box is `rect` in `px`: each color, and how much
/// of it the glyphs cover, in 255ths of a pixel, by `ink` (all of each pixel without it), in
/// the colors' order.
fn under(rect: [f32; 4], px: &Pixels, ink: Option<&Pixels>, scale: f32) -> Vec<([u8; 3], u64)> {
    let [x, y, w, h] = rect.map(|v| v * scale);
    let (x0, y0) = (x.floor().max(0.0) as u32, y.floor().max(0.0) as u32);
    let (x1, y1) = (((x + w).ceil() as u32).min(px.width), ((y + h).ceil() as u32).min(px.height));
    // A run's box holds many pixels and few colors, so it is the colors that are judged and
    // sorted, and the sums are exact whatever their order.
    let mut covered: BTreeMap<[u8; 3], u64> = BTreeMap::new();
    // Neighbors mostly share a color: each streak of one is counted, then entered once.
    let mut streak: Option<([u8; 3], u64)> = None;
    for py in y0..y1 {
        for pxl in x0..x1 {
            let cover = ink.map_or(255, |m| m.pixel(pxl, py)[0]);
            if cover == 0 {
                continue;
            }
            let [r, g, b, _] = px.pixel(pxl, py);
            match &mut streak {
                Some((color, sum)) if *color == [r, g, b] => *sum += u64::from(cover),
                _ => {
                    if let Some((color, sum)) = streak.replace(([r, g, b], u64::from(cover))) {
                        *covered.entry(color).or_default() += sum;
                    }
                }
            }
        }
    }
    if let Some((color, sum)) = streak {
        *covered.entry(color).or_default() += sum;
    }
    covered.into_iter().collect()
}

/// How text reads over what lies `under` it: its contrast at the `SPARE` quantile of those
/// colors, each counted by how much of it the glyphs cover, and the color there.
fn read(ratios: &Ratios, under: &[([u8; 3], u64)]) -> Option<(f64, [u8; 3])> {
    let total: u64 = under.iter().map(|&(_, cover)| cover).sum();
    if total == 0 {
        return None;
    }
    let mut seen: Vec<(f64, u64, [u8; 3])> = under.iter().map(|&(bg, cover)| (ratios.over(bg), cover, bg)).collect();
    scaena_core::sort::by(&mut seen, |a, b| a.0.total_cmp(&b.0));
    let (mut sum, spare) = (0, total as f64 * SPARE);
    for &(r, cover, bg) in &seen {
        sum += cover;
        if sum as f64 > spare {
            return Some((r, bg));
        }
    }
    seen.last().map(|&(r, _, bg)| (r, bg))
}

/// Whether text reads over what lies `under` it at `needs`: as [`read`] judges it, without
/// sorting. Its contrast at the `SPARE` quantile is below `needs` exactly where more than
/// `SPARE` of its glyphs' coverage lies over colors it reads below `needs` on.
fn reads(ratios: &Ratios, under: &[([u8; 3], u64)], needs: f64) -> bool {
    let total: u64 = under.iter().map(|&(_, cover)| cover).sum();
    let below: u64 = under.iter().filter(|&&(bg, _)| ratios.over(bg) < needs).map(|&(_, cover)| cover).sum();
    below as f64 <= total as f64 * SPARE
}

/// When `state`'s text is judged, ms on the global timeline: at rest, and where a shader
/// draws, at the end of the state's hold too, where it has moved most.
fn times(state: &Laid) -> Vec<f64> {
    let rest = state.slot.start + state.slot.span;
    let shader = state.scene.nodes.iter().any(|n| matches!(n.content, Content::Shader(_)));
    let mut times = vec![rest];
    if shader && state.slot.hold > 0.0 {
        times.push(rest + state.slot.hold);
    }
    times
}

/// What lies behind `state`'s text at `t` ms, painted at `scale`: the state with the layers of
/// `ids` and the glyphs of `charts` taken away. The first time anything but the surface lies
/// behind it, the coverage of its glyphs, the same at every time, is painted into `ink`; over
/// the surface alone, every pixel is its color, nothing is painted, and coverage does not
/// count (`false`).
fn behind(
    state: &Laid,
    (ids, charts): (&BTreeSet<String>, &BTreeSet<String>),
    backdrop: &mut dyn Backdrop,
    scale: f32,
    t: f64,
    ink: &mut Option<Pixels>,
) -> Result<(Pixels, bool), EngineError> {
    let paint = |backdrop: &mut dyn Backdrop, dl: &DisplayList| {
        backdrop.paint(dl, scale).map_err(|e| EngineError::Layout(format!("painting for contrast: {e}")))
    };
    let mut dl = state.scene.draw_at(t / 1000.0);
    let text = ink.is_none().then(|| coverage(&dl));
    strip(&mut dl.ops, ids, charts);
    if dl.ops.len() == 1 {
        // Sized as a painter sizes it, which refuses what it cannot hold.
        let [w, h] = dl.viewport.map(|v| (v * scale).round());
        if ![w, h].iter().all(|v| (1.0..=f32::from(u16::MAX)).contains(v)) {
            return Err(EngineError::Layout(format!(
                "painting for contrast: {w}×{h} px is not a raster this painter can make"
            )));
        }
        let [w, h] = [w, h].map(|v| v as u32);
        let [r, g, b, _] = state.scene.surface.0;
        return Ok((Pixels { width: w, height: h, rgba: [r, g, b, 255].repeat((w * h) as usize) }, false));
    }
    if let Some(text) = text {
        *ink = Some(paint(backdrop, &text)?);
    }
    Ok((paint(backdrop, &dl)?, true))
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

/// A run of a node's own text that read too faintly, as lint read it: what lies under its
/// glyphs, the opacity its color is laid on at, and what it needs. A color that fixes the node
/// reads over this at the least (PLAN 2.82).
pub(super) struct Faint {
    under: Vec<([u8; 3], u64)>,
    opacity: f64,
    needs: f64,
}

/// E110 and E111 over every state of the deck as laid out in this format; and into `faint`,
/// each run of a node's own text that fails, by its node.
pub fn check(
    cx: &Cx,
    backdrop: &mut dyn Backdrop,
    faint: &mut BTreeMap<String, Vec<Faint>>,
) -> Result<Vec<Finding>, EngineError> {
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
        let mut failed: BTreeSet<&Subject> = BTreeSet::new();
        // Where the text's glyphs are: the same at every time, painted once.
        let mut ink: Option<Pixels> = None;
        for t in times(state) {
            let (px, counts) = behind(state, (&ids, &charts), backdrop, scale, t, &mut ink)?;
            let ink = ink.as_ref().filter(|_| counts);
            for run in runs.iter().filter(|r| r.alpha > 0.0) {
                let alpha = run.alpha.clamp(0.0, 1.0);
                let key = [run.color[0], run.color[1], run.color[2], alpha].map(f64::to_bits);
                let ratios = tables.entry(key).or_insert_with(|| Ratios::new(run.color, alpha));
                let under = under(run.rect, &px, ink, scale);
                let Some((r, bg)) = read(ratios, &under) else { continue };
                let needs = if run.display { 3.0 } else { 4.5 };
                if r >= needs {
                    continue;
                }
                failed.insert(&run.subject);
                if run.subject.1.is_none() {
                    let reading = Faint { under, opacity: run.opacity, needs };
                    faint.entry(run.subject.0.clone()).or_default().push(reading);
                }
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

/// A color of the theme, under the name an inspector offers it by.
type Named = (String, Color);

/// A bundle with no files: a `choose` of a color reads none.
struct Nothing;

impl BundleFiles for Nothing {
    fn exists(&self, _: &str) -> bool {
        false
    }

    fn read_text(&self, _: &str) -> Option<String> {
        None
    }
}

/// One run of a text a fix would recolor, as it reads now: what lies under its glyphs, and
/// whether it takes the node's color, or keeps a color of its own (a run `style_text` set).
struct Recolored {
    under: Vec<([u8; 3], u64)>,
    takes: bool,
    color: [f64; 3],
    alpha: f64,
    opacity: f64,
    needs: f64,
}

/// E110's and E111's fix (PLAN 2.82): for each text node that reads too faintly, the color of
/// the theme that reads best against what lies behind it, chosen as an inspector chooses one
/// (`choose` of `style/color`, written where the node's color lives). It is kept where every
/// run of the node reads in it, in every state and format the choice reaches. Text is taken
/// away from what lies behind any text, so what a text's color changes is the text alone, and
/// each of the theme's colors is judged over the pixels painted once.
pub(super) fn recolor(
    engine: &mut Engine,
    (deck, theme, data): (&Deck, &Theme, &DataFiles),
    timelines: &[(Option<&str>, Timeline)],
    faint: &BTreeMap<String, Vec<Faint>>,
    backdrop: &mut dyn Backdrop,
    findings: &mut [Finding],
) -> Result<(), EngineError> {
    // Each text node a finding is about, and the state its color is chosen in: where it reads
    // worst in the deck's own format, else in the first format it fails in.
    let mut nodes: BTreeMap<String, String> = BTreeMap::new();
    for f in findings.iter().filter(|f| matches!(f.code.as_str(), "E110" | "E111")) {
        let (Some(node), Some(state)) = (&f.node, &f.state) else { continue };
        let text = deck.nodes.get(node).is_some_and(|n| n.node_type == NodeType::Text);
        // A chart's text is set in the theme's roles for it, a table's in its own.
        if !text || f.measure.as_ref().is_some_and(|m| m.get("part").is_some()) {
            continue;
        }
        match nodes.get_mut(node) {
            Some(at) if f.format.is_none() => state.clone_into(at),
            Some(_) => {}
            None => drop(nodes.insert(node.clone(), state.clone())),
        }
    }
    if nodes.is_empty() {
        return Ok(());
    }
    // The deck's JSON, which a choice compiles against, made once a color passes the screen.
    let mut doc: Option<Value> = None;
    // The theme's colors, each once, in the order an inspector offers them: its roles, then
    // its tokens.
    let mut colors: Vec<Named> = Vec::new();
    for name in theme.names(Vocabulary::Color) {
        if let Ok(color) = theme.color(&name)
            && !colors.iter().any(|(_, c)| *c == color)
        {
            colors.push((name, color));
        }
    }
    let mut tables: BTreeMap<[u64; 4], Ratios> = BTreeMap::new();
    for (node, state) in nodes {
        // A color fixes the node only where its runs that failed read in it, were each to take
        // it: those colors alone are judged where the choice reaches, which lays states out and
        // paints them. Text over a shader's whole gradient reads in none, and costs no more.
        let failed = faint.get(&node).map_or(&[][..], Vec::as_slice);
        let reading: Vec<&Named> = colors
            .iter()
            .filter(|(_, color)| {
                let [r, g, b, a] = color.0.map(|v| f64::from(v) / 255.0);
                failed.iter().all(|run| {
                    let alpha = (a * run.opacity).clamp(0.0, 1.0);
                    let key = [r, g, b, alpha].map(f64::to_bits);
                    let ratios = tables.entry(key).or_insert_with(|| Ratios::new([r, g, b], alpha));
                    alpha <= 0.0 || reads(ratios, &run.under, run.needs)
                })
            })
            .collect();
        if failed.is_empty() || reading.is_empty() {
            continue;
        }
        let doc = match &mut doc {
            Some(doc) => doc,
            none => none.insert(serde_json::to_value(deck).map_err(|e| EngineError::Layout(e.to_string()))?),
        };
        let Some(fix) =
            recolored(engine, (deck, doc, theme, data), timelines, (&colors, &reading), backdrop, &node, &state)
        else {
            continue;
        };
        let about = |f: &&mut Finding| matches!(f.code.as_str(), "E110" | "E111") && f.node.as_deref() == Some(&node);
        for f in findings.iter_mut().filter(about) {
            f.fix = Some(fix.clone());
        }
    }
    Ok(())
}

/// The fix for text `node` that reads too faintly in `state`: the patch that chooses it the
/// theme's color that reads best wherever the choice reaches, if one reads there. `doc` is the
/// deck's JSON, and `timelines` each format's timeline, as lint laid it out.
fn recolored(
    engine: &mut Engine,
    (deck, doc, theme, data): (&Deck, &Value, &Theme, &DataFiles),
    timelines: &[(Option<&str>, Timeline)],
    (colors, reading): (&[Named], &[&Named]),
    backdrop: &mut dyn Backdrop,
    node: &str,
    state: &str,
) -> Option<Vec<Value>> {
    let choose = |name: &str| {
        let op = json!({ "op": "choose", "node": node, "state": state, "prop": "style/color", "value": name });
        scaena_core::patch::compile_alone(doc, &[op], &Nothing).ok()
    };
    let snaps = scaena_core::resolve_states(deck).ok()?;
    let index = snaps.iter().position(|s| s.state_id == state)?;
    // The node's runs as `state` sets them now. A color none of them has, chosen, changes the
    // color of each run that takes the node's: the probe that tells them from runs that keep a
    // color of their own.
    let scene = engine.scene(deck, theme, data, &snaps[index]).ok()?;
    let laid = rested(deck, &snaps, index, scene, None);
    let cx = Cx { deck, theme, format: None, states: std::slice::from_ref(&laid) };
    let now = mine(texts(&cx, &laid).0, node);
    let opacity = now.first()?.opacity;
    let probe = colors.iter().find(|(_, c)| {
        let [r, g, b, a] = c.0.map(|v| f64::from(v) / 255.0);
        !now.iter().any(|run| (run.color, run.alpha) == ([r, g, b], a * opacity))
    })?;
    let probing = probe.1.0.map(|v| f64::from(v) / 255.0);
    let probed = Deck::from_value(&choose(&probe.0)?.doc).ok()?;
    // The states the choice reaches: each that shows the node, and shows it otherwise once the
    // node takes the probe's color.
    let after = scaena_core::resolve_states(&probed).ok()?;
    let shown = |d: &Deck, s: &Snapshot| cascade::with_overrides(d, s).nodes.get(node).cloned();
    let reached: Vec<usize> = (0..snaps.len())
        .filter(|&k| shown(deck, &snaps[k]).is_some_and(|was| Some(was) != shown(&probed, &after[k])))
        .collect();
    // How each run of the node reads now in each state the choice reaches, in every format.
    let mut runs: Vec<Recolored> = Vec::new();
    let mut formats: Vec<Option<&str>> = vec![None];
    formats.extend(deck.formats.iter().map(|f| Some(f.as_str())));
    for format in formats {
        let (d, t) = project(deck, theme, format).ok()?;
        // A listed format of the deck's own shape lays out the same.
        if format.is_some() && matches!(d, Cow::Borrowed(_)) {
            continue;
        }
        let (pd, pt) = project(&probed, theme, format).ok()?;
        let scale = SIDE / d.canvas.width.min(d.canvas.height) as f32;
        let (snaps, after) = (scaena_core::resolve_states(&d).ok()?, scaena_core::resolve_states(&pd).ok()?);
        // Where each state falls on the timeline, which a shader's clock reads.
        let slots = &timelines.iter().find(|(f, _)| *f == format)?.1.slots;
        for &k in &reached {
            let scene = engine.scene(&d, &t, data, &snaps[k]).ok()?;
            let laid = rested(&d, &snaps, k, scene, slots.get(k).cloned());
            let probe = rested(&pd, &after, k, engine.scene(&pd, &pt, data, &after[k]).ok()?, None);
            let cx = Cx { deck: &d, theme: &t, format, states: std::slice::from_ref(&laid) };
            let pcx = Cx { deck: &pd, theme: &pt, format, states: std::slice::from_ref(&probe) };
            let (all, ids, charts) = texts(&cx, &laid);
            let (was, probed) = (mine(all, node), mine(texts(&pcx, &probe).0, node));
            // A color moves no glyph: runs that do not line up were split or joined by it, and
            // what lies under them was not read. A run in the probe's color already (another
            // state's role can give it) cannot say whether it takes the node's.
            let [r, g, b, alpha] = probing;
            let unclear = was.iter().any(|run| (run.color, run.alpha) == ([r, g, b], alpha * run.opacity));
            let moved = was.len() != probed.len() || was.iter().zip(&probed).any(|(a, b)| a.rect != b.rect);
            if unclear || moved {
                return None;
            }
            let mut ink: Option<Pixels> = None;
            for time in times(&laid) {
                let (px, counts) = behind(&laid, (&ids, &charts), backdrop, scale, time, &mut ink).ok()?;
                let ink = ink.as_ref().filter(|_| counts);
                for (run, probe) in was.iter().zip(&probed).filter(|(run, _)| run.opacity > 0.0) {
                    runs.push(Recolored {
                        under: under(run.rect, &px, ink, scale),
                        takes: (run.color, run.alpha) != (probe.color, probe.alpha),
                        color: run.color,
                        alpha: run.alpha,
                        opacity: run.opacity,
                        needs: if run.display { 3.0 } else { 4.5 },
                    });
                }
            }
        }
    }
    // The color whose worst reading is best, among those every run reads in; the first such.
    let mut tables: BTreeMap<[u64; 4], Ratios> = BTreeMap::new();
    let mut best: Option<(f64, &str)> = None;
    'colors: for &(name, color) in reading {
        let [r, g, b, a] = color.0.map(|v| f64::from(v) / 255.0);
        let mut worst = f64::INFINITY;
        for run in &runs {
            let (color, alpha) = match run.takes {
                true => ([r, g, b], a * run.opacity),
                false => (run.color, run.alpha),
            };
            if alpha <= 0.0 {
                continue;
            }
            let alpha = alpha.clamp(0.0, 1.0);
            let key = [color[0], color[1], color[2], alpha].map(f64::to_bits);
            let ratios = tables.entry(key).or_insert_with(|| Ratios::new(color, alpha));
            let Some((ratio, _)) = read(ratios, &run.under) else { continue };
            if ratio < run.needs {
                continue 'colors;
            }
            worst = worst.min(ratio);
        }
        if worst.is_finite() && best.is_none_or(|(w, _)| worst > w) {
            best = Some((worst, name));
        }
    }
    let patch = choose(best?.1)?.patch;
    (!patch.is_empty()).then(|| patch.iter().filter_map(|op| serde_json::to_value(op).ok()).collect())
}

/// The runs of text node `node` among `runs`.
fn mine(runs: Vec<Run>, node: &str) -> Vec<Run> {
    runs.into_iter().filter(|r| r.subject.0 == node && r.subject.1.is_none()).collect()
}

/// State `index` of `deck`, laid out as `scene`, at rest: what contrast reads, at its place on
/// the timeline where a shader's clock needs it.
fn rested(deck: &Deck, snaps: &[Snapshot], index: usize, scene: crate::sample::Scene, slot: Option<Slot>) -> Laid {
    let id = snaps[index].state_id.clone();
    Laid {
        index,
        snapshot: cascade::with_overrides(deck, &snaps[index]),
        scene,
        slot: slot.unwrap_or(Slot { state: id, start: 0.0, span: 0.0, hold: 0.0 }),
        schedule: Schedule { transition: Timing::CUT.clock(), cues: vec![], span: 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `reads` says what `read` says, without sorting: text reads at `needs` exactly where its
    /// contrast at the `SPARE` quantile does. Over colors drawn from a fixed sequence, each
    /// covered from a sliver to a whole pixel, for text light, dark, and between, at each bar.
    #[test]
    fn reads_is_read_at_the_bar() {
        let mut seed = 0x2545_f491_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        // How many times it read, and how many it did not: the test holds both.
        let mut verdicts = [0, 0];
        for (color, alpha) in [([0.95, 0.94, 0.91], 1.0), ([0.06, 0.05, 0.08], 1.0), ([0.5, 0.42, 0.37], 0.8)] {
            let ratios = Ratios::new(color, alpha);
            for size in [1, 2, 7, 50, 400] {
                for _ in 0..40 {
                    let mut under: BTreeMap<[u8; 3], u64> = BTreeMap::new();
                    for _ in 0..size {
                        let v = next();
                        *under.entry([v as u8, (v >> 8) as u8, (v >> 16) as u8]).or_default() += 1 + (v >> 24) % 255;
                    }
                    let under: Vec<([u8; 3], u64)> = under.into_iter().collect();
                    for needs in [3.0, 4.5] {
                        let want = read(&ratios, &under).is_none_or(|(r, _)| r >= needs);
                        assert_eq!(reads(&ratios, &under, needs), want, "{size} colors at {needs}");
                        verdicts[usize::from(want)] += 1;
                    }
                }
            }
        }
        assert!(verdicts.iter().all(|&n| n > 100), "both verdicts, often: {verdicts:?}");
    }

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
