//! Shape nodes (SPEC §3.3): vector geometry in the node's box, painted from the theme.
//!
//! A shape fills its box. A `rect` is the box, its corners rounded by `radius`; an
//! `ellipse` is inscribed in it. `line`, `arrow`, and `polygon` join `points`, given as
//! fractions of the box: `[0, 0]` is its top-left and `[1, 1]` its bottom-right. A line or
//! arrow with no points crosses the box's middle, left to right. A `path` is SVG path data,
//! scaled uniformly to fit the box and centered in it: its `view` fits, where it gives one, as
//! SVG's `viewBox` does, else its own bounds (PLAN 2.104). The geometry is made for whatever
//! box the shape is given, so a shape that changes size between states morphs: a frame
//! makes it again at the box it has reached, and lays nothing out. One whose outline
//! lines up with the next state's morphs point by point, its paints mixing (PLAN 1.12),
//! and a look can draw its outline on, from where the outline starts.

use crate::EngineError;
use crate::charts::{RoundRect, lerp};
use crate::shadows::Shadow;
use crate::theme::Theme;
use scaena_core::displaylist::{Cap, Color, FillRule, Join, Op, Paint, Path, PathEl, Rect, Stop};
use scaena_core::document::Props;
use serde_json::Value;

/// A shape node, resolved against the theme and placed in its box.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeNode {
    /// The box, canvas units.
    pub rect: Rect,
    geometry: Geometry,
    fill: Option<ShapePaint>,
    stroke: Option<Stroke>,
    /// How much of its outline the stroke draws, from where the outline starts: 1 at
    /// rest, less while a look draws it on (SPEC §3.9).
    progress: f32,
    /// The shadow it casts (PLAN 2.115): a filled rectangle's or ellipse's.
    shadow: Option<Shadow>,
}

