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
use crate::data::{ColumnType, DataFiles, Datum};
use crate::fonts::BundleFonts;
use crate::scale::LinearScale;
use crate::text::{GlyphRun, TextEngine, TextLayout};
use crate::theme::Theme;
use scaena_core::Deck;
use scaena_core::displaylist::{Color, FontRef, Glyph, Path, PathEl, Point};
use scaena_core::format::{DateFormat, Locale, MINUS, NumberFormat};

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

/// What a datum draws (SPEC §3.7): a bar, a dot, an area's span, or a donut's slice.
/// Lines and areas are paths through their series' dots and spans ([`SeriesPath`]),
/// made again from the marks wherever a frame puts them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Bar(RoundRect),
    /// A point at `(x, y)`, drawn as a dot of radius `r` (nothing at 0); a line runs
    /// through it.
    Dot {
        x: f32,
        y: f32,
        r: f32,
    },
    /// A point of an area at `x`: its top, and the base it stacks on.
    Span {
        x: f32,
        top: f32,
        base: f32,
    },
    /// A donut slice around `(cx, cy)` between radii `inner` and `outer`, from turn
    /// `start` to turn `end`, clockwise from twelve o'clock.
    Arc {
        cx: f32,
        cy: f32,
        inner: f32,
        outer: f32,
        start: f32,
        end: f32,
    },
}

impl Shape {
    /// The shape `p` of the way from `a` to `b`; `None` for two kinds of mark.
    pub fn lerp(a: Shape, b: Shape, p: f32) -> Option<Shape> {
        Some(match (a, b) {
            (Shape::Bar(a), Shape::Bar(b)) => Shape::Bar(RoundRect::lerp(a, b, p)),
            (Shape::Dot { x, y, r }, Shape::Dot { x: x2, y: y2, r: r2 }) => {
                Shape::Dot { x: lerp(x, x2, p), y: lerp(y, y2, p), r: lerp(r, r2, p) }
            }
            (Shape::Span { x, top, base }, Shape::Span { x: x2, top: t2, base: b2 }) => {
                Shape::Span { x: lerp(x, x2, p), top: lerp(top, t2, p), base: lerp(base, b2, p) }
            }
            (
                Shape::Arc { cx, cy, inner, outer, start, end },
                Shape::Arc { cx: cx2, cy: cy2, inner: i2, outer: o2, start: s2, end: e2 },
            ) => Shape::Arc {
                cx: lerp(cx, cx2, p),
                cy: lerp(cy, cy2, p),
                inner: lerp(inner, i2, p),
                outer: lerp(outer, o2, p),
                start: lerp(start, s2, p),
                end: lerp(end, e2, p),
            },
            _ => return None,
        })
    }

    /// The datum with no value, at `base`: where a new value grows from and a removed
    /// one shrinks to. A bar flattens onto it, a dot drops onto it and closes, a span's
    /// top meets it, and a slice closes at its start.
    pub fn collapsed(self, base: f32) -> Shape {
        match self {
            Shape::Bar(r) => Shape::Bar(r.collapsed(base)),
            Shape::Dot { x, .. } => Shape::Dot { x, y: base, r: 0.0 },
            Shape::Span { x, .. } => Shape::Span { x, top: base, base },
            Shape::Arc { cx, cy, inner, outer, start, .. } => Shape::Arc { cx, cy, inner, outer, start, end: start },
        }
    }

    /// The same mark moved `dx` across; a slice stays where it is.
    pub fn shifted(self, dx: f32) -> Shape {
        match self {
            Shape::Bar(r) => Shape::Bar(r.shifted(dx)),
            Shape::Dot { x, y, r } => Shape::Dot { x: x + dx, y, r },
            Shape::Span { x, top, base } => Shape::Span { x: x + dx, top, base },
            Shape::Arc { .. } => self,
        }
    }

    pub fn center_x(&self) -> f32 {
        match *self {
            Shape::Bar(r) => r.center_x(),
            Shape::Dot { x, .. } | Shape::Span { x, .. } => x,
            Shape::Arc { cx, .. } => cx,
        }
    }

