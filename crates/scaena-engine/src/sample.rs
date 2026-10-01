//! Sampling (SPEC §5): a transition interpolates *resolved geometry* between two
//! snapshots laid out once, so a frame never lays anything out.
//!
//! A [`Scene`] is one snapshot after layout. A [`Transition`] holds the scene being
//! left, the scene being entered, the timing, and a plan of how every node and chart
//! mark gets from one to the other, made once when the transition is built.
//! [`Transition::frame`] takes `&self` and owns no fonts or layout engine: frames
//! only sample (SPEC §5, CLAUDE.md invariant 3), by construction.
//!
//! Phase 0 rules (PLAN 0.10; the rest of SPEC §3.9 is PLAN 1.11–1.12):
//! - Nodes match by id. Text whose layout is unchanged moves; changed text
//!   cross-fades (word-level text morphs are PLAN 1.12). A chart's marks, lines, and
//!   labels match by key and interpolate, so a change of kind morphs bars into points.
//! - A node only in the target fades in; one only in the source fades out.
//! - Numbers interpolate linearly; colors in Oklab (SPEC §3.9), through `libm`, whose
//!   pure-Rust math gives the same bits on every platform.
//! - At `t ≤ 0` the frame is the source scene at rest, and at `t ≥ duration` the
//!   target at rest, exactly: both ends draw a scene, not an interpolation.

use crate::EngineError;
use crate::charts::{ChartLayout, Label, RoundRect, Rule, Series, lerp};
use crate::render::PlacedText;
use crate::text::TextLayout;
use crate::theme::Theme;
use scaena_core::displaylist::{Blend, Cap, Color, DisplayList, FillRule, Join, Op, Paint, Path, PathEl, Point, Rect};
use scaena_core::timeline::CubicBezier;
use serde_json::Value;

/// One snapshot after layout: what a frame at rest draws, and what a transition
/// into or out of the state interpolates.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub state: String,
    pub canvas: [f32; 2],
    /// The theme's surface, painted under everything.
    pub surface: Color,
    /// Visible nodes in paint order.
    pub nodes: Vec<SceneNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneNode {
    pub id: String,
    /// Paint order: `z`, then position in the scene graph (`order`).
    pub z: i64,
    pub order: usize,
    pub opacity: f32,
    /// The node's `transition` property (SPEC §3.9).
    pub policy: Policy,
    pub content: Content,
}

/// What a node draws, laid out.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(PlacedText),
    Chart { cell: Rect, chart: ChartLayout },
}

/// A node's own transition policy: `morph` (the default), `crossfade`, or `cut`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Policy {
    #[default]
    Morph,
    Crossfade,
    Cut,
}

impl Policy {
    pub fn parse(v: Option<&Value>) -> Result<Policy, EngineError> {
        let Some(v) = v else { return Ok(Policy::Morph) };
        match v.as_str() {
            Some("morph") => Ok(Policy::Morph),
            Some("crossfade") => Ok(Policy::Crossfade),
            Some("cut") => Ok(Policy::Cut),
            _ => Err(EngineError::Layout(format!("`transition` {v}: expected morph, crossfade, or cut"))),
        }
    }
}

impl Scene {
    /// The state at rest.
    pub fn draw(&self) -> DisplayList {
        let mut dl = self.ground();
        for node in &self.nodes {
            let op = node.draw(&mut dl, node.opacity);
            dl.ops.push(op);
        }
        dl
    }

    /// Only the surface: where a transition into the first state starts.
    fn ground(&self) -> DisplayList {
        let mut dl = DisplayList::new(self.canvas);
        dl.ops.push(Op::Fill {
            path: Path::rect([0.0, 0.0, self.canvas[0], self.canvas[1]]),
            rule: FillRule::NonZero,
            paint: Paint::Solid(self.surface),
        });
        dl
    }
}

