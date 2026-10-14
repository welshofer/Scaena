//! A drawing made with the pen (PLAN 2.114), as a presentation app's drawing tool makes one: each
//! stroke a pointer, a finger, or the Pencil draws on the canvas is smoothed into curves of one
//! shape's `path`. A drawing of several strokes is one object, which moves, turns, and builds as
//! one, and the Draw build draws its strokes on in the order they were drawn. Nothing is new in
//! the format: a path shape on its `view`, stroked in the theme's accent with round ends, unfilled.

use crate::inspect::{Added, added};
use crate::{Context, OpsError};
use scaena_core::Deck;
use scaena_core::inserts::{Insert, Start};
use scaena_core::model::theme::Vocabulary;
use scaena_engine::geometry::{Snap, Targets};
use scaena_engine::theme::Theme;
use serde_json::{Value, json};

/// A drawing a stroke joins (PLAN 2.114): its id, its `path` and `view` as the deck writes them, the
/// box it is drawn in at rest, and its stroke's width, canvas units.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawing {
    pub id: String,
    pub path: String,
    pub view: [f32; 4],
    pub rect: [f32; 4],
    pub width: f32,
}

impl Drawing {
    /// How its view draws in its box, as `scaena_engine::shapes` fits a path to its box: the scale,
    /// alike both ways, and the canvas point its own units' origin falls on.
    fn frame(&self) -> (f32, [f32; 2]) {
        let ([vx, vy, vw, vh], [bx, by, bw, bh]) = (self.view, self.rect);
        let s = (bw / vw).min(bh / vh);
        (s, [bx + (bw - vw * s) / 2.0 - vx * s, by + (bh - vh * s) / 2.0 - vy * s])
    }
}

/// The stroke a drawing takes where the theme names it: its `thick` width, else its `regular`, so
/// a pen's line reads at a slide's size.
const WIDTHS: [&str; 2] = ["thick", "regular"];

/// What the pen draws (PLAN 2.114), offered as an insert is, to find the room a new drawing takes
/// (`Engine::room`): a path shape stroked in the theme's accent (or its first color) at its thick
/// width (else its regular), with round ends and joins, as ink is, unfilled, named Drawing. Its
/// `path` and `view` are its strokes'.
pub fn insert(theme: &Theme) -> Insert {
    let colors = theme.names(Vocabulary::Color);
    let paint = colors.iter().find(|c| *c == "accent").or(colors.first());
    let strokes = theme.names(Vocabulary::Stroke);
    let width = WIDTHS.into_iter().find(|w| strokes.iter().any(|s| s == w));
    let mut stroke = json!({ "cap": "round", "join": "round" });
    if let Some(paint) = paint {
        stroke["paint"] = json!(paint);
    }
    if let Some(width) = width {
        stroke["width"] = json!(width);
    }
    let node = json!({ "type": "shape", "kind": "path", "alt": "Drawing", "stroke": stroke });
    Insert { label: "Drawing".into(), node, id: "drawing".into(), start: Start::Box { w: 0.25, h: 0.25 }, sample: None }
}

