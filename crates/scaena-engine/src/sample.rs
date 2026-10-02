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
//!   cross-fades (word-level text morphs are PLAN 1.12).
//! - Charts move data. Marks match by key and interpolate: a corrected figure, the
//!   next month's values, the axis rescaling. A key that appears grows from the
//!   baseline and one that disappears shrinks onto it, each riding along with its
//!   nearest matched neighbor, so a window that advances a period scrolls: the oldest
//!   bar leaves under one side of the chart's cell as the newest arrives from the
//!   other. Value labels ride their marks and count through the numbers. A chart that
//!   enters grows its values in; one that exits shrinks them out.
//! - A shader shows the frame's time on the global timeline, so it drifts on through
//!   a transition. The same shader in both states stays drawn (moving with its rect);
//!   a changed one cross-fades (interpolating its uniforms is PLAN 1.12).
//! - Any other node only in the target fades in; one only in the source fades out.
//! - Numbers interpolate linearly; colors in Oklab (SPEC §3.9), through `libm`, whose
//!   pure-Rust math gives the same bits on every platform.
//! - At `t ≤ 0` the frame is the source scene at rest, and at `t ≥ duration` the
//!   target at rest, exactly: both ends draw a scene, not an interpolation.

use crate::EngineError;
use crate::charts::{ChartLayout, Label, Mark, Numerals, RoundRect, Rule, ValueLabel, lerp};
use crate::data;
use crate::images::ImageNode;
use crate::render::PlacedText;
use crate::shaders::ShaderNode;
use crate::shapes::ShapeNode;
use crate::text::{GlyphRun, TextLayout};
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
    /// When the state comes to rest on the global timeline, seconds: the time its
    /// shaders show at rest.
    pub time: f64,
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
    Shader(ShaderNode),
    Shape(ShapeNode),
    Image(ImageNode),
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
            let op = node.draw(&mut dl, node.opacity, self.time);
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
    /// This node's layer at `opacity`, its shader `time` seconds into the global
    /// timeline.
    fn draw(&self, dl: &mut DisplayList, opacity: f32, time: f64) -> Op {
        match &self.content {
            Content::Shader(s) => layer(Some(&self.id), [s.rect[0], s.rect[1]], opacity, vec![s.op(time)]),
            Content::Shape(s) => layer(Some(&self.id), [s.rect[0], s.rect[1]], opacity, s.ops()),
            Content::Image(i) => layer(Some(&self.id), [i.rect[0], i.rect[1]], opacity, i.ops()),
            Content::Text(placed) => layer(Some(&self.id), placed.origin, opacity, text_ops(dl, &placed.text.runs)),
            Content::Chart { cell, chart } => {
                let mut ops = Vec::new();
                if let Some(rule) = &chart.baseline {
                    ops.push(rule_op(rule, 1.0));
                }
                ops.extend(chart.marks.iter().map(|m| mark_op(m.shape, m.color, 1.0)));
                for label in chart.ticks.iter().chain(&chart.labels) {
                    ops.push(layer(None, label.origin, 1.0, text_ops(dl, &label.text.runs)));
                }
                chart_layer(&self.id, [cell[0], cell[1]], cell[2], dl.viewport[1], opacity, ops)
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
    /// Drawn the same in both: text or a shader that moves, and its opacity interpolates.
    Move { from: usize, to: usize },
    /// Drawn differently: the source fades out over the target fading in.
    Crossfade { from: usize, to: usize },
    /// Policy `cut`: the target from the first frame.
    Cut(usize),
    /// A chart: its parts match by key. With one side missing, the chart enters
    /// (grows its values in) or exits (shrinks them out).
    Chart { from: Option<usize>, to: Option<usize>, plan: ChartPlan },
}

/// How a chart's parts get from one snapshot to the next, matched by key.
#[derive(Debug, Clone, PartialEq, Default)]
struct ChartPlan {
    baseline: Pair,
    /// Each mark with its key's value labels on either side.
    marks: Vec<(Keyed, Pair)>,
    ticks: Vec<Keyed>,
}

/// A keyed chart part on either side and, for a part on one side only, the nearest
/// part on both that it rides along with, as indices in the source and the target.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Keyed {
    pair: Pair,
    ride: Option<(usize, usize)>,
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
                let track = match &a.content {
                    Content::Chart { chart, .. } => {
                        Track::Chart { from: Some(i), to: None, plan: ChartPlan::new(Some(chart), None) }
                    }
                    Content::Text(_) | Content::Shader(_) | Content::Shape(_) | Content::Image(_) => Track::Exit(i),
                };
                tracks.push((a.z, a.order, track));
            }
        }
        for (j, b) in to.nodes.iter().enumerate() {
            let partner = source.iter().position(|a| a.id == b.id).filter(|_| timing.matched);
            let track = match (partner, b.policy) {
                (None, _) => match &b.content {
                    Content::Chart { chart, .. } => {
                        Track::Chart { from: None, to: Some(j), plan: ChartPlan::new(None, Some(chart)) }
                    }
                    Content::Text(_) | Content::Shader(_) | Content::Shape(_) | Content::Image(_) => Track::Enter(j),
                },
                (Some(_), Policy::Cut) => Track::Cut(j),
                (Some(i), Policy::Crossfade) => Track::Crossfade { from: i, to: j },
                (Some(i), Policy::Morph) => match (&source[i].content, &b.content) {
                    (Content::Text(x), Content::Text(y)) if x.text == y.text => Track::Move { from: i, to: j },
                    (Content::Shader(x), Content::Shader(y)) if x.same_shader(y) => Track::Move { from: i, to: j },
                    (Content::Shape(x), Content::Shape(y)) if x.same_shape(y) => Track::Move { from: i, to: j },
                    (Content::Image(x), Content::Image(y)) if x.same_image(y) => Track::Move { from: i, to: j },
                    (Content::Chart { chart: x, .. }, Content::Chart { chart: y, .. }) => {
                        Track::Chart { from: Some(i), to: Some(j), plan: ChartPlan::new(Some(x), Some(y)) }
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
        // The global timeline reaches this state's rest time as the transition ends.
        let time = self.to.time - self.timing.duration_ms / 1000.0 + t_ms / 1000.0;
        let from = self.from.as_ref().map_or(&[][..], |s| &s.nodes[..]);
        let to = &self.to.nodes;
        let mut dl = self.to.ground();
        for track in &self.tracks {
            match track {
                Track::Exit(i) => push(&mut dl, &from[*i], from[*i].opacity * (1.0 - p), time),
                Track::Enter(j) => push(&mut dl, &to[*j], to[*j].opacity * p, time),
                Track::Cut(j) => push(&mut dl, &to[*j], to[*j].opacity, time),
                Track::Crossfade { from: i, to: j } => {
                    push(&mut dl, &from[*i], from[*i].opacity * (1.0 - p), time);
                    push(&mut dl, &to[*j], to[*j].opacity * p, time);
                }
                Track::Move { from: i, to: j } => {
                    let opacity = lerp(from[*i].opacity, to[*j].opacity, p);
                    let op = match (&from[*i].content, &to[*j].content) {
                        (Content::Text(a), Content::Text(b)) => {
                            let origin = lerp2(a.origin, b.origin, p);
                            layer(Some(&to[*j].id), origin, opacity, text_ops(&mut dl, &b.text.runs))
                        }
                        (Content::Shader(a), Content::Shader(b)) => {
                            let [x, y, w, h] = [0, 1, 2, 3].map(|k| lerp(a.rect[k], b.rect[k], p));
                            let shader = b.at([x, y, w, h]);
                            layer(Some(&to[*j].id), [x, y], opacity, vec![shader.op(time)])
                        }
                        (Content::Shape(a), Content::Shape(b)) => {
                            let shape = ShapeNode::lerp(a, b, p);
                            layer(Some(&to[*j].id), [shape.rect[0], shape.rect[1]], opacity, shape.ops())
                        }
                        (Content::Image(a), Content::Image(b)) => {
                            let image = ImageNode::lerp(a, b, p);
                            layer(Some(&to[*j].id), [image.rect[0], image.rect[1]], opacity, image.ops())
                        }
                        _ => unreachable!(
                            "Move tracks pair text with text, and a shader, a shape, or an image with itself"
                        ),
                    };
                    dl.ops.push(op);
                }
                Track::Chart { from: i, to: j, plan } => {
                    let (a, b) = (chart(i.map(|i| &from[i])), chart(j.map(|j| &to[j])));
                    let ops = plan.sample(&mut dl, a.map(|c| c.2), b.map(|c| c.2), p);
                    let (id, origin, width, opacity) = match (a, b) {
                        (Some((x, ca, _)), Some((y, cb, _))) => (
                            &y.id,
                            lerp2([ca[0], ca[1]], [cb[0], cb[1]], p),
                            lerp(ca[2], cb[2], p),
                            lerp(x.opacity, y.opacity, p),
                        ),
                        (Some((n, c, _)), None) | (None, Some((n, c, _))) => (&n.id, [c[0], c[1]], c[2], n.opacity),
                        (None, None) => unreachable!("a chart track has a side"),
                    };
                    let op = chart_layer(id, origin, width, dl.viewport[1], opacity, ops);
                    dl.ops.push(op);
                }
            }
        }
        dl
    }
}

/// A chart track's node on one side, with its cell and layout.
fn chart(node: Option<&SceneNode>) -> Option<(&SceneNode, Rect, &ChartLayout)> {
    node.map(|n| match &n.content {
        Content::Chart { cell, chart } => (n, *cell, chart),
        Content::Text(_) | Content::Shader(_) | Content::Shape(_) | Content::Image(_) => {
            unreachable!("Chart tracks pair charts")
        }
    })
}

fn push(dl: &mut DisplayList, node: &SceneNode, opacity: f32, time: f64) {
    let op = node.draw(dl, opacity, time);
    dl.ops.push(op);
}

impl ChartPlan {
    fn new(a: Option<&ChartLayout>, b: Option<&ChartLayout>) -> ChartPlan {
        // The value label for mark `i` of chart `c`, found by the mark's key.
        let label = |c: Option<&ChartLayout>, i: Option<usize>| {
            let c = c?;
            let key = &c.marks[i?].key;
            c.labels.iter().position(|l| &l.key == key)
        };
        ChartPlan {
            baseline: (a.and_then(|c| c.baseline.as_ref()).map(|_| 0), b.and_then(|c| c.baseline.as_ref()).map(|_| 0)),
            marks: rides(pair(marks_of(a), marks_of(b), |m| &m.key))
                .into_iter()
                .map(|k| (k, (label(a, k.pair.0), label(b, k.pair.1))))
                .collect(),
            ticks: rides(pair(ticks_of(a), ticks_of(b), |l| &l.key)),
        }
    }

    /// The chart's ops `p` of the way from `a` to `b`, in the order a chart at rest
    /// draws them: baseline, marks, category labels, value labels.
    fn sample(&self, dl: &mut DisplayList, a: Option<&ChartLayout>, b: Option<&ChartLayout>, p: f32) -> Vec<Op> {
        let mut ops = Vec::new();
        match (a.and_then(|c| c.baseline.as_ref()), b.and_then(|c| c.baseline.as_ref())) {
            (Some(x), Some(y)) => ops.push(rule_op(
                &Rule {
                    from: lerp2(x.from, y.from, p),
                    to: lerp2(x.to, y.to, p),
                    width: lerp(x.width, y.width, p),
                    color: mix(x.color, y.color, p),
                },
                1.0,
            )),
            (Some(x), None) => ops.push(rule_op(x, 1.0 - p)),
            (None, Some(y)) => ops.push(rule_op(y, p)),
            (None, None) => {}
        }
        // Marks: a matched key interpolates. A new one grows from the baseline where the
        // transition starts, and a removed one shrinks onto it where the transition
        // ends, each moving as far as the neighbor it rides with.
        let (ma, mb) = (marks_of(a), marks_of(b));
        let (base_a, base_b) = (a.or(b).map_or(0.0, |c| c.base), b.or(a).map_or(0.0, |c| c.base));
        let mut shapes = Vec::with_capacity(self.marks.len());
        for &(Keyed { pair: (i, j), ride }, _) in &self.marks {
            let dx = ride.map_or(0.0, |(ri, rj)| mb[rj].shape.center_x() - ma[ri].shape.center_x());
            let (shape, color) = match (i, j) {
                (Some(i), Some(j)) => (RoundRect::lerp(ma[i].shape, mb[j].shape, p), mix(ma[i].color, mb[j].color, p)),
                (Some(i), None) => {
                    (RoundRect::lerp(ma[i].shape, ma[i].shape.shifted(dx).collapsed(base_b), p), ma[i].color)
                }
                (None, Some(j)) => {
                    (RoundRect::lerp(mb[j].shape.shifted(-dx).collapsed(base_a), mb[j].shape, p), mb[j].color)
                }
                (None, None) => unreachable!("a pair has a side"),
            };
            ops.push(mark_op(shape, color, 1.0));
            shapes.push(shape);
        }
        // Category labels move; a new or removed one rides along and fades.
        let (ta, tb) = (ticks_of(a), ticks_of(b));
        for &Keyed { pair: (i, j), ride } in &self.ticks {
            let d = ride.map_or([0.0, 0.0], |(ri, rj)| {
                [tb[rj].origin[0] - ta[ri].origin[0], tb[rj].origin[1] - ta[ri].origin[1]]
            });
            let mut tick =
                |at: Point, alpha: f32, l: &Label| ops.push(layer(None, at, alpha, text_ops(dl, &l.text.runs)));
            match (i.map(|i| &ta[i]), j.map(|j| &tb[j])) {
                (Some(x), Some(y)) if x.text == y.text => tick(lerp2(x.origin, y.origin, p), 1.0, y),
                (Some(x), Some(y)) => {
                    tick(lerp2(x.origin, y.origin, p), 1.0 - p, x);
                    tick(lerp2(x.origin, y.origin, p), p, y);
                }
                (Some(x), None) => tick(lerp2(x.origin, [x.origin[0] + d[0], x.origin[1] + d[1]], p), 1.0 - p, x),
                (None, Some(y)) => tick(lerp2([y.origin[0] - d[0], y.origin[1] - d[1]], y.origin, p), p, y),
                (None, None) => unreachable!("a pair has a side"),
            }
        }
        // Value labels ride their marks and count.
        let numerals = b.and_then(|c| c.numerals.as_ref()).or_else(|| a.and_then(|c| c.numerals.as_ref()));
        let (la, lb) = (a.map_or(&[][..], |c| &c.labels), b.map_or(&[][..], |c| &c.labels));
        for (&(marks, (i, j)), shape) in self.marks.iter().zip(&shapes) {
            value_label(dl, &mut ops, (i.map(|i| &la[i]), j.map(|j| &lb[j])), marks.pair, shape, numerals, p);
        }
        ops
    }
}

fn marks_of(c: Option<&ChartLayout>) -> &[Mark] {
    c.map_or(&[], |c| &c.marks)
}

fn ticks_of(c: Option<&ChartLayout>) -> &[Label] {
    c.map_or(&[], |c| &c.ticks)
}

/// A value label on its mark (`shape`, already interpolated). Unchanged text rides
/// along; a changed number counts from the old value to the new, spelled from the
/// shaped figures. A mark that grows in counts up from 0 as its label fades in, and one
/// that shrinks out counts down to 0 as its label fades out. Text the figures cannot
/// spell cross-fades instead.
fn value_label(
    dl: &mut DisplayList,
    ops: &mut Vec<Op>,
    (x, y): (Option<&Label>, Option<&Label>),
    marks: Pair,
    shape: &RoundRect,
    numerals: Option<&Numerals>,
    p: f32,
) {
    let ride = |l: &Label| l.value.expect("value labels carry their value");
    let at = |text: &TextLayout, v: ValueLabel| {
        let [cx, baseline] = v.anchor(shape);
        [cx - 0.5 * text.width, baseline - text.lines.first().map_or(0.0, |l| l.baseline)]
    };
    if let (Some(x), Some(y)) = (x, y)
        && x.text == y.text
    {
        ops.push(layer(None, at(&y.text, ride(y)), 1.0, text_ops(dl, &y.text.runs)));
        return;
    }
    let start = x.map(|l| ride(l).value).or(marks.0.is_none().then_some(0.0));
    let end = y.map(|l| ride(l).value).or(marks.1.is_none().then_some(0.0));
    if let (Some(start), Some(end), Some(numerals), Some(label)) = (start, end, numerals, y.or(x))
        && let Some((runs, width)) = numerals.compose(&count(start, end, p))
    {
        let [cx, baseline] = ride(label).anchor(shape);
        let alpha = match marks {
            (None, _) => p,
            (_, None) => 1.0 - p,
            _ => 1.0,
        };
        ops.push(layer(None, [cx - 0.5 * width, baseline - numerals.baseline], alpha, text_ops(dl, &runs)));
        return;
    }
    if let Some(x) = x {
        ops.push(layer(None, at(&x.text, ride(x)), 1.0 - p, text_ops(dl, &x.text.runs)));
    }
    if let Some(y) = y {
        ops.push(layer(None, at(&y.text, ride(y)), p, text_ops(dl, &y.text.runs)));
    }
}

/// The number `p` of the way from `a` to `b`, to as many places as either shows.
fn count(a: f64, b: f64, p: f32) -> String {
    let places = data::decimals(a).max(data::decimals(b));
    data::format_fixed(a + (b - a) * f64::from(p), places)
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

/// Each pair with what it rides along with: a part on one side only moves with its
/// nearest neighbor on that side that is on both, the one before it on a tie.
fn rides(pairs: Vec<Pair>) -> Vec<Keyed> {
    let both: Vec<(usize, usize)> = pairs.iter().filter_map(|&(i, j)| Some((i?, j?))).collect();
    let nearest = |at: usize, side: fn(&(usize, usize)) -> usize| {
        both.iter().copied().min_by_key(|n| (side(n).abs_diff(at), side(n) > at))
    };
    pairs
        .into_iter()
        .map(|pair| {
            let ride = match pair {
                (Some(i), None) => nearest(i, |n| n.0),
                (None, Some(j)) => nearest(j, |n| n.1),
                _ => None,
            };
            Keyed { pair, ride }
        })
        .collect()
}

fn lerp2(a: Point, b: Point, p: f32) -> Point {
    [lerp(a[0], b[0], p), lerp(a[1], b[1], p)]
}

/// A chart's layer at `origin`, `width` across. It clips at the cell's sides, so
/// marks riding in or out of the window pass under them, and spans the canvas top to
/// bottom, so figures that overshoot the cap height keep their tops.
fn chart_layer(id: &str, origin: Point, width: f32, canvas_height: f32, opacity: f32, ops: Vec<Op>) -> Op {
    Op::Layer {
        node: Some(id.to_string()),
        transform: [1.0, 0.0, 0.0, 1.0, origin[0], origin[1]],
        opacity,
        blend: Blend::Normal,
        clip: Some(Path::rect([0.0, -origin[1], width, canvas_height])),
        ops,
    }
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

fn text_ops(dl: &mut DisplayList, runs: &[GlyphRun]) -> Vec<Op> {
    runs.iter()
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

    #[test]
    fn a_part_on_one_side_rides_with_its_nearest_matched_neighbor() {
        let key = |s: &&str| s.to_string();
        let names = |v: &[&str]| v.iter().map(key).collect::<Vec<_>>();
        // A window that advances: the oldest rides with the next, the newest with the last.
        let (a, b) = (names(&["q1", "q2", "q3"]), names(&["q2", "q3", "q4"]));
        let with: Vec<_> = rides(pair(&a, &b, |s| s)).into_iter().map(|k| k.ride).collect();
        assert_eq!(with, [Some((1, 0)), None, None, Some((2, 1))]);
        // A removal between two matched neighbors rides with the one before it.
        let (a, b) = (names(&["a", "b", "c"]), names(&["a", "c"]));
        assert_eq!(rides(pair(&a, &b, |s| s))[0], Keyed { pair: (Some(1), None), ride: Some((0, 0)) });
        // Nothing matched: nothing to ride with.
        let (a, b) = (names(&["a"]), names(&["b"]));
        assert!(rides(pair(&a, &b, |s| s)).iter().all(|k| k.ride.is_none()));
    }
}
