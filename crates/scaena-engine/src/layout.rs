//! Layout (SPEC §3.4): the theme grid, its templates and slots, and the typographic
//! alignment anchors (cap / baseline / x-height).
//!
//! A node outside any container, or in a group, is placed on the theme grid:
//! `at: { in: slot | canvas | grid }`, `at: { col, row }`, or the `rect` override, with
//! `inset` and `offset`. Containers lay their children out themselves
//! ([`crate::containers`]).
//!
//! Layout is per snapshot, never per frame; frames only sample (SPEC §5).

use crate::EngineError;
use crate::theme::Theme;
use scaena_core::displaylist::Rect;
use scaena_core::document::Props;
use scaena_core::model::theme::{Margin, Slot};
use scaena_core::model::values::Range;
use serde_json::Value;

/// The theme grid resolved against the canvas, in canvas units.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    canvas: [f32; 2],
    /// `(start, end)` of each column, left to right.
    cols: Vec<(f32, f32)>,
    /// `(start, end)` of each row, top to bottom.
    rows: Vec<(f32, f32)>,
}

impl Grid {
    /// Build the grid from `theme.grid`: `columns`, `rows` (default 6), `gutter`, and a
    /// `margin` with CSS shorthand semantics (one value, or vertical/horizontal, or
    /// top/horizontal/bottom, or top/right/bottom/left).
    pub fn from_theme(theme: &Theme, canvas: [f32; 2]) -> Result<Grid, EngineError> {
        let grid = &theme.grid;
        let count = |key: &str, n: u32| -> Result<usize, EngineError> {
            (n >= 1).then_some(n as usize).ok_or_else(|| EngineError::Theme(format!("grid.{key} must be ≥ 1")))
        };
        let (columns, rows) = (count("columns", grid.columns)?, count("rows", grid.rows.unwrap_or(6))?);
        let gutter = grid.gutter as f32;
        let margin: Vec<f32> = match &grid.margin {
            Margin::All(all) => vec![*all as f32],
            Margin::Sides(sides) => sides.iter().map(|v| v.0 as f32).collect(),
        };
        let [top, right, bottom, left] = match margin[..] {
            [all] => [all; 4],
            [v, h] => [v, h, v, h],
            [t, h, b] => [t, h, b, h],
            [t, r, b, l] => [t, r, b, l],
            _ => return Err(EngineError::Theme("grid.margin takes 1 to 4 values".into())),
        };
        let tracks = |start: f32, extent: f32, n: usize| -> Vec<(f32, f32)> {
            let size = (extent - gutter * (n - 1) as f32) / n as f32;
            (0..n).map(|i| start + i as f32 * (size + gutter)).map(|s| (s, s + size)).collect()
        };
        Ok(Grid {
            canvas,
            cols: tracks(left, canvas[0] - left - right, columns),
            rows: tracks(top, canvas[1] - top - bottom, rows),
        })
    }

    /// The box `at` names, in a state whose layout template is `template`.
    pub fn place(&self, theme: &Theme, template: Option<&str>, at: Option<&Value>) -> Result<Rect, EngineError> {
        let Some(at) = at else { return Ok(self.margin_box()) };
        if at.get("area").is_some() {
            return Err(EngineError::Layout("`at.area` names an area of a grid container".into()));
        }
        let mut rect = if let Some(rect) = at.get("rect") {
            let v: Vec<f32> =
                rect.as_array().into_iter().flatten().filter_map(Value::as_f64).map(|v| v as f32).collect();
            <[f32; 4]>::try_from(v)
                .map_err(|_| EngineError::Layout(format!("`rect` needs [x, y, w, h], got {rect}")))?
        } else if let Some(slot) = at.get("in").and_then(Value::as_str) {
            match slot {
                "canvas" => [0.0, 0.0, self.canvas[0], self.canvas[1]],
                "grid" => self.margin_box(),
                name => {
                    let template = template.ok_or_else(|| {
                        EngineError::Layout(format!("`in: {name}` needs the state to have a layout template"))
                    })?;
                    let slots = theme.slots(template).ok_or_else(|| {
                        EngineError::Layout(format!("layout template `{template}` is not in the theme"))
                    })?;
                    let slot = slots.get(name).ok_or_else(|| {
                        EngineError::Layout(format!("slot `{name}` is not in layout template `{template}`"))
                    })?;
                    let range =
                        |r: &Option<Range>| r.as_ref().map(|r| serde_json::to_value(r).expect("a range is JSON"));
                    self.cells(range(&slot.col).as_ref(), range(&slot.row).as_ref())?
                }
            }
        } else {
            self.cells(at.get("col"), at.get("row"))?
        };
        if let Some(inset) = at.get("inset").and_then(Value::as_f64) {
            let inset = inset as f32;
            rect = [rect[0] + inset, rect[1] + inset, rect[2] - 2.0 * inset, rect[3] - 2.0 * inset];
        }
        if let Some([dx, dy]) = at.get("offset").and_then(|o| serde_json::from_value::<[f32; 2]>(o.clone()).ok()) {
            rect = [rect[0] + dx, rect[1] + dy, rect[2], rect[3]];
        }
        Ok(rect)
    }