/// The patch that draws `stroke`, the points a pen gave in canvas units in the order it gave them,
/// in `state` (PLAN 2.114). It is smoothed: the fewest of its points that keep it within a pixel
/// or two of where it went, joined by the curve through them ([`smooth`]). A press that hardly
/// moves draws a dot.
///
/// Alone, it is a new drawing where it was drawn, `room`'s node (`Engine::room`), as [`insert`]
/// offers one: `add_node`, entering in `state`, a `place` by `rect` around it, then where it leaves,
/// so it stays on its slide (PLAN 3.24). `onto` a drawing, it is one more of that drawing's strokes,
/// `room` the drawing's (`Engine::targets`): its `path` gains the stroke in its own units, its
/// `view` grows to take it in, and a `place` puts its box where the view now draws, so what it drew
/// before stays where it was. Refused for a stroke with no point.
pub fn inking(
    deck: &Deck,
    theme: &Theme,
    room: &Targets,
    state: &str,
    stroke: &[[f32; 2]],
    onto: Option<&Drawing>,
) -> Result<Added, OpsError> {
    let finite = stroke.iter().filter(|p| p[0].is_finite() && p[1].is_finite());
    let drawn: Vec<[f32; 2]> = finite.copied().collect();
    if drawn.is_empty() {
        return Err(OpsError::new("a stroke is drawn from one point on: this one has none"));
    }
    // The drawing's own units: the canvas's for a new one, else as its view draws in its box.
    let (scale, origin) = onto.map_or((1.0, [0.0, 0.0]), Drawing::frame);
    if !(scale.is_finite() && scale > 0.0) {
        return Err(OpsError::new("the drawing has no size to draw in"));
    }
    let own = |[x, y]: [f32; 2]| [(x - origin[0]) / scale, (y - origin[1]) / scale];
    let points: Vec<[f32; 2]> = drawn.into_iter().map(own).collect();
    // Within a pixel of a 1920-wide slide shown at half its size.
    let tolerance = room.within[2].max(1.0) / 960.0 / scale;
    let (start, curve) = smooth(&points, tolerance);
    let width = onto.map_or_else(|| new_width(theme), |d| d.width);
    let pad = (width / 2.0 + 1.0) / scale;
    let [lo, hi] = bounds(start, &curve);
    let mut own_box = [lo[0] - pad, lo[1] - pad, hi[0] + pad, hi[1] + pad];
    if let Some(d) = onto {
        let [vx, vy, vw, vh] = d.view;
        own_box = [own_box[0].min(vx), own_box[1].min(vy), own_box[2].max(vx + vw), own_box[3].max(vy + vh)];
    }
    // Its box, in whole canvas units around what it draws, and its view, the box in its own units:
    // what it drew before draws where it did.
    let [x0, y0] = [(origin[0] + own_box[0] * scale).floor(), (origin[1] + own_box[1] * scale).floor()];
    let [x1, y1] = [(origin[0] + own_box[2] * scale).ceil(), (origin[1] + own_box[3] * scale).ceil()];
    let rect = [x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0)];
    let view = [(x0 - origin[0]) / scale, (y0 - origin[1]) / scale, rect[2] / scale, rect[3] / scale];
    let places = places(scale);
    let target = room.snap(Snap::Free, rect);
    let view_json = Value::Array(view.iter().map(|&v| number(v, places)).collect());
    let written = path(start, &curve, places);
    match onto {
        None => {
            let mut node = insert(theme).node;
            node["path"] = json!(written);
            node["view"] = view_json;
            added(deck, room, node, state, target)
        }
        Some(d) => {
            let target = target.ok_or_else(|| OpsError::new(format!("`{}` is not placed where it is drawn", d.id)))?;
            let at = |key: &str| format!("/nodes/{}/{key}", d.id.replace('~', "~0").replace('/', "~1"));
            let mut patch = vec![
                json!({ "op": "replace", "path": at("path"), "value": format!("{} {written}", d.path.trim_end()) }),
                json!({ "op": "replace", "path": at("view"), "value": view_json }),
            ];
            for op in target.ops(Some(state), false) {
                patch.push(serde_json::to_value(op).context("a patch")?);
            }
            Ok(Added { id: d.id.clone(), cell: target.cell, patch, copies: Default::default() })
        }
    }
}

/// Where a drawing laid out at `rect` on `view` stands once its view is `to` (PLAN 2.114): the box
/// that draws `to` at the scale and place `view` drew at in `rect`, so what it drew there stays
/// where it was. What a stroke drawn in one format makes of its box in another (ADR-0020).
pub fn regrown(view: [f32; 4], rect: [f32; 4], to: [f32; 4]) -> [f32; 4] {
    let (s, [ox, oy]) = Drawing { id: String::new(), path: String::new(), view, rect, width: 0.0 }.frame();
    if !(s.is_finite() && s > 0.0) {
        return rect;
    }
    [ox + to[0] * s, oy + to[1] * s, to[2] * s, to[3] * s].map(|v| (v * 100.0).round() / 100.0)
}

/// The width a new drawing's stroke draws at, canvas units: as [`insert`] names it, else the
/// engine's own for a stroke that names none.
fn new_width(theme: &Theme) -> f32 {
    let named = WIDTHS.into_iter().find_map(|w| theme.stroke(w).ok());
    named.or_else(|| theme.stroke("thin").ok()).unwrap_or(2.0)
}

/// The width the stroke `stroke` (a node's `stroke`, as a state shows it) draws at, canvas units,
/// as the engine draws it: its own width, else the theme's thin one, else 2.
pub fn width_of(theme: &Theme, stroke: Option<&Value>) -> f32 {
    let own = stroke.and_then(|s| s.get("width")).and_then(|w| theme.length(w, 0.0).ok());
    own.or_else(|| theme.stroke("thin").ok()).unwrap_or(2.0)
}

/// How many places after the point a drawing's numbers keep in its own units: a tenth of a canvas
/// unit, however its view is scaled.
fn places(scale: f32) -> usize {
    (1.0 - scale.log10()).ceil().clamp(1.0, 4.0) as usize
}

/// `v` to `places` places, whole where it is, as a deck writes a number.
fn number(v: f32, places: usize) -> Value {
    let text = text(v, places);
    text.parse::<i64>().map(Value::from).unwrap_or_else(|_| json!(text.parse::<f64>().unwrap_or(0.0)))
}