    /// The point a line or an area runs through: a dot's center, a span's top.
    pub fn point(&self) -> Point {
        match *self {
            Shape::Bar(r) => [r.center_x(), r.top()],
            Shape::Dot { x, y, .. } => [x, y],
            Shape::Span { x, top, .. } => [x, top],
            Shape::Arc { cx, cy, .. } => [cx, cy],
        }
    }

    /// The outline it fills; `None` for what draws only as part of a path (a span, a
    /// dot of no size).
    pub fn path(&self) -> Option<Path> {
        match *self {
            Shape::Bar(r) => Some(r.path()),
            Shape::Dot { r, .. } if r <= 0.0 => None,
            Shape::Dot { x, y, r } => Some(circle([x, y], r)),
            Shape::Span { .. } => None,
            Shape::Arc { cx, cy, inner, outer, start, end } => {
                (end > start).then(|| arc([cx, cy], inner, outer, start, end))
            }
        }
    }
}

/// A circle of four cubic quarter circles, clockwise from the top.
fn circle([x, y]: Point, r: f32) -> Path {
    let k = KAPPA * r;
    Path(vec![
        PathEl::MoveTo([x, y - r]),
        PathEl::CurveTo([x + k, y - r], [x + r, y - k], [x + r, y]),
        PathEl::CurveTo([x + r, y + k], [x + k, y + r], [x, y + r]),
        PathEl::CurveTo([x - k, y + r], [x - r, y + k], [x - r, y]),
        PathEl::CurveTo([x - r, y - k], [x - k, y - r], [x, y - r]),
        PathEl::Close,
    ])
}

/// An annular sector from turn `start` to turn `end` (clockwise from twelve o'clock),
/// cubic segments of at most a quarter turn each. Angles go through `libm`, whose
/// pure-Rust sines read the same on every platform (SPEC §13).
fn arc(center: Point, inner: f32, outer: f32, start: f32, end: f32) -> Path {
    let turn = core::f32::consts::TAU;
    let at = |r: f32, t: f32| [center[0] + r * libm::sinf(t * turn), center[1] - r * libm::cosf(t * turn)];
    // Cubic segments of one radius from turn `a` to `b`, either way round.
    let sweep = |els: &mut Vec<PathEl>, r: f32, a: f32, b: f32| {
        let n = ((b - a).abs() * 4.0).ceil().max(1.0) as usize;
        for i in 0..n {
            let (t0, t1) = (a + (b - a) * i as f32 / n as f32, a + (b - a) * (i + 1) as f32 / n as f32);
            // Handle length for an arc of θ: 4/3 · tan(θ/4) of the radius.
            let k = 4.0 / 3.0 * libm::tanf((t1 - t0) * turn / 4.0) * r;
            let (p0, p1) = (at(r, t0), at(r, t1));
            // Tangents point along increasing turns: (cos, sin) in these axes.
            let tangent = |t: f32| [libm::cosf(t * turn), libm::sinf(t * turn)];
            let (d0, d1) = (tangent(t0), tangent(t1));
            els.push(PathEl::CurveTo(
                [p0[0] + k * d0[0], p0[1] + k * d0[1]],
                [p1[0] - k * d1[0], p1[1] - k * d1[1]],
                p1,
            ));
        }
    };
    let mut els = vec![PathEl::MoveTo(at(outer, start))];
    sweep(&mut els, outer, start, end);
    if inner > 0.0 {
        els.push(PathEl::LineTo(at(inner, end)));
        sweep(&mut els, inner, end, start);
    } else {
        els.push(PathEl::LineTo(center));
    }
    els.push(PathEl::Close);
    Path(els)
}

/// One datum's mark.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// Identity across states: the chart's `key` field.
    pub key: String,
    pub shape: Shape,
    pub color: Color,
    /// Where it grows from when it enters and shrinks to when it leaves: `None` for the
    /// chart's baseline, wherever that is in each state; the stack under it otherwise.
    pub base: Option<f32>,
}