impl SceneNode {
    /// This node's layer at `opacity`.
    fn draw(&self, dl: &mut DisplayList, opacity: f32) -> Op {
        match &self.content {
            Content::Text(placed) => layer(Some(&self.id), placed.origin, opacity, text_ops(dl, &placed.text)),
            Content::Chart { cell, chart } => {
                let mut ops = Vec::new();
                if let Some(rule) = &chart.baseline {
                    ops.push(rule_op(rule, 1.0));
                }
                ops.extend(chart.series.iter().map(|s| series_op(s, 1.0)));
                ops.extend(chart.marks.iter().map(|m| mark_op(m.shape, m.color, 1.0)));
                for label in chart.ticks.iter().chain(&chart.labels) {
                    ops.push(layer(None, label.origin, 1.0, text_ops(dl, &label.text)));
                }
                layer(Some(&self.id), [cell[0], cell[1]], opacity, ops)
            }
        }
    }
}

/// How a state's transition runs (SPEC §3.9).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    pub duration_ms: f64,
    pub ease: CubicBezier,
    /// `match: "id"` (the default) morphs nodes present in both states; `"none"`
    /// fades every node out and in.
    pub matched: bool,
}

impl Timing {
    /// A state without `transition` cuts in.
    pub const CUT: Timing = Timing { duration_ms: 0.0, ease: CubicBezier::LINEAR, matched: true };

    /// A state's `transition`: a duration (ms or a theme name), or
    /// `{ duration, ease, match }`, with `duration` and `ease` defaulting to `standard`.
    pub fn parse(theme: &Theme, transition: Option<&Value>) -> Result<Timing, EngineError> {
        let Some(v) = transition else { return Ok(Timing::CUT) };
        let standard = Value::from("standard");
        let (duration, ease, matched) = match v {
            Value::Number(_) | Value::String(_) => (v, None, true),
            Value::Object(o) => {
                if o.contains_key("spring") {
                    return Err(EngineError::NotImplemented("spring transitions — PLAN 1.11"));
                }
                let matched = match o.get("match").and_then(Value::as_str) {
                    None | Some("id") => true,
                    Some("none") => false,
                    Some(other) => return Err(EngineError::Layout(format!("transition match `{other}`"))),
                };
                (o.get("duration").unwrap_or(&standard), o.get("ease"), matched)
            }
            other => return Err(EngineError::Layout(format!("transition {other}: expected a duration or an object"))),
        };
        let duration_ms = theme
            .duration(duration)
            .filter(|d| d.is_finite() && *d >= 0.0)
            .ok_or_else(|| EngineError::Theme(format!("unknown or invalid duration {duration}")))?;
        let ease = match ease {
            None => theme.easing("standard"),
            Some(Value::String(name)) => theme.easing(name),
            Some(Value::Array(a)) if a.len() == 4 => {
                let f = |i: usize| a[i].as_f64();
                Some(CubicBezier(f(0).unwrap_or(0.0), f(1).unwrap_or(0.0), f(2).unwrap_or(1.0), f(3).unwrap_or(1.0)))
            }
            Some(_) => None,
        }
        .ok_or_else(|| EngineError::Theme(format!("unknown or invalid easing in transition {v}")))?;
        Ok(Timing { duration_ms, ease, matched })
    }

    /// Eased progress at `t_ms`: 0 at or before the start, 1 at or past the end (and
    /// for a NaN time, so a bad `t` shows the state at rest).
    pub fn progress(&self, t_ms: f64) -> f64 {
        if t_ms.is_nan() || t_ms >= self.duration_ms {
            1.0
        } else if t_ms <= 0.0 {
            0.0
        } else {
            self.ease.ease(t_ms / self.duration_ms)
        }
    }
}

/// Indices of an item in the source and the target scene's lists.
type Pair = (Option<usize>, Option<usize>);