/// `v` written to `places` places, without the zeros after its last digit.
fn text(v: f32, places: usize) -> String {
    let mut s = format!("{v:.places$}");
    if s.contains('.') {
        s.truncate(s.trim_end_matches('0').trim_end_matches('.').len());
    }
    if s == "-0" { "0".into() } else { s }
}

/// A smoothed stroke as SVG path data: `M` to where it starts, then a `C` for each piece.
fn path(start: [f32; 2], curve: &[[[f32; 2]; 3]], places: usize) -> String {
    let p = |[x, y]: [f32; 2]| format!("{} {}", text(x, places), text(y, places));
    let mut out = format!("M{}", p(start));
    for [c1, c2, end] in curve {
        out.push_str(&format!(" C{} {} {}", p(*c1), p(*c2), p(*end)));
    }
    out
}

/// `points` drawn smooth (PLAN 2.114): the fewest of them that keep within `tolerance` of the line
/// through them all (Ramer–Douglas–Peucker), joined by a Catmull–Rom curve through them whose
/// handles each piece's length scales, each piece a cubic Bézier, `[c1, c2, end]`, after where it
/// starts. Where they all lie within
/// `tolerance` of the first, a dot: a piece half a unit long, which round ends draw round.
pub fn smooth(points: &[[f32; 2]], tolerance: f32) -> ([f32; 2], Vec<[[f32; 2]; 3]>) {
    let first = points[0];
    let far = |p: &[f32; 2]| (p[0] - first[0]).hypot(p[1] - first[1]) > tolerance;
    if !points.iter().any(far) {
        let end = [first[0] + 0.5, first[1]];
        return (first, vec![[first, end, end]]);
    }
    // The points kept: both ends, and each that stands furthest from the line between two kept,
    // where it stands further than `tolerance`.
    let mut keep = vec![false; points.len()];
    let last = points.len() - 1;
    (keep[0], keep[last]) = (true, true);
    let mut spans = vec![(0, last)];
    while let Some((a, b)) = spans.pop() {
        let off = (a + 1..b).map(|i| (i, apart(points[i], points[a], points[b])));
        if let Some((i, d)) = off.max_by(|x, y| x.1.total_cmp(&y.1))
            && d > tolerance
        {
            keep[i] = true;
            spans.extend([(a, i), (i, b)]);
        }
    }
    let kept: Vec<[f32; 2]> = points.iter().zip(&keep).filter(|(_, k)| **k).map(|(p, _)| *p).collect();
    let at = |i: isize| kept[i.clamp(0, kept.len() as isize - 1) as usize];
    let long = |a: [f32; 2], b: [f32; 2]| (b[0] - a[0]).hypot(b[1] - a[1]);
    // Each point's tangent runs from the point before it to the one after, and each piece's
    // handles reach along it in proportion to the piece's own length, at most a third of it: the
    // curve through points kept unevenly, close where the hand turned and far where it ran
    // straight, never hooks past them.
    let handle = |from: [f32; 2], to: [f32; 2], piece: f32, beside: f32| {
        let k = if piece + beside > 0.0 { piece / (3.0 * (piece + beside)) } else { 0.0 };
        [(to[0] - from[0]) * k, (to[1] - from[1]) * k]
    };
    let curve = (0..kept.len() as isize - 1)
        .map(|i| {
            let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
            let piece = long(p1, p2);
            let [ax, ay] = handle(p0, p2, piece, long(p0, p1));
            let [bx, by] = handle(p1, p3, piece, long(p2, p3));
            [[p1[0] + ax, p1[1] + ay], [p2[0] - bx, p2[1] - by], p2]
        })
        .collect();
    (kept[0], curve)
}

/// How far `p` stands from the segment `a`–`b`.
fn apart(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let long = dx * dx + dy * dy;
    let t = if long > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / long).clamp(0.0, 1.0) } else { 0.0 };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// The box a smoothed stroke's curve draws in, `[least, most]`: each piece's ends, and where it