/// A series drawn as one path through its marks (SPEC §3.7): a line through its dots,
/// or an area under its spans' tops and over their bases.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesPath {
    /// The series.
    pub key: String,
    pub color: Color,
    /// A line's stroke width; `None` for an area, which fills.
    pub stroke: Option<f32>,
    /// Its marks' keys.
    pub marks: Vec<String>,
}

impl SeriesPath {
    /// The path through `points`, the series' marks wherever they are, in order across.
    pub fn path(&self, shapes: &[Shape]) -> Option<Path> {
        let mut shapes: Vec<&Shape> = shapes.iter().collect();
        shapes.sort_by(|a, b| a.center_x().total_cmp(&b.center_x()));
        let (first, rest) = shapes.split_first()?;
        let mut els = vec![PathEl::MoveTo(first.point())];
        els.extend(rest.iter().map(|s| PathEl::LineTo(s.point())));
        if self.stroke.is_none() {
            for s in shapes.iter().rev() {
                if let Shape::Span { x, base, .. } = **s {
                    els.push(PathEl::LineTo([x, base]));
                }
            }
            els.push(PathEl::Close);
        }
        Some(Path(els))
    }
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
    /// From the mark's free end to the label's baseline; for a slice, from its outer
    /// edge out along its middle.
    pub offset: f32,
    /// How much of the label's width falls before its anchor: ½ centers it, 0 starts it
    /// there, 1 ends it there.
    pub align: f32,
    /// From the anchor down to the baseline (a slice's label centers its cap height on
    /// its point).
    pub drop: f32,
}

impl ValueLabel {
    /// The label's anchor (center, baseline) on `shape`.
    pub fn anchor(&self, shape: &Shape) -> [f32; 2] {
        match *shape {
            Shape::Bar(r) => {
                let end = if self.below { r.bottom() } else { r.top() };
                [r.center_x(), end + self.offset]
            }
            Shape::Dot { x, y, r } => [x, if self.below { y + r } else { y - r } + self.offset],
            Shape::Span { x, top, .. } => [x, top + self.offset],
            Shape::Arc { cx, cy, outer, start, end, .. } => {
                let (mid, r) = (0.5 * (start + end) * core::f32::consts::TAU, outer + self.offset);
                [cx + r * libm::sinf(mid), cy - r * libm::cosf(mid) + self.drop]
            }
        }
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
/// gridlines, baseline, marks, category labels, value-axis labels, titles, value
/// labels, clipped at the cell's sides.
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
    /// Lines and areas through the marks, under them.
    pub paths: Vec<SeriesPath>,
    /// Values to heights: where a tick of another snapshot's axis sits in this one.
    pub y_scale: LinearScale,
    /// The plot's box, `[x, y, w, h]` relative to the chart.
    pub plot: [f32; 4],
    /// The plot clips at its sides: something sits beside it, a value-axis gutter or a
    /// legend at the right, that a mark riding out of the window passes under.
    pub clipped: bool,
    /// The value axis, tick by tick: a gridline across the plot, a label beside it.
    pub y_axis: Vec<AxisTick>,
    /// Axis titles, keyed `x` and `y`.
    pub titles: Vec<Label>,
    /// The legend, an entry per series (per slice of a donut).
    pub legend: Vec<LegendEntry>,
    /// Gridlines across a continuous x, keyed by tick.
    pub x_grid: Vec<AxisTick>,
}

/// A legend entry: a swatch in the series' color beside its name.
#[derive(Debug, Clone, PartialEq)]
pub struct LegendEntry {
    pub key: String,
    pub swatch: RoundRect,
    pub color: Color,
    pub label: Label,
}

/// One tick of the value axis, keyed by its label's text.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisTick {
    pub key: String,
    pub value: f64,
    pub rule: Option<Rule>,
    pub label: Option<Label>,
}

/// What the compiler needs from the engine: text layout for the labels.
pub struct Ctx<'a> {
    pub text: &'a mut TextEngine,
    pub fonts: &'a mut BundleFonts,
    pub theme: &'a Theme,
    pub deck: &'a Deck,
    pub data: &'a DataFiles,
}

mod compile;
pub use compile::compile;

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