/// How one node gets from the source scene to the target.
#[derive(Debug, Clone, PartialEq)]
enum Track {
    /// Only in the source: fades out.
    Exit(usize),
    /// Only in the target: fades in.
    Enter(usize),
    /// Drawn the same in both: moves, and its opacity interpolates.
    Move { from: usize, to: usize },
    /// Drawn differently: the source fades out over the target fading in.
    Crossfade { from: usize, to: usize },
    /// Policy `cut`: the target from the first frame.
    Cut(usize),
    /// A chart in both: its parts match by key.
    Chart { from: usize, to: usize, plan: ChartPlan },
}

#[derive(Debug, Clone, PartialEq, Default)]
struct ChartPlan {
    baseline: Option<Pair>,
    series: Vec<Pair>,
    marks: Vec<Pair>,
    ticks: Vec<Pair>,
    labels: Vec<Pair>,
}

/// Two laid-out snapshots and the plan between them. Frames sample it; nothing here
/// can lay out, shape, or read fonts.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// The state being left; `None` when the transition enters the first state.
    from: Option<Scene>,
    to: Scene,
    timing: Timing,
    /// In paint order.
    tracks: Vec<Track>,
}

impl Transition {
    pub fn new(from: Option<Scene>, to: Scene, timing: Timing) -> Transition {
        let mut tracks: Vec<(i64, usize, Track)> = Vec::new();
        let source = from.as_ref().map_or(&[][..], |s| &s.nodes[..]);
        for (i, a) in source.iter().enumerate() {
            let partner = to.nodes.iter().position(|b| b.id == a.id).filter(|_| timing.matched);
            if partner.is_none() {
                tracks.push((a.z, a.order, Track::Exit(i)));
            }
        }
        for (j, b) in to.nodes.iter().enumerate() {
            let partner = source.iter().position(|a| a.id == b.id).filter(|_| timing.matched);
            let track = match (partner, b.policy) {
                (None, _) => Track::Enter(j),
                (Some(_), Policy::Cut) => Track::Cut(j),
                (Some(i), Policy::Crossfade) => Track::Crossfade { from: i, to: j },
                (Some(i), Policy::Morph) => match (&source[i].content, &b.content) {
                    (Content::Text(x), Content::Text(y)) if x.text == y.text => Track::Move { from: i, to: j },
                    (Content::Chart { chart: x, .. }, Content::Chart { chart: y, .. }) => {
                        Track::Chart { from: i, to: j, plan: ChartPlan::new(x, y) }
                    }
                    _ => Track::Crossfade { from: i, to: j },
                },
            };
            tracks.push((b.z, b.order, track));
        }
        // Stable: within one paint position, an exiting node draws under its successor.
        tracks.sort_by_key(|&(z, order, _)| (z, order));
        Transition { from, to, timing, tracks: tracks.into_iter().map(|(.., t)| t).collect() }
    }

    pub fn duration_ms(&self) -> f64 {
        self.timing.duration_ms
    }

    /// The frame `t_ms` into the transition.
    pub fn frame(&self, t_ms: f64) -> DisplayList {
        let p = self.timing.progress(t_ms);
        if p >= 1.0 {
            return self.to.draw();
        }
        if p <= 0.0 {
            return self.from.as_ref().map_or_else(|| self.to.ground(), Scene::draw);
        }
        let p = p as f32;
        let from = self.from.as_ref().map_or(&[][..], |s| &s.nodes[..]);
        let to = &self.to.nodes;
        let mut dl = self.to.ground();
        for track in &self.tracks {
            match track {
                Track::Exit(i) => push(&mut dl, &from[*i], from[*i].opacity * (1.0 - p)),
                Track::Enter(j) => push(&mut dl, &to[*j], to[*j].opacity * p),
                Track::Cut(j) => push(&mut dl, &to[*j], to[*j].opacity),
                Track::Crossfade { from: i, to: j } => {
                    push(&mut dl, &from[*i], from[*i].opacity * (1.0 - p));
                    push(&mut dl, &to[*j], to[*j].opacity * p);
                }
                Track::Move { from: i, to: j } => {
                    let (Content::Text(a), Content::Text(b)) = (&from[*i].content, &to[*j].content) else {
                        unreachable!("Move tracks pair text nodes");
                    };
                    let origin = lerp2(a.origin, b.origin, p);
                    let opacity = lerp(from[*i].opacity, to[*j].opacity, p);
                    let op = layer(Some(&to[*j].id), origin, opacity, text_ops(&mut dl, &b.text));
                    dl.ops.push(op);
                }
                Track::Chart { from: i, to: j, plan } => {
                    let (Content::Chart { cell: ca, chart: a }, Content::Chart { cell: cb, chart: b }) =
                        (&from[*i].content, &to[*j].content)
                    else {
                        unreachable!("Chart tracks pair charts");
                    };
                    let ops = plan.sample(&mut dl, a, b, p);
                    let origin = lerp2([ca[0], ca[1]], [cb[0], cb[1]], p);
                    let opacity = lerp(from[*i].opacity, to[*j].opacity, p);
                    dl.ops.push(layer(Some(&to[*j].id), origin, opacity, ops));
                }
            }
        }
        dl
    }
}

