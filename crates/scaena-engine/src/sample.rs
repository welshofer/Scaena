//! Sampling (SPEC §5): a transition interpolates *resolved geometry* between two
//! snapshots laid out once, so a frame never lays anything out.
//!
//! A [`Scene`] is one snapshot after layout. A [`Transition`] holds the scene being
//! left, the scene being entered, the timing, and a plan of how every node and chart
//! mark gets from one to the other, made once when the transition is built.
//! [`Transition::frame`] takes `&self` and owns no fonts or layout engine: frames
//! only sample (SPEC §5, CLAUDE.md invariant 3), by construction.
//!
//! The rules (PLAN 0.10, 1.11, 1.12; SPEC §2.3, §3.9):
//! - Nodes match by id. Text whose layout is unchanged moves; changed text morphs
//!   word by word: shared words move, recoloring or scaling between their boxes, and
//!   the rest fade where they stand (`WordPlan`).
//! - A shape whose outline lines up morphs point by point, its paints mixing; a
//!   shader with the same kind and seed morphs its uniforms. Anything else
//!   cross-fades.
//! - Charts move data. Marks match by key and interpolate: a corrected figure, the
//!   next month's values, the axis rescaling. A key that appears grows from the
//!   baseline and one that disappears shrinks onto it, each riding along with its
//!   nearest matched neighbor, so a window that advances a period scrolls: the oldest
//!   bar leaves under one side of the chart's cell as the newest arrives from the
//!   other. Value labels ride their marks and count through the numbers. A chart that
//!   enters grows its values in; one that exits shrinks them out.
//! - A shader shows the frame's time on the global timeline, so it drifts on through
//!   a transition, moving with its rect as its uniforms move.
//! - A group composites its members in one layer, which its own looks move.
//! - Any other node only in the target fades in; one only in the source fades out.
//! - Numbers interpolate linearly; colors in Oklab (SPEC §3.9), through `libm`, whose
//!   pure-Rust math gives the same bits on every platform.
//! - At `t ≤ 0` the frame is the source scene at rest, and at `t ≥ duration` the
//!   target at rest, exactly: both ends draw a scene, not an interpolation.

use crate::EngineError;
use crate::charts::{
    AxisTick, ChartKind, ChartLayout, Gap, Label, LegendEntry, Mark, Note, Numerals, RoundRect, Rule, SeriesPath,
    Shape, ValueLabel, lerp,
};
use crate::images::ImageNode;
use crate::render::PlacedText;
use crate::shaders::ShaderNode;
use crate::shapes::ShapeNode;
use crate::tables::{Cell, TableLayout};
use crate::text::{GlyphRun, TextLayout};
use crate::theme::Theme;
use scaena_core::displaylist::{Blend, Cap, Color, DisplayList, FillRule, Join, Op, Paint, Path, PathEl, Point, Rect};
use scaena_core::model::values::{SplitUnit, TextSplit};
use scaena_core::timeline::{Clock, CubicBezier, Curve, Item, Look, Motion, Placed, Schedule, schedule};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// One snapshot after layout: what a frame at rest draws, and what a transition
/// into or out of the state interpolates.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub state: String,
    pub canvas: [f32; 2],
    /// The theme's surface, painted under everything.
    pub surface: Color,
    /// Visible nodes in paint order: those that draw something.
    pub nodes: Vec<SceneNode>,
    /// Every visible node's place, containers and groups included: what cues on a
    /// container, or on its children one by one, move.
    pub tree: HashMap<String, Place>,
}

/// A visible node's place in its scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub parent: Option<String>,
    /// Canvas units; a group's is the box around its children's.
    pub rect: Rect,
    /// In flow order: `at.index`, then the deck's order.
    pub children: Vec<String>,
    /// A group's opacity: it composites what is in it as one layer at this opacity, and
    /// its own looks move that layer (SPEC §3.4). `None` for any other node.
    pub composite: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneNode {
    pub id: String,
    /// Paint order: `(z, nodes index)` for each node from its root container down to
    /// this one (`containers::Placement::order`); keys sort in paint order.
    pub paint: Vec<(i64, usize)>,
    /// Its own opacity times its containers', up to its group.
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
    Table { cell: Rect, table: Box<TableLayout> },
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
    /// The state at rest, its shaders `time` seconds into the global timeline.
    pub fn draw_at(&self, time: f64) -> DisplayList {
        let mut dl = self.ground();
        for node in &self.nodes {
            let op = node.draw(&mut dl, node.opacity, time);
            dl.ops.push(op);
        }
        let ops = std::mem::take(&mut dl.ops).into_iter().map(|op| (false, op)).collect();
        dl.ops = grouped(
            ops,
            |id, _| self.groups(id).into_iter().map(|g| (g, false)).collect(),
            |(g, _)| self.tree[g].composite.map(|opacity| (opacity, Seen::REST)),
        );
        dl
    }

    /// The groups `id` sits in, outermost first.
    fn groups(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut at = self.tree.get(id).and_then(|p| p.parent.as_deref());
        while let Some(parent) = at {
            let place = self.tree.get(parent);
            if place.is_some_and(|p| p.composite.is_some()) {
                out.push(parent.to_string());
            }
            at = place.and_then(|p| p.parent.as_deref());
        }
        out.reverse();
        out
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
            Content::Table { cell, table } => {
                let ops = table_ops(dl, table);
                layer(Some(&self.id), [cell[0], cell[1]], opacity, ops)
            }
            Content::Chart { cell, chart } => {
                let mut ops: Vec<Op> =
                    chart.notes.iter().filter_map(|n| n.band).map(|(rect, color)| band_op(rect, color, 1.0)).collect();
                let rules = chart.y_axis.iter().chain(&chart.x_grid).filter_map(|t| t.rule.as_ref());
                ops.extend(rules.map(|r| rule_op(r, 1.0)));
                if let Some(rule) = &chart.baseline {
                    ops.push(rule_op(rule, 1.0));
                }
                let shapes: Vec<(&str, Shape)> = chart.marks.iter().map(|m| (m.key.as_str(), m.shape)).collect();
                let mut plot: Vec<Op> = chart.paths.iter().filter_map(|s| path_op(s, &shapes, s.color, 1.0)).collect();
                plot.extend(chart.marks.iter().filter_map(|m| mark_op(m.shape, m.color, 1.0)));
                for note in &chart.notes {
                    plot.extend(note.rule.as_ref().and_then(|r| broken_rule_op(r, &spans(&note.gaps), 1.0)));
                }
                let notes = chart.notes.iter().filter_map(|n| n.label.as_ref());
                for label in chart.ticks.iter().chain(&chart.labels).chain(notes) {
                    plot.push(layer(None, label.origin, label.opacity, text_ops(dl, &label.text)));
                }
                ops.extend(plot_layer(chart.clip, [-cell[1], dl.viewport[1]], plot));
                for label in chart.y_axis.iter().filter_map(|t| t.label.as_ref()).chain(&chart.titles) {
                    ops.push(layer(None, label.origin, label.opacity, text_ops(dl, &label.text)));
                }
                for e in &chart.legend {
                    ops.extend(legend_ops(dl, e.swatch, e.color, &e.label, e.label.origin, 1.0, e.label.opacity));
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
    /// Eased, or a spring, which lasts its settle time.
    pub curve: Curve,
    /// `match: "id"` (the default) morphs nodes present in both states; `"none"`
    /// fades every node out and in.
    pub matched: bool,
}

impl Timing {
    /// A state without `transition` cuts in.
    pub const CUT: Timing = Timing { duration_ms: 0.0, curve: Curve::Ease(CubicBezier::LINEAR), matched: true };

    /// A state's `transition`: a duration (ms or a theme name), or
    /// `{ duration, ease, spring, match }`, with `duration` and `ease` defaulting to
    /// `standard`. A spring takes the place of both: the transition follows it and
    /// lasts its settle time.
    pub fn parse(theme: &Theme, transition: Option<&Value>) -> Result<Timing, EngineError> {
        let Some(v) = transition else { return Ok(Timing::CUT) };
        let standard = Value::from("standard");
        let (duration, ease, spring, matched) = match v {
            Value::Number(_) | Value::String(_) => (v, None, None, true),
            Value::Object(o) => {
                let matched = match o.get("match").and_then(Value::as_str) {
                    None | Some("id") => true,
                    Some("none") => false,
                    Some(other) => return Err(EngineError::Layout(format!("transition match `{other}`"))),
                };
                (o.get("duration").unwrap_or(&standard), o.get("ease"), o.get("spring"), matched)
            }
            other => return Err(EngineError::Layout(format!("transition {other}: expected a duration or an object"))),
        };
        if let Some(spring) = spring {
            let bad = || EngineError::Theme(format!("unknown or invalid spring in transition {v}"));
            let spring = match spring {
                Value::String(name) => theme.spring(name).ok_or_else(bad)?,
                Value::Object(o) => {
                    let at = |k: &str| o.get(k).and_then(Value::as_f64);
                    let (Some(stiffness), Some(damping)) = (at("stiffness"), at("damping")) else { return Err(bad()) };
                    scaena_core::timeline::Spring { stiffness, damping, mass: at("mass").unwrap_or(1.0) }
                }
                _ => return Err(bad()),
            };
            let curve = Curve::spring(spring);
            return Ok(Timing { duration_ms: curve.duration(0.0), curve, matched });
        }
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
        Ok(Timing { duration_ms, curve: Curve::Ease(ease), matched })
    }

    /// The transition's clock, from the start of the state's cue.
    pub fn clock(&self) -> Clock {
        Clock { start: 0.0, duration: self.duration_ms, curve: self.curve }
    }

    /// Progress at `t_ms`: 0 at or before the start, 1 at or past the end (and for a
    /// NaN time, so a bad `t` shows the state at rest); a spring may pass 1 between.
    pub fn progress(&self, t_ms: f64) -> f64 {
        self.clock().progress(t_ms)
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
    /// A table in both: its cells match by row and column, its rules by the row above.
    Table { from: usize, to: usize, plan: Box<TablePlan> },
    /// Text in both whose layout changed: its words match by their text.
    Words { from: usize, to: usize, plan: Box<WordPlan> },
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
    /// The looks marks enter and leave with, from their cues, and each one-sided mark's
    /// place among those that enter or leave with it: the `k`th of `n`, in data order.
    enter: Option<MarkLook>,
    exit: Option<MarkLook>,
    order: Vec<Option<(usize, usize)>>,
    /// Lines and areas by series.
    paths: Vec<Pair>,
    ticks: Vec<Keyed>,
    /// Value-axis ticks by their text, and axis titles by axis.
    y_axis: Vec<Pair>,
    titles: Vec<Pair>,
    /// Gridlines across the x axis by tick or category, and legend entries by series.
    x_grid: Vec<Pair>,
    legend: Vec<Pair>,
    /// Annotations by kind, axis, and place.
    notes: Vec<Pair>,
}

/// A keyed chart part on either side and, for a part on one side only, the nearest
/// part on both that it rides along with, as indices in the source and the target.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Keyed {
    pair: Pair,
    ride: Option<(usize, usize)>,
}

/// Two laid-out snapshots, the plan between them, and the state's motions on its clock.
/// Frames sample it; nothing here can lay out, shape, or read fonts.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// The state being left; `None` when the transition enters the first state.
    from: Option<Scene>,
    to: Scene,
    timing: Timing,
    /// In paint order.
    tracks: Vec<Track>,
    /// The state's motions, placed on its clock with the transition.
    schedule: Schedule,
    /// When the state's cue starts on the global timeline, seconds.
    start: f64,
}

/// What the cues make of a node: a map of canvas points, an opacity, a color its paints
/// mix toward and how far, and how much of a shape's outline is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Seen {
    map: [f64; 6],
    opacity: f64,
    tint: Option<(Color, f64)>,
    progress: f64,
}

impl Seen {
    const REST: Seen = Seen { map: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], opacity: 1.0, tint: None, progress: 1.0 };

    /// Whether this leaves a node's layer as it is: what [`looked`] applies is at rest.
    fn leaves_layer(&self) -> bool {
        (self.map, self.opacity, self.tint) == (Seen::REST.map, 1.0, None)
    }

    /// This, with `look` on a unit whose box at rest is `rect` inside it.
    fn within(self, look: &Look, rect: Rect) -> Seen {
        Seen {
            map: compose(self.map, look.affine(rect)),
            opacity: self.opacity * look.opacity,
            tint: tints(look.tint, self.tint),
            progress: self.progress * look.progress,
        }
    }
}