    /// The slot definition `at` names in layout template `template`, if it names one.
    pub fn slot<'t>(theme: &'t Theme, template: Option<&str>, at: Option<&Value>) -> Option<&'t Slot> {
        let name = at?.get("in")?.as_str()?;
        theme.slots(template?)?.get(name)
    }

    /// The default text role a slot gives nodes placed in it.
    pub fn slot_role(theme: &Theme, template: Option<&str>, at: Option<&Value>) -> Option<String> {
        Self::slot(theme, template, at)?.role.clone()
    }

    /// How a node aligns in its cell: the slot's default, then the node's `align`, then
    /// `at.align`, later wins; `start` on both axes when nobody says.
    pub fn alignment(theme: &Theme, template: Option<&str>, props: &Props) -> Result<(AlignX, AlignY), EngineError> {
        let at = props.get("at");
        let slot = Self::slot(theme, template, at)
            .and_then(|s| s.align.as_ref())
            .map(|a| serde_json::to_value(a).expect("an alignment is JSON"));
        let sources = [slot.as_ref(), props.get("align"), at.and_then(|a| a.get("align"))];
        let (mut x, mut y) = (AlignX::Start, AlignY::Start);
        for value in sources.into_iter().flatten() {
            match value {
                // One keyword aligns both axes; the typographic anchors are vertical only.
                Value::String(k) => (x, y) = (AlignX::parse(k)?, AlignY::parse(k)?),
                Value::Object(o) => {
                    if let Some(k) = o.get("x").and_then(Value::as_str) {
                        x = AlignX::parse(k)?;
                    }
                    if let Some(k) = o.get("y").and_then(Value::as_str) {
                        y = AlignY::parse(k)?;
                    }
                }
                other => {
                    return Err(EngineError::Layout(format!("`align` must be a keyword or {{x, y}}, got {other}")));
                }
            }
        }
        Ok((x, y))
    }

    fn margin_box(&self) -> Rect {
        let (x0, x1) = (self.cols[0].0, self.cols[self.cols.len() - 1].1);
        let (y0, y1) = (self.rows[0].0, self.rows[self.rows.len() - 1].1);
        [x0, y0, x1 - x0, y1 - y0]
    }

    /// Cells spanned by grid ranges (1-based, inclusive; a missing range spans every track).
    fn cells(&self, col: Option<&Value>, row: Option<&Value>) -> Result<Rect, EngineError> {
        let (x0, x1) = span(&self.cols, col, "col")?;
        let (y0, y1) = span(&self.rows, row, "row")?;
        Ok([x0, y0, x1 - x0, y1 - y0])
    }
}

/// Horizontal alignment in a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignX {
    Start,
    Center,
    End,
    Stretch,
}

impl AlignX {
    fn parse(keyword: &str) -> Result<AlignX, EngineError> {
        Ok(match keyword {
            "start" => AlignX::Start,
            "center" => AlignX::Center,
            "end" => AlignX::End,
            "stretch" => AlignX::Stretch,
            other => return Err(EngineError::Layout(format!("`{other}` is not a horizontal alignment"))),
        })
    }
}

/// Vertical alignment in a cell, including the typographic anchors (SPEC §3.4):
/// `cap` and `x-height` put the first line's cap or x-height on the cell's top edge;
/// `baseline` puts the last line's baseline on the cell's bottom edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignY {
    Start,
    Center,
    End,
    Stretch,
    Cap,
    Baseline,
    XHeight,
}