fn push(dl: &mut DisplayList, node: &SceneNode, opacity: f32) {
    let op = node.draw(dl, opacity);
    dl.ops.push(op);
}

impl ChartPlan {
    fn new(a: &ChartLayout, b: &ChartLayout) -> ChartPlan {
        let baseline = match (&a.baseline, &b.baseline) {
            (None, None) => None,
            (x, y) => Some((x.as_ref().map(|_| 0), y.as_ref().map(|_| 0))),
        };
        ChartPlan {
            baseline,
            series: pair(&a.series, &b.series, |s| &s.key),
            marks: pair(&a.marks, &b.marks, |m| &m.key),
            ticks: pair(&a.ticks, &b.ticks, |l| &l.key),
            labels: pair(&a.labels, &b.labels, |l| &l.key),
        }
    }

    /// The chart's ops `p` of the way from `a` to `b`, in the order a chart at rest
    /// draws them.
    fn sample(&self, dl: &mut DisplayList, a: &ChartLayout, b: &ChartLayout, p: f32) -> Vec<Op> {
        let mut ops = Vec::new();
        if let Some(pair) = self.baseline {
            let (x, y) = (a.baseline.as_ref(), b.baseline.as_ref());
            match (pair, x, y) {
                ((Some(_), Some(_)), Some(x), Some(y)) => ops.push(rule_op(
                    &Rule {
                        from: lerp2(x.from, y.from, p),
                        to: lerp2(x.to, y.to, p),
                        width: lerp(x.width, y.width, p),
                        color: mix(x.color, y.color, p),
                    },
                    1.0,
                )),
                (_, Some(x), None) => ops.push(rule_op(x, 1.0 - p)),
                (_, None, Some(y)) => ops.push(rule_op(y, p)),
                _ => {}
            }
        }
        for &pair in &self.series {
            match pair {
                (Some(i), Some(j)) if a.series[i].points.len() == b.series[j].points.len() => {
                    let (x, y) = (&a.series[i], &b.series[j]);
                    let points = x.points.iter().zip(&y.points).map(|(&u, &v)| lerp2(u, v, p)).collect();
                    let s = Series {
                        key: y.key.clone(),
                        points,
                        width: lerp(x.width, y.width, p),
                        color: mix(x.color, y.color, p),
                    };
                    ops.push(series_op(&s, 1.0));
                }
                (i, j) => {
                    ops.extend(i.map(|i| series_op(&a.series[i], 1.0 - p)));
                    ops.extend(j.map(|j| series_op(&b.series[j], p)));
                }
            }
        }
        for &pair in &self.marks {
            match pair {
                (Some(i), Some(j)) => {
                    let (x, y) = (&a.marks[i], &b.marks[j]);
                    ops.push(mark_op(RoundRect::lerp(x.shape, y.shape, p), mix(x.color, y.color, p), 1.0));
                }
                (i, j) => {
                    ops.extend(i.map(|i| mark_op(a.marks[i].shape, a.marks[i].color, 1.0 - p)));
                    ops.extend(j.map(|j| mark_op(b.marks[j].shape, b.marks[j].color, p)));
                }
            }
        }
        for (pairs, xs, ys) in [(&self.ticks, &a.ticks, &b.ticks), (&self.labels, &a.labels, &b.labels)] {
            for &pair in pairs {
                sample_label(dl, &mut ops, pair.0.map(|i| &xs[i]), pair.1.map(|j| &ys[j]), p);
            }
        }
        ops
    }
}

