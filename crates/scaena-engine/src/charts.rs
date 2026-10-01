//! Charts → marks (SPEC §3.7). A chart spec never stores pixels; it compiles, once per
//! snapshot, to marks keyed by data so a transition can morph mark by mark (SPEC §2.3).
//!
//! Phase 0 (PLAN 0.10) compiles `bar` and `line` with one series. Every data mark is a
//! [`RoundRect`]: a bar is a rectangle square on the baseline and rounded by the
//! theme's corner radius at its free end, and a line's point is a circle, a rounded
//! rectangle whose radii are half its side. A change of kind then interpolates six
//! numbers per mark. The other kinds, multiple series,
//! color encodings, legends, axes settings, number formats, and data transforms are
//! PLAN 1.9 and return `NotImplemented`.
//!
//! Geometry is relative to the chart's cell. It uses only `+ − × ÷`, never `sin` or
//! `cos`, whose last bits differ between platform math libraries, so chart display
//! lists stay bit-identical across platforms (SPEC §13).

use crate::EngineError;
use crate::data::{self, DataFiles, Datum};
use crate::fonts::BundleFonts;
use crate::text::{Span, TextEngine, TextLayout, TextSpec};
use crate::theme::{Numeric, TextBox, Theme};
use scaena_core::Deck;
use scaena_core::displaylist::{Color, Path, PathEl};
use scaena_core::document::Props;
use serde_json::Value;
use std::collections::BTreeSet;

/// Bézier handle length for a quarter circle of radius 1: 4/3 · (√2 − 1).
const KAPPA: f32 = 0.552_284_8;
/// A point's radius, in multiples of the line's stroke width.
const POINT_RADIUS: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Bar,
    Line,
}

/// A rectangle with one radius for its top corners and one for its bottom corners: a
/// bar (rounded only at its free end), or a point when both radii are half its side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub top_radius: f32,
    pub bottom_radius: f32,
}

impl RoundRect {
    /// The shape `p` of the way from `a` to `b`.
    pub fn lerp(a: RoundRect, b: RoundRect, p: f32) -> RoundRect {
        RoundRect {
            x: lerp(a.x, b.x, p),
            y: lerp(a.y, b.y, p),
            w: lerp(a.w, b.w, p),
            h: lerp(a.h, b.h, p),
            top_radius: lerp(a.top_radius, b.top_radius, p),
            bottom_radius: lerp(a.bottom_radius, b.bottom_radius, p),
        }
    }

    pub fn top(&self) -> f32 {
        self.y
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn center_x(&self) -> f32 {
        self.x + 0.5 * self.w
    }

    /// Clockwise from the top edge; rounded corners are cubic quarter circles, and a
    /// corner with no radius is a plain corner.
    pub fn path(&self) -> Path {
        let RoundRect { x, y, w, h, top_radius: t, bottom_radius: b } = *self;
        if t <= 0.0 && b <= 0.0 {
            return Path::rect([x, y, w, h]);
        }
        let (x1, y1, kt, kb) = (x + w, y + h, KAPPA * t, KAPPA * b);
        let mut els = vec![PathEl::MoveTo([x + t, y]), PathEl::LineTo([x1 - t, y])];
        if t > 0.0 {
            els.push(PathEl::CurveTo([x1 - t + kt, y], [x1, y + t - kt], [x1, y + t]));
        }
        els.push(PathEl::LineTo([x1, y1 - b]));
        if b > 0.0 {
            els.push(PathEl::CurveTo([x1, y1 - b + kb], [x1 - b + kb, y1], [x1 - b, y1]));
        }
        els.push(PathEl::LineTo([x + b, y1]));
        if b > 0.0 {
            els.push(PathEl::CurveTo([x + b - kb, y1], [x, y1 - b + kb], [x, y1 - b]));
        }
        els.push(PathEl::LineTo([x, y + t]));
        if t > 0.0 {
            els.push(PathEl::CurveTo([x, y + t - kt], [x + t - kt, y], [x + t, y]));
        }
        els.push(PathEl::Close);
        Path(els)
    }
}

/// `a` at `p = 0`, `b` at `p = 1`, exactly.
pub fn lerp(a: f32, b: f32, p: f32) -> f32 {
    a * (1.0 - p) + b * p
}

/// One datum's mark.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// Identity across states: the chart's `key` field.
    pub key: String,
    pub shape: RoundRect,
    pub color: Color,
}

/// A line through one series' points, in data order.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub key: String,
    pub points: Vec<[f32; 2]>,
    pub width: f32,
    pub color: Color,
}