/// A fill's or a stroke's paint: a color, or a gradient across the shape's box, made for
/// whatever box the shape has when it is drawn.
#[derive(Debug, Clone, PartialEq)]
enum ShapePaint {
    Solid(Color),
    Gradient {
        kind: GradientKind,
        /// Degrees clockwise from up: a linear gradient's direction, a conic one's start.
        angle: f32,
        /// A radial or conic gradient's center, as fractions of the box.
        center: [f32; 2],
        stops: Vec<Stop>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GradientKind {
    Linear,
    Radial,
    Conic,
}

impl ShapePaint {
    /// Whether this paint can become `other`: two colors, or two gradients of one kind
    /// with as many stops.
    fn morphs_to(&self, other: &ShapePaint) -> bool {
        match (self, other) {
            (ShapePaint::Solid(_), ShapePaint::Solid(_)) => true,
            (ShapePaint::Gradient { kind: a, stops: x, .. }, ShapePaint::Gradient { kind: b, stops: y, .. }) => {
                a == b && x.len() == y.len()
            }
            _ => false,
        }
    }

    /// `a` to `b`, `p` of the way: colors mix in Oklab, and a gradient's angle, center,
    /// and stops move.
    fn lerp(a: &ShapePaint, b: &ShapePaint, p: f32) -> ShapePaint {
        match (a, b) {
            (ShapePaint::Solid(x), ShapePaint::Solid(y)) => ShapePaint::Solid(crate::sample::mix(*x, *y, p)),
            (
                ShapePaint::Gradient { angle: r, center: c, stops: x, .. },
                ShapePaint::Gradient { kind, angle: s, center: d, stops: y },
            ) => ShapePaint::Gradient {
                kind: *kind,
                angle: lerp(*r, *s, p),
                center: [lerp(c[0], d[0], p), lerp(c[1], d[1], p)],
                stops: x.iter().zip(y).map(|(u, v)| Stop(lerp(u.0, v.0, p), crate::sample::mix(u.1, v.1, p))).collect(),
            },
            _ => b.clone(),
        }
    }

    /// The display-list paint over a box `w` × `h` from its corner. A linear gradient
    /// runs through the box's middle along `angle`, as long as the box is that way, as
    /// CSS's does; a radial one runs out from its center to the farthest corner; a conic
    /// one turns about its center from `angle`.
    fn paint(&self, w: f32, h: f32) -> Paint {
        let ShapePaint::Gradient { kind, angle, center, stops } = self else {
            let ShapePaint::Solid(c) = self else { unreachable!() };
            return Paint::Solid(*c);
        };
        let stops = stops.clone();
        let (w, h) = (f64::from(w), f64::from(h));
        let theta = f64::from(*angle).to_radians();
        let [cx, cy] = [f64::from(center[0]) * w, f64::from(center[1]) * h];
        match kind {
            GradientKind::Linear => {
                let (sin, cos) = (libm::sin(theta), libm::cos(theta));
                let half = 0.5 * ((w * sin).abs() + (h * cos).abs());
                let (mx, my) = (0.5 * w, 0.5 * h);
                let start = [(mx - sin * half) as f32, (my + cos * half) as f32];
                let end = [(mx + sin * half) as f32, (my - cos * half) as f32];
                Paint::Linear { start, end, stops }
            }
            GradientKind::Radial => {
                let corner = |x: f64, y: f64| (x - cx) * (x - cx) + (y - cy) * (y - cy);
                let far = corner(0.0, 0.0).max(corner(w, 0.0)).max(corner(0.0, h)).max(corner(w, h));
                Paint::Radial { center: [cx as f32, cy as f32], radius: far.sqrt() as f32, stops }
            }
            // A sweep's angles run from the positive x axis, clockwise with y down.
            GradientKind::Conic => {
                let start = theta - std::f64::consts::FRAC_PI_2;
                let end = start + std::f64::consts::TAU;
                Paint::Sweep { center: [cx as f32, cy as f32], start_angle: start as f32, end_angle: end as f32, stops }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Geometry {
    /// Corner radius, canvas units.
    Rect(f32),
    Ellipse,
    /// Points as fractions of the box; an arrow ends in a head.
    Line {
        points: Vec<[f32; 2]>,
        arrow: bool,
    },
    Polygon(Vec<[f32; 2]>),
    /// As written, with the box of its own coordinates that fits the shape's box when drawn:
    /// its `view`, or its own bounds.
    Path(kurbo::BezPath, kurbo::Rect),
    /// Two paths of one structure on their way from one to the other: each fitted to
    /// the box when drawn by its own frame, then `p` of the way between, point by point.
    Between((kurbo::BezPath, kurbo::Rect), (kurbo::BezPath, kurbo::Rect), f32),
}

#[derive(Debug, Clone, PartialEq)]
struct Stroke {
    paint: ShapePaint,
    width: f32,
    cap: Cap,
    join: Join,
    dash: Vec<f32>,
}

impl Stroke {
    /// `x` to `y`, `p` of the way: the paint, colors mixing in Oklab, the width, and the dashes;
    /// the caps and joins are `y`'s.
    fn lerp(x: &Stroke, y: &Stroke, p: f32) -> Stroke {
        Stroke {
            paint: ShapePaint::lerp(&x.paint, &y.paint, p),
            width: lerp(x.width, y.width, p),
            dash: x.dash.iter().zip(&y.dash).map(|(u, v)| lerp(*u, *v, p)).collect(),
            ..y.clone()
        }
    }
}

impl ShapeNode {
    /// This line or arrow run again along `points`, fractions of `rect` (PLAN 2.109): a
    /// connector as a drag of what it joins draws it.
    pub fn routed(&self, rect: Rect, points: Vec<[f32; 2]>) -> ShapeNode {
        let mut shape = self.clone();
        shape.rect = rect;
        if let Geometry::Line { points: line, .. } = &mut shape.geometry {
            *line = points;
        }
        shape
    }

    pub fn resolve(props: &Props, theme: &Theme, rect: Rect) -> Result<ShapeNode, EngineError> {
        let path = props.get("path").and_then(Value::as_str);
        let kind = props.get("kind").and_then(Value::as_str).unwrap_or(if path.is_some() { "path" } else { "rect" });
        let points = || -> Result<Vec<[f32; 2]>, EngineError> {
            let Some(v) = props.get("points") else { return Ok(Vec::new()) };
            let points: Vec<[f64; 2]> = serde_json::from_value(v.clone()).map_err(|_| {
                EngineError::Layout(format!("`points` {v}: expected [[x, y], …], fractions of the box"))
            })?;
            Ok(points.into_iter().map(|[x, y]| [x as f32, y as f32]).collect())
        };
        let geometry = match kind {
            "rect" => {
                let side = rect[2].min(rect[3]);
                let radius = match props.get("radius") {
                    Some(r) => theme.length(r, side)?,
                    None => 0.0,
                };
                Geometry::Rect(radius.clamp(0.0, side / 2.0))
            }
            "ellipse" => Geometry::Ellipse,
            "line" | "arrow" => {
                let mut points = points()?;
                if points.is_empty() {
                    points = vec![[0.0, 0.5], [1.0, 0.5]];
                }
                if points.len() < 2 {
                    return Err(EngineError::Layout(format!("a {kind} needs two points or more")));
                }
                Geometry::Line { points, arrow: kind == "arrow" }
            }
            "polygon" => {
                let points = points()?;
                if points.len() < 3 {
                    return Err(EngineError::Layout("a polygon needs three points or more".into()));
                }
                Geometry::Polygon(points)
            }
            "path" => {
                let data =
                    path.ok_or_else(|| EngineError::Layout("a path shape needs `path`, SVG path data".into()))?;
                let bez = kurbo::BezPath::from_svg(data)
                    .map_err(|e| EngineError::Layout(format!("`path` is not SVG path data: {e}")))?;
                let frame = match props.get("view") {
                    None => kurbo::Shape::bounding_box(&bez),
                    Some(v) => {
                        let [x, y, w, h] = serde_json::from_value::<[f64; 4]>(v.clone()).map_err(|_| {
                            EngineError::Layout(format!("`view` {v}: expected [x, y, w, h], in the path's own units"))
                        })?;
                        if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite() && x.is_finite() && y.is_finite()) {
                            return Err(EngineError::Layout(format!("`view` {v} has no size")));
                        }
                        kurbo::Rect::new(x, y, x + w, y + h)
                    }
                };
                Geometry::Path(bez, frame)
            }
            other => return Err(EngineError::Layout(format!("unknown shape kind `{other}`"))),
        };
        let fill = match props.get("fill") {
            Some(paint) => Some(shape_paint(theme, paint)?),
            None => None,
        };
        let open = matches!(geometry, Geometry::Line { .. });
        let stroke = match props.get("stroke") {
            Some(s) => Some(stroke(theme, s)?),
            // A line is its stroke: one it does not give is the theme's thin rule.
            None if open => Some(stroke(theme, &Value::Object(Default::default()))?),
            None => None,
        };
        // A filled rectangle or ellipse casts its shadow (PLAN 2.115): a line, an outline, a
        // polygon, and a path cast none, as a box's shadow would misdraw them.
        let shadow = Shadow::of(theme, props.get("shadow"))?;
        let casts = fill.is_some() && matches!(geometry, Geometry::Rect(_) | Geometry::Ellipse);
        let shadow = shadow.filter(|_| casts);
        Ok(ShapeNode { rect, geometry, fill, stroke, progress: 1.0, shadow })
    }

    /// Its kind as it draws, its points as fractions of its box, and a rect's corner radius in
    /// canvas units (PLAN 2.68): what a pointer edits it by. A line or an arrow that gives no
    /// points has the two it draws, across the box's middle.
    pub fn outline(&self) -> (&'static str, &[[f32; 2]], Option<f32>) {
        match &self.geometry {
            Geometry::Rect(radius) => ("rect", &[], Some(*radius)),
            Geometry::Ellipse => ("ellipse", &[], None),
            Geometry::Line { points, arrow } => (if *arrow { "arrow" } else { "line" }, points, None),
            Geometry::Polygon(points) => ("polygon", points, None),
            Geometry::Path(..) | Geometry::Between(..) => ("path", &[], None),
        }
    }

    /// Whether this shape can become `other` point by point (SPEC §3.3): the same kind,
    /// with as many points (a line, an arrow, a polygon) or the same commands in the same
    /// order (a path); a fill and a stroke on both sides or neither, their paints both
    /// colors or both gradients of one kind with as many stops, the strokes' caps, joins,
    /// and dash lengths alike. Anything else cross-fades.
    pub fn morphs_to(&self, other: &ShapeNode) -> bool {
        let geometry = match (&self.geometry, &other.geometry) {
            (Geometry::Rect(_), Geometry::Rect(_)) | (Geometry::Ellipse, Geometry::Ellipse) => true,
            (Geometry::Line { points: a, arrow: x }, Geometry::Line { points: b, arrow: y }) => {
                x == y && a.len() == b.len()
            }
            (Geometry::Polygon(a), Geometry::Polygon(b)) => a.len() == b.len(),
            (Geometry::Path(a, _), Geometry::Path(b, _)) => same_commands(a, b),
            _ => false,
        };
        let fills = match (&self.fill, &other.fill) {
            (Some(a), Some(b)) => a.morphs_to(b),
            (a, b) => a.is_none() && b.is_none(),
        };
        let strokes = match (&self.stroke, &other.stroke) {
            (Some(a), Some(b)) => {
                a.paint.morphs_to(&b.paint) && (a.cap, a.join, a.dash.len()) == (b.cap, b.join, b.dash.len())
            }
            (a, b) => a.is_none() && b.is_none(),
        };
        geometry && fills && strokes
    }

    /// `a` to `b` (which [`ShapeNode::morphs_to`] it): the box, a rect's radius, points,
    /// and a path's points `geo` of the way, which a spring may carry past 1; paints and
    /// stroke widths `p` of the way, colors mixing in Oklab.
    pub fn lerp(a: &ShapeNode, b: &ShapeNode, geo: f32, p: f32) -> ShapeNode {
        let rect = [0, 1, 2, 3].map(|k| lerp(a.rect[k], b.rect[k], geo));
        let points = |x: &[[f32; 2]], y: &[[f32; 2]]| -> Vec<[f32; 2]> {
            x.iter().zip(y).map(|(u, v)| [lerp(u[0], v[0], geo), lerp(u[1], v[1], geo)]).collect()
        };
        let geometry = match (&a.geometry, &b.geometry) {
            // A spring may carry `geo` past 1: a corner never rounds the wrong way.
            (Geometry::Rect(x), Geometry::Rect(y)) => Geometry::Rect(lerp(*x, *y, geo).max(0.0)),
            (Geometry::Line { points: x, .. }, Geometry::Line { points: y, arrow }) => {
                Geometry::Line { points: points(x, y), arrow: *arrow }
            }
            (Geometry::Polygon(x), Geometry::Polygon(y)) => Geometry::Polygon(points(x, y)),
            (Geometry::Path(x, f), Geometry::Path(y, g)) => Geometry::Between((x.clone(), *f), (y.clone(), *g), geo),
            _ => b.geometry.clone(),
        };
        let fill = match (&a.fill, &b.fill) {
            (Some(x), Some(y)) => Some(ShapePaint::lerp(x, y, p)),
            _ => b.fill.clone(),
        };
        let stroke = match (&a.stroke, &b.stroke) {
            (Some(x), Some(y)) => Some(Stroke::lerp(x, y, p)),
            _ => b.stroke.clone(),
        };
        let shadow = Shadow::between(a.shadow, b.shadow, p);
        ShapeNode { rect, geometry, fill, stroke, progress: b.progress, shadow }
    }

    /// The shape with `progress` of its outline drawn: its stroke runs from where its
    /// outline starts that far along its length, an arrow's head riding the tip. Its fill
    /// is whole.
    pub fn drawn(&self, progress: f32) -> ShapeNode {
        ShapeNode { progress: progress.clamp(0.0, 1.0), ..self.clone() }
    }

    /// What the shape draws, in its box's coordinates: its fill, then its stroke.
    pub fn ops(&self) -> Vec<Op> {
        let [_, _, w, h] = self.rect;
        let at = |[x, y]: [f32; 2]| [x * w, y * h];
        let (path, closed) = match &self.geometry {
            Geometry::Rect(r) => (RoundRect::rounded(0.0, 0.0, w, h, *r).path(), true),
            Geometry::Ellipse => {
                let ellipse = kurbo::Ellipse::new(
                    (f64::from(w) / 2.0, f64::from(h) / 2.0),
                    (f64::from(w) / 2.0, f64::from(h) / 2.0),
                    0.0,
                );
                (path_of(&kurbo::Shape::to_path(&ellipse, 0.1)), true)
            }
            Geometry::Line { points, .. } => (polyline(points.iter().copied().map(at), false), false),
            Geometry::Polygon(points) => (polyline(points.iter().copied().map(at), true), true),
            Geometry::Path(bez, frame) => (path_of(&fit(bez, *frame, w, h)), true),
            Geometry::Between((x, f), (y, g), p) => {
                (path_of(&between(&fit(x, *f, w, h), &fit(y, *g, w, h), f64::from(*p))), true)
            }
        };
        let mut ops = Vec::new();
        // Its shadow under it (PLAN 2.115), its outline the shape's: a box rounded as it is, or
        // an ellipse.
        if let Some(shadow) = &self.shadow {
            ops.push(match &self.geometry {
                Geometry::Rect(r) => shadow.op([0.0, 0.0, w, h], *r),
                _ => shadow.oval([0.0, 0.0, w, h]),
            });
        }
        if let (Some(fill), true) = (&self.fill, closed) {
            ops.push(Op::Fill { path: path.clone(), rule: FillRule::NonZero, paint: fill.paint(w, h) });
        }
        if let Some(s) = &self.stroke {
            let full = (self.progress < 1.0).then(|| length(&path));
            let mut outline = if self.progress < 1.0 { trimmed(&path, self.progress) } else { path };
            // An arrow's head sits at the tip of what is drawn, growing in over its own
            // length; its shaft stops where the head starts, so no cap shows past the point.
            let mut tip = None;
            if let (Geometry::Line { arrow: true, .. }, [.., a, b]) = (&self.geometry, outline.0.as_slice()) {
                let (from, to) = (end(a), end(b));
                let scale = full.map_or(1.0, |full| (length(&outline) / head(s).0.min(full)).min(1.0));
                if let Some(PathEl::LineTo(end)) = outline.0.last_mut() {
                    *end = head_base(from, to, s, scale);
                }
                tip = Some(arrowhead(from, to, s, scale));
            }
            ops.push(Op::Stroke {
                path: outline,
                paint: s.paint.paint(w, h),
                width: s.width,
                cap: s.cap,
                join: s.join,
                miter_limit: 4.0,
                dash: s.dash.clone(),
                dash_offset: 0.0,
            });
            if let Some(head) = tip {
                ops.push(Op::Fill { path: head, rule: FillRule::NonZero, paint: s.paint.paint(w, h) });
            }
        }
        ops
    }
}

/// An arrowhead's length along the shaft, and its half width, for a stroke.
fn head(s: &Stroke) -> (f32, f32) {
    (s.width * 4.0, s.width * 2.5)
}

/// The unit vector from `from` to `to`.
fn direction(from: [f32; 2], to: [f32; 2]) -> (f32, f32) {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let len = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
    (dx / len, dy / len)
}

/// Where the head on the segment `from`–`to` starts, the head `scale` of its size.
fn head_base(from: [f32; 2], to: [f32; 2], s: &Stroke, scale: f32) -> [f32; 2] {
    let (ux, uy) = direction(from, to);
    let long = head(s).0 * scale;
    [to[0] - ux * long, to[1] - uy * long]
}

/// A head's outline at `to`, pointing from `from`, sized by the stroke and `scale`; it
/// fills in the stroke's paint.
fn arrowhead(from: [f32; 2], to: [f32; 2], s: &Stroke, scale: f32) -> Path {
    let (ux, uy) = direction(from, to);
    let half = head(s).1 * scale;
    let base = head_base(from, to, s, scale);
    Path(vec![
        PathEl::MoveTo(to),
        PathEl::LineTo([base[0] - uy * half, base[1] + ux * half]),
        PathEl::LineTo([base[0] + uy * half, base[1] - ux * half]),
        PathEl::Close,
    ])
}

pub(crate) fn polyline(points: impl Iterator<Item = [f32; 2]>, closed: bool) -> Path {
    let mut els: Vec<PathEl> =
        points.enumerate().map(|(i, p)| if i == 0 { PathEl::MoveTo(p) } else { PathEl::LineTo(p) }).collect();
    if closed {
        els.push(PathEl::Close);
    }
    Path(els)
}

/// Whether two paths have the same commands in the same order.
fn same_commands(a: &kurbo::BezPath, b: &kurbo::BezPath) -> bool {
    use kurbo::PathEl as E;
    a.elements().len() == b.elements().len()
        && a.elements().iter().zip(b.elements()).all(|pair| {
            matches!(
                pair,
                (E::MoveTo(_), E::MoveTo(_))
                    | (E::LineTo(_), E::LineTo(_))
                    | (E::QuadTo(..), E::QuadTo(..))
                    | (E::CurveTo(..), E::CurveTo(..))
                    | (E::ClosePath, E::ClosePath)
            )
        })
}

/// Two paths of the same commands, `p` of the way from `a` to `b`, point by point.
fn between(a: &kurbo::BezPath, b: &kurbo::BezPath, p: f64) -> kurbo::BezPath {
    use kurbo::PathEl as E;
    let at = |u: kurbo::Point, v: kurbo::Point| u.lerp(v, p);
    let els = a.elements().iter().zip(b.elements()).map(|(x, y)| match (*x, *y) {
        (E::MoveTo(u), E::MoveTo(v)) => E::MoveTo(at(u, v)),
        (E::LineTo(u), E::LineTo(v)) => E::LineTo(at(u, v)),
        (E::QuadTo(u1, u2), E::QuadTo(v1, v2)) => E::QuadTo(at(u1, v1), at(u2, v2)),
        (E::CurveTo(u1, u2, u3), E::CurveTo(v1, v2, v3)) => E::CurveTo(at(u1, v1), at(u2, v2), at(u3, v3)),
        (_, el) => el,
    });
    kurbo::BezPath::from_vec(els.collect())
}

/// `bez` scaled uniformly so that `frame`, a box of its own coordinates, fits `w` × `h`, and
/// centered.
fn fit(bez: &kurbo::BezPath, frame: kurbo::Rect, w: f32, h: f32) -> kurbo::BezPath {
    let b = frame;
    let (bw, bh) = (b.width(), b.height());
    let scale = match (bw > 0.0, bh > 0.0) {
        (true, true) => (f64::from(w) / bw).min(f64::from(h) / bh),
        (true, false) => f64::from(w) / bw,
        (false, true) => f64::from(h) / bh,
        (false, false) => 1.0,
    };
    let dx = (f64::from(w) - bw * scale) / 2.0 - b.x0 * scale;
    let dy = (f64::from(h) - bh * scale) / 2.0 - b.y0 * scale;
    kurbo::Affine::new([scale, 0.0, 0.0, scale, dx, dy]) * bez.clone()
}

/// Where a path element ends.
fn end(el: &PathEl) -> [f32; 2] {
    match *el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) | PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => p,
        PathEl::Close => [f32::NAN; 2],
    }
}

/// Each segment of `path` and the length along it at each of its pieces' ends: a line
/// is one piece, a curve 32. Lengths take square roots alone, which every target
/// rounds alike (SPEC §13).
fn measured(path: &Path) -> Vec<(kurbo::PathSeg, Vec<f64>)> {
    use kurbo::ParamCurve;
    let p = |q: [f32; 2]| kurbo::Point::new(f64::from(q[0]), f64::from(q[1]));
    let bez = kurbo::BezPath::from_vec(
        (path.0.iter())
            .map(|el| match *el {
                PathEl::MoveTo(a) => kurbo::PathEl::MoveTo(p(a)),
                PathEl::LineTo(a) => kurbo::PathEl::LineTo(p(a)),
                PathEl::QuadTo(a, b) => kurbo::PathEl::QuadTo(p(a), p(b)),
                PathEl::CurveTo(a, b, c) => kurbo::PathEl::CurveTo(p(a), p(b), p(c)),
                PathEl::Close => kurbo::PathEl::ClosePath,
            })
            .collect(),
    );
    (bez.segments())
        .map(|seg| {
            let pieces = if matches!(seg, kurbo::PathSeg::Line(_)) { 1 } else { 32 };
            let mut along = vec![0.0];
            let mut last = seg.start();
            for i in 1..=pieces {
                let next = seg.eval(f64::from(i) / f64::from(pieces));
                let (dx, dy) = (next.x - last.x, next.y - last.y);
                along.push(along[along.len() - 1] + (dx * dx + dy * dy).sqrt());
                last = next;
            }
            (seg, along)
        })
        .collect()
}

/// How long `path` is.
fn length(path: &Path) -> f32 {
    measured(path).iter().map(|(_, along)| along[along.len() - 1]).sum::<f64>() as f32
}

/// The first `progress` of `path` by length, from where it starts, open.
fn trimmed(path: &Path, progress: f32) -> Path {
    use kurbo::ParamCurve;
    let segments = measured(path);
    let mut left = segments.iter().map(|(_, along)| along[along.len() - 1]).sum::<f64>() * f64::from(progress);
    let (mut out, mut at) = (kurbo::BezPath::new(), None);
    for (seg, along) in &segments {
        if left <= 0.0 {
            break;
        }
        let len = along[along.len() - 1];
        let seg = if len <= left {
            *seg
        } else {
            // Within the piece the cut falls in, by length along it.
            let pieces = along.len() - 1;
            let i = along.iter().rposition(|&a| a <= left).unwrap_or(0).min(pieces - 1);
            let within = (left - along[i]) / (along[i + 1] - along[i]).max(f64::MIN_POSITIVE);
            seg.subsegment(0.0..(i as f64 + within) / pieces as f64)
        };
        if at != Some(seg.start()) {
            out.move_to(seg.start());
        }
        out.push(seg.as_path_el());
        at = Some(seg.end());
        left -= len;
    }
    path_of(&out)
}

/// A kurbo path in the display list's terms.
pub(crate) fn path_of(bez: &kurbo::BezPath) -> Path {
    let p = |q: kurbo::Point| [q.x as f32, q.y as f32];
    Path(
        bez.elements()
            .iter()
            .map(|el| match *el {
                kurbo::PathEl::MoveTo(a) => PathEl::MoveTo(p(a)),
                kurbo::PathEl::LineTo(a) => PathEl::LineTo(p(a)),
                kurbo::PathEl::QuadTo(a, b) => PathEl::QuadTo(p(a), p(b)),
                kurbo::PathEl::CurveTo(a, b, c) => PathEl::CurveTo(p(a), p(b), p(c)),
                kurbo::PathEl::ClosePath => PathEl::Close,
            })
            .collect(),
    )
}

/// A paint (SPEC §3.3): a color, `{ "solid": color }`, or `{ "gradient": { kind, angle,
/// center, stops } }`, its colors the theme's.
fn shape_paint(theme: &Theme, paint: &Value) -> Result<ShapePaint, EngineError> {
    match paint {
        Value::String(c) => Ok(ShapePaint::Solid(theme.color(c)?)),
        Value::Object(o) if o.contains_key("solid") => {
            let c = o["solid"].as_str().ok_or_else(|| EngineError::Layout("`solid` is a color".into()))?;
            Ok(ShapePaint::Solid(theme.color(c)?))
        }
        Value::Object(o) if o.contains_key("gradient") => {
            let g = &o["gradient"];
            let kind = match g.get("kind").and_then(Value::as_str) {
                Some("linear") => GradientKind::Linear,
                Some("radial") => GradientKind::Radial,
                Some("conic") => GradientKind::Conic,
                other => {
                    return Err(EngineError::Layout(format!(
                        "gradient kind {other:?}: expected linear, radial, or conic"
                    )));
                }
            };
            let default_angle = if kind == GradientKind::Linear { 180.0 } else { 0.0 };
            let angle = g.get("angle").and_then(Value::as_f64).unwrap_or(default_angle) as f32;
            let center = match g.get("center") {
                None => [0.5, 0.5],
                Some(c) => serde_json::from_value::<[f32; 2]>(c.clone()).map_err(|_| {
                    EngineError::Layout(format!("gradient center {c}: expected [x, y], fractions of the box"))
                })?,
            };
            let mut stops = Vec::new();
            for stop in g.get("stops").and_then(Value::as_array).into_iter().flatten() {
                let at = stop.get("at").and_then(Value::as_f64).filter(|a| (0.0..=1.0).contains(a));
                let color = stop.get("color").and_then(Value::as_str);
                let (Some(at), Some(color)) = (at, color) else {
                    return Err(EngineError::Layout(format!("gradient stop {stop}: expected {{ at: 0–1, color }}")));
                };
                stops.push(Stop(at as f32, theme.color(color)?));
            }
            if stops.len() < 2 {
                return Err(EngineError::Layout("a gradient needs two stops or more".into()));
            }
            Ok(ShapePaint::Gradient { kind, angle, center, stops })
        }
        other => Err(EngineError::Layout(format!("{other} is not a paint: a color, {{solid}}, or {{gradient}}"))),
    }
}

/// A stroke: its paint (the theme's `onSurface` when it names none), its width (the
/// theme's `thin` stroke, else 2), and its cap, join, and dashes.
fn stroke(theme: &Theme, s: &Value) -> Result<Stroke, EngineError> {
    let paint = match s.get("paint") {
        Some(paint) => shape_paint(theme, paint)?,
        None => ShapePaint::Solid(theme.color("onSurface")?),
    };
    let width = match s.get("width") {
        Some(w) => theme.length(w, 0.0)?,
        None => theme.stroke("thin").unwrap_or(2.0),
    };
    let cap = match s.get("cap").and_then(Value::as_str) {
        None | Some("butt") => Cap::Butt,
        Some("round") => Cap::Round,
        Some("square") => Cap::Square,
        Some(other) => return Err(EngineError::Layout(format!("stroke cap `{other}`"))),
    };
    let join = match s.get("join").and_then(Value::as_str) {
        None | Some("miter") => Join::Miter,
        Some("round") => Join::Round,
        Some("bevel") => Join::Bevel,
        Some(other) => return Err(EngineError::Layout(format!("stroke join `{other}`"))),
    };
    let dash = match s.get("dash") {
        Some(d) => serde_json::from_value::<Vec<f32>>(d.clone())
            .map_err(|_| EngineError::Layout(format!("stroke dash {d}: expected lengths")))?,
        None => Vec::new(),
    };
    Ok(Stroke { paint, width, cap, join, dash })
}

/// A stroke resolved as a shape's is, for what strokes an outline of its own: an image's
/// border (PLAN 2.118).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Border(Stroke);

impl Border {
    pub(crate) fn of(theme: &Theme, s: &Value) -> Result<Border, EngineError> {
        stroke(theme, s).map(Border)
    }

    /// `a` to `b`, `p` of the way, as a shape's stroke moves: its paint and its width.
    pub(crate) fn lerp(a: &Border, b: &Border, p: f32) -> Border {
        Border(Stroke::lerp(&a.0, &b.0, p))
    }

    /// `path`, in a box `w` × `h` from its origin, stroked: a gradient spans the box.
    pub(crate) fn op(&self, path: Path, w: f32, h: f32) -> Op {
        let s = &self.0;
        Op::Stroke {
            path,
            paint: s.paint.paint(w, h),
            width: s.width,
            cap: s.cap,
            join: s.join,
            miter_limit: 4.0,
            dash: s.dash.clone(),
            dash_offset: 0.0,
        }
    }
}

/// What a connector joins and how it runs (PLAN 2.109): the scene keeps it, so a drag of either
/// end runs the connector again from where that end stands, nothing laid out.
#[derive(Debug, Clone, PartialEq)]
pub struct Joint {
    pub from: String,
    pub to: String,
    /// It leaves `from` by the side that faces `to` and turns at right angles.
    pub elbow: bool,
    /// How far each end stands off its box, canvas units: the theme's `space.2`, so an
    /// arrowhead points at a box rather than into it.
    pub gap: f32,
}

impl Joint {
    /// The joint a node's `props` make, if they are a connector's.
    pub fn of(props: &Props, theme: &Theme) -> Option<Joint> {
        let connect = props.get("connect")?;
        let end = |key: &str| connect.get(key).and_then(Value::as_str).map(str::to_string);
        Some(Joint {
            from: end("from")?,
            to: end("to")?,
            elbow: connect.get("route").and_then(Value::as_str) == Some("elbow"),
            gap: theme.tokens.space.scale.get(2).map_or(theme.tokens.space.unit as f32, |s| s.0 as f32),
        })
    }

    /// Its route between the boxes `from` and `to`, canvas units: its box, and its points as
    /// fractions of that box. A straight one runs along the line between the boxes' middles,
    /// from where it leaves `from` to where it meets `to`; an elbow leaves `from` by the side
    /// that faces `to`, turns twice at right angles, and meets `to`'s facing side: down, or up,
    /// where one box is above the other, as an organization chart's lines run, even where it
    /// stands to one side too, and across where they stand side by side (PLAN 2.110). Boxes that
    /// overlap are joined middle to middle, and an elbow between boxes too close for it to leave
    /// one and meet the other runs straight.
    pub fn route(&self, from: Rect, to: Rect) -> (Rect, Vec<[f32; 2]>) {
        let gap = self.gap;
        let middle = |r: Rect| [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0];
        let (a, b) = (middle(from), middle(to));
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let above = to[1] >= from[1] + from[3] || to[1] + to[3] <= from[1];
        let beside = to[0] >= from[0] + from[2] || to[0] + to[2] <= from[0];
        let across = if above || beside { !above } else { dx.abs() >= dy.abs() };
        let elbow = if !self.elbow {
            None
        } else if across {
            // Across: out of the side that faces the other box, a turn halfway, and in.
            let s = if dx < 0.0 { -1.0 } else { 1.0 };
            let (x0, x1) = (a[0] + s * (from[2] / 2.0 + gap), b[0] - s * (to[2] / 2.0 + gap));
            let xm = (x0 + x1) / 2.0;
            (s * (x1 - x0) > 0.0).then(|| vec![[x0, a[1]], [xm, a[1]], [xm, b[1]], [x1, b[1]]])
        } else {
            // Down, or up, the same way.
            let s = if dy < 0.0 { -1.0 } else { 1.0 };
            let (y0, y1) = (a[1] + s * (from[3] / 2.0 + gap), b[1] - s * (to[3] / 2.0 + gap));
            let ym = (y0 + y1) / 2.0;
            (s * (y1 - y0) > 0.0).then(|| vec![[a[0], y0], [a[0], ym], [b[0], ym], [b[0], y1]])
        };
        let points = if let Some(elbow) = elbow {
            elbow
        } else {
            let length = dx.hypot(dy);
            // How far along the line from a box's middle its edge is.
            let edge = |r: Rect, u: [f32; 2]| {
                let across = if u[0].abs() > 1e-6 { r[2] / 2.0 / u[0].abs() } else { f32::INFINITY };
                let down = if u[1].abs() > 1e-6 { r[3] / 2.0 / u[1].abs() } else { f32::INFINITY };
                across.min(down)
            };
            let u = if length > 1e-3 { [dx / length, dy / length] } else { [1.0, 0.0] };
            let (ta, tb) = (edge(from, u) + gap, edge(to, u) + gap);
            if length <= 1e-3 || ta + tb >= length {
                vec![a, b]
            } else {
                vec![[a[0] + u[0] * ta, a[1] + u[1] * ta], [b[0] - u[0] * tb, b[1] - u[1] * tb]]
            }
        };
        // Its box: the points' bounds, a unit across at the least, so each fraction is a number.
        let (lo, hi) = points.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), p| {
            ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])
        });
        let (w, h) = ((hi[0] - lo[0]).max(1.0), (hi[1] - lo[1]).max(1.0));
        let (x, y) = ((lo[0] + hi[0] - w) / 2.0, (lo[1] + hi[1] - h) / 2.0);
        let fractions = points.iter().map(|p| [(p[0] - x) / w, (p[1] - y) / h]).collect();
        ([x, y, w, h], fractions)
    }
}

