//! What a data source's rows draw, and the rows behind what is drawn (PLAN 2.64, ADR-0013): a
//! chart's mark or a table's row, in a state at rest. A chart's datum, and a table's row, is
//! made from rows of its source through its `dataTransform`
//! ([`scaena_core::transform::traced`]): a row kept, sorted, or derived draws as itself, and an
//! aggregated one draws its group. The editor selects a mark's rows in the source's sheet, and
//! marks what a row selected there draws.
//!
//! A point is on a bar or a slice inside it, on a dot within its radius or [`SLOP`], on an
//! area in the column of it nearest across, and on a line within [`REACH`] of a point the line
//! runs through; on a value label, it is on the label's mark. A point is on a table's row in
//! the band its cells stand in across the table.
//!
//! A chart's marks and its annotations name each other (PLAN 2.67): a mark says where a
//! callout on it, a highlight of it, a rule at its value, or a band from it stands, and an
//! annotation drawn at a point says which of the chart's `annotations` it is, for the editor
//! to annotate a chart from its marks, move a callout, and take an annotation away.

use crate::charts::{ChartKind, ChartLayout, Label, MarkPlace, Note, Rule, Shape};
use crate::geometry::{SLOP, reaches};
use crate::sample::{Content, Scene};
use crate::scale;
use crate::tables::TableLayout;
use scaena_core::displaylist::{Path, PathEl, Rect};
use scaena_core::model::values::{AnnotationAt, AnnotationKind, Place, Scalar};

/// How near a point must come to a point a line runs through, canvas units.
pub const REACH: f32 = 2.0 * SLOP;

/// What a row of a data source draws in a state at rest: a chart's mark, or a table's row.
#[derive(Debug, Clone, PartialEq)]
pub struct DataMark {
    /// The chart or table that draws it.
    pub node: String,
    /// The source it reads, as the deck names it.
    pub source: String,
    /// The mark's key, or the table row's.
    pub key: String,
    /// The rows of the source it draws, from 0, as the source's sheet numbers them.
    pub rows: Vec<usize>,
    /// Its outline as laid out, canvas units, as SVG path data: a bar's box, a slice, a dot or
    /// a point a line runs through as a ring around it, a table row's band.
    pub outline: String,
    /// The box around its outline, `[x, y, w, h]`, canvas units.
    pub rect: Rect,
    /// Where the node's transform, and those of what holds it, draw it from where it is laid
    /// out, as [`crate::geometry::NodeBox::transform`].
    pub transform: Option<[f32; 6]>,
    /// A chart's mark's annotations (PLAN 2.67); none for a table's row.
    pub notes: Option<MarkNotes>,
}

/// What a chart's mark is to the chart's annotations (SPEC §3.7, PLAN 2.67): where each kind
/// made from it stands, and the highlights that pick it out.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkNotes {
    /// Its x as an annotation names it: a category as its datum reads, a number, or a date in
    /// ISO 8601.
    pub x: Scalar,
    /// Its value on the value axis: where a rule at its value stands.
    pub value: f64,
    pub series: Option<String>,
    /// Whether the chart has axes for a callout, a rule, or a band to stand on: a donut has
    /// none, and takes highlights alone.
    pub axes: bool,
    /// Where a callout on it stands: at its x, on its series' mark there where the chart has
    /// series; at its x and its value where those do not pick it out alone.
    pub callout: AnnotationAt,
    /// What a highlight of it picks out: its x, and its series where the chart has series.
    pub highlight: AnnotationAt,
    /// The chart's highlights that pick it out, by their places among its `annotations`.
    pub highlighted: Vec<usize>,
}

/// One of a chart's annotations as drawn in a state at rest (PLAN 2.67): what the editor
/// selects, moves, and takes away. A highlight draws nothing of its own, so none is one; a
/// mark it picks out names it ([`MarkNotes::highlighted`]).
#[derive(Debug, Clone, PartialEq)]
pub struct NoteMark {
    /// The chart.
    pub node: String,
    /// Its place among the chart's `annotations`.
    pub index: usize,
    pub kind: AnnotationKind,
    /// What it says, as written.
    pub text: Option<String>,
    /// Its outline as laid out, canvas units, as SVG path data: its band's box, its rule's or
    /// leader's line, and its text's box.
    pub outline: String,
    /// The box around its outline, `[x, y, w, h]`, canvas units.
    pub rect: Rect,
    /// Where the chart's transform, and those of what holds it, draw it from where it is laid
    /// out.
    pub transform: Option<[f32; 6]>,
}