impl Series {
    pub fn path(&self) -> Path {
        let mut els = Vec::with_capacity(self.points.len());
        for (i, &p) in self.points.iter().enumerate() {
            els.push(if i == 0 { PathEl::MoveTo(p) } else { PathEl::LineTo(p) });
        }
        Path(els)
    }
}

/// Text inside a chart: a value label (keyed like its mark) or a category label.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub key: String,
    /// The text's top-left corner, relative to the chart.
    pub origin: [f32; 2],
    pub text: TextLayout,
}

/// A straight hairline: the axis baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub width: f32,
    pub color: Color,
}

/// A chart compiled for one snapshot, relative to its cell. Painted bottom to top:
/// baseline, lines, marks, category labels, value labels.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartLayout {
    pub kind: ChartKind,
    pub baseline: Option<Rule>,
    pub series: Vec<Series>,
    pub marks: Vec<Mark>,
    /// Category labels under the plot, keyed by category.
    pub ticks: Vec<Label>,
    /// Value labels, keyed like their marks.
    pub labels: Vec<Label>,
}

/// What the compiler needs from the engine: text layout for the labels.
pub struct Ctx<'a> {
    pub text: &'a mut TextEngine,
    pub fonts: &'a mut BundleFonts,
    pub theme: &'a Theme,
    pub deck: &'a Deck,
    pub data: &'a DataFiles,
}