/// The room a shape gives the words it holds (PLAN 2.108), canvas units: a rect's box less what
/// its rounded corners take, an ellipse's inscribed rectangle, or the largest rectangle that stays
/// inside a polygon, or inside a path as it is drawn in its box; then inset on every side by the
/// theme's `space.4`, or a twelfth of what is left's shorter side where that is less.
pub fn words_box(props: &Props, theme: &Theme, rect: Rect) -> Rect {
    let path = props.get("path").and_then(Value::as_str);
    let kind = props.get("kind").and_then(Value::as_str).unwrap_or(if path.is_some() { "path" } else { "rect" });
    let [x, y, w, h] = rect;
    let [fx, fy, fw, fh] = match kind {
        "ellipse" => {
            let m = (1.0 - std::f32::consts::FRAC_1_SQRT_2) / 2.0;
            [m, m, 1.0 - 2.0 * m, 1.0 - 2.0 * m]
        }
        "polygon" => {
            let points: Option<Vec<[f32; 2]>> =
                props.get("points").and_then(|v| serde_json::from_value(v.clone()).ok());
            match points {
                Some(points) if points.len() >= 3 && w > 0.0 && h > 0.0 => inscribed(&[points], w / h),
                _ => [0.0, 0.0, 1.0, 1.0],
            }
        }
        "path" => match path.and_then(|d| kurbo::BezPath::from_svg(d).ok()) {
            Some(bez) if w > 0.0 && h > 0.0 => {
                let view = props.get("view").and_then(|v| serde_json::from_value::<[f64; 4]>(v.clone()).ok());
                let frame = match view {
                    Some([x, y, w, h]) if w > 0.0 && h > 0.0 => kurbo::Rect::new(x, y, x + w, y + h),
                    _ => kurbo::Shape::bounding_box(&bez),
                };
                let rings = rings(&fit(&bez, frame, w, h), w, h);
                if rings.is_empty() { [0.0, 0.0, 1.0, 1.0] } else { inscribed(&rings, w / h) }
            }
            _ => [0.0, 0.0, 1.0, 1.0],
        },
        _ => [0.0, 0.0, 1.0, 1.0],
    };
    let (mut ix, mut iy, mut iw, mut ih) = (x + fx * w, y + fy * h, fw * w, fh * h);
    if kind == "rect" {
        let side = w.min(h);
        let radius = props.get("radius").and_then(|r| theme.length(r, side).ok()).unwrap_or(0.0).clamp(0.0, side / 2.0);
        let corner = radius * (1.0 - std::f32::consts::FRAC_1_SQRT_2);
        (ix, iy, iw, ih) = (ix + corner, iy + corner, iw - 2.0 * corner, ih - 2.0 * corner);
    }
    let space = theme.tokens.space.scale.get(4).map_or(3.0 * theme.tokens.space.unit as f32, |s| s.0 as f32);
    let pad = space.min(iw.min(ih).max(0.0) / 12.0);
    [ix + pad, iy + pad, (iw - 2.0 * pad).max(0.0), (ih - 2.0 * pad).max(0.0)]
}