impl Scene {
    /// The mark or table row at `point` (canvas units), in what draws topmost there, read
    /// through its transform: `None` where that is no chart or table, or the point falls
    /// between its marks.
    pub fn mark_at(&self, point: [f32; 2]) -> Option<DataMark> {
        let top = self.hit(point).into_iter().next()?;
        let node = self.nodes.iter().find(|n| n.id == top.node)?;
        let at = self.laid_out(&node.id, point)?;
        match &node.content {
            Content::Chart { cell, chart } => {
                let key = chart_key(chart, [at[0] - cell[0], at[1] - cell[1]])?;
                let mark = chart.marks.iter().find(|m| m.key == key)?;
                Some(self.chart_mark(&node.id, *cell, chart, &mark.key, mark.shape))
            }
            Content::Table { cell, table } => {
                let local = [at[0] - cell[0], at[1] - cell[1]];
                let (key, band) = table.bands.iter().find(|(_, band)| inside(*band, local))?;
                Some(self.table_mark(&node.id, *cell, table, key, *band))
            }
            _ => None,
        }
    }

    /// What `rows` of data source `source` draw in this state at rest, in paint order: each
    /// chart mark and table row made from any of them.
    pub fn marks_of(&self, source: &str, rows: &[usize]) -> Vec<DataMark> {
        let made = |from: Option<&Vec<usize>>| from.is_some_and(|from| from.iter().any(|r| rows.contains(r)));
        let mut out = Vec::new();
        for node in self.nodes.iter().filter(|n| self.tree.contains_key(&n.id) && n.opacity > 0.0) {
            match &node.content {
                Content::Chart { cell, chart } if chart.source == source => {
                    for m in chart.marks.iter().filter(|m| made(chart.rows.get(&m.key))) {
                        out.push(self.chart_mark(&node.id, *cell, chart, &m.key, m.shape));
                    }
                }
                Content::Table { cell, table } if table.source == source => {
                    for (key, band) in table.bands.iter().filter(|(key, _)| made(table.rows.get(key))) {
                        out.push(self.table_mark(&node.id, *cell, table, key, *band));
                    }
                }
                _ => {}
            }
        }
        out
    }

    fn chart_mark(&self, node: &str, cell: Rect, chart: &ChartLayout, key: &str, shape: Shape) -> DataMark {
        let shape = shape.translated([cell[0], cell[1]]);
        let outline = match shape {
            // A point a line or an area runs through, and a dot too small to point at: a ring.
            Shape::Dot { x, y, r } => Shape::Dot { x, y, r: r.max(SLOP) }.path(),
            Shape::Span { x, top, .. } => Shape::Dot { x, y: top, r: SLOP }.path(),
            _ => shape.path(),
        };
        // A slice of nothing: a hairline where it would open.
        let outline =
            outline.unwrap_or_else(|| Path(vec![PathEl::MoveTo(shape.point()), PathEl::LineTo(shape.point())]));
        DataMark {
            node: node.to_string(),
            source: chart.source.clone(),
            key: key.to_string(),
            rows: chart.rows.get(key).cloned().unwrap_or_default(),
            rect: bounds(&outline),
            outline: outline.to_svg(),
            transform: self.drawn(node),
            notes: mark_notes(chart, key),
        }
    }

    fn table_mark(&self, node: &str, cell: Rect, table: &TableLayout, key: &str, band: Rect) -> DataMark {
        let rect = [cell[0] + band[0], cell[1] + band[1], band[2], band[3]];
        DataMark {
            node: node.to_string(),
            source: table.source.clone(),
            key: key.to_string(),
            rows: table.rows.get(key).cloned().unwrap_or_default(),
            outline: Path::rect(rect).to_svg(),
            rect,
            transform: self.drawn(node),
            notes: None,
        }
    }

    /// The chart annotation drawn at `point` (canvas units) in what draws topmost there, read
    /// through its transform (PLAN 2.67): its text first, then a rule or a leader within
    /// [`SLOP`] of the point, then a band where no mark is. `None` where that is no chart, or
    /// the point is on none of its annotations.
    pub fn note_at(&self, point: [f32; 2]) -> Option<NoteMark> {
        let top = self.hit(point).into_iter().next()?;
        let node = self.nodes.iter().find(|n| n.id == top.node)?;
        let Content::Chart { cell, chart } = &node.content else { return None };
        let at = self.laid_out(&node.id, point)?;
        let note = note_at(chart, [at[0] - cell[0], at[1] - cell[1]])?;
        Some(self.note_mark(&node.id, *cell, note))
    }

