//! Shape nodes (SPEC §3.3): vector geometry in the node's box, painted from the theme.
//!
//! A shape fills its box. A `rect` is the box, its corners rounded by `radius`; an
//! `ellipse` is inscribed in it. `line`, `arrow`, and `polygon` join `points`, given as
//! fractions of the box: `[0, 0]` is its top-left and `[1, 1]` its bottom-right. A line or
//! arrow with no points crosses the box's middle, left to right. A `path` is SVG path data,
//! scaled uniformly to fit the box and centered in it. The geometry is made for whatever
//! box the shape is given, so a shape that changes size between states morphs: a frame
//! makes it again at the box it has reached, and lays nothing out.

use crate::EngineError;
use crate::charts::{RoundRect, lerp};
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
    /// As written: fitted to the box when drawn.
    Path(kurbo::BezPath),
}

#[derive(Debug, Clone, PartialEq)]
struct Stroke {
    paint: ShapePaint,
    width: f32,
    cap: Cap,
    join: Join,
    dash: Vec<f32>,
}

impl ShapeNode {
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
                Geometry::Path(bez)
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
        Ok(ShapeNode { rect, geometry, fill, stroke })
    }

    /// The same shape in another box.
    pub fn at(&self, rect: Rect) -> ShapeNode {
        let geometry = match &self.geometry {
            // The radius as it was, within what the new box allows.
            Geometry::Rect(r) => Geometry::Rect(r.min(rect[2].min(rect[3]) / 2.0)),
            other => other.clone(),
        };
        ShapeNode { rect, geometry, ..self.clone() }
    }

    /// Whether `other` is this shape, perhaps in another box: what a morph can move.
    pub fn same_shape(&self, other: &ShapeNode) -> bool {
        let kind = |g: &Geometry| std::mem::discriminant(g);
        kind(&self.geometry) == kind(&other.geometry)
            && match (&self.geometry, &other.geometry) {
                (Geometry::Rect(_), Geometry::Rect(_)) | (Geometry::Ellipse, Geometry::Ellipse) => true,
                (a, b) => a == b,
            }
            && (&self.fill, &self.stroke) == (&other.fill, &other.stroke)
    }

    /// `a` to `b`, `p` of the way: the box and a rect's radius move.
    pub fn lerp(a: &ShapeNode, b: &ShapeNode, p: f32) -> ShapeNode {
        let rect = [0, 1, 2, 3].map(|k| lerp(a.rect[k], b.rect[k], p));
        let geometry = match (&a.geometry, &b.geometry) {
            (Geometry::Rect(x), Geometry::Rect(y)) => Geometry::Rect(lerp(*x, *y, p)),
            _ => b.geometry.clone(),
        };
        ShapeNode { rect, geometry, ..b.clone() }
    }

    /// What the shape draws, in its box's coordinates: its fill, then its stroke.
    pub fn ops(&self) -> Vec<Op> {
        let [_, _, w, h] = self.rect;
        let at = |[x, y]: [f32; 2]| [x * w, y * h];
        let (path, closed) = match &self.geometry {
            Geometry::Rect(r) => (RoundRect { x: 0.0, y: 0.0, w, h, top_radius: *r, bottom_radius: *r }.path(), true),
            Geometry::Ellipse => {
                let ellipse = kurbo::Ellipse::new(
                    (f64::from(w) / 2.0, f64::from(h) / 2.0),
                    (f64::from(w) / 2.0, f64::from(h) / 2.0),
                    0.0,
                );
                (path_of(&kurbo::Shape::to_path(&ellipse, 0.1)), true)
            }
            Geometry::Line { points, arrow } => {
                let mut points: Vec<[f32; 2]> = points.iter().copied().map(at).collect();
                // An arrow's shaft stops where its head starts, so no cap shows past the point.
                if let (true, Some(s), [.., from, to]) = (*arrow, &self.stroke, points.as_mut_slice()) {
                    *to = head_base(*from, *to, s);
                }
                (polyline(points.into_iter(), false), false)
            }
            Geometry::Polygon(points) => (polyline(points.iter().copied().map(at), true), true),
            Geometry::Path(bez) => (path_of(&fit(bez, w, h)), true),
        };
        let mut ops = Vec::new();
        if let (Some(fill), true) = (&self.fill, closed) {
            ops.push(Op::Fill { path: path.clone(), rule: FillRule::NonZero, paint: fill.paint(w, h) });
        }
        if let Some(s) = &self.stroke {
            ops.push(Op::Stroke {
                path,
                paint: s.paint.paint(w, h),
                width: s.width,
                cap: s.cap,
                join: s.join,
                miter_limit: 4.0,
                dash: s.dash.clone(),
                dash_offset: 0.0,
            });
            if let Geometry::Line { points, arrow: true } = &self.geometry {
                let head = arrowhead(at(points[points.len() - 2]), at(points[points.len() - 1]), s);
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

/// Where the head on the segment `from`–`to` starts.
fn head_base(from: [f32; 2], to: [f32; 2], s: &Stroke) -> [f32; 2] {
    let (ux, uy) = direction(from, to);
    let (long, _) = head(s);
    [to[0] - ux * long, to[1] - uy * long]
}

/// A head's outline at `to`, pointing from `from`, sized by the stroke; it fills in the
/// stroke's paint.
fn arrowhead(from: [f32; 2], to: [f32; 2], s: &Stroke) -> Path {
    let (ux, uy) = direction(from, to);
    let (_, half) = head(s);
    let base = head_base(from, to, s);
    Path(vec![
        PathEl::MoveTo(to),
        PathEl::LineTo([base[0] - uy * half, base[1] + ux * half]),
        PathEl::LineTo([base[0] + uy * half, base[1] - ux * half]),
        PathEl::Close,
    ])
}

fn polyline(points: impl Iterator<Item = [f32; 2]>, closed: bool) -> Path {
    let mut els: Vec<PathEl> =
        points.enumerate().map(|(i, p)| if i == 0 { PathEl::MoveTo(p) } else { PathEl::LineTo(p) }).collect();
    if closed {
        els.push(PathEl::Close);
    }
    Path(els)
}

/// `bez` scaled uniformly to fit `w` × `h`, and centered.
fn fit(bez: &kurbo::BezPath, w: f32, h: f32) -> kurbo::BezPath {
    let b = kurbo::Shape::bounding_box(bez);
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

/// A kurbo path in the display list's terms.
fn path_of(bez: &kurbo::BezPath) -> Path {
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
    fn a_shape_morphs_between_boxes() {
        let a = shape(json!({"kind": "rect", "fill": "accent", "radius": 8}), [0.0, 0.0, 100.0, 100.0]).unwrap();
        let b = a.at([100.0, 0.0, 300.0, 50.0]);
        assert!(a.same_shape(&b));
        let mid = ShapeNode::lerp(&a, &b, 0.5);
        assert_eq!(mid.rect, [50.0, 0.0, 200.0, 75.0]);
        let other = shape(json!({"kind": "rect", "fill": "muted"}), [0.0, 0.0, 100.0, 100.0]).unwrap();
        assert!(!a.same_shape(&other), "another paint crossfades");
    }

    #[test]
    fn what_a_shape_cannot_draw_says_why() {
        let err = |v| shape(v, [0.0, 0.0, 10.0, 10.0]).unwrap_err().to_string();
        assert!(err(json!({"kind": "polygon", "points": [[0, 0], [1, 1]]})).contains("three points"));
        assert!(err(json!({"kind": "path"})).contains("needs `path`"));
        assert!(err(json!({"path": "M0 0 Q"})).contains("not SVG path data"));
        assert!(err(json!({"kind": "rect", "fill": {"gradient": {}}})).contains("gradient kind"));
        let one =
            json!({"kind": "rect", "fill": {"gradient": {"kind": "linear", "stops": [{"at": 0, "color": "accent"}]}}});
        assert!(err(one).contains("two stops"));
        assert!(err(json!({"kind": "rect", "fill": "nope"})).contains("unknown color"));
    }
}