/// An inner tint and then an outer one, as one. Mixing is linear in Oklab, so mixing
/// toward `c1` by `q1` and then toward `c2` by `q2` is mixing by `q = 1 − (1 − q1)(1 − q2)`
/// toward `c1` and `c2` mixed by `q2 / q`. A tint of nothing is none.
fn tints(inner: Option<(Color, f64)>, outer: Option<(Color, f64)>) -> Option<(Color, f64)> {
    match (inner.filter(|t| t.1 > 0.0), outer.filter(|t| t.1 > 0.0)) {
        (None, t) | (t, None) => t,
        (Some((c1, q1)), Some((c2, q2))) => {
            let q = 1.0 - (1.0 - q1) * (1.0 - q2);
            Some((mix(c1, c2, (q2 / q) as f32), q))
        }
    }
}

/// Every paint `op` draws, mixed `q` of the way toward `color`, each keeping its alpha.
fn tint(op: &mut Op, color: Color, q: f32) {
    let toward = |c: &mut Color| *c = mix(*c, Color([color.0[0], color.0[1], color.0[2], c.0[3]]), q);
    match op {
        Op::Layer { ops, .. } => ops.iter_mut().for_each(|op| tint(op, color, q)),
        Op::Fill { paint, .. } | Op::Stroke { paint, .. } | Op::Glyphs { paint, .. } => match paint {
            Paint::Solid(c) => toward(c),
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } | Paint::Sweep { stops, .. } => {
                stops.iter_mut().for_each(|s| toward(&mut s.1))
            }
        },
        _ => {}
    }
}

/// `a ∘ b`: the map that applies `b`, then `a`.
fn compose(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

/// A node's layer, through what its cues make of it. How much of a shape's outline is
/// drawn is the shape's to draw (`progress`); the rest is here.
fn looked(op: Op, seen: Seen) -> Op {
    if seen.leaves_layer() {
        return op;
    }
    let Op::Layer { node, cell, transform, opacity, blend, clip, mut ops } = op else { return op };
    if let Some((color, q)) = seen.tint {
        ops.iter_mut().for_each(|op| tint(op, color, q as f32));
    }
    let transform = compose(seen.map, transform.map(f64::from)).map(|v| v as f32);
    Op::Layer { node, cell, transform, opacity: opacity * seen.opacity as f32, blend, clip, ops }
}

/// A group as the fold below keys it: its id, and whether its layer is the one in the
/// state being left (a group that leaves, or every group under `match: none`).
type GroupKey = (String, bool);

/// `ops` in paint order, each with whether it was drawn from the state being left, with
/// each group's members gathered into one layer for the group (SPEC §3.4). `groups`
/// names the groups an op's node sits in, outermost first; `look` gives a group's
/// opacity and what its own cues make of it, or `None` where it is not drawn, nor
/// anything in it. A group that changes nothing (opaque, at rest) adds no layer.
fn grouped(
    ops: Vec<(bool, Op)>,
    groups: impl Fn(&str, bool) -> Vec<GroupKey>,
    look: impl Fn(&GroupKey) -> Option<(f32, Seen)>,
) -> Vec<Op> {
    // The groups open at this point in paint order, innermost last, with what each has gathered.
    let mut open: Vec<(GroupKey, Vec<Op>)> = Vec::new();
    let mut out = Vec::new();
    let close = |open: &mut Vec<(GroupKey, Vec<Op>)>, out: &mut Vec<Op>| {
        let (key, ops) = open.pop().expect("a group is open");
        let Some((opacity, seen)) = look(&key) else { return };
        let into = match open.last_mut() {
            Some((_, ops)) => ops,
            None => out,
        };
        if opacity == 1.0 && seen.leaves_layer() {
            into.extend(ops);
        } else {
            into.push(looked(layer(Some(&key.0), [0.0, 0.0], opacity, ops), seen));
        }
    };
    for (source, op) in ops {
        let path = match &op {
            Op::Layer { node: Some(id), .. } => groups(id, source),
            _ => Vec::new(),
        };
        let keep = open.iter().zip(&path).take_while(|((g, _), h)| g == *h).count();
        while open.len() > keep {
            close(&mut open, &mut out);
        }
        open.extend(path[keep..].iter().map(|g| (g.clone(), Vec::new())));
        match open.last_mut() {
            Some((_, ops)) => ops.push(op),
            None => out.push(op),
        }
    }
    while !open.is_empty() {
        close(&mut open, &mut out);
    }
    out
}

/// Every cue in `items`, groups opened.
fn cues(items: &[Item]) -> Vec<&scaena_core::timeline::Cue> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Cue(c) => out.push(c),
            Item::Sequence { items, .. } | Item::Parallel { items, .. } => out.extend(cues(items)),
        }
    }
    out
}

/// The text split a cue's unit names, if it names one.
fn text_split(split: Option<SplitUnit>) -> Option<TextSplit> {
    match split? {
        SplitUnit::Lines => Some(TextSplit::Lines),
        SplitUnit::Words => Some(TextSplit::Words),
        SplitUnit::Glyphs => Some(TextSplit::Glyphs),
        SplitUnit::Children | SplitUnit::Marks => None,
    }
}

impl Transition {
    /// The transition from `from` (the state before, at rest; `None` into the first
    /// state) into `to`, with the state's motions (`items`, from
    /// [`crate::motion::items`]), its cue starting `start` seconds into the global
    /// timeline. What a cue splits its targets into is counted here, from the scenes.
    pub fn new(
        from: Option<Scene>,
        to: Scene,
        timing: Timing,
        items: &[Item],
        start: f64,
    ) -> Result<Transition, EngineError> {
        let all = cues(items);
        let names = |c: &scaena_core::timeline::Cue, id: &str| c.targets.iter().any(|t| t == id);
        let entering = |m: &Motion| matches!(m, Motion::Enter(_));
        let leaving = |m: &Motion| matches!(m, Motion::Exit(_));
        // A chart's marks enter and leave with a cue on its marks; a cue on the whole
        // chart moves it as one, its values at rest.
        let marks = |id: &str, on: &dyn Fn(&Motion) -> bool| {
            all.iter().find(|c| names(c, id) && c.split == Some(SplitUnit::Marks) && on(&c.motion))
        };
        let whole = |id: &str, on: &dyn Fn(&Motion) -> bool| {
            all.iter().any(|c| names(c, id) && c.split.is_none() && on(&c.motion))
        };
        let look_of = |id: &str, on: &dyn Fn(&Motion) -> bool| marks(id, on).and_then(|c| MarkLook::of(&c.motion));

        let mut tracks: Vec<(Vec<(i64, usize)>, Track)> = Vec::new();
        let source = from.as_ref().map_or(&[][..], |s| &s.nodes[..]);
        for (i, a) in source.iter().enumerate() {
            let partner = to.nodes.iter().position(|b| b.id == a.id).filter(|_| timing.matched);
            if partner.is_none() {
                let track = match &a.content {
                    Content::Chart { chart, .. } if !whole(&a.id, &leaving) => Track::Chart {
                        from: Some(i),
                        to: None,
                        plan: Box::new(ChartPlan::new(Some(chart), None, None, look_of(&a.id, &leaving))),
                    },
                    _ => Track::Exit(i),
                };
                tracks.push((a.paint.clone(), track));
            }
        }
        for (j, b) in to.nodes.iter().enumerate() {
            let partner = source.iter().position(|a| a.id == b.id).filter(|_| timing.matched);
            let track = match (partner, b.policy) {
                (None, _) => match &b.content {
                    Content::Chart { chart, .. } if !whole(&b.id, &entering) => Track::Chart {
                        from: None,
                        to: Some(j),
                        plan: Box::new(ChartPlan::new(None, Some(chart), look_of(&b.id, &entering), None)),
                    },
                    _ => Track::Enter(j),
                },
                (Some(_), Policy::Cut) => Track::Cut(j),
                (Some(i), Policy::Crossfade) => Track::Crossfade { from: i, to: j },
                (Some(i), Policy::Morph) => match (&source[i].content, &b.content) {
                    (Content::Text(x), Content::Text(y)) if x.text == y.text => Track::Move { from: i, to: j },
                    (Content::Text(x), Content::Text(y)) => {
                        Track::Words { from: i, to: j, plan: Box::new(WordPlan::new(&x.text, &y.text)) }
                    }
                    (Content::Shader(x), Content::Shader(y)) if x.morphs_to(y) => Track::Move { from: i, to: j },
                    (Content::Shape(x), Content::Shape(y)) if x.morphs_to(y) => Track::Move { from: i, to: j },
                    (Content::Image(x), Content::Image(y)) if x.same_image(y) => Track::Move { from: i, to: j },
                    // Charts morph mark by mark between kinds that draw the same
                    // marks; any other change of kind cross-fades.
                    (Content::Table { table: x, .. }, Content::Table { table: y, .. }) => {
                        Track::Table { from: i, to: j, plan: Box::new(TablePlan::new(x, y)) }
                    }
                    (Content::Chart { chart: x, .. }, Content::Chart { chart: y, .. }) if x.kind.morphs_to(y.kind) => {
                        let (enter, exit) = (look_of(&b.id, &entering), look_of(&b.id, &leaving));
                        Track::Chart {
                            from: Some(i),
                            to: Some(j),
                            plan: Box::new(ChartPlan::new(Some(x), Some(y), enter, exit)),
                        }
                    }
                    _ => Track::Crossfade { from: i, to: j },
                },
            };
            tracks.push((b.paint.clone(), track));
        }
        // Stable: within one paint position, an exiting node draws under its successor.
        tracks.sort_by(|a, b| a.0.cmp(&b.0));
        let tracks: Vec<Track> = tracks.into_iter().map(|(_, t)| t).collect();

        // What each split cue's targets split into: an exit's in the state left, any
        // other's in this one.
        for cue in &all {
            let Some(split) = cue.split else { continue };
            for id in &cue.targets {
                let scene = if leaving(&cue.motion) { from.as_ref() } else { Some(&to) };
                let Some(node) = scene.and_then(|s| s.nodes.iter().find(|n| &n.id == id)) else { continue };
                let fits = match (split, &node.content) {
                    (SplitUnit::Lines | SplitUnit::Words | SplitUnit::Glyphs, Content::Text(_)) => true,
                    (SplitUnit::Marks, Content::Chart { .. }) => {
                        !matches!(cue.motion, Motion::Emphasis(_) | Motion::Keys(_))
                    }
                    (SplitUnit::Children, _) => true,
                    _ => false,
                };
                if !fits {
                    let split = serde_json::to_value(split).unwrap_or_default();
                    return Err(EngineError::Layout(format!(
                        "node `{id}`: it cannot split into {split} for this motion"
                    )));
                }
            }
        }
        let plan_of = |id: &str| {
            tracks.iter().find_map(|t| match t {
                Track::Chart { from: i, to: j, plan } => {
                    let node = j.map(|j| &to.nodes[j]).or(i.map(|i| &source[i]))?;
                    (node.id == id).then_some(&**plan)
                }
                _ => None,
            })
        };
        let mut units = |id: &str, split: SplitUnit, motion: &Motion| -> usize {
            let scene = if leaving(motion) { from.as_ref() } else { Some(&to) };
            let Some(scene) = scene else { return 0 };
            match (split, text_split(Some(split))) {
                (_, Some(unit)) => scene.nodes.iter().find(|n| n.id == id).map_or(0, |n| match &n.content {
                    Content::Text(placed) => placed.text.units(unit).len(),
                    _ => 0,
                }),
                (SplitUnit::Children, _) => scene.tree.get(id).map_or(0, |p| p.children.len()),
                (SplitUnit::Marks, _) => plan_of(id).map_or(0, |plan| plan.one_sided(entering(motion))),
                _ => 0,
            }
        };
        let schedule = schedule(timing.clock(), items, &mut units);
        Ok(Transition { from, to, timing, tracks, schedule, start })
    }