    /// Chart `node`'s marks and annotations, as a pointer finds each (PLAN 2.75): its marks in data
    /// order, then its annotations in the order it writes them. `None` for a node this state does
    /// not draw, or one that is no chart.
    pub fn marks_in(&self, node: &str) -> Option<(Vec<DataMark>, Vec<NoteMark>)> {
        let drawn = self.nodes.iter().find(|n| n.id == node)?;
        let Content::Chart { cell, chart } = &drawn.content else { return None };
        let marks = chart.marks.iter().map(|m| self.chart_mark(node, *cell, chart, &m.key, m.shape)).collect();
        let mut notes: Vec<&Note> = chart.notes.iter().collect();
        scaena_core::sort::by(&mut notes, |a, b| a.index.cmp(&b.index));
        Some((marks, notes.into_iter().map(|n| self.note_mark(node, *cell, n)).collect()))
    }

    fn note_mark(&self, node: &str, cell: Rect, note: &Note) -> NoteMark {
        let [x, y] = [cell[0], cell[1]];
        let mut els = Vec::new();
        if let Some(([bx, by, bw, bh], _)) = note.band {
            els.extend(Path::rect([x + bx, y + by, bw, bh]).0);
        }
        if let Some(r) = &note.rule {
            els.extend([PathEl::MoveTo([x + r.from[0], y + r.from[1]]), PathEl::LineTo([x + r.to[0], y + r.to[1]])]);
        }
        if let Some(l) = &note.label {
            let [lx, ly, lw, lh] = label_box(l);
            els.extend(Path::rect([x + lx, y + ly, lw, lh]).0);
        }
        let outline = Path(els);
        NoteMark {
            node: node.to_string(),
            index: note.index,
            kind: note.kind,
            text: note.text.clone(),
            rect: bounds(&outline),
            outline: outline.to_svg(),
            transform: self.drawn(node),
        }
    }

    /// Where a callout of chart `node` dropped at `point` (canvas units) would stand (PLAN
    /// 2.67), read through the chart's transform: on the mark there, as the mark's own
    /// callout stands ([`MarkNotes::callout`]); elsewhere, at the category or the x of a mark
    /// nearest across, and the value at the point, to a tenth of the value axis's step and
    /// within its domain. `None` for a node that is no chart, or a donut, which takes no
    /// callouts.
    pub fn callout_at(&self, node: &str, point: [f32; 2]) -> Option<AnnotationAt> {
        let scene = self.nodes.iter().find(|n| n.id == node)?;
        let Content::Chart { cell, chart } = &scene.content else { return None };
        if chart.kind == ChartKind::Donut {
            return None;
        }
        let at = self.laid_out(node, point)?;
        let p = [at[0] - cell[0], at[1] - cell[1]];
        if let Some(key) = chart_key(chart, p) {
            return mark_notes(chart, key).map(|m| m.callout);
        }
        let middle = |[a, b]: [f32; 2]| (a + b) / 2.0;
        let x = match chart.categories.is_empty() {
            false => (chart.categories.iter())
                .min_by(|a, b| (middle(a.1) - p[0]).abs().total_cmp(&(middle(b.1) - p[0]).abs()))
                .map(|(x, _)| x.clone())?,
            true => (chart.marks.iter())
                .filter_map(|m| Some(((m.shape.center_x() - p[0]).abs(), &chart.places.get(&m.key)?.x)))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, x)| x.clone())?,
        };
        let [d0, d1] = chart.y_scale.domain;
        if !(d0.is_finite() && d1.is_finite()) {
            return None;
        }
        let step = scale::tick_step(d0.min(d1), d0.max(d1), 5) / 10.0;
        let value = chart.y_scale.invert(p[1]).clamp(d0.min(d1), d0.max(d1));
        Some(AnnotationAt { x: one(x), y: one(Scalar::Number(round_to(value, step))), series: None })
    }
}

/// `Some` of one value.
fn one(v: Scalar) -> Option<Place> {
    Some(Place::One(v))
}

/// `v` to the nearest multiple of `step`, as a person writes it: with the decimals the step
/// has, so a tenth of 2 is 0.2, never 0.20000000000000001.
fn round_to(v: f64, step: f64) -> f64 {
    if !(step.is_finite() && step > 0.0) {
        return v;
    }
    let places = (-step.log10().floor()).max(0.0) as i32;
    let scale = 10f64.powi(places);
    ((v / step).round() * step * scale).round() / scale
}