/// A label moves when its text is unchanged and cross-fades when it changed.
fn sample_label(dl: &mut DisplayList, ops: &mut Vec<Op>, x: Option<&Label>, y: Option<&Label>, p: f32) {
    match (x, y) {
        (Some(x), Some(y)) if x.text == y.text => {
            ops.push(layer(None, lerp2(x.origin, y.origin, p), 1.0, text_ops(dl, &y.text)));
        }
        (x, y) => {
            if let Some(x) = x {
                ops.push(layer(None, x.origin, 1.0 - p, text_ops(dl, &x.text)));
            }
            if let Some(y) = y {
                ops.push(layer(None, y.origin, p, text_ops(dl, &y.text)));
            }
        }
    }
}

/// Match two keyed lists: items only in `a` first (they exit under the rest), then
/// `b` in order, each with its partner in `a` if the key matches.
fn pair<T>(a: &[T], b: &[T], key: impl Fn(&T) -> &String) -> Vec<Pair> {
    let mut out: Vec<Pair> = Vec::with_capacity(a.len().max(b.len()));
    for (i, x) in a.iter().enumerate() {
        if !b.iter().any(|y| key(y) == key(x)) {
            out.push((Some(i), None));
        }
    }
    for (j, y) in b.iter().enumerate() {
        out.push((a.iter().position(|x| key(x) == key(y)), Some(j)));
    }
    out
}

fn lerp2(a: Point, b: Point, p: f32) -> Point {
    [lerp(a[0], b[0], p), lerp(a[1], b[1], p)]
}

fn layer(node: Option<&str>, origin: Point, opacity: f32, ops: Vec<Op>) -> Op {
    Op::Layer {
        node: node.map(str::to_string),
        transform: [1.0, 0.0, 0.0, 1.0, origin[0], origin[1]],
        opacity,
        blend: Blend::Normal,
        clip: None,
        ops,
    }
}

fn text_ops(dl: &mut DisplayList, text: &TextLayout) -> Vec<Op> {
    text.runs
        .iter()
        .map(|run| Op::Glyphs {
            font: dl.font(run.font.clone()),
            size: run.size,
            coords: run.coords.clone(),
            paint: Paint::Solid(run.color),
            glyphs: run.glyphs.clone(),
        })
        .collect()
}

/// A single fill or stroke fades through its paint's alpha: one shape, one coverage,
/// so it equals the shape drawn into a layer at that opacity, without isolating one.
fn fade(color: Color, alpha: f32) -> Color {
    if alpha >= 1.0 {
        return color;
    }
    let [r, g, b, a] = color.0;
    Color([r, g, b, (f32::from(a) * alpha.max(0.0)).round() as u8])
}

fn mark_op(shape: RoundRect, color: Color, alpha: f32) -> Op {
    Op::Fill { path: shape.path(), rule: FillRule::NonZero, paint: Paint::Solid(fade(color, alpha)) }
}

fn series_op(series: &Series, alpha: f32) -> Op {
    Op::Stroke {
        path: series.path(),
        paint: Paint::Solid(fade(series.color, alpha)),
        width: series.width,
        cap: Cap::Round,
        join: Join::Round,
        miter_limit: 4.0,
        dash: Vec::new(),
        dash_offset: 0.0,
    }
}

fn rule_op(rule: &Rule, alpha: f32) -> Op {
    Op::Stroke {
        path: Path(vec![PathEl::MoveTo(rule.from), PathEl::LineTo(rule.to)]),
        paint: Paint::Solid(fade(rule.color, alpha)),
        width: rule.width,
        cap: Cap::Butt,
        join: Join::Miter,
        miter_limit: 4.0,
        dash: Vec::new(),
        dash_offset: 0.0,
    }
}

