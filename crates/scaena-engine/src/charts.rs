//! Charts → marks (SPEC §3.7). A chart spec never stores pixels; it compiles, once per
//! snapshot, to marks keyed by data, so a transition can carry each mark to its next
//! value (SPEC §2.3): one period to the next, values animating in, growth.
//!
//! Phase 0 (PLAN 0.10) compiles bar charts with one series, which is enough to prove
//! data motion on the timeline. Labels print numbers and dates through the encodings'
//! formats (`docs/spec/format.md`) in the deck's language. Every other kind, multiple
//! series, color encodings, legends, axes settings, and data transforms wait for the
//! rest of the chart and table sprint (PLAN 1.9) and return `NotImplemented`.
//!
//! Geometry is relative to the chart's cell and uses only `+ − × ÷`, never `sin` or
//! `cos`, whose last bits differ between platform math libraries, so chart display
//! lists stay bit-identical across platforms (SPEC §13).

use crate::EngineError;
use crate::data::{self, ColumnType, DataFiles, Datum};
use crate::fonts::BundleFonts;
use crate::text::{GlyphRun, TextEngine, TextLayout, TextSpec};
use crate::theme::{Numeric, TextBox, Theme};
use scaena_core::Deck;
use scaena_core::displaylist::{Color, FontRef, Glyph, Path, PathEl};
use scaena_core::document::Props;
use scaena_core::format::{DateFormat, Locale, MINUS, NumberFormat};
use serde_json::Value;
use std::collections::BTreeSet;

/// Bézier handle length for a quarter circle of radius 1: 4/3 · (√2 − 1).
const KAPPA: f32 = 0.552_284_8;

/// A rectangle with one radius for its top corners and one for its bottom corners.
/// A bar is square on its baseline and rounded at its free end.
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

    /// The same bar with no height, on the baseline at `base`: where a new value grows
    /// from and a removed one shrinks to.
    pub fn collapsed(self, base: f32) -> RoundRect {
        RoundRect { y: base, h: 0.0, top_radius: 0.0, bottom_radius: 0.0, ..self }
    }

    /// The same bar moved `dx` across.
    pub fn shifted(self, dx: f32) -> RoundRect {
        RoundRect { x: self.x + dx, ..self }
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

/// Text inside a chart: a category label, or a value label riding its mark.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    /// A category for a category label; the mark's key for a value label.
    pub key: String,
    /// The text's top-left corner, relative to the chart.
    pub origin: [f32; 2],
    pub text: TextLayout,
    pub value: Option<ValueLabel>,
}

/// What a value label shows and where it rides on its mark.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueLabel {
    pub value: f64,
    /// Under the mark (a negative bar) rather than over it.
    pub below: bool,
    /// From the mark's free end to the label's baseline.
    pub offset: f32,
}

impl ValueLabel {
    /// The label's anchor (center, baseline) on `shape`.
    pub fn anchor(&self, shape: &RoundRect) -> [f32; 2] {
        let end = if self.below { shape.bottom() } else { shape.top() };
        [shape.center_x(), end + self.offset]
    }
}

/// A straight hairline: the axis baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub width: f32,
    pub color: Color,
}

/// The value labels' figures, shaped once per snapshot. Tabular figures share one
/// advance and do not kern, so a number spelled from them glyph by glyph is the number
/// shaped: a counting label is composed each frame without shaping (SPEC §5).
#[derive(Debug, Clone, PartialEq)]
pub struct Numerals {
    figures: Vec<(char, Figure)>,
    /// From the top of the text to its baseline.
    pub baseline: f32,
    /// How a counting label spells a number: the encoding's format, else as many places
    /// as either end shows.
    format: Option<NumberFormat>,
    locale: &'static Locale,
    /// The minus sign the label's font sets: U+2212, else the hyphen-minus.
    minus: char,
}

#[derive(Debug, Clone, PartialEq)]
struct Figure {
    font: FontRef,
    size: f32,
    coords: Vec<i16>,
    color: Color,
    id: u32,
    advance: f32,
}