impl AlignY {
    fn parse(keyword: &str) -> Result<AlignY, EngineError> {
        Ok(match keyword {
            "start" => AlignY::Start,
            "center" => AlignY::Center,
            "end" => AlignY::End,
            "stretch" => AlignY::Stretch,
            "cap" => AlignY::Cap,
            "baseline" => AlignY::Baseline,
            "x-height" => AlignY::XHeight,
            other => return Err(EngineError::Layout(format!("`{other}` is not a vertical alignment"))),
        })
    }
}

fn span(tracks: &[(f32, f32)], range: Option<&Value>, axis: &str) -> Result<(f32, f32), EngineError> {
    let (a, b) = match range {
        None => (1, tracks.len() as u64),
        Some(Value::Number(n)) => (n.as_u64().unwrap_or(0), n.as_u64().unwrap_or(0)),
        Some(Value::Array(v)) if v.len() == 2 => (v[0].as_u64().unwrap_or(0), v[1].as_u64().unwrap_or(0)),
        Some(other) => return Err(EngineError::Layout(format!("`{axis}` must be n or [a, b], got {other}"))),
    };
    if a < 1 || b < a || b as usize > tracks.len() {
        return Err(EngineError::Layout(format!("`{axis}` [{a}, {b}] is outside the grid's {} tracks", tracks.len())));
    }
    Ok((tracks[a as usize - 1].0, tracks[b as usize - 1].1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn torture() -> Theme {
        Theme::from_json(include_str!("../../../tests/fixtures/torture.scaena/theme.json")).unwrap()
    }

    #[test]
    fn columns_and_rows_follow_margin_and_gutter() {
        let grid = Grid::from_theme(&torture(), [1920.0, 1080.0]).unwrap();
        // 12 columns in 1920 - 2·96 with 24 gutters: 122 each; 8 rows in 1080 - 2·96: 90 each.
        assert_eq!(grid.cols[0], (96.0, 218.0));
        assert_eq!(grid.cols[11].1, 1824.0);
        assert_eq!(grid.rows[1], (210.0, 300.0));
        let t = torture();
        assert_eq!(
            grid.place(&t, None, Some(&json!({"col": [1, 12], "row": [2, 3]}))).unwrap(),
            [96.0, 210.0, 1728.0, 204.0]
        );
        assert_eq!(grid.place(&t, None, Some(&json!({"col": [1, 6]}))).unwrap(), [96.0, 96.0, 852.0, 888.0]);
        assert_eq!(grid.place(&t, None, Some(&json!({"in": "canvas"}))).unwrap(), [0.0, 0.0, 1920.0, 1080.0]);
        assert_eq!(grid.place(&t, Some("specimen"), Some(&json!({"in": "case"}))).unwrap(), [96.0, 96.0, 1728.0, 90.0]);
        assert_eq!(Grid::slot_role(&t, Some("specimen"), Some(&json!({"in": "case"}))).as_deref(), Some("label"));
    }

    #[test]
    fn css_margin_shorthand() {
        let dusk = Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        let grid = Grid::from_theme(&dusk, [1920.0, 1080.0]).unwrap();
        // Dusk: margin [96, 120] = 96 top/bottom, 120 left/right.
        assert_eq!((grid.cols[0].0, grid.rows[0].0), (120.0, 96.0));
        assert_eq!((grid.cols[11].1, grid.rows[5].1), (1800.0, 984.0));
    }

    #[test]
    fn bad_placements_say_why() {
        let t = torture();
        let grid = Grid::from_theme(&t, [1920.0, 1080.0]).unwrap();
        let err = |at: Value, template| grid.place(&t, template, Some(&at)).unwrap_err().to_string();
        assert!(err(json!({"col": [0, 3]}), None).contains("outside the grid"));
        assert!(err(json!({"in": "nope"}), Some("specimen")).contains("slot `nope`"));
        assert!(err(json!({"in": "case"}), None).contains("needs the state to have a layout template"));
        assert!(err(json!({"area": "head"}), None).contains("grid container"));
        // A group's child is placed on the slide by the rest of its `at`.
        assert_eq!(
            grid.place(&t, None, Some(&json!({"parent": "g", "in": "canvas"}))).unwrap(),
            [0.0, 0.0, 1920.0, 1080.0]
        );
    }
}