    /// The transition into the state alone, ms.
    pub fn duration_ms(&self) -> f64 {
        self.timing.duration_ms
    }

    /// How the transition into the state runs: its time, its curve, and whether it morphs.
    pub fn timing(&self) -> Timing {
        self.timing
    }

    /// The state's span, ms: its transition and every motion. From then on, at rest.
    pub fn span_ms(&self) -> f64 {
        self.schedule.span
    }

    /// The state's motions, placed on its clock.
    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    /// What the cues on `id` and on its containers make of it `t` ms in, outermost
    /// first: `None` if one has it out of sight. Exits move what was on screen
    /// (`source`); every other cue, what is. A text node's units and a chart's marks are
    /// drawn by the node itself. The flag: whether a cue brings the node on or takes it
    /// off, so the transition's own fade does not.
    fn seen(&self, id: &str, source: bool, t: f64) -> (Option<Seen>, bool) {
        let scene = if source { self.from.as_ref() } else { Some(&self.to) };
        let Some(scene) = scene else { return (Some(Seen::REST), false) };
        // Up to the group it sits in, if any: the group's layer takes the group's own
        // looks (SPEC §3.4). What reaches its members is how much of their outlines is
        // drawn, and whether they come and go with it.
        let group = |n: &str| scene.tree.get(n).is_some_and(|p| p.composite.is_some());
        let mut chain = vec![id];
        while let Some(parent) = chain.last().and_then(|n| scene.tree.get(*n)).and_then(|p| p.parent.as_deref()) {
            chain.push(parent);
            if group(parent) {
                break;
            }
        }
        chain.reverse();
        let in_group = chain.len() > 1 && group(chain[0]);
        let edge = if source { Motion::Exit(Look::REST) } else { Motion::Enter(Look::REST) };
        let (mut seen, mut governed) = (Seen::REST, in_group && self.applies(&edge, chain[0]));
        for (depth, &node) in chain.iter().enumerate() {
            for cue in self.schedule.of(node) {
                if matches!(cue.motion, Motion::Exit(_)) != source {
                    continue;
                }
                let comes_or_goes = matches!(cue.motion, Motion::Enter(_) | Motion::Exit(_));
                let place = |n: &str| scene.tree.get(n);
                let unit = match cue.split {
                    None => place(node).filter(|_| self.applies(&cue.motion, node)).map(|p| (0, p.rect)),
                    // A container's own panel moves with none of its children.
                    Some(SplitUnit::Children) => chain.get(depth + 1).and_then(|child| {
                        let k = place(node)?.children.iter().position(|c| c == child)?;
                        self.applies(&cue.motion, child).then_some((k, place(child)?.rect))
                    }),
                    Some(_) => {
                        governed |= comes_or_goes && depth + 1 == chain.len() && self.applies(&cue.motion, node);
                        None
                    }
                };
                let Some((k, rect)) = unit else { continue };
                governed |= comes_or_goes;
                match cue.motion.look(&cue.clock(k), t) {
                    Some(look) if depth == 0 && in_group && cue.split.is_none() => seen.progress *= look.progress,
                    Some(look) => seen = seen.within(&look, rect),
                    None => return (None, true),
                }
            }
        }
        (Some(seen), governed)
    }

    /// Whether `motion` moves `unit` (a node): an entrance what enters in this state, an
    /// exit what leaves in it, anything else what is on screen. An entrance cue on a node
    /// that stays does nothing.
    fn applies(&self, motion: &Motion, unit: &str) -> bool {
        let was = self.from.as_ref().is_some_and(|f| f.tree.contains_key(unit));
        let is = self.to.tree.contains_key(unit);
        match motion {
            Motion::Enter(_) => is && !(was && self.timing.matched),
            Motion::Exit(_) => was && !(is && self.timing.matched),
            Motion::Emphasis(_) | Motion::Keys(_) => true,
        }
    }

    /// `node`'s layer `op`, its text split into the units of its cue if one splits it.
    fn units(&self, dl: &mut DisplayList, op: Op, node: &SceneNode, source: bool, t: f64) -> Op {
        let Content::Text(placed) = &node.content else { return op };
        let cue = self.schedule.of(&node.id).find(|c| {
            text_split(c.split).is_some()
                && matches!(c.motion, Motion::Exit(_)) == source
                && self.applies(&c.motion, &node.id)
        });
        let (Some(cue), Op::Layer { node, cell, transform, opacity, blend, clip, .. }) = (cue, &op) else { return op };
        let split = text_split(cue.split).expect("found by its split");
        let ops = unit_ops(dl, &placed.text, split, cue, t);
        Op::Layer {
            node: node.clone(),
            cell: *cell,
            transform: *transform,
            opacity: *opacity,
            blend: *blend,
            clip: clip.clone(),
            ops,
        }
    }

    /// Draws `node` at rest, through its cues, `t` ms into the state's cue.
    fn put(&self, dl: &mut DisplayList, node: &SceneNode, seen: Seen, opacity: f32, source: bool, t: f64) {
        let op = match &node.content {
            Content::Shape(shape) if seen.progress < 1.0 => {
                let shape = shape.drawn(seen.progress as f32);
                layer(Some(&node.id), [shape.rect[0], shape.rect[1]], opacity, shape.ops())
            }
            _ => node.draw(dl, opacity, self.start + t / 1000.0),
        };
        let op = self.units(dl, op, node, source, t);
        dl.ops.push(looked(op, seen));
    }

