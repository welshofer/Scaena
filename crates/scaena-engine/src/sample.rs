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
use crate::charts::{
    AxisTick, ChartKind, ChartLayout, Label, LegendEntry, Mark, MarkPreset, Numerals, RoundRect, Rule, SeriesPath,
    Shape, ValueLabel, lerp,
};
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
    /// Paint order: `(z, nodes index)` for each node from its root container down to
    /// this one (`containers::Placement::order`); keys sort in paint order.
    pub paint: Vec<(i64, usize)>,
    /// Its own opacity times its containers'.
    pub opacity: f32,
    /// The node's `transition` property (SPEC §3.9).
    pub policy: Policy,
    pub content: Content,
}

/// What a node draws, laid out.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(PlacedText),
    Chart { cell: Rect, chart: Box<ChartLayout> },
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
            Content::Text(placed) => text_layer(dl, &self.id, placed, placed.origin, opacity),
            Content::Chart { cell, chart } => {
                let rules = chart.y_axis.iter().chain(&chart.x_grid).filter_map(|t| t.rule.as_ref());
                let mut ops: Vec<Op> = rules.map(|r| rule_op(r, 1.0)).collect();
                if let Some(rule) = &chart.baseline {
                    ops.push(rule_op(rule, 1.0));
                }
                let shapes: Vec<(&str, Shape)> = chart.marks.iter().map(|m| (m.key.as_str(), m.shape)).collect();
                let mut plot: Vec<Op> = chart.paths.iter().filter_map(|s| path_op(s, &shapes, s.color, 1.0)).collect();
                plot.extend(chart.marks.iter().filter_map(|m| mark_op(m.shape, m.color, 1.0)));
                for label in chart.ticks.iter().chain(&chart.labels) {
                    plot.push(layer(None, label.origin, 1.0, text_ops(dl, &label.text.runs)));
                }
                let [left, _, width, _] = chart.plot;
                let clip = chart.clipped.then_some([left, left + width]);
                ops.extend(plot_layer(clip, [-cell[1], dl.viewport[1]], plot));
                for label in chart.y_axis.iter().filter_map(|t| t.label.as_ref()).chain(&chart.titles) {
                    ops.push(layer(None, label.origin, 1.0, text_ops(dl, &label.text.runs)));
                }
                for entry in &chart.legend {
                    ops.extend(legend_ops(dl, entry.swatch, entry.color, &entry.label, entry.label.origin, 1.0));
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
    Chart { from: Option<usize>, to: Option<usize>, plan: Box<ChartPlan> },
}

/// How a chart's parts get from one snapshot to the next, matched by key.
#[derive(Debug, Clone, PartialEq, Default)]
struct ChartPlan {
    baseline: Pair,
    /// Each mark with its key's value labels on either side.
    marks: Vec<(Keyed, Pair)>,
    /// Where each mark starts and ends; `None` where two kinds of mark meet.
    ends: Vec<Option<(Shape, Shape)>>,
    /// Bars that regroup, in stages: into a stack (`Some(true)`: heights, then widths)
    /// or out of one (`Some(false)`: widths, then heights).
    regroup: Option<bool>,
    /// The presets marks enter (the target's) and leave (the source's) with, and each
    /// one-sided mark's place in its stagger: the `k`th of `n`, in data order.
    enter: Option<MarkPreset>,
    exit: Option<MarkPreset>,
    order: Vec<Option<(usize, usize)>>,
    /// Lines and areas by series.
    paths: Vec<Pair>,
    ticks: Vec<Keyed>,
    /// Value-axis ticks by their text, and axis titles by axis.
    y_axis: Vec<Pair>,
    titles: Vec<Pair>,
    /// Gridlines across a continuous x by tick, and legend entries by series.
    x_grid: Vec<Pair>,
    legend: Vec<Pair>,
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
        let mut tracks: Vec<(Vec<(i64, usize)>, Track)> = Vec::new();
        let source = from.as_ref().map_or(&[][..], |s| &s.nodes[..]);
        for (i, a) in source.iter().enumerate() {
            let partner = to.nodes.iter().position(|b| b.id == a.id).filter(|_| timing.matched);
            if partner.is_none() {
                let track = match &a.content {
                    Content::Chart { chart, .. } => {
                        Track::Chart { from: Some(i), to: None, plan: Box::new(ChartPlan::new(Some(chart), None)) }
                    }
                    Content::Text(_) | Content::Shader(_) | Content::Shape(_) | Content::Image(_) => Track::Exit(i),
                };
                tracks.push((a.paint.clone(), track));
            }
        }
        for (j, b) in to.nodes.iter().enumerate() {
            let partner = source.iter().position(|a| a.id == b.id).filter(|_| timing.matched);
            let track = match (partner, b.policy) {
                (None, _) => match &b.content {
                    Content::Chart { chart, .. } => {
                        Track::Chart { from: None, to: Some(j), plan: Box::new(ChartPlan::new(None, Some(chart))) }
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
                    // Charts morph mark by mark between kinds that draw the same
                    // marks; any other change of kind cross-fades.
                    (Content::Chart { chart: x, .. }, Content::Chart { chart: y, .. }) if x.kind.morphs_to(y.kind) => {
                        Track::Chart { from: Some(i), to: Some(j), plan: Box::new(ChartPlan::new(Some(x), Some(y))) }
                    }
                    _ => Track::Crossfade { from: i, to: j },
                },
            };
            tracks.push((b.paint.clone(), track));
        }
        // Stable: within one paint position, an exiting node draws under its successor.
        tracks.sort_by(|a, b| a.0.cmp(&b.0));
        Transition { from, to, timing, tracks: tracks.into_iter().map(|(_, t)| t).collect() }
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
                            text_layer(&mut dl, &to[*j].id, b, origin, opacity)
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
                    let clip_y = [-origin[1], dl.viewport[1]];
                    let ops = plan.sample(&mut dl, a.map(|c| c.2), b.map(|c| c.2), p, t_ms, &self.timing, clip_y);
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
        Content::Chart { cell, chart } => (n, *cell, &**chart),
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
        let marks: Vec<(Keyed, Pair)> = rides(pair(marks_of(a), marks_of(b), |m| &m.key))
            .into_iter()
            .map(|k| (k, (label(a, k.pair.0), label(b, k.pair.1))))
            .collect();
        let (enter, exit) = (b.and_then(|c| c.enter), a.and_then(|c| c.exit));
        // Entering marks stagger in the target's order, leaving ones in the source's.
        let side = |k: &Keyed| match k.pair {
            (None, Some(j)) => Some((1, j)),
            (Some(i), None) => Some((0, i)),
            _ => None,
        };
        let order = marks
            .iter()
            .map(|(k, _)| {
                let (s, at) = side(k)?;
                let peers = marks.iter().filter_map(|(o, _)| side(o).filter(|(t, _)| *t == s));
                let (before, n) = peers.fold((0, 0), |(b, n), (_, x)| (b + usize::from(x < at), n + 1));
                Some((before, n))
            })
            .collect();
        ChartPlan {
            baseline: (a.and_then(|c| c.baseline.as_ref()).map(|_| 0), b.and_then(|c| c.baseline.as_ref()).map(|_| 0)),
            ends: marks.iter().map(|(k, _)| ends(*k, a, b, enter, exit)).collect(),
            enter,
            exit,
            order,
            regroup: match (a.map(|c| c.kind), b.map(|c| c.kind)) {
                (Some(ChartKind::Bar), Some(ChartKind::StackedBar)) => Some(true),
                (Some(ChartKind::StackedBar), Some(ChartKind::Bar)) => Some(false),
                _ => None,
            },
            marks,
            paths: pair(paths_of(a), paths_of(b), |s| &s.key),
            ticks: rides(pair(ticks_of(a), ticks_of(b), |l| &l.key)),
            y_axis: pair(axis_of(a), axis_of(b), |t| &t.key),
            titles: pair(titles_of(a), titles_of(b), |l| &l.key),
            x_grid: pair(a.map_or(&[][..], |c| &c.x_grid), b.map_or(&[][..], |c| &c.x_grid), |t| &t.key),
            legend: pair(legend_of(a), legend_of(b), |e| &e.key),
        }
    }

    /// The chart's ops `p` of the way from `a` to `b`, in the order a chart at rest
    /// draws them: gridlines, baseline; marks, category labels, and value labels in the
    /// plot (see [`plot_layer`]); value-axis labels, titles. `clip_y` is the plot clip's
    /// top and height.
    #[allow(clippy::too_many_arguments)]
    fn sample(
        &self,
        dl: &mut DisplayList,
        a: Option<&ChartLayout>,
        b: Option<&ChartLayout>,
        p: f32,
        t_ms: f64,
        timing: &Timing,
        clip_y: [f32; 2],
    ) -> Vec<Op> {
        let mut ops = Vec::new();
        // The value axis rescales as d3's does: a tick on both sides moves; one on one
        // side only rides from or to where its value sits on the other side's scale,
        // fading.
        let (ya, yb) = (axis_of(a), axis_of(b));
        let ticks: Vec<(Option<&AxisTick>, Option<&AxisTick>)> =
            self.y_axis.iter().map(|&(i, j)| (i.map(|i| &ya[i]), j.map(|j| &yb[j]))).collect();
        for &(x, y) in &ticks {
            let (rx, ry) = (x.and_then(|t| t.rule.as_ref()), y.and_then(|t| t.rule.as_ref()));
            let value = x.or(y).map_or(0.0, |t| t.value);
            match (rx, ry) {
                (Some(rx), Some(ry)) => ops.push(rule_op(&lerp_rule(rx, ry, p), 1.0)),
                (Some(rx), None) => ops.push(rule_op(&lerp_rule(rx, &rule_on(rx, b, value), p), 1.0 - p)),
                (None, Some(ry)) => ops.push(rule_op(&lerp_rule(&rule_on(ry, a, value), ry, p), p)),
                (None, None) => {}
            }
        }
        // Gridlines across a continuous x move by tick, and fade on one side only.
        let (xa, xb) = (a.map_or(&[][..], |c| &c.x_grid), b.map_or(&[][..], |c| &c.x_grid));
        for &(i, j) in &self.x_grid {
            match (i.and_then(|i| xa[i].rule.as_ref()), j.and_then(|j| xb[j].rule.as_ref())) {
                (Some(x), Some(y)) => ops.push(rule_op(&lerp_rule(x, y, p), 1.0)),
                (Some(x), None) => ops.push(rule_op(x, 1.0 - p)),
                (None, Some(y)) => ops.push(rule_op(y, p)),
                (None, None) => {}
            }
        }
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
        // Marks move from where they start to where they end (see [`ends`]).
        let mut plot = Vec::new();
        let (ma, mb) = (marks_of(a), marks_of(b));
        let (base_a, base_b) = (a.or(b).map_or(0.0, |c| c.base), b.or(a).map_or(0.0, |c| c.base));
        // Each mark at this frame, and how opaque, by key, for the paths through them.
        let mut shapes: Vec<(&str, Shape, Color, f32)> = Vec::with_capacity(self.marks.len());
        // How far along each mark is: a preset staggers the marks that enter or leave.
        let mut progress = Vec::with_capacity(self.marks.len());
        for ((&(Keyed { pair: (i, j), .. }, _), ends), order) in self.marks.iter().zip(&self.ends).zip(&self.order) {
            let preset = match (i, j) {
                (None, Some(_)) => self.enter.as_ref().map(|e| (e, true)),
                (Some(_), None) => self.exit.as_ref().map(|e| (e, false)),
                _ => None,
            };
            let (p, look) = match (preset, order) {
                (Some((preset, entering)), Some((k, n))) => {
                    let q = staggered(preset, *k, *n, t_ms, timing);
                    // Entering, the look fades as the mark arrives; leaving, it comes on.
                    let w = if entering { 1.0 - q } else { q };
                    (q, Some((lerp(1.0, preset.opacity, w), [preset.translate[0] * w, preset.translate[1] * w])))
                }
                _ => (p, None),
            };
            progress.push(p);
            let (alpha, offset) = look.unwrap_or((1.0, [0.0, 0.0]));
            match (i.map(|i| &ma[i]), j.map(|j| &mb[j]), ends) {
                (Some(x), Some(y), Some((from, to))) => {
                    let shape = match (self.regroup, from, to) {
                        (Some(heights_first), Shape::Bar(f), Shape::Bar(t)) => {
                            Shape::Bar(regrouped(*f, *t, p, heights_first))
                        }
                        _ => Shape::lerp(*from, *to, p).unwrap_or(*to),
                    };
                    shapes.push((&y.key, shape, mix(x.color, y.color, p), 1.0))
                }
                (Some(m), None, Some((from, to))) | (None, Some(m), Some((from, to))) => {
                    let shape = Shape::lerp(*from, *to, p).unwrap_or(m.shape).translated(offset);
                    shapes.push((&m.key, shape, m.color, alpha))
                }
                // Two kinds of mark: the old one shrinks out as the new one grows in.
                (Some(x), Some(y), None) => {
                    let gone = Shape::lerp(x.shape, x.shape.collapsed(base_b), p).unwrap_or(x.shape);
                    shapes.push((&x.key, gone, x.color, 1.0));
                    let come = Shape::lerp(y.shape.collapsed(base_a), y.shape, p).unwrap_or(y.shape);
                    shapes.push((&y.key, come, y.color, 1.0));
                }
                _ => unreachable!("a pair has a side, and one side has one kind of mark"),
            }
        }
        // Lines and areas run through their marks where this frame puts them; a series
        // on one side only fades.
        let (pa, pb) = (paths_of(a), paths_of(b));
        let at: Vec<(&str, Shape)> = shapes.iter().map(|&(k, s, ..)| (k, s)).collect();
        for &(i, j) in &self.paths {
            let path = match (i.map(|i| &pa[i]), j.map(|j| &pb[j])) {
                (Some(x), Some(y)) => {
                    let mut merged = y.clone();
                    merged.marks.extend(x.marks.iter().filter(|k| !y.marks.contains(k)).cloned());
                    merged.stroke = match (x.stroke, y.stroke) {
                        (Some(w), Some(v)) => Some(lerp(w, v, p)),
                        (_, v) => v,
                    };
                    path_op(&merged, &at, mix(x.color, y.color, p), 1.0)
                }
                (Some(x), None) => path_op(x, &at, x.color, 1.0 - p),
                (None, Some(y)) => path_op(y, &at, y.color, p),
                (None, None) => None,
            };
            plot.extend(path);
        }
        let mut drawn = Vec::with_capacity(self.marks.len());
        for &(_, shape, color, alpha) in &shapes {
            plot.extend(mark_op(shape, color, alpha));
            drawn.push(shape);
        }
        // A value label rides its mark: the target's when the kind changed.
        let mut sampled = Vec::with_capacity(self.marks.len());
        let mut k = 0;
        for &(Keyed { pair: (i, j), .. }, _) in &self.marks {
            let two = matches!((i, j), (Some(i), Some(j)) if Shape::lerp(ma[i].shape, mb[j].shape, p).is_none());
            sampled.push(drawn[if two { k + 1 } else { k }]);
            k += if two { 2 } else { 1 };
        }
        // Category labels move; a new or removed one rides along and fades.
        let (ta, tb) = (ticks_of(a), ticks_of(b));
        for &Keyed { pair: (i, j), ride } in &self.ticks {
            let d = ride.map_or([0.0, 0.0], |(ri, rj)| {
                [tb[rj].origin[0] - ta[ri].origin[0], tb[rj].origin[1] - ta[ri].origin[1]]
            });
            let mut tick =
                |at: Point, alpha: f32, l: &Label| plot.push(layer(None, at, alpha, text_ops(dl, &l.text.runs)));
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
        for ((&(marks, (i, j)), shape), &p) in self.marks.iter().zip(&sampled).zip(&progress) {
            value_label(dl, &mut plot, (i.map(|i| &la[i]), j.map(|j| &lb[j])), marks.pair, shape, numerals, p);
        }
        // The plot clips while either side's does, its sides moving from one to the
        // other's.
        let edge = |c: &ChartLayout| [c.plot[0], c.plot[0] + c.plot[2]];
        let clip = match (a, b) {
            (Some(x), Some(y)) => (x.clipped || y.clipped).then(|| lerp2(edge(x), edge(y), p)),
            (Some(c), None) | (None, Some(c)) => c.clipped.then(|| edge(c)),
            (None, None) => None,
        };
        ops.extend(plot_layer(clip, clip_y, plot));
        // Value-axis labels ride with their ticks.
        for &(x, y) in &ticks {
            let (lx, ly) = (x.and_then(|t| t.label.as_ref()), y.and_then(|t| t.label.as_ref()));
            let value = x.or(y).map_or(0.0, |t| t.value);
            let mut label =
                |at: Point, alpha: f32, l: &Label| ops.push(layer(None, at, alpha, text_ops(dl, &l.text.runs)));
            match (lx, ly) {
                (Some(lx), Some(ly)) => label(lerp2(lx.origin, ly.origin, p), 1.0, ly),
                (Some(lx), None) => label(lerp2(lx.origin, label_on(lx, a, b, value), p), 1.0 - p, lx),
                (None, Some(ly)) => label(lerp2(label_on(ly, b, a, value), ly.origin, p), p, ly),
                (None, None) => {}
            }
        }
        // Titles move; changed text cross-fades.
        let (ta, tb) = (titles_of(a), titles_of(b));
        for &(i, j) in &self.titles {
            let mut title =
                |at: Point, alpha: f32, l: &Label| ops.push(layer(None, at, alpha, text_ops(dl, &l.text.runs)));
            match (i.map(|i| &ta[i]), j.map(|j| &tb[j])) {
                (Some(x), Some(y)) if x.text == y.text => title(lerp2(x.origin, y.origin, p), 1.0, y),
                (Some(x), Some(y)) => {
                    title(lerp2(x.origin, y.origin, p), 1.0 - p, x);
                    title(lerp2(x.origin, y.origin, p), p, y);
                }
                (Some(x), None) => title(x.origin, 1.0 - p, x),
                (None, Some(y)) => title(y.origin, p, y),
                (None, None) => {}
            }
        }
        // Legend entries move and change color; one on one side only fades.
        let (ea, eb) = (legend_of(a), legend_of(b));
        for &(i, j) in &self.legend {
            match (i.map(|i| &ea[i]), j.map(|j| &eb[j])) {
                (Some(x), Some(y)) => {
                    let swatch = RoundRect::lerp(x.swatch, y.swatch, p);
                    let at = lerp2(x.label.origin, y.label.origin, p);
                    ops.extend(legend_ops(dl, swatch, mix(x.color, y.color, p), &y.label, at, 1.0));
                }
                (Some(x), None) => ops.extend(legend_ops(dl, x.swatch, x.color, &x.label, x.label.origin, 1.0 - p)),
                (None, Some(y)) => ops.extend(legend_ops(dl, y.swatch, y.color, &y.label, y.label.origin, p)),
                (None, None) => {}
            }
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

fn paths_of(c: Option<&ChartLayout>) -> &[SeriesPath] {
    c.map_or(&[], |c| &c.paths)
}

fn legend_of(c: Option<&ChartLayout>) -> &[LegendEntry] {
    c.map_or(&[], |c| &c.legend)
}

/// The plot's ops: in a layer clipped to the plot's sides, `[left, right]`, when
/// something sits beside it (a value-axis gutter, a legend), so a mark or label riding
/// out of the window passes under the plot's edge rather than over them; else as they
/// are.
fn plot_layer(span: Option<[f32; 2]>, clip_y: [f32; 2], ops: Vec<Op>) -> Vec<Op> {
    let Some(span) = span else { return ops };
    vec![Op::Layer {
        node: None,
        transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        opacity: 1.0,
        blend: Blend::Normal,
        clip: Some(Path::rect([span[0], clip_y[0], span[1] - span[0], clip_y[1]])),
        ops,
    }]
}

fn axis_of(c: Option<&ChartLayout>) -> &[AxisTick] {
    c.map_or(&[], |c| &c.y_axis)
}

fn titles_of(c: Option<&ChartLayout>) -> &[Label] {
    c.map_or(&[], |c| &c.titles)
}

fn lerp_rule(a: &Rule, b: &Rule, p: f32) -> Rule {
    Rule {
        from: lerp2(a.from, b.from, p),
        to: lerp2(a.to, b.to, p),
        width: lerp(a.width, b.width, p),
        color: mix(a.color, b.color, p),
    }
}

/// `rule`, a gridline at `value`, where that value sits on chart `c`'s scale, across its
/// plot; where it is when there is no `c`.
fn rule_on(rule: &Rule, c: Option<&ChartLayout>, value: f64) -> Rule {
    let Some(c) = c else { return rule.clone() };
    let y = c.y_scale.map(value);
    Rule { from: [c.plot[0], y], to: [c.plot[0] + c.plot[2], y], ..rule.clone() }
}

/// Where `label`, beside the tick at `value` on chart `own`, sits beside that value on
/// chart `other`: moved as far as the value moves and as the gutter's edge moves.
fn label_on(label: &Label, own: Option<&ChartLayout>, other: Option<&ChartLayout>, value: f64) -> Point {
    match (own, other) {
        (Some(own), Some(other)) => [
            label.origin[0] + (other.plot[0] - own.plot[0]),
            label.origin[1] + (other.y_scale.map(value) - own.y_scale.map(value)),
        ],
        _ => label.origin,
    }
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
    shape: &Shape,
    numerals: Option<&Numerals>,
    p: f32,
) {
    let ride = |l: &Label| l.value.expect("value labels carry their value");
    let at = |text: &TextLayout, v: ValueLabel| {
        let [cx, baseline] = v.anchor(shape);
        [cx - v.align * text.width, baseline - text.lines.first().map_or(0.0, |l| l.baseline)]
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
        && let Some((runs, width)) = numerals.compose(&numerals.count(start, end, p))
    {
        let v = ride(label);
        let [cx, baseline] = v.anchor(shape);
        let alpha = match marks {
            (None, _) => p,
            (_, None) => 1.0 - p,
            _ => 1.0,
        };
        ops.push(layer(None, [cx - v.align * width, baseline - numerals.baseline], alpha, text_ops(dl, &runs)));
        return;
    }
    if let Some(x) = x {
        ops.push(layer(None, at(&x.text, ride(x)), 1.0 - p, text_ops(dl, &x.text.runs)));
    }
    if let Some(y) = y {
        ops.push(layer(None, at(&y.text, ride(y)), p, text_ops(dl, &y.text.runs)));
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

/// A bar `p` of the way from `a` to `b` in two stages, as d3's grouped and stacked bars
/// regroup: heights first and then widths, or widths first and then heights. Each stage
/// takes half the transition, so the bars never cross mid-way.
fn regrouped(a: RoundRect, b: RoundRect, p: f32, heights_first: bool) -> RoundRect {
    let (first, second) = ((2.0 * p).min(1.0), (2.0 * p - 1.0).max(0.0));
    let (ph, pw) = if heights_first { (first, second) } else { (second, first) };
    RoundRect {
        x: lerp(a.x, b.x, pw),
        w: lerp(a.w, b.w, pw),
        y: lerp(a.y, b.y, ph),
        h: lerp(a.h, b.h, ph),
        top_radius: lerp(a.top_radius, b.top_radius, ph),
        bottom_radius: lerp(a.bottom_radius, b.bottom_radius, ph),
    }
}

/// Where a mark of the plan starts and ends (SPEC §3.7 Motion). A matched key moves
/// from its shape on one side to its shape on the other; a key on one side only comes
/// from, or goes to, where it would stand on the other side ([`entry`]). `None` where
/// two kinds of mark meet.
/// A preset that does not scale its marks keeps them whole: they ride in or out with
/// their neighbors and only fade or move as it says.
fn ends(
    k: Keyed,
    a: Option<&ChartLayout>,
    b: Option<&ChartLayout>,
    enter: Option<MarkPreset>,
    exit: Option<MarkPreset>,
) -> Option<(Shape, Shape)> {
    let (ma, mb) = (marks_of(a), marks_of(b));
    let dx = k.ride.map_or(0.0, |(ri, rj)| mb[rj].shape.center_x() - ma[ri].shape.center_x());
    let grows = |preset: Option<MarkPreset>| preset.is_none_or(|p| p.grow);
    match k.pair {
        (Some(i), Some(j)) => Shape::lerp(ma[i].shape, mb[j].shape, 0.0).map(|_| (ma[i].shape, mb[j].shape)),
        (None, Some(j)) if grows(enter) => Some((entry(&mb[j], a, b, true, -dx), mb[j].shape)),
        (None, Some(j)) => Some((mb[j].shape.shifted(-dx), mb[j].shape)),
        (Some(i), None) if grows(exit) => Some((ma[i].shape, entry(&ma[i], a, b, false, dx))),
        (Some(i), None) => Some((ma[i].shape, ma[i].shape.shifted(dx))),
        (None, None) => unreachable!("a pair has a side"),
    }
}

/// How far along the `k`th of `n` marks that enter or leave with `preset` is, `t_ms`
/// into a transition timed by `timing`. Mark `k` starts `delay + k · stagger` in and
/// runs for its own time (the preset's duration, its spring's settle time, or the
/// transition's); a schedule longer than the transition shrinks to fit it, so every
/// mark is at rest when the transition is. Then it eases, or follows its spring.
fn staggered(preset: &MarkPreset, k: usize, n: usize, t_ms: f64, timing: &Timing) -> f32 {
    let whole = timing.duration_ms;
    let own = preset.duration.or(preset.spring.map(|(_, settle)| 1000.0 * settle)).unwrap_or(whole);
    let span = preset.delay + n.saturating_sub(1) as f64 * preset.stagger + own;
    let fit = if span > whole && span > 0.0 { whole / span } else { 1.0 };
    let (start, length) = (fit * (preset.delay + k as f64 * preset.stagger), fit * own);
    let u = match length > 0.0 {
        true => ((t_ms - start) / length).clamp(0.0, 1.0),
        false => f64::from(u8::from(t_ms >= start)),
    };
    let q = match (preset.spring, preset.ease) {
        (Some((spring, settle)), _) => spring.position(u * settle, 0.0),
        (None, Some(ease)) => ease.ease(u),
        (None, None) => timing.ease.ease(u),
    };
    q as f32
}

/// Where mark `m`, only in the target (`entering`) or only in the source, stands on the
/// side that lacks it, moved as far as the neighbor it rides with (`dx`):
/// - A point of a line or an area whose series runs on that side lies on the series'
///   path where its x falls, level with the path's end past it: a vertex bends out of
///   the line, and a new period slides in off the end.
/// - A member of a stack opens with no extent where it stands among the stack's members
///   there, so a stack never gaps and a donut sweeps open from twelve o'clock.
/// - A bar flattens onto the baseline, and a point of a line drops onto it. A dot of a
///   scatter or a dot plot closes where it is.
fn entry(m: &Mark, a: Option<&ChartLayout>, b: Option<&ChartLayout>, entering: bool, dx: f32) -> Shape {
    let (own, other) = if entering { (b, a) } else { (a, b) };
    let foot = other.or(own).map_or(0.0, |c| c.base);
    let series = own.and_then(|c| c.paths.iter().find(|s| s.marks.contains(&m.key)));
    if let (Some(series), Some(o)) = (series, other)
        && let Some(path) = o.paths.iter().find(|p| p.key == series.key)
    {
        let mut points: Vec<Shape> =
            path.marks.iter().filter_map(|k| o.marks.iter().find(|x| x.key == *k)).map(|x| x.shape).collect();
        points.sort_by(|p, q| p.center_x().total_cmp(&q.center_x()));
        if !points.is_empty() {
            return on_path(m.shape, &points, m.shape.center_x() + dx);
        }
    }
    if let Some(stack) = &m.stack {
        fn members<'c>(c: Option<&'c ChartLayout>, stack: &str) -> Vec<&'c Mark> {
            marks_of(c).iter().filter(|x| x.stack.as_ref().is_some_and(|s| s.key == stack)).collect()
        }
        let (sa, sb) = (members(a, &stack.key), members(b, &stack.key));
        let keys = |s: &[&Mark]| -> Vec<String> { s.iter().map(|x| x.key.clone()).collect() };
        let order = merged(&keys(&sa), &keys(&sb));
        let side = if entering { &sa } else { &sb };
        let start = if matches!(m.shape, Shape::Arc { .. }) { 0.0 } else { foot };
        let at = boundary(&order, side, &m.key, start);
        let beside = side.first().map(|x| x.shape);
        let shape = if beside.is_some() { m.shape } else { m.shape.shifted(dx) };
        return shape.opened_at(at, beside);
    }
    match m.shape {
        Shape::Dot { x, y, .. } if series.is_none() => Shape::Dot { x: x + dx, y, r: 0.0 },
        shape => shape.shifted(dx).collapsed(foot),
    }
}

/// `shape`, a point of a line or an area, moved to `x` on the path through `points` (in
/// x order): between the two points around `x`, or level with the nearer end.
fn on_path(shape: Shape, points: &[Shape], x: f32) -> Shape {
    let n = points.partition_point(|q| q.center_x() < x);
    let along = match n {
        0 => points[0],
        n if n == points.len() => points[n - 1],
        n => {
            let (p, q) = (points[n - 1], points[n]);
            let span = q.center_x() - p.center_x();
            Shape::lerp(p, q, if span > 0.0 { (x - p.center_x()) / span } else { 0.0 }).unwrap_or(p)
        }
    };
    match (shape, along) {
        (Shape::Dot { r, .. }, Shape::Dot { y, .. }) => Shape::Dot { x, y, r },
        (Shape::Span { .. }, Shape::Span { top, base, .. }) => Shape::Span { x, top, base },
        (shape, _) => shape.shifted(x - shape.center_x()),
    }
}

/// The keys of `a` and `b` in one order that keeps each one's: a key on one side only
/// stays among the keys around it on that side.
fn merged(a: &[String], b: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let mut next = 0;
    for k in b {
        if let Some(at) = a.iter().position(|x| x == k) {
            out.extend(a.get(next..at).unwrap_or_default().iter().filter(|x| !b.contains(x)).cloned());
            next = next.max(at + 1);
        }
        out.push(k.clone());
    }
    out.extend(a.get(next..).unwrap_or_default().iter().filter(|x| !b.contains(x)).cloned());
    out
}

/// Where `key`, missing from `side`, would stand among `side`'s members of its stack:
/// at the end of the member before it in `order`, else at the stack's `start`.
fn boundary(order: &[String], side: &[&Mark], key: &str, start: f32) -> f32 {
    let mut at = start;
    for k in order {
        if k == key {
            break;
        }
        if let Some(stack) = side.iter().find(|m| m.key == *k).and_then(|m| m.stack.as_ref()) {
            at = stack.to;
        }
    }
    at
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

/// A text node's layer at `origin`, clipped to its box under `fit: clip` (the clip moves
/// with the text).
fn text_layer(dl: &mut DisplayList, id: &str, placed: &PlacedText, origin: Point, opacity: f32) -> Op {
    let Op::Layer { node, transform, opacity, blend, ops, .. } =
        layer(Some(id), origin, opacity, text_ops(dl, &placed.text.runs))
    else {
        unreachable!("`layer` makes layers")
    };
    let clip = placed.clip.map(|[x, y, w, h]| Path::rect([x - placed.origin[0], y - placed.origin[1], w, h]));
    Op::Layer { node, transform, opacity, blend, clip, ops }
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

fn mark_op(shape: Shape, color: Color, alpha: f32) -> Option<Op> {
    Some(Op::Fill { path: shape.path()?, rule: FillRule::NonZero, paint: Paint::Solid(fade(color, alpha)) })
}

/// A legend entry: its swatch filled in `color`, its label at `at`, both at `alpha`.
fn legend_ops(dl: &mut DisplayList, swatch: RoundRect, color: Color, label: &Label, at: Point, alpha: f32) -> Vec<Op> {
    let mut ops: Vec<Op> = mark_op(Shape::Bar(swatch), color, alpha).into_iter().collect();
    ops.push(layer(None, at, alpha, text_ops(dl, &label.text.runs)));
    ops
}

/// A series' line or area through its marks' `shapes` (by key) in `color`.
fn path_op(series: &SeriesPath, shapes: &[(&str, Shape)], color: Color, alpha: f32) -> Option<Op> {
    let mine: Vec<Shape> =
        shapes.iter().filter(|(k, _)| series.marks.iter().any(|m| m == k)).map(|&(_, s)| s).collect();
    let path = series.path(&mine)?;
    let paint = Paint::Solid(fade(color, alpha));
    Some(match series.stroke {
        Some(width) => Op::Stroke {
            path,
            paint,
            width,
            cap: Cap::Round,
            join: Join::Round,
            miter_limit: 4.0,
            dash: Vec::new(),
            dash_offset: 0.0,
        },
        None => Op::Fill { path, rule: FillRule::NonZero, paint },
    })
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

    // --- chart motion -------------------------------------------------------------

    use crate::charts::Stack;
    use crate::scale::LinearScale;

    fn chart(kind: ChartKind, marks: Vec<Mark>, paths: Vec<SeriesPath>) -> ChartLayout {
        ChartLayout {
            kind,
            enter: None,
            exit: None,
            base: 100.0,
            baseline: None,
            marks,
            ticks: Vec::new(),
            labels: Vec::new(),
            numerals: None,
            paths,
            y_scale: LinearScale { domain: [0.0, 1.0], range: [100.0, 0.0] },
            plot: [0.0, 0.0, 400.0, 100.0],
            clipped: false,
            y_axis: Vec::new(),
            titles: Vec::new(),
            legend: Vec::new(),
            x_grid: Vec::new(),
            collisions: Vec::new(),
        }
    }

    fn mark(key: &str, shape: Shape, stack: Option<(&str, f32, f32)>) -> Mark {
        let stack = stack.map(|(k, from, to)| Stack { key: k.into(), from, to });
        Mark { key: key.into(), shape, color: Color([0, 0, 0, 255]), stack }
    }

    /// A segment of stack `s` at `x` from `from` up to `to` (canvas y, so `to < from`).
    fn segment(key: &str, x: f32, from: f32, to: f32) -> Mark {
        let r = RoundRect { x, y: to, w: 20.0, h: from - to, top_radius: 0.0, bottom_radius: 0.0 };
        mark(key, Shape::Bar(r), Some(("s", from, to)))
    }

    fn slice(key: &str, start: f32, end: f32) -> Mark {
        let arc = Shape::Arc { cx: 50.0, cy: 50.0, inner: 20.0, outer: 40.0, start, end };
        mark(key, arc, Some(("", start, end)))
    }

    /// Every mark of the plan `p` of the way, by key.
    fn at(a: Option<&ChartLayout>, b: Option<&ChartLayout>, p: f32) -> Vec<(String, Shape)> {
        let plan = ChartPlan::new(a, b);
        let (ma, mb) = (marks_of(a), marks_of(b));
        plan.marks
            .iter()
            .zip(&plan.ends)
            .map(|((k, _), ends)| {
                let m = k.pair.1.map(|j| &mb[j]).or(k.pair.0.map(|i| &ma[i])).unwrap();
                let (from, to) = ends.unwrap();
                (m.key.clone(), Shape::lerp(from, to, p).unwrap())
            })
            .collect()
    }

    /// The extents of `shapes` along their stack, end to end from `start`: each one's
    /// start is the last one's end.
    fn partition(mut spans: Vec<(f32, f32)>, start: f32, end: f32) {
        spans.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
        let mut at = start;
        for (from, to) in &spans {
            assert!((from - at).abs() < 1e-4, "a gap or an overlap at {at}: {spans:?}");
            at = *to;
        }
        assert!((at - end).abs() < 1e-4, "ends at {at}, not {end}: {spans:?}");
    }

    #[test]
    fn a_stack_member_that_enters_or_leaves_opens_where_it_stands_and_the_stack_never_gaps() {
        // [a, x, b] becomes [a, n, b]: x closes and n opens between a and b.
        let before = chart(
            ChartKind::StackedBar,
            vec![segment("a", 10.0, 100.0, 80.0), segment("x", 10.0, 80.0, 70.0), segment("b", 10.0, 70.0, 40.0)],
            Vec::new(),
        );
        let after = chart(
            ChartKind::StackedBar,
            vec![segment("a", 10.0, 100.0, 90.0), segment("n", 10.0, 90.0, 60.0), segment("b", 10.0, 60.0, 50.0)],
            Vec::new(),
        );
        let ends = |key: &str| {
            let plan = ChartPlan::new(Some(&before), Some(&after));
            let i = plan.marks.iter().position(|(k, _)| {
                let m = k.pair.1.map(|j| &after.marks[j]).or(k.pair.0.map(|i| &before.marks[i])).unwrap();
                m.key == key
            });
            plan.ends[i.unwrap()].unwrap()
        };
        let span = |s: Shape| match s {
            Shape::Bar(r) => (r.top(), r.bottom()),
            other => panic!("{other:?}"),
        };
        // In the merged order a, n, x, b: n opens on a, under x, and x closes on n, under b.
        assert_eq!(span(ends("n").0), (80.0, 80.0));
        assert_eq!(span(ends("x").1), (60.0, 60.0));
        for p in [0.0, 0.3, 0.5, 0.8, 1.0] {
            let bars: Vec<(f32, f32)> = at(Some(&before), Some(&after), p)
                .into_iter()
                .map(|(_, s)| {
                    let (top, bottom) = span(s);
                    (-bottom, -top)
                })
                .collect();
            let top = lerp(40.0, 50.0, p);
            partition(bars, -100.0, -top);
        }
        // A stack that is new rides in whole from the baseline.
        let grown = at(None, Some(&after), 0.0);
        assert!(grown.iter().all(|(_, s)| span(*s) == (100.0, 100.0)), "{grown:?}");
    }

    #[test]
    fn a_donut_sweeps_open_from_twelve_and_keeps_its_ring_whole() {
        let before = chart(
            ChartKind::Donut,
            vec![slice("a", 0.0, 0.5), slice("x", 0.5, 0.75), slice("b", 0.75, 1.0)],
            Vec::new(),
        );
        let after = chart(
            ChartKind::Donut,
            vec![slice("a", 0.0, 0.25), slice("n", 0.25, 0.6), slice("b", 0.6, 1.0)],
            Vec::new(),
        );
        let turns = |s: Shape| match s {
            Shape::Arc { start, end, .. } => (start, end),
            other => panic!("{other:?}"),
        };
        for p in [0.0, 0.25, 0.5, 0.9, 1.0] {
            partition(at(Some(&before), Some(&after), p).into_iter().map(|(_, s)| turns(s)).collect(), 0.0, 1.0);
            // Entering: every slice opens from twelve o'clock, so the ring sweeps round.
            let entering: Vec<(f32, f32)> = at(None, Some(&after), p).into_iter().map(|(_, s)| turns(s)).collect();
            partition(entering.clone(), 0.0, p);
        }
    }

    #[test]
    fn a_point_of_a_line_enters_on_the_line_and_a_new_period_slides_in_off_its_end() {
        let dot = |key: &str, x: f32, y: f32| mark(key, Shape::Dot { x, y, r: 3.0 }, None);
        let path = |keys: &[&str]| SeriesPath {
            key: "s".into(),
            color: Color([0, 0, 0, 255]),
            stroke: Some(2.0),
            marks: keys.iter().map(|k| k.to_string()).collect(),
        };
        let before =
            chart(ChartKind::Line, vec![dot("q1", 0.0, 80.0), dot("q2", 100.0, 40.0)], vec![path(&["q1", "q2"])]);
        // A point between two others bends out of the segment between them.
        let between = chart(
            ChartKind::Line,
            vec![dot("q1", 0.0, 80.0), dot("mid", 25.0, 10.0), dot("q2", 100.0, 40.0)],
            vec![path(&["q1", "mid", "q2"])],
        );
        let start = at(Some(&before), Some(&between), 0.0);
        let mid = start.iter().find(|(k, _)| k == "mid").unwrap().1;
        assert_eq!(mid, Shape::Dot { x: 25.0, y: 70.0, r: 3.0 }, "a quarter of the way from q1 to q2");
        // The window advances: q3 rides in with q2, level with where the line ended.
        let next =
            chart(ChartKind::Line, vec![dot("q2", 0.0, 40.0), dot("q3", 100.0, 60.0)], vec![path(&["q2", "q3"])]);
        let start = at(Some(&before), Some(&next), 0.0);
        let q3 = start.iter().find(|(k, _)| k == "q3").unwrap().1;
        assert_eq!(q3, Shape::Dot { x: 200.0, y: 40.0, r: 3.0 }, "off the end, one period on, level with q2");
        let end = at(Some(&before), Some(&next), 1.0);
        let q1 = end.iter().find(|(k, _)| k == "q1").unwrap().1;
        assert_eq!(q1, Shape::Dot { x: -100.0, y: 40.0, r: 3.0 }, "q1 leaves off the start, level with q2");
        // With no line to enter on, the points rise from the baseline.
        let rising = at(None, Some(&before), 0.0);
        assert!(rising.iter().all(|(_, s)| matches!(s, Shape::Dot { y: 100.0, r: 0.0, .. })), "{rising:?}");
    }

    #[test]
    fn a_dot_of_a_scatter_opens_where_it_stands() {
        let dots =
            chart(ChartKind::Scatter, vec![mark("p", Shape::Dot { x: 30.0, y: 20.0, r: 8.0 }, None)], Vec::new());
        assert_eq!(at(None, Some(&dots), 0.0)[0].1, Shape::Dot { x: 30.0, y: 20.0, r: 0.0 });
        assert_eq!(at(Some(&dots), None, 1.0)[0].1, Shape::Dot { x: 30.0, y: 20.0, r: 0.0 });
    }

    #[test]
    fn charts_morph_between_kinds_that_draw_the_same_marks_and_cross_fade_otherwise() {
        use ChartKind::*;
        assert!(Bar.morphs_to(StackedBar) && StackedBar.morphs_to(Bar) && Donut.morphs_to(Donut));
        for (x, y) in [(Bar, Line), (Line, Area), (Dot, Scatter), (Area, StackedBar)] {
            assert!(!x.morphs_to(y), "{x:?} → {y:?}");
        }
    }

    #[test]
    fn bars_regroup_in_two_stages() {
        let side = RoundRect { x: 0.0, y: 60.0, w: 10.0, h: 40.0, top_radius: 2.0, bottom_radius: 0.0 };
        let stacked = RoundRect { x: 0.0, y: 20.0, w: 30.0, h: 40.0, top_radius: 0.0, bottom_radius: 0.0 };
        // Into a stack: heights first, then widths.
        let quarter = regrouped(side, stacked, 0.25, true);
        assert_eq!((quarter.y, quarter.w, quarter.top_radius), (40.0, 10.0, 1.0));
        let three = regrouped(side, stacked, 0.75, true);
        assert_eq!((three.y, three.w), (20.0, 20.0));
        // Out of one: widths first, then heights.
        let quarter = regrouped(stacked, side, 0.25, false);
        assert_eq!((quarter.y, quarter.w), (20.0, 20.0));
        assert_eq!(regrouped(stacked, side, 1.0, false), side);
        assert_eq!(regrouped(side, stacked, 0.0, true), side);
    }

    fn preset(stagger: f64, grow: bool) -> MarkPreset {
        MarkPreset {
            opacity: 0.0,
            translate: [0.0, 24.0],
            grow,
            delay: 0.0,
            stagger,
            duration: None,
            ease: Some(CubicBezier::LINEAR),
            spring: None,
        }
    }

    #[test]
    fn staggered_marks_start_in_turn_and_all_rest_when_the_transition_does() {
        let timing = Timing { duration_ms: 420.0, ease: CubicBezier::LINEAR, matched: true };
        let fade = preset(40.0, false);
        // Three marks 40 ms apart, each as long as the transition: 500 ms shrinks to 420.
        let at = |k: usize, t: f64| staggered(&fade, k, 3, t, &timing);
        assert_eq!([at(0, 0.0), at(1, 0.0), at(2, 0.0)], [0.0, 0.0, 0.0]);
        assert_eq!([at(0, 420.0), at(1, 420.0), at(2, 420.0)], [1.0, 1.0, 1.0]);
        assert!(at(0, 60.0) > 0.0 && at(2, 60.0) == 0.0, "the third starts 67.2 ms in");
        assert!(at(0, 200.0) > at(1, 200.0) && at(1, 200.0) > at(2, 200.0));
        // A schedule that fits keeps its own times: 100 ms each, 40 ms apart.
        let short = MarkPreset { duration: Some(100.0), ..fade };
        assert_eq!(staggered(&short, 1, 3, 90.0, &timing), 0.5);
        // A spring runs to rest over its settle time.
        let snappy = scaena_core::timeline::Spring { stiffness: 420.0, damping: 34.0, mass: 1.0 };
        let sprung = MarkPreset { spring: Some((snappy, snappy.settle_time(0.0))), ..fade };
        assert!((staggered(&sprung, 0, 1, 420.0, &timing) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn a_preset_that_does_not_scale_its_marks_keeps_them_whole() {
        let bar = |key: &str, h: f32| {
            let r = RoundRect { x: 10.0, y: 100.0 - h, w: 20.0, h, top_radius: 0.0, bottom_radius: 0.0 };
            mark(key, Shape::Bar(r), None)
        };
        let mut after = chart(ChartKind::Bar, vec![bar("q1", 40.0)], Vec::new());
        after.enter = Some(preset(0.0, false));
        let start = at(None, Some(&after), 0.0);
        assert_eq!(start[0].1, after.marks[0].shape, "whole from the start, only fading and rising");
        after.enter = Some(preset(0.0, true));
        assert!(matches!(at(None, Some(&after), 0.0)[0].1, Shape::Bar(r) if r.h == 0.0), "a scaling preset grows");
    }

    #[test]
    fn the_merged_order_keeps_both_sides_orders() {
        let v = |s: &[&str]| s.iter().map(|k| k.to_string()).collect::<Vec<_>>();
        assert_eq!(merged(&v(&["a", "x", "b"]), &v(&["a", "n", "b"])), v(&["a", "n", "x", "b"]));
        assert_eq!(merged(&v(&["a", "b"]), &v(&["b", "c"])), v(&["a", "b", "c"]));
        assert_eq!(merged(&v(&["a", "b", "z"]), &v(&["n"])), v(&["n", "a", "b", "z"]));
    }
}