impl Numerals {
    /// Shape the figures as they sit inside a number: the digits in a row, a sign before
    /// one, and every other character the format can print (`alphabet`: separators,
    /// currency, percent, suffixes) between two. Fonts substitute some of them in
    /// context (Roboto Serif sets a tabular period between figures), so a figure shaped
    /// alone can be the wrong glyph. `None` if the digits do not shape to one glyph each;
    /// another character that does not is left out, and a label that needs it
    /// cross-fades.
    fn shape(
        alphabet: &str,
        format: Option<NumberFormat>,
        locale: &'static Locale,
        minus: char,
        mut set: impl FnMut(String) -> Result<TextLayout, EngineError>,
    ) -> Result<Option<Numerals>, EngineError> {
        let mut figures: Vec<(char, Figure)> = Vec::new();
        let mut baseline = 0.0;
        // (sample, which of its characters to take)
        let mut samples = vec![("01234567890".to_string(), 0..10)];
        for c in alphabet.chars().map(|c| if c == MINUS { minus } else { c }) {
            if figures.iter().any(|(k, _)| *k == c) || samples.iter().any(|(s, t)| s[t.clone()].contains(c)) {
                continue;
            }
            samples.push(match c {
                '-' | MINUS | '+' | '(' => (format!("{c}0"), 0..c.len_utf8()),
                _ => (format!("0{c}0"), 1..1 + c.len_utf8()),
            });
        }
        for (i, (sample, take)) in samples.iter().enumerate() {
            let text = set(sample.clone())?;
            let glyphs: Vec<(&GlyphRun, &Glyph)> =
                text.runs.iter().flat_map(|r| r.glyphs.iter().map(move |g| (r, g))).collect();
            if glyphs.len() != sample.chars().count() {
                if i == 0 {
                    return Ok(None);
                }
                continue;
            }
            baseline = text.lines.first().map_or(0.0, |l| l.baseline);
            for (k, (at, c)) in sample.char_indices().enumerate() {
                if !take.contains(&at) {
                    continue;
                }
                let (run, glyph) = glyphs[k];
                // Every taken figure has a figure after it, so its advance is the gap.
                let advance = glyphs[k + 1].1.x - glyph.x;
                let figure = Figure {
                    font: run.font.clone(),
                    size: run.size,
                    coords: run.coords.clone(),
                    color: run.color,
                    id: glyph.id,
                    advance,
                };
                figures.push((c, figure));
            }
        }
        Ok(Some(Numerals { figures, baseline, format, locale, minus }))
    }

    /// The number `p` of the way from `a` to `b`, as its label spells it: in the
    /// encoding's format, else to as many places as either end shows.
    pub fn count(&self, a: f64, b: f64, p: f32) -> String {
        let v = a + (b - a) * f64::from(p);
        let text = match &self.format {
            Some(f) => f.format(v, self.locale),
            None => {
                let places = |x: f64| {
                    let s = NumberFormat::plain().format(x, self.locale);
                    s.split_once(self.locale.decimal).map_or(0, |(_, f)| f.len())
                };
                NumberFormat::fixed(places(a).max(places(b))).format(v, self.locale)
            }
        };
        typeset_minus(text, self.minus)
    }

    /// `text` spelled from the figures, as glyph runs relative to the text's top-left
    /// corner, and its advance. `None` if a character has no figure.
    pub fn compose(&self, text: &str) -> Option<(Vec<GlyphRun>, f32)> {
        let mut runs: Vec<GlyphRun> = Vec::new();
        let mut x = 0.0;
        for (at, c) in text.char_indices() {
            let (_, f) = self.figures.iter().find(|(k, _)| *k == c)?;
            let glyph = Glyph { id: f.id, x, y: self.baseline };
            match runs.last_mut() {
                Some(run) if run.font == f.font && run.size == f.size && run.coords == f.coords => {
                    run.glyphs.push(glyph);
                    run.clusters.push(at);
                }
                _ => runs.push(GlyphRun {
                    font: f.font.clone(),
                    size: f.size,
                    coords: f.coords.clone(),
                    color: f.color,
                    glyphs: vec![glyph],
                    clusters: vec![at],
                    line: 0,
                }),
            }
            x += f.advance;
        }
        Some((runs, x))
    }
}

/// `text` with its minus signs as `minus`: U+2212 where the font has it, else the
/// hyphen-minus.
fn typeset_minus(text: String, minus: char) -> String {
    if minus == MINUS { text } else { text.replace(MINUS, &minus.to_string()) }
}

/// How a category prints: a date column through a date format, a number column through
/// a number format.
enum CategoryFormat {
    Number(NumberFormat),
    Date(DateFormat),
}

impl CategoryFormat {
    /// `spec` for a column of type `kind`; a string column takes no format.
    fn parse(spec: &str, kind: ColumnType, field: &str) -> Result<CategoryFormat, EngineError> {
        let bad = |e: scaena_core::format::FormatError| EngineError::Layout(format!("`x.format`: {e}"));
        match kind {
            ColumnType::Number => Ok(CategoryFormat::Number(NumberFormat::parse(spec).map_err(bad)?)),
            ColumnType::Date => Ok(CategoryFormat::Date(DateFormat::parse(spec).map_err(bad)?)),
            _ => Err(EngineError::Layout(format!(
                "`x.format` formats numbers and dates; `{field}` is a {} column",
                kind.name()
            ))),
        }
    }