    /// The frame `t_ms` into the state's cue: into its transition, then its motions.
    pub fn frame(&self, t_ms: f64) -> DisplayList {
        let span = self.schedule.span;
        if t_ms.is_nan() || t_ms >= span {
            let at = if t_ms.is_finite() { t_ms } else { span };
            return self.to.draw_at(self.start + at / 1000.0);
        }
        // The global timeline: the state's start, and as far into its cue.
        let time = self.start + t_ms / 1000.0;
        if t_ms <= 0.0 {
            return self.from.as_ref().map_or_else(|| self.to.ground(), |s| s.draw_at(time));
        }
        // The transition runs until its duration, a spring perhaps past its target; then
        // the motions that outlast it run over the state at rest.
        let moving = t_ms < self.timing.duration_ms;
        let geo = self.timing.progress(t_ms) as f32;
        let p = geo.clamp(0.0, 1.0);
        let from = self.from.as_ref().map_or(&[][..], |s| &s.nodes[..]);
        let to = &self.to.nodes;
        let mut dl = self.to.ground();
        // Where each track's ops start, to tell which were drawn from the state being left.
        let mut starts = Vec::with_capacity(self.tracks.len());
        for track in &self.tracks {
            starts.push(dl.ops.len());
            match track {
                Track::Exit(i) => {
                    let node = &from[*i];
                    let (Some(seen), governed) = self.seen(&node.id, true, t_ms) else { continue };
                    if governed {
                        self.put(&mut dl, node, seen, node.opacity, true, t_ms);
                    } else if moving {
                        self.put(&mut dl, node, seen, node.opacity * (1.0 - p), true, t_ms);
                    }
                }
                Track::Enter(j) => {
                    let node = &to[*j];
                    let (Some(seen), governed) = self.seen(&node.id, false, t_ms) else { continue };
                    let fade = if governed || !moving { 1.0 } else { p };
                    self.put(&mut dl, node, seen, node.opacity * fade, false, t_ms);
                }
                Track::Cut(j) => {
                    let node = &to[*j];
                    let (Some(seen), _) = self.seen(&node.id, false, t_ms) else { continue };
                    self.put(&mut dl, node, seen, node.opacity, false, t_ms);
                }
                Track::Crossfade { from: i, to: j } => {
                    let (Some(seen), _) = self.seen(&to[*j].id, false, t_ms) else { continue };
                    if moving {
                        push(&mut dl, &from[*i], from[*i].opacity * (1.0 - p), time);
                        self.put(&mut dl, &to[*j], seen, to[*j].opacity * p, false, t_ms);
                    } else {
                        self.put(&mut dl, &to[*j], seen, to[*j].opacity, false, t_ms);
                    }
                }
                Track::Move { from: i, to: j } => {
                    let (Some(seen), _) = self.seen(&to[*j].id, false, t_ms) else { continue };
                    if !moving {
                        self.put(&mut dl, &to[*j], seen, to[*j].opacity, false, t_ms);
                        continue;
                    }
                    let opacity = lerp(from[*i].opacity, to[*j].opacity, p);
                    let op = match (&from[*i].content, &to[*j].content) {
                        (Content::Text(a), Content::Text(b)) => {
                            let origin = lerp2(a.origin, b.origin, geo);
                            text_layer(&mut dl, &to[*j].id, b, origin, opacity)
                        }
                        (Content::Shader(a), Content::Shader(b)) => {
                            let shader = ShaderNode::lerp(a, b, geo, p);
                            layer(Some(&to[*j].id), [shader.rect[0], shader.rect[1]], opacity, vec![shader.op(time)])
                        }
                        (Content::Shape(a), Content::Shape(b)) => {
                            let shape = ShapeNode::lerp(a, b, geo, p).drawn(seen.progress as f32);
                            layer(Some(&to[*j].id), [shape.rect[0], shape.rect[1]], opacity, shape.ops())
                        }
                        (Content::Image(a), Content::Image(b)) => {
                            let image = ImageNode::lerp(a, b, geo);
                            layer(Some(&to[*j].id), [image.rect[0], image.rect[1]], opacity, image.ops())
                        }
                        _ => unreachable!(
                            "Move tracks pair text with text, and a shader, a shape, or an image with itself"
                        ),
                    };
                    let op = self.units(&mut dl, op, &to[*j], false, t_ms);
                    dl.ops.push(looked(op, seen));
                }
                Track::Chart { from: i, to: j, plan } => {
                    let (a, b) = (chart(i.map(|i| &from[i])), chart(j.map(|j| &to[j])));
                    let id = &b.or(a).expect("a chart track has a side").0.id;
                    let cue = |on: fn(&Motion) -> bool| {
                        self.schedule.of(id).find(|c| c.split == Some(SplitUnit::Marks) && on(&c.motion))
                    };
                    let (enter, exit) = (cue(|m| matches!(m, Motion::Enter(_))), cue(|m| matches!(m, Motion::Exit(_))));
                    let running = [enter, exit].into_iter().flatten().any(|c| t_ms < c.end());
                    let (Some(seen), _) = self.seen(id, b.is_none(), t_ms) else { continue };
                    if !moving && !running {
                        // At rest, or gone.
                        if let Some(j) = j {
                            self.put(&mut dl, &to[*j], seen, to[*j].opacity, false, t_ms);
                        }
                        continue;
                    }
                    // A chart that comes or goes with a cue on its marks shows its frame
                    // (axes, legend, titles) with its first mark in, or its last out.
                    let p = match (a, b, enter, exit) {
                        (None, Some(_), Some(c), _) => c.clock(0).progress(t_ms).clamp(0.0, 1.0) as f32,
                        (Some(_), None, _, Some(c)) => {
                            c.clock(c.units.saturating_sub(1)).progress(t_ms).clamp(0.0, 1.0) as f32
                        }
                        _ if moving => p,
                        _ => 1.0,
                    };
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
                    let ops = plan.sample(&mut dl, a.map(|c| c.2), b.map(|c| c.2), p, t_ms, [enter, exit], clip_y);
                    let op = chart_layer(id, origin, width, dl.viewport[1], opacity, ops);
                    dl.ops.push(looked(op, seen));
                }
                Track::Table { from: i, to: j, plan } => {
                    let (x, y) = (&from[*i], &to[*j]);
                    let (Some(seen), _) = self.seen(&y.id, false, t_ms) else { continue };
                    if !moving {
                        self.put(&mut dl, y, seen, y.opacity, false, t_ms);
                        continue;
                    }
                    let (Content::Table { cell: ca, table: a }, Content::Table { cell: cb, table: b }) =
                        (&x.content, &y.content)
                    else {
                        unreachable!("Table tracks pair tables")
                    };
                    let origin = lerp2([ca[0], ca[1]], [cb[0], cb[1]], p);
                    let ops = plan.sample(&mut dl, a, b, p);
                    dl.ops.push(looked(layer(Some(&y.id), origin, lerp(x.opacity, y.opacity, p), ops), seen));
                }
                Track::Words { from: i, to: j, plan } => {
                    let (x, y) = (&from[*i], &to[*j]);
                    let (Some(seen), _) = self.seen(&y.id, false, t_ms) else { continue };
                    if !moving {
                        self.put(&mut dl, y, seen, y.opacity, false, t_ms);
                        continue;
                    }
                    let (Content::Text(a), Content::Text(b)) = (&x.content, &y.content) else {
                        unreachable!("Words tracks pair text")
                    };
                    let origin = lerp2(a.origin, b.origin, geo);
                    let ops = plan.sample(&mut dl, [a.origin, b.origin], origin, p, geo);
                    // Clipped text stays clipped where it stands at either end.
                    let clip = (a.clip.is_some() && b.clip.is_some())
                        .then(|| b.clip.map(|[x, y, w, h]| Path::rect([x - origin[0], y - origin[1], w, h])))
                        .flatten();
                    let Op::Layer { node, cell, transform, opacity, blend, ops, .. } =
                        layer(Some(&y.id), origin, lerp(x.opacity, y.opacity, p), ops)
                    else {
                        unreachable!("`layer` makes layers")
                    };
                    dl.ops.push(looked(Op::Layer { node, cell, transform, opacity, blend, clip, ops }, seen));
                }
            }
        }
        // Ops drawn from the state being left: its own groups gather those.
        let mut left = vec![false; dl.ops.len()];
        for (k, (track, &start)) in self.tracks.iter().zip(&starts).enumerate() {
            let end = starts.get(k + 1).copied().unwrap_or(dl.ops.len());
            match track {
                Track::Exit(_) | Track::Chart { to: None, .. } => left[start..end].fill(true),
                // A cross-fade draws what leaves first.
                Track::Crossfade { .. } if moving && end > start => left[start] = true,
                _ => {}
            }
        }
        let ops = left.into_iter().zip(std::mem::take(&mut dl.ops)).collect();
        dl.ops = grouped(ops, |id, source| self.groups_of(id, source), |key| self.group_layer(key, t_ms, moving, p));
        dl
    }

    /// The groups the node `id` sits in, outermost first, in the state it is drawn from:
    /// a group in both states, under `match: id`, is one layer.
    fn groups_of(&self, id: &str, source: bool) -> Vec<GroupKey> {
        let scene = if source { self.from.as_ref() } else { Some(&self.to) };
        let stays = |g: &str| self.timing.matched && self.to.tree.contains_key(g);
        (scene.map(|s| s.groups(id)).unwrap_or_default().into_iter())
            .map(|g| {
                let left = source && !stays(&g);
                (g, left)
            })
            .collect()
    }

    /// A group's layer `t` ms into the cue: its opacity, moving between its two states
    /// or fading as it comes or goes with the transition, and what its own cues make of
    /// it. `None` where it is not drawn.
    fn group_layer(&self, (g, left): &GroupKey, t: f64, moving: bool, p: f32) -> Option<(f32, Seen)> {
        let composite = |s: Option<&Scene>| s.and_then(|s| s.tree.get(g.as_str())).and_then(|place| place.composite);
        let (before, now) = (composite(self.from.as_ref()), composite(Some(&self.to)));
        let (seen, governed) = self.seen(g, *left, t);
        let seen = seen?;
        let opacity = if *left {
            let a = before?;
            match (governed, moving) {
                (true, _) => a,
                (false, true) => a * (1.0 - p),
                (false, false) => return None,
            }
        } else {
            let b = now?;
            match before.filter(|_| self.timing.matched) {
                Some(a) if moving => lerp(a, b, p),
                Some(_) => b,
                None if governed || !moving => b,
                None => b * p,
            }
        };
        Some((opacity, seen))
    }
}

/// How a text node's words get from one layout to the next (SPEC §2.3). Words match in
/// order by their text (a longest common subsequence, spaces and soft hyphens aside, and
/// the punctuation around a word a word of its own). A shared word that draws the same
/// moves from its old place to its new one, its color mixing in Oklab; one that draws
/// differently (another size, font, or instance) maps between its two boxes as its two
/// drawings cross-fade. A word on one side only fades where it stands: one that leaves
/// over the first half of the transition, one that arrives over the second.
#[derive(Debug, Clone, PartialEq)]
struct WordPlan {
    pairs: Vec<WordPair>,
    /// Words of the source alone, and of the target alone.
    gone: Vec<Word>,
    came: Vec<Word>,
}

/// One word's glyphs, run by run, what each run says, and its box: relative to its
/// text's top-left corner.
#[derive(Debug, Clone, PartialEq)]
struct Word {
    runs: Vec<GlyphRun>,
    said: Vec<(String, Vec<u32>)>,
    rect: Rect,
}

#[derive(Debug, Clone, PartialEq)]
struct WordPair {
    a: Word,
    b: Word,
    /// Whether both draw the same glyphs the same way, colors aside.
    same: bool,
}

impl WordPlan {
    fn new(a: &TextLayout, b: &TextLayout) -> WordPlan {
        // A word's ink: its glyphs less the spaces after it, which draw nothing, so a word
        // that ends a line on one side and not the other is still the same drawing. The
        // punctuation before and after a word is a word of its own, so "grew." is "grew"
        // with a period after it, and the period can stay at the end of the sentence.
        let words = |t: &TextLayout| -> Vec<(String, Word)> {
            let ink = |r: usize, g: usize| !t.text[t.runs[r].clusters[g]..].starts_with(char::is_whitespace);
            let starts = t.cluster_starts();
            let mut out = Vec::new();
            for u in t.units(TextSplit::Words) {
                let text = &t.text[u.text.clone()];
                let first = text.find(char::is_alphanumeric).unwrap_or(text.len());
                let last =
                    text.char_indices().rfind(|(_, c)| c.is_alphanumeric()).map_or(first, |(i, c)| i + c.len_utf8());
                for (lo, hi) in [(0, first), (first, last), (last, text.len())] {
                    let range = u.text.start + lo..u.text.start + hi;
                    let key: String =
                        t.text[range.clone()].chars().filter(|c| *c != '\u{AD}' && !c.is_whitespace()).collect();
                    let glyphs: Vec<(usize, usize)> = (u.glyphs.iter().copied())
                        .filter(|&(r, g)| ink(r, g) && range.contains(&t.runs[r].clusters[g]))
                        .collect();
                    if key.is_empty() || glyphs.is_empty() {
                        continue;
                    }
                    let runs = subset(t, |r, g| glyphs.contains(&(r, g)));
                    let said = runs.iter().map(|run| said(&t.text, &starts, run)).collect();
                    out.push((key, Word { runs, said, rect: unit_box(t, &glyphs) }));
                }
            }
            out
        };
        let (wa, wb) = (words(a), words(b));
        // The longest common subsequence of the two word lists, by their text.
        let (n, m) = (wa.len(), wb.len());
        let mut lcs = vec![vec![0usize; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i][j] = if wa[i].0 == wb[j].0 { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
            }
        }
        let (mut plan, mut i, mut j) = (WordPlan { pairs: Vec::new(), gone: Vec::new(), came: Vec::new() }, 0, 0);
        while i < n || j < m {
            if i < n && j < m && wa[i].0 == wb[j].0 {
                let (a, b) = (wa[i].1.clone(), wb[j].1.clone());
                plan.pairs.push(WordPair { same: same_glyphs(&a, &b), a, b });
                (i, j) = (i + 1, j + 1);
            } else if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
                plan.came.push(wb[j].1.clone());
                j += 1;
            } else {
                plan.gone.push(wa[i].1.clone());
                i += 1;
            }
        }
        plan
    }

    /// The words `p` of the way across (`geo` for where they stand, which a spring may
    /// carry past 1), in a layer at `origin`: the source's words laid out at `origins[0]`,
    /// the target's at `origins[1]`.
    fn sample(&self, dl: &mut DisplayList, origins: [Point; 2], origin: Point, p: f32, geo: f32) -> Vec<Op> {
        let at = |o: Point| [o[0] - origin[0], o[1] - origin[1]];
        let (from, to) = (at(origins[0]), at(origins[1]));
        let word = |dl: &mut DisplayList, runs: &[GlyphRun], w: &Word, transform: [f32; 6], opacity: f32| Op::Layer {
            node: None,
            cell: None,
            transform,
            opacity,
            blend: Blend::Normal,
            clip: None,
            ops: runs.iter().zip(&w.said).map(|(run, said)| glyph_op(dl, run, said.clone())).collect(),
        };
        let shift = |d: Point| [1.0, 0.0, 0.0, 1.0, d[0], d[1]];
        let mut ops = Vec::with_capacity(self.pairs.len() + self.gone.len() + self.came.len());
        // Words that leave fade out over the first half, and words that arrive fade in over
        // the second, so neither shows under the words moving to make or take the room.
        for (words, at, opacity) in [(&self.gone, from, 1.0 - 2.0 * p), (&self.came, to, 2.0 * p - 1.0)] {
            if opacity > 0.0 {
                ops.extend(words.iter().map(|w| word(dl, &w.runs, w, shift(at), opacity)));
            }
        }
        for pair in &self.pairs {
            let (ra, rb) = (pair.a.rect, pair.b.rect);
            // Where the word's box stands, in the layer: from the source's to the target's.
            let start = [from[0] + ra[0], from[1] + ra[1], ra[2], ra[3]];
            let end = [to[0] + rb[0], to[1] + rb[1], rb[2], rb[3]];
            let now = [0, 1, 2, 3].map(|k| lerp(start[k], end[k], geo));
            if pair.same {
                let runs: Vec<GlyphRun> = (pair.a.runs.iter().zip(&pair.b.runs))
                    .map(|(x, y)| GlyphRun { color: mix(x.color, y.color, p), ..y.clone() })
                    .collect();
                ops.push(word(dl, &runs, &pair.b, shift([now[0] - rb[0], now[1] - rb[1]]), 1.0));
            } else {
                // Each drawing scaled from its own box onto the box between.
                let onto = |r: Rect| {
                    let (sx, sy) = (now[2] / r[2].max(1e-6), now[3] / r[3].max(1e-6));
                    [sx, 0.0, 0.0, sy, now[0] - r[0] * sx, now[1] - r[1] * sy]
                };
                ops.push(word(dl, &pair.a.runs, &pair.a, onto(ra), 1.0 - p));
                ops.push(word(dl, &pair.b.runs, &pair.b, onto(rb), p));
            }
        }
        ops
    }
}