/// The closed outlines a path drawn `w` × `h` makes, each a ring of points as fractions of its
/// box: its curves flattened to within half a canvas unit, a ring of fewer than three points left
/// out.
fn rings(bez: &kurbo::BezPath, w: f32, h: f32) -> Vec<Vec<[f32; 2]>> {
    let mut rings: Vec<Vec<[f32; 2]>> = Vec::new();
    let at = |p: kurbo::Point| [p.x as f32 / w, p.y as f32 / h];
    kurbo::flatten(bez.iter(), 0.5, |el| match el {
        kurbo::PathEl::MoveTo(p) => rings.push(vec![at(p)]),
        kurbo::PathEl::LineTo(p) => match rings.last_mut() {
            Some(ring) => ring.push(at(p)),
            None => rings.push(vec![at(p)]),
        },
        _ => {}
    });
    rings.retain(|ring| ring.len() >= 3);
    rings
}

/// The largest rectangle inside the outlines `rings` (fractions of a box `aspect` times as wide
/// as it is tall), each a polygon, that crosses none of them, as `[x, y, w, h]` in fractions of
/// the box: of rectangles of a few proportions about points across them, the one of most area,
/// each as large as fits by bisection, then grown side by side as far as each side goes.
fn inscribed(rings: &[Vec<[f32; 2]>], aspect: f32) -> [f32; 4] {
    // Measured where the box is as wide as it is: a unit of x is `aspect` units of y.
    let poly: Vec<Vec<[f32; 2]>> =
        rings.iter().map(|ring| ring.iter().map(|p| [p[0] * aspect, p[1]]).collect()).collect();
    let (lo, hi) = poly.iter().flatten().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])
    });
    let (bw, bh) = (hi[0] - lo[0], hi[1] - lo[1]);
    const AROUND: usize = 9;
    const PROPORTIONS: [f32; 9] = [0.25, 0.4, 0.6, 0.8, 1.0, 1.25, 1.6, 2.5, 4.0];
    let mut best = ([0.0f32; 4], 0.0f32);
    for i in 1..AROUND {
        for j in 1..AROUND {
            let c = [lo[0] + bw * i as f32 / AROUND as f32, lo[1] + bh * j as f32 / AROUND as f32];
            if !inside(c, &poly) {
                continue;
            }
            for r in PROPORTIONS {
                // Half its height: bisected between none and the box's.
                let (mut fits_at, mut miss) = (0.0f32, bh.max(bw / r));
                for _ in 0..16 {
                    let s = 0.5 * (fits_at + miss);
                    if holds(&poly, [c[0] - r * s, c[1] - s, c[0] + r * s, c[1] + s]) {
                        fits_at = s;
                    } else {
                        miss = s;
                    }
                }
                let area = 4.0 * r * fits_at * fits_at;
                if area > best.1 {
                    best = ([c[0] - r * fits_at, c[1] - fits_at, c[0] + r * fits_at, c[1] + fits_at], area);
                }
            }
        }
    }
    if best.1 <= 0.0 {
        return [0.0, 0.0, 1.0, 1.0];
    }
    // Then each side moved out as far as it goes, twice round: the search keeps to a few middles
    // and proportions, and a polygon's room is seldom one of them.
    let mut r = best.0;
    let bounds = [lo[0], lo[1], hi[0], hi[1]];
    for _ in 0..2 {
        for side in 0..4 {
            let (mut fits_to, mut miss) = (r[side], bounds[side]);
            for _ in 0..16 {
                let mid = 0.5 * (fits_to + miss);
                let mut tried = r;
                tried[side] = mid;
                if holds(&poly, tried) {
                    fits_to = mid;
                } else {
                    miss = mid;
                }
            }
            r[side] = fits_to;
        }
    }
    let [x0, y0, x1, y1] = r;
    [x0 / aspect, y0, (x1 - x0) / aspect, y1 - y0]
}

