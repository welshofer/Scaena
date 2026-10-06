//! Guides (PLAN 2.57, ADR-0013): what an editor draws over the canvas to place things by.
//!
//! On request, the theme's grid in the format shown ([`grid`]): its columns and rows, the
//! gutters between them, the margins around them, and the baseline grid. And as a drag moves
//! or resizes a box, a line wherever one of its edges, or its middle, meets another box's or
//! the canvas's ([`meets`]); a box moved off the grid goes first the least way that brings
//! one of them onto another's, within reach ([`align`]).
//!
//! The boxes are as laid out, canvas units ([`around`]): what else draws in the state at rest,
//! each where it is drawn, and the canvas. A box a drag moves is where its placement puts it:
//! where its own transform, or what holds it, draws it elsewhere, an editor shows no guides.

use crate::EngineError;
use crate::geometry::NodeBox;
use crate::layout::Grid;
use crate::render::project;
use crate::theme::Theme;
use scaena_core::displaylist::Rect;
use scaena_core::document::Deck;

/// The theme's grid as a format lays it out (SPEC §3.4), canvas units.
#[derive(Debug, Clone, PartialEq)]
pub struct Guides {
    /// The canvas, `[width, height]`.
    pub canvas: [f32; 2],
    /// The grid's columns, each `[start, end]`, left to right: the gutters lie between them,
    /// and the margins between the canvas's edges and the first and the last.
    pub columns: Vec<[f32; 2]>,
    /// Its rows, each `[start, end]`, top to bottom.
    pub rows: Vec<[f32; 2]>,
    /// The baseline grid's lines, each a `y`, a pitch apart from the top margin to the bottom
    /// one, as text that snaps sits on them (SPEC §3.4); none where the theme sets no
    /// `grid.baseline`, or one so fine that more than [`BASELINES`] would fill the canvas.
    pub baselines: Vec<f32>,
}

/// The most baseline lines guides draw: a pitch finer than the canvas's height over this is
/// not one a person tells apart.
pub const BASELINES: usize = 2000;

/// The theme's grid as `deck` lays out in `format`: one of its `formats`, or `None` for its
/// own canvas.
pub fn grid(deck: &Deck, theme: &Theme, format: Option<&str>) -> Result<Guides, EngineError> {
    let (deck, theme) = project(deck, theme, format)?;
    let canvas = [deck.canvas.width as f32, deck.canvas.height as f32];
    let grid = Grid::from_theme(&theme, canvas)?;
    let (columns, rows) = (grid.columns(), grid.rows());
    let baselines = match (theme.grid.baseline.map(|p| p as f32), rows.first(), rows.last()) {
        (Some(pitch), Some(first), Some(last)) if pitch.is_finite() && pitch > 0.0 => {
            let (top, bottom) = (first[0], last[1]);
            let lines = ((bottom - top) / pitch).floor() as usize + 1;
            match lines <= BASELINES {
                // A whole number of pitches from the top margin, as a snapped text's lines are.
                true => (0..lines).map(|i| top + i as f32 * pitch).collect(),
                false => Vec::new(),
            }
        }
        _ => Vec::new(),
    };
    Ok(Guides { canvas, columns, rows, baselines })
}

/// A slot of a theme's layout as the canvas shows it (PLAN 2.71): its name, its box in the
/// format shown, its cells as the theme writes them, and whether that format writes it itself.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SlotBox {
    pub name: String,
    /// Canvas units.
    pub rect: Rect,
    /// `col` and `row` as the theme writes them, the format's own where it has one: what an
    /// edit changes. None where the slot spans the grid that way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<serde_json::Value>,
    /// The format shown writes this slot itself (`layouts.L.formats.F.slots`); on the deck's
    /// own canvas, always false.
    pub own: bool,
}