/// turns back on either axis.
fn bounds(start: [f32; 2], curve: &[[[f32; 2]; 3]]) -> [[f32; 2]; 2] {
    let (mut lo, mut hi) = (start, start);
    let mut from = start;
    for &[c1, c2, end] in curve {
        for axis in 0..2 {
            let [p0, p1, p2, p3] = [from[axis], c1[axis], c2[axis], end[axis]];
            // Where the piece's derivative on this axis is nought: a t² + b t + c = 0.
            let (a, b, c) = (p1 - p0, p2 - p1, p3 - p2);
            let (qa, qb, qc) = (a - 2.0 * b + c, 2.0 * (b - a), a);
            let mut turns = [f32::NAN; 2];
            if qa.abs() < 1e-6 {
                if qb.abs() > 1e-6 {
                    turns[0] = -qc / qb;
                }
            } else {
                let disc = qb * qb - 4.0 * qa * qc;
                if disc >= 0.0 {
                    let root = disc.sqrt();
                    turns = [(-qb + root) / (2.0 * qa), (-qb - root) / (2.0 * qa)];
                }
            }
            let at = |t: f32| {
                let u = 1.0 - t;
                u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
            };
            for v in turns.into_iter().filter(|t| *t > 0.0 && *t < 1.0).map(at).chain([p3]) {
                lo[axis] = lo[axis].min(v);
                hi[axis] = hi[axis].max(v);
            }
        }
        from = end;
    }
    [lo, hi]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hands_stroke_is_smoothed_to_the_fewest_points_that_keep_its_way() {
        // A line drawn with a tremor of half a unit keeps its ends alone, one piece.
        let shaky: Vec<[f32; 2]> = (0..=100).map(|i| [i as f32 * 4.0, if i % 2 == 0 { 0.5 } else { -0.5 }]).collect();
        let (start, curve) = smooth(&shaky, 2.0);
        assert_eq!(start, [0.0, 0.5]);
        assert_eq!(curve.len(), 1, "{curve:?}");
        // A V keeps its corner.
        let vee: Vec<[f32; 2]> = (0..=40).map(|i| [i as f32 * 5.0, (100 - (i * 5 - 100i32).abs()) as f32]).collect();
        let (_, curve) = smooth(&vee, 2.0);
        assert_eq!(curve.len(), 2, "{curve:?}");
        assert_eq!(curve[0][2], [100.0, 100.0]);
        // A circle keeps enough points to stay round, and runs through each kept.
        let ring: Vec<[f32; 2]> = (0..=72)
            .map(|i| {
                let a = i as f32 / 72.0 * std::f32::consts::TAU;
                [100.0 + 80.0 * a.cos(), 100.0 + 80.0 * a.sin()]
            })
            .collect();
        let (start, curve) = smooth(&ring, 2.0);
        assert!((8..=40).contains(&curve.len()), "{}", curve.len());
        let [lo, hi] = bounds(start, &curve);
        for (got, want) in [(lo[0], 20.0), (lo[1], 20.0), (hi[0], 180.0), (hi[1], 180.0)] {
            assert!((got - want).abs() < 2.5, "the ring's bounds: {lo:?} {hi:?}");
        }
        // Kept close where it turns and far where it runs straight, no piece's handles reach past
        // a third of it, so the curve hooks past no point.
        let wave = (0..=12).map(|i| [i as f32 * 4.0, (i as f32 * 0.8).sin() * 12.0]);
        let uneven: Vec<[f32; 2]> = wave.chain([[240.0, 0.0]]).collect();
        let (start, curve) = smooth(&uneven, 0.5);
        assert!(curve.len() > 3, "{curve:?}");
        let mut from = start;
        for &[c1, c2, end] in &curve {
            let piece = (end[0] - from[0]).hypot(end[1] - from[1]);
            for (at, handle) in [(from, c1), (end, c2)] {
                let reach = (handle[0] - at[0]).hypot(handle[1] - at[1]);
                assert!(reach <= piece / 3.0 + 1e-3, "a handle {reach} long on a piece {piece} long: {curve:?}");
            }
            from = end;
        }
        // A press that hardly moves is a dot.
        let (start, curve) = smooth(&[[10.0, 10.0], [10.4, 10.2], [10.1, 9.8]], 2.0);
        assert_eq!((start, curve.len()), ([10.0, 10.0], 1));
        assert_eq!(curve[0][2], [10.5, 10.0]);
    }

    #[test]
    fn a_curves_bounds_take_in_where_it_turns_back() {
        // A piece that bulges past its ends: its box takes in the bulge.
        let [lo, hi] = bounds([0.0, 0.0], &[[[0.0, 100.0], [100.0, 100.0], [100.0, 0.0]]]);
        assert_eq!(lo, [0.0, 0.0]);
        assert!((hi[1] - 75.0).abs() < 1e-3 && hi[0] == 100.0, "{hi:?}");
    }

    #[test]
    fn numbers_are_written_as_short_as_they_read() {
        assert_eq!(text(12.0, 1), "12");
        assert_eq!(text(12.26, 1), "12.3");
        assert_eq!(text(-0.01, 1), "0");
        assert_eq!(text(1.23456, 3), "1.235");
        assert_eq!(number(1080.0, 1), json!(1080));
        assert_eq!(number(10.5, 2), json!(10.5));
        assert_eq!((places(1.0), places(0.5), places(0.05)), (1, 2, 3));
    }
}