/// The annotations `chart`'s mark `key` makes: see [`MarkNotes`].
fn mark_notes(chart: &ChartLayout, key: &str) -> Option<MarkNotes> {
    let p: &MarkPlace = chart.places.get(key)?;
    let axes = chart.kind != ChartKind::Donut;
    let series = p.series.clone().filter(|_| axes).map(Scalar::Text);
    let alone =
        chart.places.values().filter(|q| q.x == p.x && (p.series.is_none() || q.series == p.series)).count() == 1;
    let callout = match alone {
        true => AnnotationAt { x: one(p.x.clone()), y: None, series: series.clone().and_then(one) },
        false => AnnotationAt { x: one(p.x.clone()), y: one(Scalar::Number(p.value)), series: None },
    };
    Some(MarkNotes {
        x: p.x.clone(),
        value: p.value,
        series: p.series.clone(),
        axes,
        callout,
        highlight: AnnotationAt { x: one(p.x.clone()), y: None, series: series.and_then(one) },
        highlighted: (chart.highlights.iter())
            .filter(|(_, keys)| keys.iter().any(|k| k == key))
            .map(|(i, _)| *i)
            .collect(),
    })
}

/// `chart`'s annotation at `p`, relative to the chart: the last drawn whose text holds it;
/// else whose rule or leader passes within [`SLOP`] of it; else whose band holds it, where no
/// mark is.
fn note_at(chart: &ChartLayout, p: [f32; 2]) -> Option<&Note> {
    let notes = || chart.notes.iter().rev();
    (notes().find(|n| n.label.as_ref().is_some_and(|l| inside(label_box(l), p))))
        .or_else(|| notes().find(|n| n.rule.as_ref().is_some_and(|r| near(r, p))))
        .or_else(|| match chart_key(chart, p) {
            Some(_) => None,
            None => notes().find(|n| n.band.is_some_and(|(b, _)| inside(b, p))),
        })
}

/// Whether `p` is within [`SLOP`] of `rule`, or within its width.
fn near(rule: &Rule, p: [f32; 2]) -> bool {
    let ([ax, ay], [bx, by]) = (rule.from, rule.to);
    let (dx, dy) = (bx - ax, by - ay);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 { (((p[0] - ax) * dx + (p[1] - ay) * dy) / length).clamp(0.0, 1.0) } else { 0.0 };
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    (p[0] - cx).hypot(p[1] - cy) <= SLOP.max(rule.width / 2.0)
}