/// The layout state `state` uses, by its name, and its slots in the format shown (PLAN 2.71),
/// in the order the theme writes them. None for a state that names no layout.
pub fn layout(
    deck: &Deck,
    theme: &Theme,
    state: &str,
    format: Option<&str>,
) -> Result<Option<(String, Vec<SlotBox>)>, EngineError> {
    let snaps = scaena_core::resolve_states(deck).map_err(|e| EngineError::Layout(e.to_string()))?;
    let snap =
        snaps.iter().find(|s| s.state_id == state).ok_or_else(|| EngineError::UnknownState(state.to_string()))?;
    let Some(name) = snap.layout.clone() else { return Ok(None) };
    let own: Vec<String> = match format.and_then(scaena_core::model::Format::parse) {
        Some(f) => (theme.layouts.get(&name))
            .and_then(|l| l.formats.as_ref())
            .and_then(|fs| fs.get(&f))
            .map(|lf| lf.slots.keys().cloned().collect())
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let (projected, laid) = project(deck, theme, format)?;
    let canvas = [projected.canvas.width as f32, projected.canvas.height as f32];
    let grid = Grid::from_theme(&laid, canvas)?;
    let slots = laid.slots(&name).ok_or_else(|| EngineError::Theme(format!("layout `{name}` is not in the theme")))?;
    let boxes = slots
        .iter()
        .map(|(slot, def)| {
            let rect = grid.cell(&laid, Some(&name), Some(&serde_json::json!({ "in": slot })))?;
            let value =
                |r: &Option<scaena_core::model::values::Range>| r.as_ref().and_then(|r| serde_json::to_value(r).ok());
            Ok(SlotBox {
                name: slot.clone(),
                rect,
                col: value(&def.col),
                row: value(&def.row),
                own: own.contains(slot),
            })
        })
        .collect::<Result<_, EngineError>>()?;
    Ok(Some((name, boxes)))
}

/// A guide where boxes meet: `[x1, y1, x2, y2]`, canvas units, a line down or across.
pub type Line = [f32; 4];

/// How near one box's edge or middle must come to another's to meet it, canvas units. A box
/// placed off the grid stands on whole units, so one lined up with an edge on a fraction of
/// one meets it still.
pub const MEET: f32 = 0.5;

/// A box's stops across one axis (`0` for `x`, `1` for `y`): its start, middle, and end.
fn stops(rect: Rect, axis: usize) -> [f32; 3] {
    [rect[axis], rect[axis] + rect[axis + 2] / 2.0, rect[axis] + rect[axis + 2]]
}

/// What a box a drag moves may meet: the box of each node that draws (`boxes`, as
/// [`crate::sample::Scene::boxes`] gives them), where it is drawn (the box around it, where a
/// transform turns it), but `moving`'s and those of what they hold; then the canvas's.
pub fn around(boxes: &[NodeBox], canvas: [f32; 2], moving: &[&str]) -> Vec<Rect> {
    let mut out: Vec<Rect> = (boxes.iter().filter(|b| b.draws && !moves(boxes, moving, &b.node)))
        .map(|b| b.transform.map_or(b.rect, |map| bounds(b.rect, map)))
        .filter(|r| r.iter().all(|v| v.is_finite()))
        .collect();
    out.push([0.0, 0.0, canvas[0], canvas[1]]);
    out
}

/// Whether `id`, or anything that holds it, is one of `moving`.
fn moves(boxes: &[NodeBox], moving: &[&str], id: &str) -> bool {
    let mut at = Some(id);
    // No deeper than the boxes are many, whatever the parents say.
    for _ in 0..=boxes.len() {
        let Some(id) = at else { return false };
        if moving.contains(&id) {
            return true;
        }
        at = boxes.iter().find(|b| b.node == id).and_then(|b| b.parent.as_deref());
    }
    false
}

/// The box around `rect` drawn through `map` (`[a, b, c, d, e, f]`, `x' = a·x + c·y + e`).
fn bounds(rect: Rect, map: [f32; 6]) -> Rect {
    let [x, y, w, h] = rect;
    let [a, b, c, d, e, f] = map;
    let corners = [[x, y], [x + w, y], [x, y + h], [x + w, y + h]].map(|[x, y]| [a * x + c * y + e, b * x + d * y + f]);
    let low = |i: usize| corners.iter().map(|p| p[i]).fold(f32::INFINITY, f32::min);
    let high = |i: usize| corners.iter().map(|p| p[i]).fold(f32::NEG_INFINITY, f32::max);
    [low(0), low(1), high(0) - low(0), high(1) - low(1)]
}

/// The box around `boxes`; `None` for none.
pub fn union(boxes: &[Rect]) -> Option<Rect> {
    let first = boxes.first()?;
    let mut out = [first[0], first[1], first[0] + first[2], first[1] + first[3]];
    for b in &boxes[1..] {
        out = [out[0].min(b[0]), out[1].min(b[1]), out[2].max(b[0] + b[2]), out[3].max(b[1] + b[3])];
    }
    Some([out[0], out[1], out[2] - out[0], out[3] - out[1]])
}

/// The least way, within `reach`, from one of `mine` to one of `others`' stops on `axis`:
/// the first found of the nearest.
fn nearest(others: &[Rect], axis: usize, mine: &[f32], reach: f32) -> Option<f32> {
    let mut best: Option<f32> = None;
    for at in others.iter().flat_map(|o| stops(*o, axis)) {
        for m in mine {
            let way = at - m;
            if way.abs() <= reach && best.is_none_or(|b| way.abs() < b.abs()) {
                best = Some(way);
            }
        }
    }
    best
}

/// `cell`, as a drag made it of `from`, gone on each axis the least way that brings an edge,
/// or its middle, within `reach` onto one of `others`'. Moved (the same size as `from`), the
/// box goes whole; resized, each edge the drag moved goes alone, and the box keeps a canvas
/// unit at least. Where nothing is within reach on an axis, it stays as the drag left it.
pub fn align(cell: Rect, from: Rect, others: &[Rect], reach: f32) -> Rect {
    if !(reach > 0.0 && cell.iter().chain(&from).all(|v| v.is_finite())) {
        return cell;
    }
    let moved = (cell[2] - from[2]).abs() < 1e-3 && (cell[3] - from[3]).abs() < 1e-3;
    let mut out = cell;
    for axis in 0..2 {
        let [start, _, end] = stops(cell, axis);
        if moved {
            if let Some(way) = nearest(others, axis, &stops(cell, axis), reach) {
                out[axis] += way;
            }
            continue;
        }
        let [was, _, was_end] = stops(from, axis);
        let go = |at: f32, was: f32| match (at - was).abs() > 1e-3 {
            true => at + nearest(others, axis, &[at], reach).unwrap_or(0.0),
            false => at,
        };
        let (start, end) = (go(start, was), go(end, was_end));
        if end - start >= 1.0 {
            (out[axis], out[axis + 2]) = (start, end - start);
        }
    }
    out
}

/// The guides where an edge or the middle of `cell` meets one of `others`' (within [`MEET`]):
/// each where the other's is, drawn across both boxes, one line for all that meet at one place;
/// those down the canvas, left to right, then those across it, top to bottom.
pub fn meets(cell: Rect, others: &[Rect]) -> Vec<Line> {
    let mut lines = Vec::new();
    if !cell.iter().all(|v| v.is_finite()) {
        return lines;
    }
    for axis in 0..2 {
        let across = 1 - axis;
        let mine = stops(cell, axis);
        // Each place a line meets, and how far across it runs.
        let mut found: Vec<[f32; 3]> = Vec::new();
        for other in others {
            let (from, to) = (cell[across].min(other[across]), stops(cell, across)[2].max(stops(*other, across)[2]));
            for at in stops(*other, axis) {
                if !mine.iter().any(|m| (m - at).abs() <= MEET) {
                    continue;
                }
                match found.iter_mut().find(|f| (f[0] - at).abs() <= MEET) {
                    Some(f) => (f[1], f[2]) = (f[1].min(from), f[2].max(to)),
                    None => found.push([at, from, to]),
                }
            }
        }
        scaena_core::sort::by(&mut found, |a, b| a[0].total_cmp(&b[0]));
        lines.extend(found.into_iter().map(|[at, from, to]| match axis {
            0 => [at, from, at, to],
            _ => [from, at, to, at],
        }));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANVAS: Rect = [0.0, 0.0, 1920.0, 1080.0];

    #[test]
    fn a_box_moved_goes_the_least_way_onto_an_edge_or_a_middle_within_reach() {
        let title = [160.0, 120.0, 800.0, 200.0];
        let others = [title, CANVAS];
        let from = [600.0, 500.0, 300.0, 100.0];
        // Its left edge 4 units right of the title's: it goes the 4.
        assert_eq!(align([164.0, 503.0, 300.0, 100.0], from, &others, 6.0), [160.0, 503.0, 300.0, 100.0]);
        // Its middle 3 units from the canvas's: the middle meets the canvas's, not the far edge.
        assert_eq!(align([807.0, 503.0, 300.0, 100.0], from, &others, 6.0), [810.0, 503.0, 300.0, 100.0]);
        // Out of reach on both axes, it stays where the drag left it.
        assert_eq!(align([180.0, 600.0, 300.0, 100.0], from, &others, 6.0), [180.0, 600.0, 300.0, 100.0]);
        // Its bottom edge 2 above the title's top: onto it, the box whole.
        assert_eq!(align([1200.0, 18.0, 300.0, 100.0], from, &others, 6.0), [1200.0, 20.0, 300.0, 100.0]);
        // No reach, no aligning.
        assert_eq!(align([164.0, 503.0, 300.0, 100.0], from, &others, 0.0), [164.0, 503.0, 300.0, 100.0]);
    }

    #[test]
    fn a_box_resized_moves_only_the_edges_the_drag_moved() {
        let title = [160.0, 120.0, 800.0, 200.0];
        let others = [title, CANVAS];
        let from = [400.0, 500.0, 300.0, 100.0];
        // Its right edge dragged to 955: to the title's right edge at 960; its left edge stays,
        // though the canvas's middle is no farther from it than reach.
        assert_eq!(align([400.0, 500.0, 555.0, 100.0], from, &others, 6.0), [400.0, 500.0, 560.0, 100.0]);
        // Its left edge dragged to 163, its right staying: onto the title's left edge.
        assert_eq!(align([163.0, 500.0, 537.0, 100.0], from, &others, 6.0), [160.0, 500.0, 540.0, 100.0]);
        // Its corner dragged: right edge and bottom edge each to the nearest within reach.
        let corner = align([400.0, 500.0, 562.0, 577.0], from, &others, 6.0);
        assert_eq!(corner, [400.0, 500.0, 560.0, 580.0]);
        // An edge that would leave the box less than a canvas unit wide stays where the drag
        // left it.
        let thin = [400.0, 500.0, 3.0, 100.0];
        assert_eq!(align(thin, from, &[[400.2, 0.0, 0.0, 10.0]], 6.0), thin);
    }

    #[test]
    fn guides_run_across_both_boxes_where_edges_and_middles_meet() {
        let title = [160.0, 120.0, 800.0, 200.0];
        let others = [title, CANVAS];
        // Its left edge on the title's, a fraction off (a rect stands on whole units): one line
        // down from the title's top to the box's bottom; its middle on the canvas's middle
        // across: one line across the canvas.
        let lines = meets([160.4, 490.0, 300.0, 100.0], &others);
        assert_eq!(lines, [[160.0, 120.0, 160.0, 590.0], [0.0, 540.0, 1920.0, 540.0]]);
        // Two boxes meeting at one place make one line, across all three.
        let card = [160.0, 700.0, 400.0, 100.0];
        let lines = meets([160.0, 400.0, 300.0, 100.0], &[title, card]);
        assert_eq!(lines, [[160.0, 120.0, 160.0, 800.0]]);
        // Nothing meets: no guides.
        assert!(meets([171.0, 401.0, 301.0, 101.0], &[title]).is_empty());
    }

    #[test]
    fn a_box_drawn_turned_is_met_by_the_box_around_it() {
        // A square turned a quarter about the origin: x' = -y, y' = x.
        let quarter = bounds([10.0, 20.0, 30.0, 40.0], [0.0, 1.0, -1.0, 0.0, 0.0, 0.0]);
        assert_eq!(quarter, [-60.0, 10.0, 40.0, 30.0]);
    }
}