/// Compile a chart node's resolved props for a cell `size` wide and high.
pub fn compile(cx: &mut Ctx, props: &Props, size: [f32; 2]) -> Result<ChartLayout, EngineError> {
    let kind = match props.get("kind").and_then(Value::as_str) {
        Some("bar") => ChartKind::Bar,
        Some("line") => ChartKind::Line,
        Some("stackedBar" | "area" | "scatter" | "dot" | "donut") => {
            return Err(EngineError::NotImplemented("chart kinds other than bar and line — PLAN 1.9"));
        }
        other => return Err(EngineError::Layout(format!("unknown chart kind {other:?}"))),
    };
    for (key, task) in [
        ("series", "multi-series charts — PLAN 1.9"),
        ("color", "chart color encodings — PLAN 1.9"),
        ("transform", "chart data transforms — PLAN 1.9"),
        ("annotations", "chart annotations — PLAN 1.9"),
        ("axes", "chart axes settings — PLAN 1.9"),
    ] {
        if props.contains_key(key) {
            return Err(EngineError::NotImplemented(task));
        }
    }
    if props.get("legend").is_some_and(|l| l.as_str() != Some("none")) {
        return Err(EngineError::NotImplemented("chart legends — PLAN 1.9"));
    }
    let encoding = |axis: &str| -> Result<&serde_json::Map<String, Value>, EngineError> {
        props.get(axis).and_then(Value::as_object).ok_or_else(|| EngineError::Layout(format!("chart has no `{axis}`")))
    };
    let (x, y) = (encoding("x")?, encoding("y")?);
    if matches!(x.get("type").and_then(Value::as_str), Some("quantitative" | "temporal")) {
        return Err(EngineError::NotImplemented("continuous x scales — PLAN 1.9"));
    }
    if y.contains_key("format") {
        return Err(EngineError::NotImplemented("number formats — PLAN 1.9"));
    }
    let field =
        |e: &serde_json::Map<String, Value>| e.get("field").and_then(Value::as_str).unwrap_or_default().to_string();
    let (x_field, y_field) = (field(x), field(y));
    let key_field = props.get("key").and_then(Value::as_str).map_or_else(|| x_field.clone(), str::to_string);

    // Data: one row per category, in source order.
    let source = props.get("data").and_then(Value::as_str).and_then(|d| d.strip_prefix('@'));
    let table = data::load(cx.deck, cx.data, source.ok_or_else(|| EngineError::Layout("chart has no `data`".into()))?)?;
    let col = |name: &str| table.column(name).ok_or_else(|| EngineError::Data(format!("no column `{name}`")));
    let (xc, yc, kc) = (col(&x_field)?, col(&y_field)?, col(&key_field)?);
    let mut rows: Vec<(String, f64, String)> = Vec::with_capacity(table.rows.len());
    let mut seen = BTreeSet::new();
    for row in &table.rows {
        let Datum::Number(v) = row[yc] else {
            return Err(EngineError::Data(format!("`{y_field}` must be a number in every row")));
        };
        let key = row[kc].label();
        if !seen.insert(key.clone()) {
            return Err(EngineError::Data(format!("key `{key}` repeats; chart keys must be unique")));
        }
        rows.push((row[xc].label(), v, key));
    }
    if rows.is_empty() {
        return Err(EngineError::Data("chart data has no rows".into()));
    }

    // Theme: chart styles, all tokens.
    let charts = cx.theme.raw.get("charts").cloned().unwrap_or(Value::Null);
    let style = |path: &[&str]| path.iter().try_fold(&charts, |v, k| v.get(*k));
    let stroke = |name: &str| {
        cx.theme
            .raw
            .pointer(&format!("/tokens/stroke/{name}"))
            .and_then(Value::as_f64)
            .map(|w| w as f32)
            .ok_or_else(|| EngineError::Theme(format!("unknown stroke token `{name}`")))
    };
    let color = |name: &str| -> Result<Color, EngineError> {
        let hex = cx.theme.color(name).ok_or_else(|| EngineError::Theme(format!("unknown color `{name}`")))?;
        Color::from_hex(hex).map_err(|e| EngineError::Theme(e.to_string()))
    };
    let line_width = stroke(style(&["strokeWidth"]).and_then(Value::as_str).unwrap_or("thin"))?;
    let axis_width = stroke(style(&["axis", "stroke"]).and_then(Value::as_str).unwrap_or("hairline"))?;
    let axis_color = color(style(&["axis", "color"]).and_then(Value::as_str).unwrap_or("onSurfaceMuted"))?;
    let corner = style(&["cornerRadius"]).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let bar_gap = style(&["barGap"]).and_then(Value::as_f64).unwrap_or(0.2) as f32;
    let ink = cx
        .theme
        .raw
        .pointer("/tokens/data/categorical/0")
        .and_then(Value::as_str)
        .ok_or_else(|| EngineError::Theme("charts need tokens.data.categorical".into()))?;
    let ink = Color::from_hex(ink).map_err(|e| EngineError::Theme(e.to_string()))?;
    let gap = cx.theme.raw.pointer("/tokens/space/unit").and_then(Value::as_f64).unwrap_or(8.0) as f32;
    let labels = props.get("labels");
    let label_role = labels
        .and_then(|l| l.get("role"))
        .or_else(|| style(&["label", "role"]))
        .and_then(Value::as_str)
        .unwrap_or("label")
        .to_string();
    let tick_role = style(&["axis", "role"]).and_then(Value::as_str).unwrap_or("label").to_string();
    let show = labels.and_then(|l| l.get("show")).and_then(Value::as_str).unwrap_or("none");
    let labelled = |i: usize| match show {
        "all" => Ok(true),
        "ends" => Ok(i == 0 || i + 1 == rows.len()),
        "none" => Ok(false),
        other => Err(EngineError::Layout(format!("labels.show `{other}`: expected all, ends, or none"))),
    };

    // Text first: the plot is what the labels leave.
    let mut set = |text: String, role: &str| -> Result<TextLayout, EngineError> {
        let spec = TextSpec {
            spans: vec![Span { text, role: role.to_string() }],
            role: role.to_string(),
            numeric: Some(Numeric::TabularLining),
            ..TextSpec::default()
        };
        cx.text.layout(cx.fonts, cx.theme, &spec, f32::INFINITY)
    };
    let mut values = Vec::new();
    for (i, (_, v, _)) in rows.iter().enumerate() {
        values.push(if labelled(i)? { Some(set(data::format_number(*v), &label_role)?) } else { None });
    }
    let ticks: Vec<TextLayout> = rows.iter().map(|(c, ..)| set(c.clone(), &tick_role)).collect::<Result<_, _>>()?;
    // Cap height above the baseline; line-box height below the cap top.
    let cap = |t: &TextLayout| t.lines.first().map_or(0.0, |l| l.cap_height.unwrap_or(l.ascent));
    let below_cap = |t: &TextLayout| t.lines.first().map_or(0.0, |l| t.height - (l.baseline - cap(t)));
    let radius = POINT_RADIUS * line_width;
    let label_room = values.iter().flatten().map(cap).fold(0.0_f32, f32::max);
    let top = radius + if label_room > 0.0 { label_room + gap } else { 0.0 };
    let bottom = size[1] - gap - ticks.iter().map(below_cap).fold(0.0_f32, f32::max);
    if bottom - top <= 0.0 {
        return Err(EngineError::Layout(format!("chart cell {}×{} cu leaves no room to plot", size[0], size[1])));
    }

    // Scales: bands across, values up from the domain's low end.
    let domain = y.get("domain").and_then(Value::as_array);
    let bound = |i: usize| domain.and_then(|d| d.get(i)).and_then(Value::as_f64);
    let (min, max) = rows.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), r| (lo.min(r.1), hi.max(r.1)));
    let lo = bound(0).unwrap_or(min.min(0.0));
    let hi = bound(1).unwrap_or(max).max(lo + f64::EPSILON);
    let to_y = |v: f64| (f64::from(bottom) - (v - lo) / (hi - lo) * f64::from(bottom - top)) as f32;
    let base = to_y(0.0_f64.clamp(lo, hi));
    let band = size[0] / rows.len() as f32;

    let mut out = ChartLayout {
        kind,
        baseline: Some(Rule { from: [0.0, base], to: [size[0], base], width: axis_width, color: axis_color }),
        series: Vec::new(),
        marks: Vec::with_capacity(rows.len()),
        ticks: Vec::with_capacity(rows.len()),
        labels: Vec::new(),
    };
    let mut points = Vec::with_capacity(rows.len());
    for (i, ((_, v, key), (value, tick))) in rows.iter().zip(values.into_iter().zip(ticks)).enumerate() {
        let center = (i as f32 + 0.5) * band;
        let at = to_y(*v);
        let shape = match kind {
            ChartKind::Bar => {
                // Square on the baseline, rounded at the free end.
                let (w, top, bottom) = (band * (1.0 - bar_gap), at.min(base), at.max(base));
                let r = corner.min(0.5 * w).min(bottom - top);
                let (rt, rb) = if at <= base { (r, 0.0) } else { (0.0, r) };
                RoundRect { x: center - 0.5 * w, y: top, w, h: bottom - top, top_radius: rt, bottom_radius: rb }
            }
            ChartKind::Line => RoundRect {
                x: center - radius,
                y: at - radius,
                w: 2.0 * radius,
                h: 2.0 * radius,
                top_radius: radius,
                bottom_radius: radius,
            },
        };
        points.push([center, at]);
        if let Some(text) = value {
            // Above the mark for values at or over the baseline, below it otherwise.
            let first = &text.lines[0];
            let origin = if at <= base {
                [center - 0.5 * text.width, shape.top() - gap - first.baseline]
            } else {
                [center - 0.5 * text.width, shape.bottom() + gap - text.trimmed(TextBox::Cap).0]
            };
            out.labels.push(Label { key: key.clone(), origin, text });
        }
        let origin = [center - 0.5 * tick.width, bottom + gap - tick.trimmed(TextBox::Cap).0];
        out.ticks.push(Label { key: rows[i].0.clone(), origin, text: tick });
        out.marks.push(Mark { key: key.clone(), shape, color: ink });
    }
    if kind == ChartKind::Line {
        out.series.push(Series { key: y_field, points, width: line_width, color: ink });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_hits_both_ends_exactly() {
        for (a, b) in [(1.0_f32, 2.0), (-3.25, 7.5), (0.1, 0.3), (1e-7, 4e6)] {
            assert_eq!(lerp(a, b, 0.0), a);
            assert_eq!(lerp(a, b, 1.0), b);
        }
    }

    #[test]
    fn a_circle_is_a_rounded_rect_with_half_side_radii() {
        let dot = RoundRect { x: 10.0, y: 20.0, w: 12.0, h: 12.0, top_radius: 6.0, bottom_radius: 6.0 };
        let path = dot.path();
        assert_eq!(path.0.len(), 10);
        // The straight runs between corners have zero length; the curves meet.
        assert_eq!(path.0[0], PathEl::MoveTo([16.0, 20.0]));
        assert_eq!(path.0[1], PathEl::LineTo([16.0, 20.0]));
        assert_eq!(path.0[2], PathEl::CurveTo([16.0 + KAPPA * 6.0, 20.0], [22.0, 26.0 - KAPPA * 6.0], [22.0, 26.0]));
        assert_eq!(
            RoundRect { top_radius: 0.0, bottom_radius: 0.0, ..dot }.path(),
            Path::rect([10.0, 20.0, 12.0, 12.0])
        );
        // A bar: rounded on top, square at the base.
        let bar = RoundRect { x: 0.0, y: 0.0, w: 10.0, h: 40.0, top_radius: 2.0, bottom_radius: 0.0 }.path();
        assert_eq!(bar.0.len(), 8, "{bar:?}");
        assert!(bar.0.contains(&PathEl::LineTo([10.0, 40.0])) && bar.0.contains(&PathEl::LineTo([0.0, 40.0])));
    }
}