/// Whether two words draw the same glyphs the same way, colors aside: run by run, the
/// same font, size, and instance, and each glyph the same where it stands in its box.
fn same_glyphs(a: &Word, b: &Word) -> bool {
    a.runs.len() == b.runs.len()
        && a.runs.iter().zip(&b.runs).all(|(x, y)| {
            x.font == y.font
                && x.size == y.size
                && x.coords == y.coords
                && x.glyphs.len() == y.glyphs.len()
                && x.glyphs.iter().zip(&y.glyphs).all(|(g, h)| {
                    g.id == h.id
                        && ((g.x - a.rect[0]) - (h.x - b.rect[0])).abs() < 1e-3
                        && ((g.y - a.rect[1]) - (h.y - b.rect[1])).abs() < 1e-3
                })
        })
}

/// A text node's glyphs as its cue's units, each in its own layer through its look,
/// `t` ms into the state's cue: a unit out of sight is left out, and glyphs in no unit
/// (spaces between lines) stay as they are, under the units.
fn unit_ops(dl: &mut DisplayList, text: &TextLayout, split: TextSplit, cue: &Placed, t: f64) -> Vec<Op> {
    let units = text.units(split);
    let starts = text.cluster_starts();
    let mut taken: HashSet<(usize, usize)> = HashSet::new();
    let mut layers = Vec::with_capacity(units.len());
    for (k, unit) in units.iter().enumerate() {
        taken.extend(unit.glyphs.iter().copied());
        let Some(look) = cue.motion.look(&cue.clock(k), t) else { continue };
        let runs = subset(text, |r, g| unit.glyphs.contains(&(r, g)));
        let mut ops = glyph_ops(dl, &text.text, &starts, &runs);
        if let Some((color, q)) = look.tint.filter(|t| t.1 > 0.0) {
            ops.iter_mut().for_each(|op| tint(op, color, q as f32));
        }
        let [a, b, c, d, e, f] = look.affine(unit_box(text, &unit.glyphs)).map(|v| v as f32);
        let transform = [a, b, c, d, e, f];
        let opacity = look.opacity as f32;
        layers.push(Op::Layer { node: None, cell: None, transform, opacity, blend: Blend::Normal, clip: None, ops });
    }
    let rest = subset(text, |r, g| !taken.contains(&(r, g)));
    let mut ops = glyph_ops(dl, &text.text, &starts, &rest);
    ops.extend(layers);
    ops
}

/// The glyphs of `text`'s runs that `keep` keeps, run by run; runs left empty go.
fn subset(text: &TextLayout, keep: impl Fn(usize, usize) -> bool) -> Vec<GlyphRun> {
    let mut out = Vec::new();
    for (r, run) in text.runs.iter().enumerate() {
        let picked: Vec<usize> = (0..run.glyphs.len()).filter(|&g| keep(r, g)).collect();
        if picked.is_empty() {
            continue;
        }
        let mut part = run.clone();
        part.glyphs = picked.iter().map(|&g| run.glyphs[g]).collect();
        part.clusters = picked.iter().map(|&g| run.clusters[g]).collect();
        part.advances = picked.iter().map(|&g| run.advances[g]).collect();
        out.push(part);
    }
    out
}

/// A text unit's box, relative to the text's top-left corner: across its glyphs'
/// advances, and down the line boxes they sit in.
fn unit_box(text: &TextLayout, glyphs: &[(usize, usize)]) -> Rect {
    let (mut x0, mut x1, mut y0, mut y1) = (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY);
    for &(r, g) in glyphs {
        let run = &text.runs[r];
        let (x, advance) = (run.glyphs[g].x, run.advances[g]);
        (x0, x1) = (x0.min(x.min(x + advance)), x1.max(x.max(x + advance)));
        if let Some(line) = text.lines.get(run.line) {
            (y0, y1) = (y0.min(line.top), y1.max(line.top + line.height));
        }
    }
    if !(x0.is_finite() && y0.is_finite()) {
        return [0.0; 4];
    }
    [x0, y0, x1 - x0, y1 - y0]
}

/// How a table's parts get from one snapshot to the next: cells by row and column (the
/// header's by column), rules by the row above them (the header's rule by none).
#[derive(Debug, Clone, PartialEq)]
struct TablePlan {
    cells: Vec<Pair>,
    rules: Vec<Pair>,
}

impl TablePlan {
    fn new(a: &TableLayout, b: &TableLayout) -> TablePlan {
        let cells = |t: &TableLayout| -> Vec<String> {
            t.header.iter().chain(&t.cells).map(|c| format!("{}\u{1f}{}", c.row, c.column)).collect()
        };
        let rules = |t: &TableLayout| -> Vec<String> {
            t.rule.iter().map(|_| "\u{1f}".to_string()).chain(t.row_rules.iter().map(|(k, _)| k.clone())).collect()
        };
        TablePlan { cells: pair(&cells(a), &cells(b), |k| k), rules: pair(&rules(a), &rules(b), |k| k) }
    }

    /// The table's ops `p` of the way from `a` to `b`: a cell on both sides moves, and
    /// cross-fades if its text changed; one on one side only fades where it is, as rows
    /// and columns come and go. Rules move, or fade.
    fn sample(&self, dl: &mut DisplayList, a: &TableLayout, b: &TableLayout, p: f32) -> Vec<Op> {
        let all = |t: &TableLayout| -> Vec<Rule> {
            t.rule.iter().chain(t.row_rules.iter().map(|(_, r)| r)).cloned().collect()
        };
        let (ra, rb) = (all(a), all(b));
        let mut ops = Vec::new();
        for &(i, j) in &self.rules {
            match (i.map(|i| &ra[i]), j.map(|j| &rb[j])) {
                (Some(x), Some(y)) => ops.push(rule_op(&lerp_rule(x, y, p), 1.0)),
                (Some(x), None) => ops.push(rule_op(x, 1.0 - p)),
                (None, Some(y)) => ops.push(rule_op(y, p)),
                (None, None) => {}
            }
        }
        let (ca, cb): (Vec<&Cell>, Vec<&Cell>) =
            (a.header.iter().chain(&a.cells).collect(), b.header.iter().chain(&b.cells).collect());
        let mut cell = |at: Point, alpha: f32, c: &Cell| ops.push(cell_layer(dl, c, at, alpha));
        for &(i, j) in &self.cells {
            match (i.map(|i| ca[i]), j.map(|j| cb[j])) {
                (Some(x), Some(y)) if x.text.text == y.text.text && x.text.runs == y.text.runs => {
                    cell(lerp2(x.origin, y.origin, p), 1.0, y)
                }
                // Changed text: both keep to the anchor as it moves.
                (Some(x), Some(y)) => {
                    let at = lerp2(x.anchor, y.anchor, p);
                    let place = |c: &Cell| [at[0] + c.origin[0] - c.anchor[0], at[1] + c.origin[1] - c.anchor[1]];
                    cell(place(x), 1.0 - p, x);
                    cell(place(y), p, y);
                }
                (Some(x), None) => cell(x.origin, 1.0 - p, x),
                (None, Some(y)) => cell(y.origin, p, y),
                (None, None) => {}
            }
        }
        ops
    }
}

/// A chart track's node on one side, with its cell and layout.
fn chart(node: Option<&SceneNode>) -> Option<(&SceneNode, Rect, &ChartLayout)> {
    node.map(|n| match &n.content {
        Content::Chart { cell, chart } => (n, *cell, &**chart),
        Content::Text(_) | Content::Shader(_) | Content::Shape(_) | Content::Image(_) | Content::Table { .. } => {
            unreachable!("Chart tracks pair charts")
        }
    })
}

/// A table's ops: its rules under its text, the header's and then the body's.
fn table_ops(dl: &mut DisplayList, table: &TableLayout) -> Vec<Op> {
    let rules = table.rule.iter().chain(table.row_rules.iter().map(|(_, r)| r));
    let mut ops: Vec<Op> = rules.map(|r| rule_op(r, 1.0)).collect();
    for cell in table.header.iter().chain(&table.cells) {
        ops.push(cell_layer(dl, cell, cell.origin, 1.0));
    }
    ops
}

fn push(dl: &mut DisplayList, node: &SceneNode, opacity: f32, time: f64) {
    let op = node.draw(dl, opacity, time);
    dl.ops.push(op);
}

/// What a mark cue does to one mark (SPEC §3.7): where an entering mark starts and a
/// leaving one ends. A cue that scales grows the mark from where it would stand with no
/// value, whatever its anchor; one that does not keeps the mark whole, fading or moving.
#[derive(Debug, Clone, Copy, PartialEq)]
struct MarkLook {
    opacity: f32,
    translate: [f32; 2],
    grow: bool,
}

impl MarkLook {
    fn of(motion: &Motion) -> Option<MarkLook> {
        let (Motion::Enter(look) | Motion::Exit(look)) = motion else { return None };
        Some(MarkLook {
            opacity: look.opacity as f32,
            translate: look.translate.map(|v| v as f32),
            grow: look.scale != [1.0, 1.0],
        })
    }
}