/// Each edge of the outlines `poly`, each ring closed.
fn edges(poly: &[Vec<[f32; 2]>]) -> impl Iterator<Item = ([f32; 2], [f32; 2])> + '_ {
    poly.iter().flat_map(|ring| ring.iter().enumerate().map(|(i, &a)| (a, ring[(i + 1) % ring.len()])))
}

/// Whether `p` is inside the outlines `poly`: inside one of them, each by the even-odd rule. An
/// outline inside another makes no hole in the room, so the room of an icon drawn in outline is
/// inside its line, not in the band of the line itself.
fn inside(p: [f32; 2], poly: &[Vec<[f32; 2]>]) -> bool {
    poly.iter().any(|ring| {
        let mut odd = false;
        for (a, b) in edges(std::slice::from_ref(ring)) {
            if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]) {
                odd = !odd;
            }
        }
        odd
    })
}

/// Whether the rectangle `[x0, y0, x1, y1]` lies inside the outlines `poly`: its corners inside,
/// none of their corners within it, and no edge of theirs crossing it.
fn holds(poly: &[Vec<[f32; 2]>], [x0, y0, x1, y1]: [f32; 4]) -> bool {
    let e = 1e-4 * (x1 - x0).max(y1 - y0);
    let corners = [[x0 + e, y0 + e], [x1 - e, y0 + e], [x1 - e, y1 - e], [x0 + e, y1 - e]];
    if !corners.iter().all(|&c| inside(c, poly)) {
        return false;
    }
    if poly.iter().flatten().any(|p| p[0] > x0 + e && p[0] < x1 - e && p[1] > y0 + e && p[1] < y1 - e) {
        return false;
    }
    let sides =
        [(corners[0], corners[1]), (corners[1], corners[2]), (corners[2], corners[3]), (corners[3], corners[0])];
    !edges(poly).any(|(a, b)| sides.iter().any(|&(c, d)| crosses(a, b, c, d)))
}