// --- color ----------------------------------------------------------------------

/// `a` to `b` in Oklab (SPEC §3.9), alpha linear. Equal colors stay bit-equal.
pub fn mix(a: Color, b: Color, p: f32) -> Color {
    if a == b {
        return a;
    }
    let (x, y) = (oklab(a), oklab(b));
    let [l, m, s] = [lerp(x[0], y[0], p), lerp(x[1], y[1], p), lerp(x[2], y[2], p)];
    let alpha = lerp(f32::from(a.0[3]), f32::from(b.0[3]), p).round() as u8;
    let [r, g, b] = srgb_from_oklab([l, m, s]);
    Color([r, g, b, alpha])
}

fn linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.040_45 { c / 12.92 } else { libm::powf((c + 0.055) / 1.055, 2.4) }
}

fn encode(l: f32) -> u8 {
    let c = if l <= 0.003_130_8 { 12.92 * l } else { 1.055 * libm::powf(l, 1.0 / 2.4) - 0.055 };
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Björn Ottosson's Oklab, from straight sRGB.
fn oklab(c: Color) -> [f32; 3] {
    let [r, g, b] = [linear(c.0[0]), linear(c.0[1]), linear(c.0[2])];
    let l = libm::cbrtf(0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b);
    let m = libm::cbrtf(0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b);
    let s = libm::cbrtf(0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b);
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn srgb_from_oklab([l, a, b]: [f32; 3]) -> [u8; 3] {
    let cube = |x: f32| x * x * x;
    let l_ = cube(l + 0.396_337_78 * a + 0.215_803_76 * b);
    let m_ = cube(l - 0.105_561_346 * a - 0.063_854_17 * b);
    let s_ = cube(l - 0.089_484_18 * a - 1.291_485_5 * b);
    [
        encode(4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_),
        encode(-1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_),
        encode(-0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oklab_round_trips_and_mixes_through_lightness() {
        for hex in ["#C2410C", "#0F766E", "#FBFAF6", "#16140F", "#000000", "#FFFFFF"] {
            let c = Color::from_hex(hex).unwrap();
            let [r, g, b] = srgb_from_oklab(oklab(c));
            assert_eq!([r, g, b, 255], c.0, "{hex} round-trips");
        }
        let (black, white) = (Color::from_hex("#000000").unwrap(), Color::from_hex("#FFFFFF").unwrap());
        // Oklab's midpoint is perceptual: L = 0.5 is about 39% grey in sRGB bytes, not 50%.
        assert_eq!(mix(black, white, 0.5).0, [99, 99, 99, 255]);
        assert_eq!(mix(black, white, 0.0), black);
        assert_eq!(mix(black, white, 1.0), white);
    }

    #[test]
    fn progress_is_exact_at_both_ends_and_eased_between() {
        let t = Timing { duration_ms: 400.0, ease: CubicBezier(0.2, 0.0, 0.0, 1.0), matched: true };
        assert_eq!((t.progress(-5.0), t.progress(0.0)), (0.0, 0.0));
        assert_eq!((t.progress(400.0), t.progress(f64::INFINITY), t.progress(f64::NAN)), (1.0, 1.0, 1.0));
        assert!(t.progress(100.0) > 0.25, "ease-out runs ahead early: {}", t.progress(100.0));
        assert_eq!(Timing::CUT.progress(0.0), 1.0, "a cut is at rest from its first frame");
    }

    #[test]
    fn pairs_keep_exits_first_then_target_order() {
        let (a, b) = (vec!["x".to_string(), "y".into(), "z".into()], vec!["z".to_string(), "w".into(), "x".into()]);
        assert_eq!(pair(&a, &b, |s| s), [(Some(1), None), (Some(2), Some(0)), (None, Some(1)), (Some(0), Some(2))]);
    }
}