impl ChartPlan {
    fn new(
        a: Option<&ChartLayout>,
        b: Option<&ChartLayout>,
        enter: Option<MarkLook>,
        exit: Option<MarkLook>,
    ) -> ChartPlan {
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
        // Entering marks stagger in the target's order, leaving ones in the source's: a
        // line's or an area's by series, so a series' points move together and its path
        // keeps its shape, and any other chart's mark by mark.
        let unit = |k: &Keyed| match k.pair {
            (None, Some(j)) => b.map(|c| (1, unit_of(c, j))),
            (Some(i), None) => a.map(|c| (0, unit_of(c, i))),
            _ => None,
        };
        let units: Vec<Option<(usize, usize)>> = marks.iter().map(|(k, _)| unit(k)).collect();
        let peers = |s: usize| {
            let mut peers: Vec<usize> = units.iter().flatten().filter(|(t, _)| *t == s).map(|&(_, u)| u).collect();
            peers.sort_unstable();
            peers.dedup();
            peers
        };
        let peers = [peers(0), peers(1)];
        let order =
            (units.iter()).map(|u| u.map(|(s, u)| (peers[s].partition_point(|&x| x < u), peers[s].len()))).collect();
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
            notes: pair(notes_of(a), notes_of(b), |n| &n.key),
        }
    }

    /// How many units enter (`true`) or leave: what a cue on the chart's marks counts. A
    /// line's or an area's unit is a series, any other chart's a mark.
    fn one_sided(&self, entering: bool) -> usize {
        let side = |k: &Keyed| match k.pair {
            (None, Some(_)) => entering,
            (Some(_), None) => !entering,
            _ => false,
        };
        (self.marks.iter().zip(&self.order))
            .find_map(|((k, _), order)| order.filter(|_| side(k)).map(|(_, n)| n))
            .unwrap_or(0)
    }

    /// The chart's ops `p` of the way from `a` to `b`, in the order a chart at rest
    /// draws them: gridlines, baseline; marks, category labels, and value labels in the
    /// plot (see [`plot_layer`]); value-axis labels, titles. Marks that enter or leave
    /// with a cue (`cues`: the entering marks', the leaving marks') run on its clocks,
    /// `t_ms` into the state's cue. `clip_y` is the plot clip's top and height.
    #[allow(clippy::too_many_arguments)]
    fn sample(
        &self,
        dl: &mut DisplayList,
        a: Option<&ChartLayout>,
        b: Option<&ChartLayout>,
        p: f32,
        t_ms: f64,
        cues: [Option<&Placed>; 2],
        clip_y: [f32; 2],
    ) -> Vec<Op> {
        let mut ops = Vec::new();
        // Annotations by key: a band, a rule, a leader, and text on both sides move (text
        // that changed cross-fades); one on one side only fades where it is.
        let (na, nb) = (notes_of(a), notes_of(b));
        let notes: Vec<(Option<&Note>, Option<&Note>)> =
            self.notes.iter().map(|&(i, j)| (i.map(|i| &na[i]), j.map(|j| &nb[j]))).collect();
        for &(x, y) in &notes {
            match (x.and_then(|n| n.band), y.and_then(|n| n.band)) {
                (Some((r, c)), Some((s, d))) => {
                    let rect = [0, 1, 2, 3].map(|k| lerp(r[k], s[k], p));
                    ops.push(band_op(rect, mix(c, d, p), 1.0));
                }
                (Some((r, c)), None) => ops.push(band_op(r, c, 1.0 - p)),
                (None, Some((s, d))) => ops.push(band_op(s, d, p)),
                (None, None) => {}
            }
        }
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
        // How far along each mark is: a cue staggers the marks that enter or leave.
        let mut progress = Vec::with_capacity(self.marks.len());
        for ((&(Keyed { pair: (i, j), .. }, _), ends), order) in self.marks.iter().zip(&self.ends).zip(&self.order) {
            let cue = match (i, j) {
                (None, Some(_)) => self.enter.zip(cues[0]).map(|(look, cue)| (look, cue, true)),
                (Some(_), None) => self.exit.zip(cues[1]).map(|(look, cue)| (look, cue, false)),
                _ => None,
            };
            let (p, look) = match (cue, order) {
                (Some((look, cue, entering)), Some((k, _))) => {
                    let q = cue.clock(*k).progress(t_ms) as f32;
                    // Entering, the look fades as the mark arrives; leaving, it comes on.
                    let w = if entering { 1.0 - q } else { q };
                    let alpha = lerp(1.0, look.opacity, w).clamp(0.0, 1.0);
                    (q, Some((alpha, [look.translate[0] * w, look.translate[1] * w])))
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
        let same_kind = a.zip(b).is_none_or(|(x, y)| x.kind == y.kind);
        let mut counted = progress.clone();
        if same_kind && self.regroup.is_none() && shapes.len() == self.marks.len() {
            for (i, q) in restack(&mut shapes, &progress, a, b) {
                counted[i] = q;
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
        for &(x, y) in &notes {
            let (ga, gb) = (x.map_or(&[][..], |n| &n.gaps), y.map_or(&[][..], |n| &n.gaps));
            let rule = match (x.and_then(|n| n.rule.as_ref()), y.and_then(|n| n.rule.as_ref())) {
                (Some(r), Some(s)) => {
                    let rule = lerp_rule(r, s, p);
                    broken_rule_op(&rule, &lerp_gaps(ga, gb, p, &rule), 1.0)
                }
                (Some(r), None) => broken_rule_op(r, &spans(ga), 1.0 - p),
                (None, Some(s)) => broken_rule_op(s, &spans(gb), p),
                (None, None) => None,
            };
            plot.extend(rule);
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
            let mut tick = |at: Point, alpha: f32, l: &Label| plot.push(layer(None, at, alpha, text_ops(dl, &l.text)));
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
        for ((&(marks, (i, j)), shape), &p) in self.marks.iter().zip(&sampled).zip(&counted) {
            value_label(dl, &mut plot, (i.map(|i| &la[i]), j.map(|j| &lb[j])), marks.pair, shape, numerals, p);
        }
        for &(x, y) in &notes {
            let (lx, ly) = (x.and_then(|n| n.label.as_ref()), y.and_then(|n| n.label.as_ref()));
            text_between(dl, &mut plot, lx, ly, p);
        }
        // The plot clips while either side's does, its sides moving from one to the
        // other's.
        let edge = |c: &ChartLayout| c.clip.unwrap_or([c.plot[0], c.plot[0] + c.plot[2]]);
        let clip = match (a, b) {
            (Some(x), Some(y)) => (x.clip.is_some() || y.clip.is_some()).then(|| lerp2(edge(x), edge(y), p)),
            (Some(c), None) | (None, Some(c)) => c.clip,
            (None, None) => None,
        };
        ops.extend(plot_layer(clip, clip_y, plot));
        // Value-axis labels ride with their ticks.
        for &(x, y) in &ticks {
            let (lx, ly) = (x.and_then(|t| t.label.as_ref()), y.and_then(|t| t.label.as_ref()));
            let value = x.or(y).map_or(0.0, |t| t.value);
            let mut label = |at: Point, alpha: f32, l: &Label| ops.push(layer(None, at, alpha, text_ops(dl, &l.text)));
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
            text_between(dl, &mut ops, i.map(|i| &ta[i]), j.map(|j| &tb[j]), p);
        }
        // Legend entries move and change color; one on one side only fades, and so does
        // one that turns from a key's entry to a direct name or back, where it is.
        let (ea, eb) = (legend_of(a), legend_of(b));
        let named = |e: &LegendEntry| !(e.swatch.w > 0.0 && e.swatch.h > 0.0);
        for &(i, j) in &self.legend {
            match (i.map(|i| &ea[i]), j.map(|j| &eb[j])) {
                (Some(x), Some(y)) if named(x) != named(y) => {
                    ops.extend(legend_ops(dl, x.swatch, x.color, &x.label, x.label.origin, 1.0 - p, x.label.opacity));
                    ops.extend(legend_ops(dl, y.swatch, y.color, &y.label, y.label.origin, p, y.label.opacity));
                }
                (Some(x), Some(y)) => {
                    let swatch = RoundRect::lerp(x.swatch, y.swatch, p);
                    let at = lerp2(x.label.origin, y.label.origin, p);
                    let opacity = lerp(x.label.opacity, y.label.opacity, p);
                    ops.extend(legend_ops(dl, swatch, mix(x.color, y.color, p), &y.label, at, 1.0, opacity));
                }
                (Some(x), None) => {
                    ops.extend(legend_ops(dl, x.swatch, x.color, &x.label, x.label.origin, 1.0 - p, x.label.opacity))
                }
                (None, Some(y)) => {
                    ops.extend(legend_ops(dl, y.swatch, y.color, &y.label, y.label.origin, p, y.label.opacity))
                }
                (None, None) => {}
            }
        }
        ops
    }
}

/// Text on either side `p` of the way across: the same text moves, at its opacity
/// between the two; changed text cross-fades as it moves; text on one side only fades
/// where it is.
fn text_between(dl: &mut DisplayList, ops: &mut Vec<Op>, x: Option<&Label>, y: Option<&Label>, p: f32) {
    let mut put = |at: Point, alpha: f32, l: &Label| ops.push(layer(None, at, alpha, text_ops(dl, &l.text)));
    match (x, y) {
        (Some(x), Some(y)) if x.text == y.text => put(lerp2(x.origin, y.origin, p), lerp(x.opacity, y.opacity, p), y),
        (Some(x), Some(y)) => {
            put(lerp2(x.origin, y.origin, p), (1.0 - p) * x.opacity, x);
            put(lerp2(x.origin, y.origin, p), p * y.opacity, y);
        }
        (Some(x), None) => put(x.origin, (1.0 - p) * x.opacity, x),
        (None, Some(y)) => put(y.origin, p * y.opacity, y),
        (None, None) => {}
    }
}

/// Mark `i`'s unit when chart `c`'s marks move one after another: its series' place on
/// a line or an area, else its own.
fn unit_of(c: &ChartLayout, i: usize) -> usize {
    let key = &c.marks[i].key;
    c.paths.iter().position(|p| p.marks.contains(key)).unwrap_or(i)
}

/// Each stack re-stacked at this frame, where its members move on different clocks (a
/// stagger, or a member entering on its cue while the rest move with the transition).
/// In the stack's order, each member keeps the extent its own progress gives it and
/// starts where the one before it ends, so the stack never gaps or overlaps. At either
/// end, and wherever its members share one progress, a stack already stands so and is
/// left as it is. A stack whose order differs between the sides, or whose members
/// change stacks (a value changing sign), is left as it is too.
///
/// Returns how far each re-stacked bar's label has counted: a stack prints one label,
/// its total, which counts with the stack, by how much of it stands, not with its top
/// member alone.
fn restack(
    shapes: &mut [(&str, Shape, Color, f32)],
    progress: &[f32],
    a: Option<&ChartLayout>,
    b: Option<&ChartLayout>,
) -> Vec<(usize, f32)> {
    let mut totals = Vec::new();
    fn stack_of<'c>(c: Option<&'c ChartLayout>, key: &str) -> Option<&'c crate::charts::Stack> {
        marks_of(c).iter().find(|m| m.key == key).and_then(|m| m.stack.as_ref())
    }
    let mut stacks: Vec<&str> = Vec::new();
    for m in marks_of(a).iter().chain(marks_of(b)) {
        if let Some(s) = &m.stack
            && !stacks.contains(&s.key.as_str())
        {
            stacks.push(&s.key);
        }
    }
    for stack in stacks {
        let members = |c: Option<&ChartLayout>| -> Vec<String> {
            let on = marks_of(c).iter().filter(|m| m.stack.as_ref().is_some_and(|s| s.key == stack));
            on.map(|m| m.key.clone()).collect()
        };
        let (sa, sb) = (members(a), members(b));
        let order = merged(&sa, &sb);
        let keeps = |side: &[String]| order.iter().filter(|k| side.contains(k)).eq(side.iter());
        let stays = |key: &str| [a, b].iter().all(|&c| stack_of(c, key).is_none_or(|s| s.key == stack));
        if !keeps(&sa) || !keeps(&sb) || !order.iter().all(|k| stays(k)) {
            continue;
        }
        let Some(at): Option<Vec<usize>> =
            order.iter().map(|k| shapes.iter().position(|s| s.0 == k.as_str())).collect()
        else {
            continue;
        };
        if at.iter().all(|&i| progress[i] == progress[at[0]]) {
            continue;
        }
        // How tall the stack stands on either side, and at this frame.
        let extent = |c: Option<&ChartLayout>, key: &str| stack_of(c, key).map_or(0.0, |s| (s.to - s.from).abs());
        let (was, will) = order.iter().fold((0.0, 0.0), |(x, y), k| (x + extent(a, k), y + extent(b, k)));
        let mut now = 0.0;
        let top = *at.last().expect("a stack has members");
        let bars = matches!(shapes[top].1, Shape::Bar(_));
        let mut next: Option<f32> = None;
        for (i, key) in at.iter().copied().zip(&order) {
            // A bar's stack rises from its foot, or for values below zero falls from it.
            let rising = stack_of(b, key).or(stack_of(a, key)).is_some_and(|s| s.to < s.from);
            let shape = shapes[i].1;
            let (from, extent) = match shape {
                Shape::Bar(r) if rising => (r.y + r.h, -r.h),
                Shape::Bar(r) => (r.y, r.h),
                Shape::Span { top, base, .. } => (base, top - base),
                Shape::Arc { start, end, .. } => (start, end - start),
                Shape::Dot { .. } => continue,
            };
            let from = next.unwrap_or(from);
            next = Some(from + extent);
            now += extent.abs();
            shapes[i].1 = match shape {
                Shape::Bar(r) => Shape::Bar(RoundRect { y: from.min(from + extent), h: extent.abs(), ..r }),
                Shape::Span { x, .. } => Shape::Span { x, top: from + extent, base: from },
                Shape::Arc { cx, cy, inner, outer, .. } => {
                    Shape::Arc { cx, cy, inner, outer, start: from, end: from + extent }
                }
                dot => dot,
            };
        }
        if bars {
            let counted = if (will - was).abs() > f32::EPSILON { (now - was) / (will - was) } else { progress[top] };
            totals.extend(at.into_iter().map(|i| (i, counted)));
        }
    }
    totals
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

fn notes_of(c: Option<&ChartLayout>) -> &[Note] {
    c.map_or(&[], |c| &c.notes)
}

/// The plot's ops: in a layer clipped across `span`, the plot's sides (and the room a
/// line's end values take beside them), when something sits beside it (a value-axis
/// gutter, a legend), so a mark or label riding out of the window passes under the
/// plot's edge rather than over them; else as they are.
fn plot_layer(span: Option<[f32; 2]>, clip_y: [f32; 2], ops: Vec<Op>) -> Vec<Op> {
    let Some(span) = span else { return ops };
    vec![Op::Layer {
        node: None,
        cell: None,
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
    // A highlight dims a label on one side, or both.
    let opacity = match (x, y) {
        (Some(x), Some(y)) => lerp(x.opacity, y.opacity, p),
        (Some(l), None) | (None, Some(l)) => l.opacity,
        (None, None) => 1.0,
    };
    if let (Some(x), Some(y)) = (x, y)
        && x.text == y.text
    {
        ops.push(layer(None, at(&y.text, ride(y)), opacity, text_ops(dl, &y.text)));
        return;
    }
    let start = x.map(|l| ride(l).value).or(marks.0.is_none().then_some(0.0));
    let end = y.map(|l| ride(l).value).or(marks.1.is_none().then_some(0.0));
    if let (Some(start), Some(end), Some(numerals), Some(label)) = (start, end, numerals, y.or(x))
        && let count = numerals.count(start, end, p)
        && let Some((runs, width)) = numerals.compose(&count)
    {
        let v = ride(label);
        let [cx, baseline] = v.anchor(shape);
        let alpha = match marks {
            (None, _) => p,
            (_, None) => 1.0 - p,
            _ => 1.0,
        };
        let origin = [cx - v.align * width, baseline - numerals.baseline];
        let starts: Vec<usize> = count.char_indices().map(|(i, _)| i).collect();
        ops.push(layer(None, origin, alpha * opacity, glyph_ops(dl, &count, &starts, &runs)));
        return;
    }
    if let Some(x) = x {
        ops.push(layer(None, at(&x.text, ride(x)), (1.0 - p) * x.opacity, text_ops(dl, &x.text)));
    }
    if let Some(y) = y {
        ops.push(layer(None, at(&y.text, ride(y)), p * y.opacity, text_ops(dl, &y.text)));
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
/// A cue that does not scale its marks keeps them whole: they ride in or out with their
/// neighbors and only fade or move as it says.
fn ends(
    k: Keyed,
    a: Option<&ChartLayout>,
    b: Option<&ChartLayout>,
    enter: Option<MarkLook>,
    exit: Option<MarkLook>,
) -> Option<(Shape, Shape)> {
    let (ma, mb) = (marks_of(a), marks_of(b));
    let dx = k.ride.map_or(0.0, |(ri, rj)| mb[rj].shape.center_x() - ma[ri].shape.center_x());
    let grows = |look: Option<MarkLook>| look.is_none_or(|l| l.grow);
    match k.pair {
        (Some(i), Some(j)) => Shape::lerp(ma[i].shape, mb[j].shape, 0.0).map(|_| (ma[i].shape, mb[j].shape)),
        (None, Some(j)) if grows(enter) => Some((entry(&mb[j], a, b, true, -dx), mb[j].shape)),
        (None, Some(j)) => Some((mb[j].shape.shifted(-dx), mb[j].shape)),
        (Some(i), None) if grows(exit) => Some((ma[i].shape, entry(&ma[i], a, b, false, dx))),
        (Some(i), None) => Some((ma[i].shape, ma[i].shape.shifted(dx))),
        (None, None) => unreachable!("a pair has a side"),
    }
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
        cell: None,
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
        cell: None,
        transform: [1.0, 0.0, 0.0, 1.0, origin[0], origin[1]],
        opacity,
        blend: Blend::Normal,
        clip: None,
        ops,
    }
}

/// A table cell's layer at `origin`, saying where the cell stands in its table.
fn cell_layer(dl: &mut DisplayList, cell: &Cell, origin: Point, opacity: f32) -> Op {
    let Op::Layer { node, transform, opacity, blend, clip, ops, .. } =
        layer(None, origin, opacity, text_ops(dl, &cell.text))
    else {
        unreachable!("`layer` makes layers")
    };
    Op::Layer { node, cell: Some(cell.at), transform, opacity, blend, clip, ops }
}

/// A text node's layer at `origin`, clipped to its box under `fit: clip` (the clip moves
/// with the text).
fn text_layer(dl: &mut DisplayList, id: &str, placed: &PlacedText, origin: Point, opacity: f32) -> Op {
    let Op::Layer { node, cell, transform, opacity, blend, ops, .. } =
        layer(Some(id), origin, opacity, text_ops(dl, &placed.text))
    else {
        unreachable!("`layer` makes layers")
    };
    let clip = placed.clip.map(|[x, y, w, h]| Path::rect([x - placed.origin[0], y - placed.origin[1], w, h]));
    Op::Layer { node, cell, transform, opacity, blend, clip, ops }
}

/// A text's runs as glyph ops, each with the text it sets (SPEC §6).
fn text_ops(dl: &mut DisplayList, text: &TextLayout) -> Vec<Op> {
    glyph_ops(dl, &text.text, &text.cluster_starts(), &text.runs)
}

/// `runs`, glyphs of `text` whose clusters start at `starts` (in order), as glyph ops.
fn glyph_ops(dl: &mut DisplayList, text: &str, starts: &[usize], runs: &[GlyphRun]) -> Vec<Op> {
    runs.iter().map(|run| glyph_op(dl, run, said(text, starts, run))).collect()
}

fn glyph_op(dl: &mut DisplayList, run: &GlyphRun, (text, clusters): (String, Vec<u32>)) -> Op {
    Op::Glyphs {
        font: dl.font(run.font.clone()),
        size: run.size,
        coords: run.coords.clone(),
        paint: Paint::Solid(run.color),
        text,
        glyphs: run.glyphs.clone(),
        clusters,
    }
}

/// What a run of `text` says: the text from its first cluster to the end of its last,
/// which runs to the next of `starts` (every cluster's start in `text`, in order) or to
/// the end; and each glyph's cluster start in that cut. A hyphen drawn at a break says
/// the soft hyphen it stands for.
fn said(text: &str, starts: &[usize], run: &GlyphRun) -> (String, Vec<u32>) {
    let clusters = &run.clusters;
    if run.hyphen {
        return ("\u{AD}".to_string(), vec![0; clusters.len()]);
    }
    let (Some(&lo), Some(&last)) = (clusters.iter().min(), clusters.iter().max()) else {
        return (String::new(), Vec::new());
    };
    let hi = starts.get(starts.partition_point(|&s| s <= last)).copied().unwrap_or(text.len());
    match text.get(lo..hi) {
        Some(cut) => (cut.to_string(), clusters.iter().map(|&c| (c - lo) as u32).collect()),
        None => (String::new(), Vec::new()),
    }
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

/// A legend entry: its swatch filled in `color`, its label at `at`, both at `alpha`,
/// the label at `opacity` too (a highlight dims it).
fn legend_ops(
    dl: &mut DisplayList,
    swatch: RoundRect,
    color: Color,
    label: &Label,
    at: Point,
    alpha: f32,
    opacity: f32,
) -> Vec<Op> {
    // A name at a series' end has no swatch.
    let mut ops: Vec<Op> = match swatch.w > 0.0 && swatch.h > 0.0 {
        true => mark_op(Shape::Bar(swatch), color, alpha).into_iter().collect(),
        false => Vec::new(),
    };
    ops.push(layer(None, at, alpha * opacity, text_ops(dl, &label.text)));
    ops
}

/// An annotation's band: its box filled in `color` at `alpha`.
fn band_op(rect: [f32; 4], color: Color, alpha: f32) -> Op {
    Op::Fill { path: Path::rect(rect), rule: FillRule::NonZero, paint: Paint::Solid(fade(color, alpha)) }
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
    rule_path_op(rule, Path(vec![PathEl::MoveTo(rule.from), PathEl::LineTo(rule.to)]), alpha)
}

/// `rule` with `gaps` left out of it, stretches along its long axis (x across a level
/// rule, y up an upright one); `None` where nothing of it is left.
fn broken_rule_op(rule: &Rule, gaps: &[[f32; 2]], alpha: f32) -> Option<Op> {
    let along = usize::from((rule.to[0] - rule.from[0]).abs() < (rule.to[1] - rule.from[1]).abs());
    let (a, b) = (rule.from[along], rule.to[along]);
    let (lo, hi) = (a.min(b), a.max(b));
    let mut cuts: Vec<[f32; 2]> = gaps.iter().map(|g| [g[0].max(lo), g[1].min(hi)]).filter(|g| g[1] > g[0]).collect();
    if cuts.is_empty() {
        return Some(rule_op(rule, alpha));
    }
    cuts.sort_by(|g, h| g[0].total_cmp(&h[0]));
    let point = |at: f32| lerp2(rule.from, rule.to, (at - a) / (b - a));
    let (mut els, mut at) = (Vec::new(), lo);
    for [start, end] in cuts.into_iter().chain([[hi, hi]]) {
        if start > at {
            els.extend([PathEl::MoveTo(point(at)), PathEl::LineTo(point(start))]);
        }
        at = at.max(end);
    }
    (!els.is_empty()).then(|| rule_path_op(rule, Path(els), alpha))
}

/// The stretches `gaps` leave out of a rule.
fn spans(gaps: &[Gap]) -> Vec<[f32; 2]> {
    gaps.iter().map(|g| g.along).collect()
}

/// The stretches `rule`, `p` of the way across, leaves out between gaps `a` and `b`: the
/// gap for the same text on both sides moves; one on one side only closes on its
/// middle, or opens from it. A gap holds only while the rule crosses where its text
/// stands.
fn lerp_gaps(a: &[Gap], b: &[Gap], p: f32, rule: &Rule) -> Vec<[f32; 2]> {
    let across = usize::from((rule.to[0] - rule.from[0]).abs() >= (rule.to[1] - rule.from[1]).abs());
    let at = rule.from[across];
    let mid = |g: [f32; 2]| [0.5 * (g[0] + g[1]); 2];
    let from = a.iter().map(|g| match b.iter().find(|h| h.key == g.key) {
        Some(h) => (lerp2(g.along, h.along, p), lerp2(g.across, h.across, p)),
        None => (lerp2(g.along, mid(g.along), p), g.across),
    });
    let to =
        (b.iter()).filter(|h| !a.iter().any(|g| g.key == h.key)).map(|h| (lerp2(mid(h.along), h.along, p), h.across));
    from.chain(to).filter(|(_, [lo, hi])| at > *lo && at < *hi).map(|(along, _)| along).collect()
}

fn rule_path_op(rule: &Rule, path: Path, alpha: f32) -> Op {
    Op::Stroke {
        path,
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
        let t = Timing { duration_ms: 400.0, curve: Curve::Ease(CubicBezier(0.2, 0.0, 0.0, 1.0)), matched: true };
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
            base: 100.0,
            baseline: None,
            marks,
            ticks: Vec::new(),
            labels: Vec::new(),
            numerals: None,
            paths,
            y_scale: LinearScale { domain: [0.0, 1.0], range: [100.0, 0.0] },
            plot: [0.0, 0.0, 400.0, 100.0],
            clip: None,
            y_axis: Vec::new(),
            titles: Vec::new(),
            legend: Vec::new(),
            x_grid: Vec::new(),
            notes: Vec::new(),
            collisions: Vec::new(),
            covers: Vec::new(),
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
        at_with(a, b, p, None)
    }

    /// The same, with marks entering by a cue's look.
    fn at_with(
        a: Option<&ChartLayout>,
        b: Option<&ChartLayout>,
        p: f32,
        enter: Option<MarkLook>,
    ) -> Vec<(String, Shape)> {
        let plan = ChartPlan::new(a, b, enter, None);
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
            let plan = ChartPlan::new(Some(&before), Some(&after), None, None);
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

    fn look(grow: bool) -> MarkLook {
        MarkLook { opacity: 0.0, translate: [0.0, 24.0], grow }
    }

    #[test]
    fn a_mark_cue_reads_its_look_and_counts_what_enters_and_leaves() {
        let rise = Motion::Enter(Look { opacity: 0.0, translate: [0.0, 24.0], ..Look::REST });
        assert_eq!(MarkLook::of(&rise), Some(look(false)));
        let grow = Motion::Exit(Look { scale: [1.0, 0.0], anchor: [0.5, 1.0], ..Look::REST });
        assert!(MarkLook::of(&grow).unwrap().grow, "a cue that scales grows its marks from their foot");
        assert_eq!(MarkLook::of(&Motion::Emphasis(Look::REST)), None);
        let bar = |key: &str| {
            let r = RoundRect { x: 10.0, y: 60.0, w: 20.0, h: 40.0, top_radius: 0.0, bottom_radius: 0.0 };
            mark(key, Shape::Bar(r), None)
        };
        let before = chart(ChartKind::Bar, vec![bar("q1"), bar("q2")], Vec::new());
        let after = chart(ChartKind::Bar, vec![bar("q2"), bar("q3"), bar("q4")], Vec::new());
        let plan = ChartPlan::new(Some(&before), Some(&after), None, None);
        assert_eq!((plan.one_sided(true), plan.one_sided(false)), (2, 1));
    }

    #[test]
    fn a_cue_that_does_not_scale_its_marks_keeps_them_whole() {
        let bar = |key: &str, h: f32| {
            let r = RoundRect { x: 10.0, y: 100.0 - h, w: 20.0, h, top_radius: 0.0, bottom_radius: 0.0 };
            mark(key, Shape::Bar(r), None)
        };
        let after = chart(ChartKind::Bar, vec![bar("q1", 40.0)], Vec::new());
        let start = at_with(None, Some(&after), 0.0, Some(look(false)));
        assert_eq!(start[0].1, after.marks[0].shape, "whole from the start, only fading and rising");
        let grown = at_with(None, Some(&after), 0.0, Some(look(true)));
        assert!(matches!(grown[0].1, Shape::Bar(r) if r.h == 0.0), "a scaling cue grows");
    }

    #[test]
    fn the_merged_order_keeps_both_sides_orders() {
        let v = |s: &[&str]| s.iter().map(|k| k.to_string()).collect::<Vec<_>>();
        assert_eq!(merged(&v(&["a", "x", "b"]), &v(&["a", "n", "b"])), v(&["a", "n", "x", "b"]));
        assert_eq!(merged(&v(&["a", "b"]), &v(&["b", "c"])), v(&["a", "b", "c"]));
        assert_eq!(merged(&v(&["a", "b", "z"]), &v(&["n"])), v(&["n", "a", "b", "z"]));
    }

    /// Marks by key, and how far each re-stacked total has counted.
    type Staggered = (Vec<(String, Shape)>, Vec<(usize, f32)>);

    /// Each mark of `b`, entering with a growing cue, at its own progress, re-stacked,
    /// and how far each stack's total has counted.
    fn staggered(b: &ChartLayout, progress: &[f32]) -> Staggered {
        let mut shapes: Vec<(String, Shape)> = progress
            .iter()
            .enumerate()
            .map(|(i, &q)| at_with(None, Some(b), q, Some(look(true))).swap_remove(i))
            .collect();
        let mut sampled: Vec<(&str, Shape, Color, f32)> =
            shapes.iter().map(|(k, s)| (k.as_str(), *s, Color([0, 0, 0, 255]), 1.0)).collect();
        let totals = restack(&mut sampled, progress, None, Some(b));
        let restacked: Vec<Shape> = sampled.iter().map(|s| s.1).collect();
        for (s, r) in shapes.iter_mut().zip(restacked) {
            s.1 = r;
        }
        (shapes, totals)
    }

    #[test]
    fn a_staggered_stack_builds_up_member_on_member() {
        // Three segments enter one after another: the first is in, the second half way,
        // the third not yet begun. Each stands on the one before it as it is now.
        let stack = chart(
            ChartKind::StackedBar,
            vec![segment("a", 10.0, 100.0, 80.0), segment("b", 10.0, 80.0, 70.0), segment("c", 10.0, 70.0, 40.0)],
            Vec::new(),
        );
        let (bars, totals) = staggered(&stack, &[1.0, 0.5, 0.0]);
        let span = |s: Shape| match s {
            Shape::Bar(r) => (r.top(), r.bottom()),
            other => panic!("{other:?}"),
        };
        partition(bars.iter().map(|(_, s)| span(*s)).collect(), 75.0, 100.0);
        assert_eq!(span(bars[1].1), (75.0, 80.0), "half its extent, on the first");
        // Its total counts with the stack: 25 of its 60 stand.
        assert_eq!(totals, [(0, 25.0 / 60.0), (1, 25.0 / 60.0), (2, 25.0 / 60.0)]);
        // Left as they were, the second would open from the foot over the first.
        let unstacked = at_with(None, Some(&stack), 0.5, Some(look(true)));
        assert_eq!(span(unstacked[1].1), (85.0, 90.0));

        // A donut's slices sweep open one after another, each from the last one's end.
        let ring =
            chart(ChartKind::Donut, vec![slice("x", 0.0, 0.5), slice("y", 0.5, 0.8), slice("z", 0.8, 1.0)], vec![]);
        let (slices, totals) = staggered(&ring, &[1.0, 0.5, 0.0]);
        assert!(totals.is_empty(), "each slice prints its own value, on its own clock");
        let turns = |s: Shape| match s {
            Shape::Arc { start, end, .. } => (start, end),
            other => panic!("{other:?}"),
        };
        partition(slices.iter().map(|(_, s)| turns(*s)).collect(), 0.0, 0.65);
        // Members on one clock already stand so, and keep their exact places.
        let (together, _) = staggered(&ring, &[0.5, 0.5, 0.5]);
        let lerped = at_with(None, Some(&ring), 0.5, Some(look(true)));
        assert_eq!(together, lerped);
    }

    #[test]
    fn a_line_or_an_area_staggers_by_series() {
        let point = |key: &str, x: f32| mark(key, Shape::Dot { x, y: 50.0, r: 3.0 }, None);
        let series = |key: &str, marks: &[&str]| SeriesPath {
            key: key.into(),
            color: Color([0, 0, 0, 255]),
            stroke: Some(2.0),
            marks: marks.iter().map(|k| k.to_string()).collect(),
        };
        let lines = chart(
            ChartKind::Line,
            vec![point("a1", 0.0), point("b1", 0.0), point("a2", 50.0), point("b2", 50.0), point("a3", 99.0)],
            vec![series("a", &["a1", "a2", "a3"]), series("b", &["b1", "b2"])],
        );
        let plan = ChartPlan::new(None, Some(&lines), Some(look(true)), None);
        // Two units, one a series: each point moves with the rest of its series.
        assert_eq!(plan.one_sided(true), 2);
        let order: Vec<Option<(usize, usize)>> = plan.order.clone();
        assert_eq!(order, [Some((0, 2)), Some((1, 2)), Some((0, 2)), Some((1, 2)), Some((0, 2))]);
        // Bars stagger one by one.
        let bars = chart(ChartKind::Bar, vec![point("q1", 0.0), point("q2", 50.0)], Vec::new());
        assert_eq!(ChartPlan::new(None, Some(&bars), Some(look(true)), None).one_sided(true), 2);
    }
}