    fn print(&self, d: &Datum, locale: &Locale) -> String {
        match (self, d) {
            (CategoryFormat::Number(f), Datum::Number(n)) => f.format(*n, locale),
            (CategoryFormat::Date(f), Datum::Date(t)) => f.format(*t, locale),
            (_, d) => d.label(),
        }
    }
}

/// A chart compiled for one snapshot, relative to its cell. Painted bottom to top:
/// baseline, marks, category labels, value labels, clipped at the cell's sides.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartLayout {
    /// The baseline's y: where a new value grows from and a removed one shrinks to.
    pub base: f32,
    pub baseline: Option<Rule>,
    pub marks: Vec<Mark>,
    /// Category labels under the plot, keyed by category.
    pub ticks: Vec<Label>,
    /// Value labels, keyed like their marks.
    pub labels: Vec<Label>,
    /// For counting the value labels; `None` without them.
    pub numerals: Option<Numerals>,
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
    match props.get("kind").and_then(Value::as_str) {
        Some("bar") => {}
        Some("stackedBar" | "line" | "area" | "scatter" | "dot" | "donut") => {
            return Err(EngineError::NotImplemented("chart kinds other than bar — PLAN 1.9 (chart and table sprint)"));
        }
        other => return Err(EngineError::Layout(format!("unknown chart kind {other:?}"))),
    }
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
    let locale = Locale::of(cx.deck.meta.as_ref().and_then(|m| m.lang.as_deref()));
    let y_format = (y.get("format").and_then(Value::as_str))
        .map(|f| NumberFormat::parse(f).map_err(|e| EngineError::Layout(format!("`y.format`: {e}"))))
        .transpose()?;
    let field =
        |e: &serde_json::Map<String, Value>| e.get("field").and_then(Value::as_str).unwrap_or_default().to_string();
    let (x_field, y_field) = (field(x), field(y));
    let key_field = props.get("key").and_then(Value::as_str).map_or_else(|| x_field.clone(), str::to_string);

    // Data: one row per category, in source order.
    let source = props.get("data").and_then(Value::as_str).and_then(|d| d.strip_prefix('@'));
    let table = data::load(cx.deck, cx.data, source.ok_or_else(|| EngineError::Layout("chart has no `data`".into()))?)?;
    let col = |name: &str| table.column(name).ok_or_else(|| EngineError::Data(format!("no column `{name}`")));
    let (xc, yc, kc) = (col(&x_field)?, col(&y_field)?, col(&key_field)?);
    let x_format = (x.get("format").and_then(Value::as_str))
        .map(|f| CategoryFormat::parse(f, table.types[xc], &x_field))
        .transpose()?;
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
        let category = match &x_format {
            Some(f) => f.print(&row[xc], locale),
            None => row[xc].label(),
        };
        rows.push((category, v, key));
    }
    if rows.is_empty() {
        return Err(EngineError::Data("chart data has no rows".into()));
    }

    // Theme: chart styles, all tokens.
    let charts = cx.theme.charts.as_ref();
    let axis = charts.and_then(|c| c.axis.as_ref());
    let axis_width = cx.theme.stroke(axis.and_then(|a| a.stroke.as_deref()).unwrap_or("hairline"))?;
    let axis_color = cx.theme.color(axis.and_then(|a| a.color.as_deref()).unwrap_or("onSurfaceMuted"))?;
    let corner = charts.and_then(|c| c.corner_radius).unwrap_or(0.0) as f32;
    let bar_gap = charts.and_then(|c| c.bar_gap).unwrap_or(0.2) as f32;
    let ink = cx
        .theme
        .tokens
        .data
        .categorical
        .first()
        .ok_or_else(|| EngineError::Theme("charts need tokens.data.categorical".into()))?;
    let ink = scaena_core::color::parse(&ink.0).map_err(EngineError::Theme)?;
    let gap = cx.theme.tokens.space.unit as f32;
    let labels = props.get("labels");
    let label_role = labels
        .and_then(|l| l.get("role"))
        .and_then(Value::as_str)
        .or_else(|| charts.and_then(|c| c.label.as_ref()).and_then(|l| l.role.as_deref()))
        .unwrap_or("label")
        .to_string();
    let tick_role = axis.and_then(|a| a.role.as_deref()).unwrap_or("label").to_string();
    let show = labels.and_then(|l| l.get("show")).and_then(Value::as_str).unwrap_or("none");
    let labelled = |i: usize| match show {
        "all" => Ok(true),
        "ends" => Ok(i == 0 || i + 1 == rows.len()),
        "none" => Ok(false),
        other => Err(EngineError::Layout(format!("labels.show `{other}`: expected all, ends, or none"))),
    };

    // Text first: the plot is what the labels leave.
    let mut set = |text: String, role: &str| -> Result<TextLayout, EngineError> {
        let spec =
            TextSpec { numeric: Some(Numeric::TabularLining), ..TextSpec::plain(cx.theme.text_role(role)?, text) };
        cx.text.layout(cx.fonts, cx.theme, &spec, f32::INFINITY)
    };
    let value_format = y_format.clone().unwrap_or_else(NumberFormat::plain);
    let labelled_any = (0..rows.len()).map(&labelled).collect::<Result<Vec<_>, _>>()?.into_iter().any(|l| l);
    // The minus sign the labels' font sets: U+2212 if it has one.
    let minus = match labelled_any {
        true => {
            let probe = set(format!("{MINUS}0"), &label_role)?;
            if probe.runs.iter().flat_map(|r| &r.glyphs).any(|g| g.id == 0) { '-' } else { MINUS }
        }
        false => MINUS,
    };
    let mut values = Vec::new();
    for (i, (_, v, _)) in rows.iter().enumerate() {
        let text = typeset_minus(value_format.format(*v, locale), minus);
        values.push(if labelled(i)? { Some(set(text, &label_role)?) } else { None });
    }
    let numerals = match labelled_any {
        true => Numerals::shape(&value_format.alphabet(locale), y_format, locale, minus, |c| set(c, &label_role))?,
        false => None,
    };
    let ticks: Vec<TextLayout> = rows.iter().map(|(c, ..)| set(c.clone(), &tick_role)).collect::<Result<_, _>>()?;
    // Cap height above the baseline; line-box height below the cap top.
    let cap = |t: &TextLayout| t.lines.first().map_or(0.0, |l| l.cap_height.unwrap_or(l.ascent));
    let below_cap = |t: &TextLayout| t.lines.first().map_or(0.0, |l| t.height - (l.baseline - cap(t)));
    let label_room = values.iter().flatten().map(cap).fold(0.0_f32, f32::max);
    let top = if label_room > 0.0 { label_room + gap } else { 0.0 };
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
        base,
        baseline: Some(Rule { from: [0.0, base], to: [size[0], base], width: axis_width, color: axis_color }),
        marks: Vec::with_capacity(rows.len()),
        ticks: Vec::with_capacity(rows.len()),
        labels: Vec::new(),
        numerals,
    };
    for (i, ((category, v, key), (value, tick))) in rows.iter().zip(values.into_iter().zip(ticks)).enumerate() {
        let center = (i as f32 + 0.5) * band;
        let at = to_y(*v);
        // Square on the baseline, rounded at the free end.
        let (w, top, bottom) = (band * (1.0 - bar_gap), at.min(base), at.max(base));
        let r = corner.min(0.5 * w).min(bottom - top);
        let below = at > base;
        let (top_radius, bottom_radius) = if below { (0.0, r) } else { (r, 0.0) };
        let shape = RoundRect { x: center - 0.5 * w, y: top, w, h: bottom - top, top_radius, bottom_radius };
        if let Some(text) = value {
            // Over the mark for values at or above the baseline, under it otherwise.
            let first = &text.lines[0];
            let (origin, offset) = if below {
                let origin = [center - 0.5 * text.width, shape.bottom() + gap - text.trimmed(TextBox::Cap).0];
                (origin, origin[1] + first.baseline - shape.bottom())
            } else {
                ([center - 0.5 * text.width, shape.top() - gap - first.baseline], -gap)
            };
            let value = Some(ValueLabel { value: *v, below, offset });
            out.labels.push(Label { key: key.clone(), origin, text, value });
        }
        let origin = [center - 0.5 * tick.width, bottom + gap - tick.trimmed(TextBox::Cap).0];
        out.ticks.push(Label { key: category.clone(), origin, text: tick, value: None });
        out.marks.push(Mark { key: key.clone(), shape, color: ink });
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
    fn bars_round_their_free_end_and_collapse_onto_the_baseline() {
        let bar = RoundRect { x: 0.0, y: 0.0, w: 10.0, h: 40.0, top_radius: 2.0, bottom_radius: 0.0 };
        let path = bar.path();
        assert_eq!(path.0.len(), 8, "{path:?}");
        assert!(path.0.contains(&PathEl::LineTo([10.0, 40.0])) && path.0.contains(&PathEl::LineTo([0.0, 40.0])));
        assert_eq!(
            path.0[2],
            PathEl::CurveTo([8.0 + KAPPA * 2.0, 0.0], [10.0, 2.0 - KAPPA * 2.0], [10.0, 2.0]),
            "a quarter circle"
        );
        let flat = bar.collapsed(40.0);
        assert_eq!((flat.y, flat.h, flat.top_radius), (40.0, 0.0, 0.0));
        assert_eq!(flat.path(), Path::rect([0.0, 40.0, 10.0, 0.0]));
    }
}