/// The key of `chart`'s mark at `p`, relative to the chart: the topmost bar, dot, or slice that
/// holds it; else the column of an area it stands in, nearest across; else the point a line
/// runs through nearest it, within [`REACH`]; else the mark whose value label it is on.
fn chart_key(chart: &ChartLayout, p: [f32; 2]) -> Option<&str> {
    if let Some(m) = chart.marks.iter().rev().find(|m| holds(m.shape, p)) {
        return Some(&m.key);
    }
    let nearest = |how: &dyn Fn(Shape) -> Option<f32>| {
        (chart.marks.iter())
            .filter_map(|m| Some((m.key.as_str(), how(m.shape)?)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(key, _)| key)
    };
    let [left, _, width, _] = chart.plot;
    let across = p[0] >= left && p[0] <= left + width;
    let column = |s: Shape| match s {
        Shape::Span { x, top, base } if across && p[1] >= top.min(base) - SLOP && p[1] <= top.max(base) + SLOP => {
            Some((p[0] - x).abs())
        }
        _ => None,
    };
    let point = |s: Shape| match s {
        Shape::Dot { x, y, .. } | Shape::Span { x, top: y, .. } => {
            Some((p[0] - x).hypot(p[1] - y)).filter(|d| *d <= REACH)
        }
        _ => None,
    };
    (nearest(&column)).or_else(|| nearest(&point)).or_else(|| {
        chart.labels.iter().rev().find(|l| l.value.is_some() && inside(label_box(l), p)).map(|l| l.key.as_str())
    })
}

/// Whether `shape` holds `p`: a bar its box, or within [`SLOP`] of a bar too thin to point at;
/// a dot within its radius, or [`SLOP`] of a small one; a slice between its radii and its turns.
fn holds(shape: Shape, p: [f32; 2]) -> bool {
    match shape {
        Shape::Bar(r) => {
            let (top, bottom) = (r.top().min(r.bottom()), r.top().max(r.bottom()));
            reaches([r.x, top, r.w, bottom - top], p)
        }
        Shape::Dot { x, y, r } => r > 0.0 && (p[0] - x).hypot(p[1] - y) <= r.max(SLOP),
        Shape::Span { .. } => false,
        Shape::Arc { cx, cy, inner, outer, start, end } => {
            let (dx, dy) = (p[0] - cx, p[1] - cy);
            let radius = dx.hypot(dy);
            if end <= start || radius < inner || radius > outer {
                return false;
            }
            // Turns clockwise from twelve o'clock, as the slice is drawn.
            let turn = libm::atan2f(dx, -dy) / core::f32::consts::TAU;
            (turn - start).rem_euclid(1.0) <= end - start
        }
    }
}

/// A label's box, `[x, y, w, h]`, relative to the chart.
fn label_box(l: &Label) -> Rect {
    [l.origin[0], l.origin[1], l.text.width, l.text.height]
}

/// Whether `p` falls in `rect`, its edges included.
fn inside([x, y, w, h]: Rect, p: [f32; 2]) -> bool {
    p[0] >= x && p[0] <= x + w && p[1] >= y && p[1] <= y + h
}

/// The box around `path`'s points, its control points too.
fn bounds(path: &Path) -> Rect {
    let mut lo = [f32::INFINITY; 2];
    let mut hi = [f32::NEG_INFINITY; 2];
    let mut add = |p: [f32; 2]| {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    };
    for el in &path.0 {
        match *el {
            PathEl::MoveTo(a) | PathEl::LineTo(a) => add(a),
            PathEl::QuadTo(a, b) => [a, b].into_iter().for_each(&mut add),
            PathEl::CurveTo(a, b, c) => [a, b, c].into_iter().for_each(&mut add),
            PathEl::Close => {}
        }
    }
    if lo[0] > hi[0] {
        return [0.0; 4];
    }
    [lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::charts::RoundRect;

    #[test]
    fn a_point_holds_a_bar_a_dot_or_a_slice() {
        let bar = Shape::Bar(RoundRect { x: 10.0, y: 20.0, w: 30.0, h: 100.0, top_radius: 0.0, bottom_radius: 0.0 });
        assert!(holds(bar, [10.0, 20.0]) && holds(bar, [40.0, 120.0]) && !holds(bar, [41.0, 60.0]));
        // A bar at 0 is a hairline: pointed at within SLOP of it.
        let flat = Shape::Bar(RoundRect { x: 10.0, y: 120.0, w: 30.0, h: 0.0, top_radius: 0.0, bottom_radius: 0.0 });
        assert!(holds(flat, [20.0, 114.0]) && !holds(flat, [20.0, 113.0]));
        let dot = Shape::Dot { x: 100.0, y: 100.0, r: 2.0 };
        assert!(holds(dot, [104.0, 104.0]) && !holds(dot, [105.0, 105.0]), "a small dot within SLOP");
        assert!(!holds(Shape::Dot { x: 100.0, y: 100.0, r: 0.0 }, [100.0, 100.0]), "a line's point is near, not held");
        // A quarter ring from twelve o'clock to three.
        let slice = Shape::Arc { cx: 0.0, cy: 0.0, inner: 50.0, outer: 100.0, start: 0.0, end: 0.25 };
        assert!(holds(slice, [50.0, -50.0]) && !holds(slice, [-50.0, -50.0]) && !holds(slice, [50.0, 50.0]));
        assert!(!holds(slice, [10.0, -10.0]), "in the hole");
        // A slice that wraps past twelve.
        let wraps = Shape::Arc { cx: 0.0, cy: 0.0, inner: 0.0, outer: 100.0, start: 0.875, end: 1.125 };
        assert!(holds(wraps, [-10.0, -50.0]) && holds(wraps, [10.0, -50.0]) && !holds(wraps, [0.0, 50.0]));
    }

    #[test]
    fn a_value_rounds_to_its_step_as_a_person_writes_it() {
        assert_eq!(round_to(3123.4, 100.0), 3100.0);
        assert_eq!(round_to(0.234, 0.02), 0.24);
        assert_eq!(round_to(0.3, 0.1), 0.3, "not 0.30000000000000004");
        assert_eq!(round_to(-47.0, 5.0), -45.0);
        assert_eq!(round_to(5.0, 0.0), 5.0, "no step, no rounding");
    }

    #[test]
    fn a_path_is_bounded_by_its_points() {
        let ring = Shape::Dot { x: 10.0, y: 20.0, r: 5.0 }.path().unwrap();
        assert_eq!(bounds(&ring), [5.0, 15.0, 10.0, 10.0]);
        assert_eq!(bounds(&Path(Vec::new())), [0.0; 4]);
    }
}
