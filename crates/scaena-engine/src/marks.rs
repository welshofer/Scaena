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

use crate::charts::{ChartLayout, Label, Shape};
use crate::geometry::{SLOP, reaches};
use crate::sample::{Content, Scene};
use crate::tables::TableLayout;
use scaena_core::displaylist::{Path, PathEl, Rect};

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
        }
    }
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
    fn a_path_is_bounded_by_its_points() {
        let ring = Shape::Dot { x: 10.0, y: 20.0, r: 5.0 }.path().unwrap();
        assert_eq!(bounds(&ring), [5.0, 15.0, 10.0, 10.0]);
        assert_eq!(bounds(&Path(Vec::new())), [0.0; 4]);
    }
}