/// Whether segments `ab` and `cd` cross, each strictly between its ends.
fn crosses(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> bool {
    let side = |p: [f32; 2], q: [f32; 2], r: [f32; 2]| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn theme() -> Theme {
        Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap()
    }

    fn shape(v: Value, rect: Rect) -> Result<ShapeNode, EngineError> {
        ShapeNode::resolve(&serde_json::from_value(v).unwrap(), &theme(), rect)
    }

    #[test]
    fn a_rect_fills_its_box_with_theme_paints() {
        let s = shape(json!({"kind": "rect", "fill": "accent", "radius": 8}), [10.0, 20.0, 200.0, 100.0]).unwrap();
        let ops = s.ops();
        let Op::Fill { path, paint, .. } = &ops[0] else { panic!("{ops:?}") };
        assert_eq!(*paint, Paint::Solid(theme().color("accent").unwrap()));
        assert_eq!(path.0.first(), Some(&PathEl::MoveTo([8.0, 0.0])), "box-local, corner rounded");
        assert_eq!(ops.len(), 1, "no stroke unless it says");
        // A radius past half the short side is a pill.
        let pill = shape(json!({"kind": "rect", "fill": "accent", "radius": 500}), [0.0, 0.0, 200.0, 40.0]).unwrap();
        assert_eq!(pill.geometry, Geometry::Rect(20.0));
    }

    /// Shadows (PLAN 2.115): a filled rectangle casts its shadow under it, rounded as it is, and an
    /// ellipse as a circle is; an outline, a line, and a polygon cast none; a morph moves it.
    #[test]
    fn a_filled_box_casts_its_shadow_under_it() {
        let card =
            shape(json!({"kind": "rect", "fill": "accent", "radius": 8, "shadow": "soft"}), [10.0, 20.0, 200.0, 100.0]);
        let ops = card.unwrap().ops();
        let Op::Shadow { rect, radius, blur, color } = &ops[0] else {
            panic!("the shadow first, under the fill: {ops:?}")
        };
        assert_eq!((*rect, *radius), ([0.0, 6.0, 200.0, 100.0], 8.0), "box-local, fallen as the theme says");
        assert!(*blur > 0.0 && color.0[3] > 0 && color.0[3] < 255, "{blur} {color:?}");
        assert!(matches!(ops[1], Op::Fill { .. }));
        let dot =
            shape(json!({"kind": "ellipse", "fill": "accent", "shadow": "soft"}), [0.0, 0.0, 40.0, 40.0]).unwrap();
        assert!(matches!(dot.ops()[0], Op::Shadow { radius: 20.0, .. }), "a circle's shadow is round");
        let oval =
            shape(json!({"kind": "ellipse", "fill": "accent", "shadow": "soft"}), [0.0, 0.0, 80.0, 40.0]).unwrap();
        assert!(
            matches!(oval.ops()[0], Op::Layer { transform: [2.0, 0.0, 0.0, 1.0, 0.0, 0.0], .. }),
            "an oval's is too"
        );
        for none in [
            json!({"kind": "rect", "stroke": {"paint": "accent"}, "shadow": "soft"}),
            json!({"kind": "line", "shadow": "soft"}),
            json!({"kind": "polygon", "points": [[0, 1], [0.5, 0], [1, 1]], "fill": "accent", "shadow": "soft"}),
        ] {
            let ops = shape(none.clone(), [0.0, 0.0, 40.0, 40.0]).unwrap().ops();
            assert!(!ops.iter().any(|op| matches!(op, Op::Shadow { .. })), "{none}");
        }
        // A morph from no shadow to one fades it in where it falls.
        let bare = shape(json!({"kind": "rect", "fill": "accent"}), [0.0, 0.0, 40.0, 40.0]).unwrap();
        let lifted =
            shape(json!({"kind": "rect", "fill": "accent", "shadow": "raised"}), [0.0, 0.0, 40.0, 40.0]).unwrap();
        let half = ShapeNode::lerp(&bare, &lifted, 0.5, 0.5).ops();
        let (Op::Shadow { color: c, .. }, Op::Shadow { color: full, .. }) = (&half[0], &lifted.ops()[0]) else {
            panic!("{half:?}")
        };
        assert!(c.0[3] < full.0[3] && c.0[3] > 0, "half faded in: {c:?} of {full:?}");
        // A name the theme lacks says which it has.
        let err = shape(json!({"kind": "rect", "fill": "accent", "shadow": "dreamy"}), [0.0, 0.0, 9.0, 9.0]);
        assert!(err.unwrap_err().to_string().contains("soft, raised, floating"));
    }

    #[test]
    fn lines_and_arrows_are_strokes_across_the_box() {
        let line = shape(json!({"kind": "line"}), [0.0, 0.0, 400.0, 10.0]).unwrap();
        let ops = line.ops();
        let Op::Stroke { path, paint, width, .. } = &ops[0] else { panic!("{ops:?}") };
        assert_eq!(path.0, vec![PathEl::MoveTo([0.0, 5.0]), PathEl::LineTo([400.0, 5.0])]);
        assert_eq!(
            (paint.clone(), *width),
            (Paint::Solid(theme().color("onSurface").unwrap()), theme().stroke("thin").unwrap())
        );
        let arrow = shape(
            json!({"kind": "arrow", "points": [[0, 0], [1, 1]], "stroke": {"paint": "accent", "width": 4}}),
            [0.0, 0.0, 100.0, 100.0],
        )
        .unwrap();
        let ops = arrow.ops();
        assert_eq!(ops.len(), 2, "the shaft and its head");
        let Op::Fill { path, .. } = &ops[1] else { panic!("{ops:?}") };
        assert_eq!(path.0[0], PathEl::MoveTo([100.0, 100.0]), "the head's point is the last point");
    }

    #[test]
    fn gradients_run_across_the_box_they_are_drawn_in() {
        let stops = json!([{ "at": 0, "color": "accent" }, { "at": 1, "color": "onSurface" }]);
        let paint = |kind: &str, extra: Value| {
            let mut g = json!({ "kind": kind, "stops": stops });
            g.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            let s = shape(json!({ "kind": "rect", "fill": { "gradient": g } }), [10.0, 20.0, 200.0, 100.0]).unwrap();
            let Op::Fill { paint, .. } = &s.ops()[0] else { panic!() };
            paint.clone()
        };
        // Linear: top to bottom by default, through the box's middle, as tall as it is.
        let close = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4;
        let Paint::Linear { start, end, stops } = paint("linear", json!({})) else { panic!() };
        assert!(close(start, [100.0, 0.0]) && close(end, [100.0, 100.0]), "{start:?} {end:?}");
        assert_eq!((stops[0].0, stops[0].1), (0.0, theme().color("accent").unwrap()));
        // 90°: left to right, as wide as the box.
        let Paint::Linear { start, end, .. } = paint("linear", json!({ "angle": 90 })) else { panic!() };
        assert!(close(start, [0.0, 50.0]) && close(end, [200.0, 50.0]), "{start:?} {end:?}");
        // Radial: to the farthest corner.
        let Paint::Radial { center, radius, .. } = paint("radial", json!({ "center": [0, 0] })) else { panic!() };
        assert_eq!(center, [0.0, 0.0]);
        assert!((radius - (200.0_f32 * 200.0 + 100.0 * 100.0).sqrt()).abs() < 1e-3);
        // Conic from up, as a sweep from the x axis: a quarter turn back.
        let Paint::Sweep { start_angle, end_angle, .. } = paint("conic", json!({})) else { panic!() };
        assert!((start_angle + core::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((end_angle - start_angle - core::f32::consts::TAU).abs() < 1e-5);
    }

    #[test]
    fn a_path_is_fitted_to_its_box() {
        let s = shape(json!({"path": "M0 0 L24 0 L24 12 Z", "fill": "accent"}), [0.0, 0.0, 100.0, 100.0]).unwrap();
        let Op::Fill { path, .. } = &s.ops()[0] else { panic!() };
        // 24 × 12 scaled ×100/24 to 100 × 50, centered vertically.
        assert_eq!(path.0[0], PathEl::MoveTo([0.0, 25.0]));
        assert_eq!(path.0[2], PathEl::LineTo([100.0, 75.0]));
    }

    #[test]
    fn words_in_a_path_stand_inside_its_outline_as_in_a_polygon() {
        // A diamond written as a path and as a polygon gives its words the same room (PLAN 2.108),
        // a square inside it, not the box around it.
        let theme = theme();
        let rect = [100.0, 100.0, 400.0, 400.0];
        let props = |v: Value| -> Props { serde_json::from_value(v).unwrap() };
        let path = props(json!({ "kind": "path", "path": "M0.5 0 L1 0.5 L0.5 1 L0 0.5 Z", "view": [0, 0, 1, 1] }));
        let polygon = props(json!({ "kind": "polygon", "points": [[0.5, 0], [1, 0.5], [0.5, 1], [0, 0.5]] }));
        let (a, b) = (words_box(&path, &theme, rect), words_box(&polygon, &theme, rect));
        assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1.0), "{a:?} as {b:?}");
        assert!(a[2] < 250.0 && a[0] > 150.0, "inside the diamond: {a:?}");
        // A square drawn in outline, as an icon's regular weight draws one, two outlines a line's
        // width apart: its room is inside its line, crossing neither outline, not in the line.
        let ring = props(json!({ "kind": "path", "view": [0, 0, 10, 10],
            "path": "M0 0 H10 V10 H0 Z M1 1 H9 V9 H1 Z" }));
        let r = words_box(&ring, &theme, rect);
        let (x1, y1) = (r[0] + r[2], r[1] + r[3]);
        assert!(r[0] >= 140.0 && r[1] >= 140.0 && x1 <= 460.0 && y1 <= 460.0, "inside the line: {r:?}");
        assert!(r[2] > 250.0 && r[3] > 250.0, "the room inside the line, not the line: {r:?}");
    }

    #[test]
    fn a_path_with_a_view_keeps_the_room_its_view_gives_it() {
        // A mark 12 wide on a view 24 square, in a box 100 wide and 200 tall: the view fits as
        // 100 × 100, centered, so the mark is drawn half as wide as the box, where its view puts it.
        let at = |props: Value| {
            let s = shape(props, [0.0, 0.0, 100.0, 200.0]).unwrap();
            let Op::Fill { path, .. } = &s.ops()[0] else { panic!() };
            path.0.clone()
        };
        let viewed = at(json!({"path": "M6 6 L18 6 L18 18 Z", "view": [0, 0, 24, 24], "fill": "accent"}));
        assert_eq!(viewed[0], PathEl::MoveTo([25.0, 75.0]));
        assert_eq!(viewed[2], PathEl::LineTo([75.0, 125.0]));
        // Without one, its own bounds fit the box: the same mark fills its width.
        let bare = at(json!({"path": "M6 6 L18 6 L18 18 Z", "fill": "accent"}));
        assert_eq!((bare[0], bare[2]), (PathEl::MoveTo([0.0, 50.0]), PathEl::LineTo([100.0, 150.0])));
        // A view that starts off its origin moves the path as its own coordinates say.
        let moved = at(json!({"path": "M106 6 L118 6 L118 18 Z", "view": [100, 0, 24, 24], "fill": "accent"}));
        assert_eq!(moved, viewed);
        // Two icons on one view keep one weight: a narrow one is not scaled up to the box.
        let narrow = at(json!({"path": "M11 2 L13 2 L13 22 L11 22 Z", "view": [0, 0, 24, 24], "fill": "accent"}));
        let PathEl::LineTo([x, _]) = narrow[1] else { panic!("{narrow:?}") };
        let PathEl::MoveTo([x0, _]) = narrow[0] else { panic!() };
        assert!((x - x0 - 100.0 * 2.0 / 24.0).abs() < 1e-3, "its stem is as wide as the view makes it");
    }

    #[test]
    fn a_shape_morphs_between_boxes_corners_and_paints() {
        let a = shape(json!({"kind": "rect", "fill": "accent", "radius": 8}), [0.0, 0.0, 100.0, 100.0]).unwrap();
        let b = shape(json!({"kind": "rect", "fill": "muted"}), [100.0, 0.0, 300.0, 50.0]).unwrap();
        assert!(a.morphs_to(&b) && b.morphs_to(&a));
        let mid = ShapeNode::lerp(&a, &b, 0.5, 0.5);
        assert_eq!((mid.rect, &mid.geometry), ([50.0, 0.0, 200.0, 75.0], &Geometry::Rect(4.0)));
        let [Op::Fill { paint: Paint::Solid(c), .. }] = mid.ops()[..] else { panic!("{:?}", mid.ops()) };
        let (x, y) = (theme().color("accent").unwrap(), theme().color("muted").unwrap());
        assert!(c != x && c != y, "the fill mixes: {c:?}");
        // A spring past the end never rounds a corner the wrong way.
        assert_eq!(ShapeNode::lerp(&a, &b, 1.5, 1.0).geometry, Geometry::Rect(0.0));
        // Another kind, or a stroke on one side only, cross-fades.
        let ellipse = shape(json!({"kind": "ellipse", "fill": "accent"}), [0.0, 0.0, 100.0, 100.0]).unwrap();
        let outlined = shape(json!({"kind": "rect", "fill": "accent", "stroke": {"color": "ink", "width": 2}}), a.rect);
        assert!(!a.morphs_to(&ellipse) && !a.morphs_to(&outlined.unwrap()));
    }

    #[test]
    fn points_and_paths_morph_point_by_point_when_they_line_up() {
        let tri =
            |pts: Value| shape(json!({"kind": "polygon", "points": pts, "fill": "accent"}), [0.0, 0.0, 100.0, 100.0]);
        let (a, b) = (tri(json!([[0, 1], [0.5, 0], [1, 1]])).unwrap(), tri(json!([[0, 0], [1, 0], [0.5, 1]])).unwrap());
        assert!(a.morphs_to(&b));
        let Op::Fill { path, .. } = &ShapeNode::lerp(&a, &b, 0.5, 0.5).ops()[0] else { panic!() };
        assert_eq!(
            path.0[..3],
            [PathEl::MoveTo([0.0, 50.0]), PathEl::LineTo([75.0, 0.0]), PathEl::LineTo([75.0, 100.0])]
        );
        let square = tri(json!([[0, 0], [1, 0], [1, 1], [0, 1]])).unwrap();
        assert!(!a.morphs_to(&square), "a point more cross-fades");
        // Paths: the same commands in the same order, each fitted to the box, then
        // point by point.
        let path = |d: &str| shape(json!({"path": d, "fill": "accent"}), [0.0, 0.0, 100.0, 100.0]).unwrap();
        let (x, y) = (path("M0 0 L10 0 L10 10 Z"), path("M0 0 L10 10 L0 10 Z"));
        assert!(x.morphs_to(&y) && !x.morphs_to(&path("M0 0 Q10 0 10 10 Z")));
        let Op::Fill { path: mid, .. } = &ShapeNode::lerp(&x, &y, 0.5, 0.5).ops()[0] else { panic!() };
        assert_eq!(mid.0[1], PathEl::LineTo([100.0, 50.0]));
    }

    #[test]
    fn what_a_shape_cannot_draw_says_why() {
        let err = |v| shape(v, [0.0, 0.0, 10.0, 10.0]).unwrap_err().to_string();
        assert!(err(json!({"kind": "polygon", "points": [[0, 0], [1, 1]]})).contains("three points"));
        assert!(err(json!({"kind": "path"})).contains("needs `path`"));
        assert!(err(json!({"path": "M0 0 Q"})).contains("not SVG path data"));
        assert!(err(json!({"path": "M0 0 L1 1", "view": [0, 0, 0, 24]})).contains("has no size"));
        assert!(err(json!({"path": "M0 0 L1 1", "view": [0, 0, 24]})).contains("expected [x, y, w, h]"));
        assert!(err(json!({"kind": "rect", "fill": {"gradient": {}}})).contains("gradient kind"));
        let one =
            json!({"kind": "rect", "fill": {"gradient": {"kind": "linear", "stops": [{"at": 0, "color": "accent"}]}}});
        assert!(err(one).contains("two stops"));
        assert!(err(json!({"kind": "rect", "fill": "nope"})).contains("unknown color"));
    }
}
