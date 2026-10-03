//! The chart compiler (SPEC §3.7): a chart node's resolved props and its data to marks,
//! labels, axes, and a legend, once per snapshot. Every v1 kind is a list of keyed
//! marks, one per datum, so a transition carries each datum to its next value whatever
//! the kind; lines and areas are paths through their series' marks.

use super::{
    AxisTick, CategoryFormat, ChartLayout, Ctx, Gap, Label, LegendEntry, Mark, Note, Numerals, RoundRect, Rule,
    SeriesPath, Shape, Stack, ValueLabel, typeset_minus,
};
use crate::EngineError;
use crate::data::{self, ColumnType, Datum};
use crate::scale::{self, LinearScale};
use crate::text::{TextLayout, TextSpec};
use crate::theme::{Numeric, TextBox};
use scaena_core::displaylist::Color;
use scaena_core::document::Props;
use scaena_core::format::{DateFormat, DateTime, Locale, MINUS, NumberFormat};
use scaena_core::model::nodes::{LabelShow, LegendPlace};
use scaena_core::model::values::{Annotation, AnnotationKind, Place, Scalar};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

use super::ChartKind as Kind;

impl Kind {
    fn parse(v: Option<&Value>) -> Result<Kind, EngineError> {
        Ok(match v.and_then(Value::as_str) {
            Some("bar") => Kind::Bar,
            Some("stackedBar") => Kind::StackedBar,
            Some("line") => Kind::Line,
            Some("area") => Kind::Area,
            Some("scatter") => Kind::Scatter,
            Some("dot") => Kind::Dot,
            Some("donut") => Kind::Donut,
            other => return Err(EngineError::Layout(format!("unknown chart kind {other:?}"))),
        })
    }

    /// Its value axis starts at zero: a bar's or an area's length is its value.
    fn zero_based(self) -> bool {
        matches!(self, Kind::Bar | Kind::StackedBar | Kind::Area)
    }

    /// It can run along a continuous x: a number or a date.
    fn continuous(self) -> bool {
        matches!(self, Kind::Line | Kind::Area | Kind::Scatter)
    }
}

/// One datum, read through the encodings.
struct Row {
    /// Its category's identity, and the category as it prints.
    category: String,
    label: String,
    /// Its x as a number, on a continuous x: a number, or a date in seconds.
    x: Option<f64>,
    /// Its x, when that is a date.
    date: Option<DateTime>,
    y: f64,
    /// Its series, if the chart has one.
    series: Option<String>,
    /// What a numeric color encoding reads.
    shade: Option<f64>,
    size: Option<f64>,
    key: String,
    /// A forecast or an estimate (PLAN 1.28): a line's or an area's row its `projected`
    /// marks.
    projected: bool,
}

type Encoding<'a> = Option<&'a Map<String, Value>>;

fn field<'a>(e: Encoding<'a>) -> Option<&'a str> {
    e.and_then(|e| e.get("field")).and_then(Value::as_str)
}

/// Whether a row's datum marks it projected: it is `value`, or, with no value, true.
fn marks_projected(d: &Datum, value: Option<&Value>) -> bool {
    match (d, value) {
        (Datum::Bool(b), None) => *b,
        (_, None) | (Datum::Null, _) => false,
        (Datum::Bool(b), Some(v)) => v.as_bool() == Some(*b),
        (Datum::Number(n), Some(v)) => v.as_f64() == Some(*n),
        (Datum::Text(t), Some(v)) => v.as_str() == Some(t.as_str()),
        (Datum::Date(_), Some(v)) => v.as_str() == Some(d.label().as_str()),
    }
}

/// The calendar unit a column of dates steps by, read off what all its dates share: a
/// year's first day, a month's, a midnight, or none of them (times of day).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DateUnit {
    Years,
    Months,
    Days,
    Times,
}

/// How a date column with no `x.format` prints (SPEC §3.7): by its unit (`%b` for
/// months), and where a label begins a run of the unit above it, the year (a time's
/// day), naming that too (`long`): `Jan 2026`, `Feb`, … `Dec`, `Jan 2027`. A label
/// alone, as a donut's legend entry, prints long.
struct DateLabels {
    unit: DateUnit,
    short: DateFormat,
    /// None where it would print as `short` does (years).
    long: Option<DateFormat>,
}

impl DateLabels {
    fn of(dates: &[DateTime]) -> Option<DateLabels> {
        let unit = (dates.iter().map(|t| t.civil()))
            .map(|c| {
                if (c.hour, c.minute, c.second) != (0, 0, 0) {
                    DateUnit::Times
                } else if c.day != 1 {
                    DateUnit::Days
                } else if c.month != 1 {
                    DateUnit::Months
                } else {
                    DateUnit::Years
                }
            })
            .max()?;
        let minutes = dates.iter().any(|t| t.civil().minute != 0);
        let (short, long) = match unit {
            DateUnit::Years => ("%Y", None),
            DateUnit::Months => ("%b", Some("%b %Y")),
            DateUnit::Days => ("%b %-d", Some("%b %-d, %Y")),
            DateUnit::Times if minutes => ("%-I:%M %p", Some("%b %-d, %-I:%M %p")),
            DateUnit::Times => ("%-I %p", Some("%b %-d, %-I %p")),
        };
        let parse = |f: &str| DateFormat::parse(f).expect("date label formats parse");
        Some(DateLabels { unit, short: parse(short), long: long.map(parse) })
    }

    /// How `t` prints alone.
    fn whole(&self, t: DateTime, locale: &Locale) -> String {
        self.long.as_ref().unwrap_or(&self.short).format(t, locale)
    }

    /// The run of the unit above its own that `t` falls in: its year, or a time's day.
    fn period(&self, t: DateTime) -> i64 {
        match self.unit {
            DateUnit::Times => t.0.div_euclid(86_400),
            _ => t.civil().year,
        }
    }
}

/// How many categories apart the labels of a crowded ordered axis stand, the fewest
/// first: steps of the calendar for dates (a quarter of months, a week of days), 1–2–5
/// steps for years and numbers; then the last of those times 2, 5, 10, 20, ….
fn strides(unit: Option<DateUnit>) -> impl Iterator<Item = usize> {
    let nice: &'static [usize] = match unit {
        Some(DateUnit::Months) => &[1, 2, 3, 4, 6, 12],
        Some(DateUnit::Days) => &[1, 2, 7, 14],
        Some(DateUnit::Times) => &[1, 2, 3, 6, 12, 24],
        Some(DateUnit::Years) | None => &[1, 2, 5, 10],
    };
    let last = nice[nice.len() - 1];
    let more =
        (0..).map(|e| 10usize.saturating_pow(e)).flat_map(move |p| [2, 5, 10].map(|m| last.saturating_mul(m * p)));
    nice.iter().copied().chain(more)
}

/// A datum as a number: a number, or a date in seconds.
fn number(d: &Datum) -> Option<f64> {
    match d {
        Datum::Number(n) => Some(*n),
        Datum::Date(t) => Some(t.0 as f64),
        _ => None,
    }
}

/// What a chart's categorical colors go to, in the order each first appears in its
/// data: its series (or a color field of text), or a donut's categories. Empty when its
/// colors go to nothing, or when its props or data do not read, which compiling it
/// reports.
pub fn color_keys(deck: &scaena_core::Deck, files: &data::DataFiles, props: &Props) -> Vec<String> {
    let encoding = |name: &str| props.get(name).and_then(Value::as_object);
    let Ok(kind) = Kind::parse(props.get("kind")) else { return Vec::new() };
    let Some(source) = props.get("data").and_then(Value::as_str).and_then(|d| d.strip_prefix('@')) else {
        return Vec::new();
    };
    let Ok(table) = data::load(deck, files, source).and_then(|t| data::transform(t, props.get("dataTransform"))) else {
        return Vec::new();
    };
    let color =
        field(encoding("color")).and_then(|f| table.column(f)).filter(|&c| table.types[c] != ColumnType::Number);
    let by = match kind {
        Kind::Donut => field(encoding("x")).and_then(|f| table.column(f)),
        _ => field(encoding("series")).and_then(|f| table.column(f)).or(color),
    };
    let Some(by) = by else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    for row in &table.rows {
        let key = row[by].label();
        if !out.contains(&key) {
            out.push(key);
        }
    }
    out
}

/// Compile a chart node's resolved props for a cell `size` wide and high.
pub fn compile(cx: &mut Ctx, props: &Props, size: [f32; 2]) -> Result<ChartLayout, EngineError> {
    let kind = Kind::parse(props.get("kind"))?;
    let encoding = |name: &str| props.get(name).and_then(Value::as_object);
    let required = |name: &str| encoding(name).ok_or_else(|| EngineError::Layout(format!("chart has no `{name}`")));
    let (x, y) = (required("x")?, required("y")?);
    let (series_enc, color_enc, size_enc) = (encoding("series"), encoding("color"), encoding("sizeEncoding"));
    // Axes: categories (or x ticks) under the plot by default, and the value axis of a
    // scatter or an area, which print no values by default; the value axis, its
    // gridlines, and titles when asked for.
    let axes = props.get("axes");
    let setting = |name: &str, key: &str| axes.and_then(|a| a.get(name)).and_then(|a| a.get(key));
    let flag = |name: &str, key: &str, default: bool| setting(name, key).and_then(Value::as_bool).unwrap_or(default);
    let donut = kind == Kind::Donut;
    // Annotations, each standing where its kind can.
    let notes: Vec<Annotation> = match props.get("annotations") {
        None | Some(Value::Null) => Vec::new(),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| EngineError::Layout(format!("`annotations`: {e}")))?,
    };
    for (k, note) in notes.iter().enumerate() {
        note.check(donut).map_err(|e| EngineError::Layout(format!("annotation {k}: {e}")))?;
    }
    let (x_show, y_show, y_grid) = (
        flag("x", "show", !donut) && !donut,
        flag("y", "show", matches!(kind, Kind::Scatter | Kind::Area)) && !donut,
        flag("y", "gridlines", false) && !donut,
    );
    let title = |name: &str, e: &Map<String, Value>| {
        (setting(name, "title").and_then(Value::as_str))
            .or_else(|| e.get("title").and_then(Value::as_str))
            .map(str::to_string)
    };
    let (x_title, y_title) = if donut { (None, None) } else { (title("x", x), title("y", y)) };
    let locale = Locale::of(cx.deck.meta.as_ref().and_then(|m| m.lang.as_deref()));
    let y_format = (y.get("format").and_then(Value::as_str))
        .map(|f| NumberFormat::parse(f).map_err(|e| EngineError::Layout(format!("`y.format`: {e}"))))
        .transpose()?;

    // Data.
    let source = props.get("data").and_then(Value::as_str).and_then(|d| d.strip_prefix('@'));
    let table = data::load(cx.deck, cx.data, source.ok_or_else(|| EngineError::Layout("chart has no `data`".into()))?)?;
    let table = data::transform(table, props.get("dataTransform"))?;
    let col = |name: &str| table.column(name).ok_or_else(|| EngineError::Data(format!("no column `{name}`")));
    let (x_field, y_field) = (field(Some(x)).unwrap_or_default(), field(Some(y)).unwrap_or_default());
    let (xc, yc) = (col(x_field)?, col(y_field)?);
    let x_type = x.get("type").and_then(Value::as_str);
    let x_numeric = matches!(table.types[xc], ColumnType::Number | ColumnType::Date);
    let continuous = kind.continuous()
        && match x_type {
            Some("quantitative" | "temporal") => true,
            Some(_) => false,
            None => x_numeric && kind == Kind::Scatter,
        };
    if kind == Kind::Scatter && !continuous {
        return Err(EngineError::Layout(format!("a scatter's x is a number or a date; `{x_field}` is not")));
    }
    if continuous && !x_numeric {
        return Err(EngineError::Data(format!("`{x_field}` must hold numbers or dates to run along a continuous x")));
    }
    let x_format = (x.get("format").and_then(Value::as_str))
        .map(|f| CategoryFormat::parse(f, table.types[xc], x_field, "x"))
        .transpose()?;
    let date = |d: &Datum| match d {
        Datum::Date(t) => Some(*t),
        _ => None,
    };
    let date_labels = match (&x_format, table.types[xc]) {
        (None, ColumnType::Date) => DateLabels::of(&table.rows.iter().filter_map(|r| date(&r[xc])).collect::<Vec<_>>()),
        _ => None,
    };
    let series_col = field(series_enc).map(col).transpose()?;
    let color_col = field(color_enc).map(col).transpose()?;
    let size_col = field(size_enc).map(col).transpose()?;
    let key_col = props.get("key").and_then(Value::as_str).map(col).transpose()?;
    // A line's or an area's rows that are a forecast or an estimate.
    let projected = encoding("projected").filter(|_| matches!(kind, Kind::Line | Kind::Area));
    let projected_col = field(projected).map(col).transpose()?;
    let projected_value = projected.and_then(|p| p.get("value"));
    let shaded = color_col.is_some_and(|c| table.types[c] == ColumnType::Number);
    // Without a series, a categorical color field groups like one.
    let group_col = series_col.or(color_col.filter(|_| !shaded));
    let mut rows: Vec<Row> = Vec::with_capacity(table.rows.len());
    let mut seen = BTreeSet::new();
    for row in &table.rows {
        let v = match &row[yc] {
            Datum::Number(v) => *v,
            // A missing value is a gap, not a zero.
            Datum::Null => continue,
            _ => return Err(EngineError::Data(format!("`{y_field}` must be a number in every row"))),
        };
        let label = match (&x_format, &date_labels, date(&row[xc])) {
            (Some(f), ..) => f.print(&row[xc], locale),
            (None, Some(d), Some(t)) => d.whole(t, locale),
            _ => row[xc].label(),
        };
        let category = row[xc].label();
        let series = group_col.map(|c| row[c].label());
        let base = key_col.map_or_else(|| category.clone(), |c| row[c].label());
        let key = match (&series, donut) {
            (Some(s), false) if Some(group_col) != Some(key_col) => format!("{base}\u{1f}{s}"),
            _ => base,
        };
        if !seen.insert(key.clone()) {
            return Err(EngineError::Data(format!(
                "key `{}` repeats; chart keys must be unique (give the chart a `key` field)",
                key.replace('\u{1f}', " · ")
            )));
        }
        let shade = color_col.filter(|_| shaded).and_then(|c| number(&row[c]));
        let size = size_col.and_then(|c| number(&row[c]));
        rows.push(Row {
            category,
            label,
            x: continuous.then(|| number(&row[xc])).flatten(),
            date: date(&row[xc]),
            y: v,
            series,
            shade,
            size,
            key,
            projected: projected_col.is_some_and(|c| marks_projected(&row[c], projected_value)),
        });
    }
    if rows.is_empty() {
        return Err(EngineError::Data("chart data has no rows".into()));
    }
    if donut && rows.iter().any(|r| r.y < 0.0) {
        return Err(EngineError::Data(format!("a donut's `{y_field}` cannot be negative")));
    }
    // Categories and series in order of first appearance.
    let mut categories: Vec<(String, String)> = Vec::new();
    let mut category_dates: Vec<Option<DateTime>> = Vec::new();
    let mut series: Vec<String> = Vec::new();
    for r in &rows {
        if !categories.iter().any(|(k, _)| *k == r.category) {
            categories.push((r.category.clone(), r.label.clone()));
            category_dates.push(r.date);
        }
        if let Some(s) = &r.series
            && !series.contains(s)
        {
            series.push(s.clone());
        }
    }

    // Theme: chart styles, all tokens.
    let theme = cx.theme;
    let cx_colors = cx.colors;
    let charts = theme.charts.as_ref();
    let axis = charts.and_then(|c| c.axis.as_ref());
    let axis_width = theme.stroke(axis.and_then(|a| a.stroke.as_deref()).unwrap_or("hairline"))?;
    let axis_color = theme.color(axis.and_then(|a| a.color.as_deref()).unwrap_or("onSurfaceMuted"))?;
    let corner = charts.and_then(|c| c.corner_radius).unwrap_or(0.0) as f32;
    let bar_gap = charts.and_then(|c| c.bar_gap).unwrap_or(0.5) as f32;
    let group_gap = charts.and_then(|c| c.group_gap).unwrap_or(0.1) as f32;
    let point_radius = charts.and_then(|c| c.point_radius).unwrap_or(0.0) as f32;
    let dot_radius = charts.and_then(|c| c.dot_radius).unwrap_or(6.0) as f32;
    let hole = charts.and_then(|c| c.donut_hole).unwrap_or(0.72).clamp(0.0, 0.99) as f32;
    let line_width = theme.stroke(charts.and_then(|c| c.stroke_width.as_deref()).unwrap_or("thin"))?;
    // What is projected: a line dashes, in widths of the line; an area is lighter; and a
    // value says it is an estimate.
    let estimate = charts.and_then(|c| c.projected.as_ref());
    let [dash, dash_gap] = estimate.and_then(|p| p.dash).unwrap_or([3.0, 2.0]);
    let dash = [dash as f32 * line_width, dash_gap as f32 * line_width];
    let fade = estimate.and_then(|p| p.opacity).unwrap_or(0.5).clamp(0.0, 1.0) as f32;
    let note = (projected.and_then(|p| p.get("note")).and_then(Value::as_str))
        .or_else(|| estimate.and_then(|p| p.note.as_deref()))
        .unwrap_or("est.");
    let palette: Vec<Color> = (theme.tokens.data.categorical.iter())
        .map(|c| scaena_core::color::parse(&c.0).map_err(EngineError::Theme))
        .collect::<Result<_, _>>()?;
    if palette.is_empty() {
        return Err(EngineError::Theme("charts need tokens.data.categorical".into()));
    }
    let gap = theme.tokens.space.unit as f32;
    let labels = props.get("labels");
    let label_role = (labels.and_then(|l| l.get("role")).and_then(Value::as_str))
        .or_else(|| charts.and_then(|c| c.label.as_ref()).and_then(|l| l.role.as_deref()))
        .unwrap_or("label")
        .to_string();
    let tick_role = axis.and_then(|a| a.role.as_deref()).unwrap_or("label").to_string();
    let title_role = (charts.and_then(|c| c.title.as_ref()).and_then(|t| t.role.clone())).unwrap_or(tick_role.clone());
    let legend_role =
        (charts.and_then(|c| c.legend.as_ref()).and_then(|t| t.role.clone())).unwrap_or(tick_role.clone());
    let grid = charts.and_then(|c| c.gridlines.as_ref());
    let grid_width = theme.stroke(grid.and_then(|g| g.stroke.as_deref()).unwrap_or("hairline"))?;
    let grid_color = {
        let Color([r, g, b, a]) = theme.color(grid.and_then(|g| g.color.as_deref()).unwrap_or("onSurfaceMuted"))?;
        let opacity = grid.and_then(|g| g.opacity).unwrap_or(1.0).clamp(0.0, 1.0);
        Color([r, g, b, (f64::from(a) * opacity).round() as u8])
    };
    let tick_count = charts.and_then(|c| c.tick_count).unwrap_or(5) as usize;
    let max_ticks = charts.and_then(|c| c.max_ticks).unwrap_or(5) as usize;
    let x_grid = flag("x", "gridlines", false) && !donut;
    // Annotations: rules, leaders, and bands in one color, text in its role. A theme
    // needs the color and the stroke only for a chart that draws one.
    let style = charts.and_then(|c| c.annotation.as_ref());
    let note_token = style.and_then(|a| a.color.as_deref()).unwrap_or("accent");
    let (note_color, note_width) = match notes.iter().any(|n| n.kind != AnnotationKind::Highlight) {
        true => (theme.color(note_token)?, theme.stroke(style.and_then(|a| a.stroke.as_deref()).unwrap_or("thin"))?),
        false => (Color([0, 0, 0, 0]), 0.0),
    };
    let with_alpha =
        |Color([r, g, b, a]): Color, by: f64| Color([r, g, b, (f64::from(a) * by.clamp(0.0, 1.0)).round() as u8]);
    let rule_color = with_alpha(note_color, style.and_then(|a| a.opacity).unwrap_or(1.0));
    let band_color = with_alpha(note_color, style.and_then(|a| a.band).unwrap_or(0.12));
    let dimmed = style.and_then(|a| a.dimmed).unwrap_or(0.5).clamp(0.0, 1.0) as f32;

    // Colors: by series (or a categorical color field), by category on a donut, along
    // the sequential or diverging palette for a numeric color field, else the first.
    let shade_scale = shaded.then(|| {
        let values: Vec<f64> = rows.iter().filter_map(|r| r.shade).collect();
        let (lo, hi) = values.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
        let diverging = color_enc.and_then(|c| c.get("scale")).and_then(Value::as_str) == Some("diverging");
        let stops = if diverging { &theme.tokens.data.diverging } else { &theme.tokens.data.sequential };
        let stops: Vec<Color> = stops.iter().flatten().filter_map(|c| scaena_core::color::parse(&c.0).ok()).collect();
        let m = lo.abs().max(hi.abs());
        let domain = if diverging { [-m, m] } else { [lo, hi] };
        (stops, domain)
    });
    let color_of = |r: &Row| -> Color {
        if let (Some((stops, [lo, hi])), Some(v)) = (&shade_scale, r.shade)
            && !stops.is_empty()
        {
            let t = if hi > lo { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) as f32 } else { 0.5 };
            return along(stops, t);
        }
        // Its place among what the chart colors across the deck, else in this state.
        let deck_wide = |key: &str, local: usize| cx_colors.iter().position(|k| k == key).unwrap_or(local);
        let index = match (&r.series, donut) {
            (Some(s), _) => deck_wide(s, series.iter().position(|x| x == s).unwrap_or(0)),
            (None, true) => deck_wide(&r.category, categories.iter().position(|(k, _)| *k == r.category).unwrap_or(0)),
            (None, false) => 0,
        };
        palette[index % palette.len()]
    };
    // The legend: one entry per series, or per slice of a donut, when there are two
    // or more and the chart does not say `none`. A chart that does not say places it as
    // the theme does, else names each series where it ends (`direct`): beside its last
    // point, dot, or stack, or, on a donut, each slice beside its value. Bars grouped
    // side by side have no end to stand a name by, so their key stands over the plot.
    let grouped = kind == Kind::Bar && series.len() > 1;
    let auto = match charts.and_then(|c| c.legend.as_ref()).and_then(|l| l.place) {
        Some(LegendPlace::Top) => "top",
        Some(LegendPlace::Bottom) => "bottom",
        Some(LegendPlace::Right) => "right",
        Some(LegendPlace::None) => "none",
        Some(LegendPlace::Direct | LegendPlace::Auto) | None if grouped => "top",
        Some(LegendPlace::Direct | LegendPlace::Auto) | None => "direct",
    };
    let place = |place: &str| -> Result<&'static str, EngineError> {
        let place = match place {
            "auto" => auto,
            "direct" => "direct",
            "top" => "top",
            "bottom" => "bottom",
            "right" => "right",
            "none" => "none",
            other => {
                return Err(EngineError::Layout(format!(
                    "legend `{other}`: expected auto, direct, top, bottom, right, or none"
                )));
            }
        };
        Ok(place)
    };
    let (legend_place, legend_title) = match props.get("legend") {
        None => (place("auto")?, None),
        Some(Value::String(p)) => (place(p)?, None),
        // Unplaced, an object legend goes where `auto` puts it, but a titled one that
        // would stand at the ends goes on top, since `direct` takes no title.
        Some(Value::Object(spec)) => {
            let title = spec.get("title").and_then(Value::as_str);
            let place = match spec.get("place").and_then(Value::as_str) {
                Some(p) => place(p)?,
                None => match place("auto")? {
                    "direct" if title.is_some() => "top",
                    p => p,
                },
            };
            (place, title)
        }
        Some(other) => return Err(EngineError::Layout(format!("legend {other}: expected a place or an object"))),
    };
    if legend_place == "direct" && legend_title.is_some() {
        return Err(EngineError::Layout(
            "a `direct` legend names each series where it ends, and takes no title; place a titled legend `top`, \
             `bottom`, or `right`"
                .into(),
        ));
    }
    let direct = legend_place == "direct";
    // Names in a column past the series' ends, or a donut's beside its slices.
    let at_ends = direct && !donut;
    let entries: Vec<(String, Color)> = if donut {
        categories.iter().map(|(k, l)| (l.clone(), color_of(rows.iter().find(|r| r.category == *k).unwrap()))).collect()
    } else {
        series
            .iter()
            .map(|s| (s.clone(), color_of(rows.iter().find(|r| r.series.as_ref() == Some(s)).unwrap())))
            .collect()
    };
    let entries = if legend_place != "none" && entries.len() > 1 { entries } else { Vec::new() };

    // Text first: the plot is what the labels leave. Annotations speak in their own
    // role, in the annotation color.
    let note_texts: Vec<Option<TextLayout>> = notes
        .iter()
        .map(|n| {
            let Some(text) = &n.text else { return Ok(None) };
            let role = n.role.as_deref().or_else(|| style.and_then(|a| a.role.as_deref())).unwrap_or(&label_role);
            let mut role = theme.text_role(role)?;
            role.color = Some(note_token.to_string());
            cx.text.layout(cx.fonts, theme, &TextSpec::plain(role, text.clone()), f32::INFINITY).map(Some)
        })
        .collect::<Result<_, EngineError>>()?;
    let mut set = |text: String, role: &str| -> Result<TextLayout, EngineError> {
        let spec = TextSpec { numeric: Some(Numeric::TabularLining), ..TextSpec::plain(theme.text_role(role)?, text) };
        cx.text.layout(cx.fonts, theme, &spec, f32::INFINITY)
    };
    // Which values print when the chart does not say: the theme's choice, else by kind
    // (`auto`). The data goes on the marks: every bar, dot, and slice, and a line's
    // first and last; a scatter's or an area's none, read off the value axis instead.
    let theme_show = charts.and_then(|c| c.label.as_ref()).and_then(|l| l.show).map(|s| match s {
        LabelShow::All => "all",
        LabelShow::Ends => "ends",
        LabelShow::None => "none",
        LabelShow::Auto => "auto",
    });
    let show = (labels.and_then(|l| l.get("show")).and_then(Value::as_str)).or(theme_show).unwrap_or("auto");
    // Values nobody asked for print where they fit: those that collide hide, unless the
    // chart says how they resolve.
    let chosen = show != "auto";
    let show = match show {
        "auto" => match kind {
            Kind::Bar | Kind::StackedBar | Kind::Dot | Kind::Donut => "all",
            Kind::Line => "ends",
            Kind::Area | Kind::Scatter => "none",
        },
        "all" | "ends" | "none" => show,
        other => return Err(EngineError::Layout(format!("labels.show `{other}`: expected auto, all, ends, or none"))),
    };
    // Which rows' values print: all, or each series' first and last (a stack's first
    // and last category), or none.
    let ends: BTreeSet<usize> = {
        let mut out = BTreeSet::new();
        let mut first = std::collections::BTreeMap::new();
        let mut last = std::collections::BTreeMap::new();
        for (i, r) in rows.iter().enumerate() {
            let s = if kind == Kind::StackedBar { None } else { r.series.clone() };
            first.entry(s.clone()).or_insert(i);
            last.insert(s, i);
        }
        out.extend(first.into_values());
        out.extend(last.into_values());
        out
    };
    let labelled = |i: usize| -> bool {
        match (kind, show) {
            // A stack prints one label, its total, on its last segment.
            (Kind::StackedBar, "all" | "ends") => {
                let r = &rows[i];
                let c = categories.iter().position(|(k, _)| *k == r.category).unwrap_or(0);
                let last = rows.iter().rposition(|o| o.category == r.category) == Some(i);
                last && (show == "all" || c == 0 || c + 1 == categories.len())
            }
            (_, "all") => true,
            (_, "ends") => ends.contains(&i),
            _ => false,
        }
    };
    let value_format = y_format.clone().unwrap_or_else(NumberFormat::plain);
    let labelled_any = (0..rows.len()).any(labelled);
    // The minus sign the labels' font sets: U+2212 if it has one.
    let minus = match labelled_any || y_show {
        true => {
            let probe = set(format!("{MINUS}0"), if labelled_any { &label_role } else { &tick_role })?;
            if probe.runs.iter().flat_map(|r| &r.glyphs).any(|g| g.id == 0) { '-' } else { MINUS }
        }
        false => MINUS,
    };
    // A stacked bar's label prints its stack's total, on its top segment.
    let totals: Vec<(String, f64)> = match kind {
        Kind::StackedBar => categories
            .iter()
            .map(|(k, _)| (k.clone(), rows.iter().filter(|r| r.category == *k).map(|r| r.y).sum()))
            .collect(),
        _ => Vec::new(),
    };
    let mut values: Vec<Option<TextLayout>> = Vec::with_capacity(rows.len());
    for (i, r) in rows.iter().enumerate() {
        let shown = labelled(i);
        let v = match kind {
            Kind::StackedBar => totals.iter().find(|(k, _)| *k == r.category).map_or(r.y, |t| t.1),
            _ => r.y,
        };
        let mut text = typeset_minus(value_format.format(v, locale), minus);
        if r.projected {
            text = format!("{text}\u{a0}{note}");
        }
        values.push(if shown { Some(set(text, &label_role)?) } else { None });
    }
    let numerals = match labelled_any {
        true => {
            Numerals::shape(&value_format.alphabet(locale), y_format.clone(), locale, minus, |c| set(c, &label_role))?
        }
        false => None,
    };

    // The value axis: the data's extent (from zero for lengths; stacked, the stacks'),
    // widened to round ticks when the axis or its gridlines show. A bound the author
    // set stays.
    let stacked = kind == Kind::StackedBar || (kind == Kind::Area && !series.is_empty());
    let (min, max) = if stacked {
        categories.iter().fold((0.0_f64, f64::NEG_INFINITY), |(lo, hi), (k, _)| {
            let (pos, neg) = rows
                .iter()
                .filter(|r| r.category == *k)
                .fold((0.0, 0.0), |(p, n), r| if r.y >= 0.0 { (p + r.y, n) } else { (p, n + r.y) });
            (lo.min(neg), hi.max(pos))
        })
    } else {
        rows.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), r| (lo.min(r.y), hi.max(r.y)))
    };
    // An annotation's value is on the axis: a rule at a target the data has not reached
    // widens it as the data would.
    let note_ys: Vec<f64> =
        notes.iter().flat_map(|n| n.at.y.iter().flat_map(Place::values)).filter_map(|v| v.position(false)).collect();
    let (min, max) = note_ys.iter().fold((min, max), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let domain = y.get("domain").and_then(Value::as_array);
    let bound = |i: usize| domain.and_then(|d| d.get(i)).and_then(Value::as_f64);
    let lo = bound(0).unwrap_or(if kind.zero_based() { min.min(0.0) } else { min });
    let hi = bound(1).unwrap_or(max).max(lo + f64::EPSILON);
    let ruled = y_show || y_grid;
    // About `tickCount` ticks, and no more than `maxTicks`: the axis asks for fewer,
    // down to two, until its ticks fit, so it draws at most that many reference lines.
    let widen = [bound(0).is_none(), bound(1).is_none()];
    let fits = |count: usize| {
        let (a, b) = scale::nice(lo, hi, count, widen);
        scale::ticks(a, b, count).len() <= max_ticks
    };
    let y_count = (2..=tick_count.max(2)).rev().find(|&c| fits(c)).unwrap_or(2);
    let (lo, hi) = if ruled { scale::nice(lo, hi, y_count, widen) } else { (lo, hi) };
    if let Some(v) = note_ys.iter().find(|&&v| v < lo || v > hi) {
        return Err(EngineError::Layout(format!(
            "an annotation stands at y {v}, outside the value axis's `domain` [{lo}, {hi}]: widen the domain"
        )));
    }
    let tick_values = if ruled { scale::ticks(lo, hi, y_count) } else { Vec::new() };
    let step = scale::tick_step(lo, hi, y_count);
    let tick_format = match &y_format {
        Some(f) => f.for_ticks(step, lo.abs().max(hi.abs())),
        None => NumberFormat::ticks(step),
    };
    let tick_labels: Vec<(f64, String, Option<TextLayout>)> = tick_values
        .iter()
        .map(|&v| {
            let text = typeset_minus(tick_format.format(v, locale), minus);
            let label = if y_show { Some(set(text.clone(), &tick_role)?) } else { None };
            Ok((v, text, label))
        })
        .collect::<Result<_, EngineError>>()?;

    // The x axis: categories, or round ticks along a continuous x. A number axis widens
    // to round values as the value axis does, so no datum sits on the plot's edge; a
    // time axis spans the data.
    let temporal = continuous && table.types[xc] == ColumnType::Date;
    let x_extent = continuous.then(|| {
        let placed = notes.iter().filter(|n| n.kind != AnnotationKind::Highlight);
        let note_xs = placed.flat_map(|n| n.at.x.iter().flat_map(Place::values)).filter_map(|v| v.position(temporal));
        let (a, b) = (rows.iter().filter_map(|r| r.x).chain(note_xs))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
        if temporal { (a, b) } else { scale::nice(a, b, tick_count, [true, true]) }
    });
    let x_ticks: Vec<(f64, String)> = match x_extent {
        Some((a, b)) if temporal => {
            let (ticks, interval) = scale::time_ticks(DateTime(a as i64), DateTime(b as i64), tick_count);
            let f = match &x_format {
                Some(CategoryFormat::Date(f)) => f.clone(),
                _ => DateFormat::parse(interval.format()).expect("interval formats parse"),
            };
            ticks.into_iter().map(|t| (t.0 as f64, f.format(t, locale))).collect()
        }
        Some((a, b)) => {
            let step = scale::tick_step(a, b, tick_count);
            let f = match &x_format {
                Some(CategoryFormat::Number(f)) => f.for_ticks(step, a.abs().max(b.abs())),
                _ => NumberFormat::ticks(step),
            };
            scale::ticks(a, b, tick_count).into_iter().map(|v| (v, typeset_minus(f.format(v, locale), minus))).collect()
        }
        None => Vec::new(),
    };
    // Under a band, a date with no format prints short (`Feb`), or long (`Jan 2026`) where
    // it begins a run of its year among the labels the axis keeps, which depends on how
    // crowded they are (below): so each is set both ways where it could begin one.
    let periods: Vec<Option<i64>> = match &date_labels {
        Some(d) if !continuous => category_dates.iter().map(|t| t.map(|t| d.period(t))).collect(),
        _ => Vec::new(),
    };
    let one_period = periods.iter().flatten().collect::<BTreeSet<_>>().len() <= 1;
    let x_texts: Vec<(String, TextLayout)> = match (x_show, continuous) {
        (false, _) => Vec::new(),
        (true, false) => (categories.iter().zip(&category_dates))
            .map(|((k, l), t)| {
                let text = match (&date_labels, t) {
                    (Some(d), Some(t)) => d.short.format(*t, locale),
                    _ => l.clone(),
                };
                Ok((k.clone(), set(text, &tick_role)?))
            })
            .collect::<Result<_, EngineError>>()?,
        (true, true) => x_ticks
            .iter()
            .map(|(_, t)| Ok((t.clone(), set(t.clone(), &tick_role)?)))
            .collect::<Result<_, EngineError>>()?,
    };
    let x_longs: Vec<Option<TextLayout>> = match (&date_labels, x_show && !continuous) {
        (Some(DateLabels { long: Some(long), .. }), true) => (category_dates.iter().enumerate())
            .map(|(i, t)| match t {
                Some(t) if i == 0 || !one_period => set(long.format(*t, locale), &tick_role).map(Some),
                _ => Ok(None),
            })
            .collect::<Result<_, EngineError>>()?,
        _ => Vec::new(),
    };
    let titles: Vec<(&str, TextLayout)> = [("y", &y_title), ("x", &x_title)]
        .into_iter()
        .filter_map(|(k, t)| t.as_ref().map(|t| (k, t)))
        .map(|(k, t)| Ok((k, set(t.clone(), &title_role)?)))
        .collect::<Result<_, EngineError>>()?;
    // A legend's title, in the titles' role, when it has entries.
    let legend_title = match (legend_title, entries.is_empty()) {
        (Some(t), false) => Some(set(t.to_string(), &title_role)?),
        _ => None,
    };
    // Its entries in the legend's role and its color: a name says which series a mark
    // is by where it stands, not by its color, which may be a grey only a swatch shows.
    let legend_texts: Vec<(String, Color, TextLayout)> = entries
        .iter()
        .map(|(t, c)| {
            let role = theme.text_role(&legend_role)?;
            let spec = TextSpec { numeric: Some(Numeric::TabularLining), ..TextSpec::plain(role, t.clone()) };
            Ok((t.clone(), *c, cx.text.layout(cx.fonts, theme, &spec, f32::INFINITY)?))
        })
        .collect::<Result<_, EngineError>>()?;

    // The plot is what the text leaves.
    let cap = |t: &TextLayout| t.lines.first().map_or(0.0, |l| l.cap_height.unwrap_or(l.ascent));
    let below_cap = |t: &TextLayout| t.lines.first().map_or(0.0, |l| t.height - (l.baseline - cap(t)));
    let title_height = |key: &str| titles.iter().find(|(k, _)| *k == key).map_or(0.0, |(_, t)| t.height + gap);
    let (title_y, title_x) = (title_height("y"), title_height("x"));
    // The legend wraps across the chart's width, or stands in a column at its right. A
    // title starts it: the column's first line, or the start of the first row, the rows
    // of entries wrapping under one another after it.
    let mut legend_rows: Vec<Vec<usize>> = Vec::new();
    let swatch = legend_texts.iter().map(|(.., t)| cap(t)).fold(0.0_f32, f32::max);
    let beside = legend_place == "right";
    let indent = match (&legend_title, beside) {
        (Some(t), false) => t.width + 2.0 * gap,
        _ => 0.0,
    };
    if !direct {
        let mut used = f32::INFINITY;
        for (i, (.., t)) in legend_texts.iter().enumerate() {
            let w = swatch + 0.5 * gap + t.width;
            if beside || used + 2.0 * gap + w > size[0] - indent || legend_rows.is_empty() {
                legend_rows.push(Vec::new());
                used = -2.0 * gap;
            }
            legend_rows.last_mut().unwrap().push(i);
            used += 2.0 * gap + w;
        }
    }
    // A line above the entries in a column holds its title.
    let legend_lead = usize::from(beside && legend_title.is_some());
    let legend_texts_and_title = legend_texts.iter().map(|(.., t)| t).chain(&legend_title);
    let legend_line = legend_texts_and_title.clone().map(|t| t.height).fold(0.0_f32, f32::max);
    // Every entry, and the title, on one baseline a row.
    let legend_baseline = legend_texts_and_title.map(|t| t.lines[0].baseline).fold(0.0_f32, f32::max);
    let legend_height = match legend_rows.is_empty() || beside {
        true => 0.0,
        false => legend_rows.len() as f32 * legend_line + gap,
    };
    // Beside the plot, a gutter as wide as the widest value-axis label.
    let gutter = tick_labels.iter().filter_map(|(.., l)| l.as_ref()).map(|l| l.width + gap).fold(0.0_f32, f32::max);
    // Where a line's row stands in its series: at its first point (`true`) or its last.
    let end_of = |i: usize| -> Option<bool> {
        if kind != Kind::Line {
            return None;
        }
        let same = |o: &Row| o.series == rows[i].series;
        match (rows[..i].iter().any(same), rows[i + 1..].iter().any(same)) {
            (false, true) => Some(true),
            (true, false) => Some(false),
            _ => None,
        }
    };
    // A line's first value ends at its point and its last begins there (below), so the
    // plot leaves each the room it needs past its side: its width, less what its point
    // stands `f` of the plot's width in from that side (on a category axis half a band;
    // on a continuous one, what the axis widened past the data). That is the room `g`
    // with `width <= g + f × (the plot that g leaves)`.
    let ends: Vec<(bool, f32, f32)> = (rows.iter().zip(&values).enumerate())
        .filter_map(|(i, (r, text))| {
            let first = end_of(i)?;
            let at = match x_extent {
                Some((a, b)) if b > a => ((r.x? - a) / (b - a)) as f32,
                Some(_) => 0.5,
                None => {
                    let c = categories.iter().position(|(k, _)| *k == r.category)?;
                    (c as f32 + 0.5) / categories.len() as f32
                }
            };
            Some((first, text.as_ref()?.width, if first { at } else { 1.0 - at }))
        })
        .collect();
    let pad = |first: bool, plot: f32| {
        (ends.iter().filter(|e| e.0 == first))
            .map(|&(_, width, f)| {
                let f = f.clamp(0.0, 0.9);
                ((width - f * plot) / (1.0 - f)).max(0.0)
            })
            .fold(0.0_f32, f32::max)
    };
    // First values stand two spaces from the value axis's labels, so the two read apart.
    let first = pad(true, size[0] - gutter);
    let left = gutter + first + if gutter > 0.0 && first > 0.0 { gap } else { 0.0 };
    // A legend at the right takes its widest entry, or its title, and two spaces from the
    // plot.
    let widest = legend_texts.iter().map(|(.., t)| t.width).fold(0.0_f32, f32::max);
    // How far past a series' end its name starts: a space, or on a line, a space past
    // the value label that begins at its last point.
    let lead = match kind {
        Kind::Line => values.iter().flatten().map(|t| t.width + gap).fold(gap, f32::max),
        _ => gap,
    };
    let legend_width = match (beside, at_ends, legend_texts.is_empty()) {
        (_, _, true) | (false, false, _) => 0.0,
        (true, ..) => (swatch + 0.5 * gap + widest).max(legend_title.as_ref().map_or(0.0, |t| t.width)) + 2.0 * gap,
        // Names `lead` past the series' ends. The farthest end stands `f` of the plot's
        // width in from its side: on a category axis a line's or a dot's half a band, or
        // a bar's half its gap; on a continuous one, what the axis widened past the data.
        // A dot's name starts past the dot's edge. So the names need only what that leaves
        // them short of: the room `g` with `edge + lead + widest <= g + f × (the plot that
        // g leaves)`.
        (false, true, _) => {
            let f = match x_extent {
                Some((a, b)) => {
                    let last = rows.iter().filter_map(|r| r.x).fold(f64::NEG_INFINITY, f64::max);
                    if b > a && last.is_finite() { ((b - last) / (b - a)) as f32 } else { 0.0 }
                }
                None => {
                    let k = if matches!(kind, Kind::Bar | Kind::StackedBar) { 0.5 * bar_gap } else { 0.5 };
                    k / categories.len().max(1) as f32
                }
            };
            let f = f.clamp(0.0, 0.9);
            let edge = match kind {
                Kind::Dot => dot_radius,
                Kind::Scatter if rows.iter().any(|r| r.size.is_some()) => 2.5 * dot_radius,
                Kind::Scatter => dot_radius,
                _ => 0.0,
            };
            ((edge + lead + widest - f * (size[0] - left)) / (1.0 - f)).max(0.0)
        }
    };
    // Names at the ends stand past the value there, so their gutter has its room.
    let named = at_ends && !legend_texts.is_empty();
    let pad_right = if named { 0.0 } else { pad(false, size[0] - legend_width - left) };
    let right = size[0] - legend_width - pad_right;
    // How far the ends' values stand past the plot's sides, within the room beside it.
    let reach = |first: bool| {
        (ends.iter().filter(|e| e.0 == first))
            .map(|&(_, width, f)| (width - f * (right - left)).max(0.0))
            .fold(0.0_f32, f32::max)
    };
    let room = [reach(true).min(left - gutter), reach(false).min(if named { legend_width } else { pad_right })];
    let label_room = values.iter().flatten().map(cap).fold(0.0_f32, f32::max);
    let tick_cap = tick_labels.iter().filter_map(|(.., l)| l.as_ref()).map(cap).fold(0.0_f32, f32::max);
    let legend_top = if legend_place == "top" { legend_height } else { 0.0 };
    // A callout's text stands three space units over its point, and a rule's half a
    // space unit over the rule, either of which may be the top of the plot.
    let note_room = (notes.iter().zip(&note_texts))
        .filter_map(|(n, t)| match n.kind {
            AnnotationKind::Callout => t.as_ref().map(|t| 3.0 * gap + cap(t)),
            AnnotationKind::Rule if n.at.y.is_some() => t.as_ref().map(|t| 0.5 * gap + cap(t)),
            _ => None,
        })
        .fold(0.0_f32, f32::max);
    // Above the plot: the value axis's title, the legend, then room for value labels
    // over the tallest mark or half a tick label's cap over the top gridline, and for
    // annotations over them.
    let top =
        title_y + legend_top + (if label_room > 0.0 { label_room + gap } else { 0.0 }).max(0.5 * tick_cap) + note_room;
    let mut bottom = size[1];
    if x_show {
        bottom = bottom - gap - x_texts.iter().map(|(_, t)| below_cap(t)).fold(0.0_f32, f32::max);
    }
    bottom -= title_x;
    if legend_place == "bottom" {
        bottom -= legend_height;
    }
    if bottom - top <= 0.0 || right - left <= 0.0 {
        return Err(EngineError::Layout(format!("chart cell {}×{} cu leaves no room to plot", size[0], size[1])));
    }

    // Scales.
    let y_scale = LinearScale { domain: [lo, hi], range: [bottom, top] };
    let to_y = |v: f64| y_scale.map(v);
    let base = to_y(0.0_f64.clamp(lo, hi));
    let band = (right - left) / categories.len() as f32;
    let x_scale =
        x_extent.map(|(a, b)| LinearScale { domain: [a, if b > a { b } else { a + 1.0 }], range: [left, right] });
    let center_of = |r: &Row| -> f32 {
        match (&x_scale, r.x) {
            (Some(s), Some(x)) => s.map(x),
            _ => {
                let i = categories.iter().position(|(k, _)| *k == r.category).unwrap_or(0);
                left + (i as f32 + 0.5) * band
            }
        }
    };

    // A rule at 0 where the value axis reaches it: else no rule reads as zero.
    let zero_ruled = !donut && lo <= 0.0 && 0.0 <= hi;
    let mut out = ChartLayout {
        kind,
        base,
        baseline: zero_ruled.then_some(Rule {
            from: [left, base],
            to: [right, base],
            width: axis_width,
            color: axis_color,
        }),
        marks: Vec::with_capacity(rows.len()),
        ticks: Vec::new(),
        labels: Vec::new(),
        numerals,
        paths: Vec::new(),
        y_scale,
        plot: [left, top, right - left, bottom - top],
        clip: (gutter > 0.0 || legend_width > 0.0).then_some([left - room[0], right + room[1]]),
        y_axis: Vec::with_capacity(tick_labels.len()),
        titles: Vec::new(),
        legend: Vec::new(),
        x_grid: Vec::new(),
        collisions: Vec::new(),
        crowded: Vec::new(),
        covers: Vec::new(),
        notes: Vec::new(),
    };
    for (v, key, label) in tick_labels {
        let y = to_y(v);
        // The baseline already rules the line it sits on.
        let rule = (y_grid && !(zero_ruled && y == base)).then_some(Rule {
            from: [left, y],
            to: [right, y],
            width: grid_width,
            color: grid_color,
        });
        // Right-aligned in the gutter, the middle of its cap height on the tick.
        let label = label.map(|text| {
            let first = &text.lines[0];
            let origin = [gutter - gap - text.width, y + 0.5 * cap(&text) - first.baseline];
            Label::new(key.clone(), origin, text, None)
        });
        out.y_axis.push(AxisTick { key, value: v, rule, label });
    }
    if let Some(s) = &x_scale
        && x_grid
    {
        for (v, key) in &x_ticks {
            let x = s.map(*v);
            let rule = Rule { from: [x, top], to: [x, bottom], width: grid_width, color: grid_color };
            out.x_grid.push(AxisTick { key: key.clone(), value: *v, rule: Some(rule), label: None });
        }
    }
    // Across a category axis: between the bands of bars, through each category of the
    // rest. A gridline is keyed by the category after it, and moves with it.
    if x_scale.is_none() && x_grid {
        let between = matches!(kind, Kind::Bar | Kind::StackedBar);
        for (i, (key, _)) in categories.iter().enumerate().skip(usize::from(between)) {
            let x = left + (i as f32 + if between { 0.0 } else { 0.5 }) * band;
            let rule = Rule { from: [x, top], to: [x, bottom], width: grid_width, color: grid_color };
            out.x_grid.push(AxisTick { key: key.clone(), value: i as f64, rule: Some(rule), label: None });
        }
    }
    for (key, text) in titles {
        // The value axis's title above the plot at the chart's left; the category
        // axis's title centered under the category labels.
        let under = if legend_place == "bottom" { legend_height } else { 0.0 };
        let origin = match key {
            "y" => [0.0, 0.0],
            _ => [left + 0.5 * (right - left) - 0.5 * text.width, size[1] - under - text.height],
        };
        out.titles.push(Label::new(key, origin, text, None));
    }
    // Legend entries: a swatch, its cap height square, on the label's baseline; above
    // the plot under the value axis's title, at the chart's foot, or in a column right
    // of the plot from its top.
    let legend_y = match legend_place {
        "bottom" => size[1] - legend_height + gap,
        "right" => top,
        _ => title_y,
    };
    let legend_x = if beside { size[0] - legend_width + 2.0 * gap } else { 0.0 };
    if let Some(text) = legend_title {
        let origin = [legend_x, legend_y + legend_baseline - text.lines[0].baseline];
        out.titles.push(Label::new("legend", origin, text, None));
    }
    let mut legend_texts: Vec<Option<(String, Color, TextLayout)>> = legend_texts.into_iter().map(Some).collect();
    for (r, line) in legend_rows.iter().enumerate() {
        let mut x0 = legend_x + indent;
        for &i in line {
            let (key, color, text) = legend_texts[i].take().expect("each entry once");
            let first = &text.lines[0];
            let baseline = legend_y + (r + legend_lead) as f32 * legend_line + legend_baseline;
            let swatch = RoundRect {
                x: x0,
                y: baseline - swatch,
                w: swatch,
                h: swatch,
                top_radius: corner.min(0.5 * swatch),
                bottom_radius: corner.min(0.5 * swatch),
            };
            let origin = [x0 + swatch.w + 0.5 * gap, baseline - first.baseline];
            x0 = origin[0] + text.width + 2.0 * gap;
            out.legend.push(LegendEntry {
                key: key.clone(),
                swatch,
                color,
                label: Label::new(key, origin, text, None),
            });
        }
    }

    // Marks.
    // A value label over its mark (under a bar below its foot), centered on `cx`.
    let label_at = |out: &mut ChartLayout,
                    value: Option<TextLayout>,
                    key: &str,
                    shape: &Shape,
                    cx: f32,
                    v: f64,
                    below: bool,
                    side: f32| {
        let Some(text) = value else { return };
        let first = &text.lines[0];
        let (offset, origin_y) = match shape {
            Shape::Bar(r) if below => {
                let origin_y = r.bottom() + gap - text.trimmed(TextBox::Cap).0;
                (origin_y + first.baseline - r.bottom(), origin_y)
            }
            Shape::Bar(r) => (-gap, r.top() - gap - first.baseline),
            _ => {
                let at = ValueLabel { value: v, below: false, offset: -gap, align: 0.5, drop: 0.0 };
                (-gap, at.anchor(shape)[1] - first.baseline)
            }
        };
        // Centered on its mark (or ending or beginning there, as `side` says), unless
        // that would put it past the plot's side, or a line's end value past the room
        // beside it.
        let (from, to) = if side == 0.5 { (left, right) } else { (left - room[0], right + room[1]) };
        let x0 = (cx - side * text.width).clamp(from, (to - text.width).max(from));
        let align = if x0 == cx - side * text.width { side } else { (cx - x0) / text.width.max(f32::EPSILON) };
        let value = ValueLabel { value: v, below, offset, align, drop: 0.0 };
        out.labels.push(Label::new(key, [x0, origin_y], text, Some(value)));
    };
    match kind {
        Kind::Bar | Kind::StackedBar => {
            let groups = if kind == Kind::Bar { series.len().max(1) } else { 1 };
            // Each category's stack so far, up and down.
            let mut stacks: Vec<(f64, f64)> = vec![(0.0, 0.0); categories.len()];
            for (i, (r, value)) in rows.iter().zip(values).enumerate() {
                let c = categories.iter().position(|(k, _)| *k == r.category).unwrap_or(0);
                let center = left + (c as f32 + 0.5) * band;
                let w = band * (1.0 - bar_gap);
                // A group shares the bar's width; a lone bar keeps all of it.
                let (cx, w) = match (groups, &r.series) {
                    (1, _) | (_, None) => (center, w),
                    (n, Some(s)) => {
                        let slot = w / n as f32;
                        let k = series.iter().position(|x| x == s).unwrap_or(0);
                        (center - 0.5 * w + (k as f32 + 0.5) * slot, slot * (1.0 - group_gap))
                    }
                };
                // A stacked segment stands on the stack under it (values below zero
                // stack down from the baseline), a bar on the baseline.
                let (to, foot, place) = if kind == Kind::StackedBar {
                    let stack = &mut stacks[c];
                    let from = if r.y >= 0.0 { stack.0 } else { stack.1 };
                    let to = from + r.y;
                    if r.y >= 0.0 {
                        stack.0 = to;
                    } else {
                        stack.1 = to;
                    }
                    let key = format!("{}\u{1f}{}", r.category, if r.y >= 0.0 { '+' } else { '-' });
                    (to, to_y(from), Some(Stack { key, from: to_y(from), to: to_y(to) }))
                } else {
                    (r.y, base, None)
                };
                let at = to_y(to);
                let (top, bottom) = (at.min(foot), at.max(foot));
                let below = at > foot;
                // Square on its foot, rounded at its free end; a stack rounds only its
                // outermost segment each way.
                let outermost = kind != Kind::StackedBar
                    || rows.iter().rposition(|o| o.category == r.category && (o.y >= 0.0) == (r.y >= 0.0)) == Some(i);
                let radius = if outermost { corner.min(0.5 * w).min(bottom - top) } else { 0.0 };
                let (top_radius, bottom_radius) = if below { (0.0, radius) } else { (radius, 0.0) };
                let shape =
                    Shape::Bar(RoundRect { x: cx - 0.5 * w, y: top, w, h: bottom - top, top_radius, bottom_radius });
                let v = match kind {
                    Kind::StackedBar => totals.iter().find(|(k, _)| *k == r.category).map_or(r.y, |t| t.1),
                    _ => r.y,
                };
                label_at(&mut out, value, &r.key, &shape, cx, v, below, 0.5);
                out.marks.push(Mark { key: r.key.clone(), shape, color: color_of(r), stack: place });
            }
        }
        Kind::Line | Kind::Area | Kind::Dot | Kind::Scatter => {
            let stack_areas = kind == Kind::Area && !series.is_empty();
            let mut stacks: Vec<f64> = vec![0.0; categories.len()];
            let size_max = rows.iter().filter_map(|r| r.size).fold(0.0_f64, f64::max);
            for (i, (r, value)) in rows.iter().zip(values).enumerate() {
                let x = center_of(r);
                let mut place = None;
                let shape = match kind {
                    Kind::Area => {
                        let c = categories.iter().position(|(k, _)| *k == r.category).unwrap_or(0);
                        let from = if stack_areas { stacks[c] } else { 0.0_f64.clamp(lo, hi) };
                        let to = if stack_areas { from + r.y } else { r.y };
                        if stack_areas {
                            stacks[c] = to;
                            place = Some(Stack { key: r.category.clone(), from: to_y(from), to: to_y(to) });
                        }
                        Shape::Span { x, top: to_y(to), base: to_y(from) }
                    }
                    Kind::Scatter => {
                        // By area, the largest two and a half dots across, and none
                        // smaller than a dot: a speck does not read.
                        let r_ = match (r.size, size_max > 0.0) {
                            (Some(s), true) => {
                                (2.5 * dot_radius * (s.max(0.0) / size_max).sqrt() as f32).max(dot_radius)
                            }
                            _ => dot_radius,
                        };
                        Shape::Dot { x, y: to_y(r.y), r: r_ }
                    }
                    Kind::Dot => Shape::Dot { x, y: to_y(r.y), r: dot_radius },
                    _ => Shape::Dot { x, y: to_y(r.y), r: point_radius },
                };
                // A line's first value ends at its point and its last begins there, away
                // from the line, which leaves the one and comes to the other.
                let side = match end_of(i) {
                    Some(true) => 1.0,
                    Some(false) => 0.0,
                    None => 0.5,
                };
                label_at(&mut out, value, &r.key, &shape, x, r.y, false, side);
                if r.projected
                    && let Some(label) = out.labels.last_mut().filter(|l| l.key == r.key)
                {
                    label.noted = true;
                }
                out.marks.push(Mark { key: r.key.clone(), shape, color: color_of(r), stack: place });
            }
            if matches!(kind, Kind::Line | Kind::Area) {
                let groups: Vec<Option<String>> =
                    if series.is_empty() { vec![None] } else { series.iter().cloned().map(Some).collect() };
                for s in groups {
                    let members: Vec<&Row> = rows.iter().filter(|r| r.series == s).collect();
                    let Some(first) = members.first() else { continue };
                    out.paths.push(SeriesPath {
                        key: s.unwrap_or_default(),
                        color: color_of(first),
                        stroke: (kind == Kind::Line).then_some(line_width),
                        marks: members.iter().map(|r| r.key.clone()).collect(),
                        projected: members.iter().filter(|r| r.projected).map(|r| r.key.clone()).collect(),
                        dash,
                        fade,
                    });
                }
            }
        }
        Kind::Donut => {
            let total: f64 = rows.iter().map(|r| r.y).sum();
            let (cx, cy) = (left + 0.5 * (right - left), top + 0.5 * (bottom - top));
            // Each slice's start and end, as fractions of the turn.
            let mut at = 0.0_f64;
            let turns: Vec<(f32, f32)> = (rows.iter())
                .map(|r| {
                    let start = if total > 0.0 { (at / total) as f32 } else { 0.0 };
                    at += r.y;
                    (start, if total > 0.0 { (at / total) as f32 } else { 0.0 })
                })
                .collect();
            // A value stands outside its slice's middle, on its side of the ring: its
            // start there on the right, its end on the left, its middle at the top and
            // foot, the middle of its cap height level with the point. The point is `gap`
            // past the ring, and half the cap height more toward the top and the foot, so
            // the value clears the ring all round. A slice's name (`direct`) stands with
            // it, on its far side from the ring: over it on the ring's upper half, under it
            // on the lower; or alone, where its value would.
            let side = |sin: f32| match sin {
                s if s > 0.05 => 0.0,
                s if s < -0.05 => 1.0,
                _ => 0.5,
            };
            let offset = |cos: f32, text: &TextLayout| gap + cos.abs() * 0.5 * cap(text);
            let mut names: Vec<Option<(Color, TextLayout)>> = (rows.iter())
                .map(|r| {
                    let slot = legend_texts.iter_mut().find(|e| e.as_ref().is_some_and(|(k, ..)| *k == r.label))?;
                    slot.take().map(|(_, color, text)| (color, text))
                })
                .collect();
            // Each slice's words about its point: how wide, and how far they rise over it
            // and fall under it.
            let block = |value: Option<&TextLayout>, name: Option<&TextLayout>, cos: f32| {
                let lead = value.or(name).expect("a value or a name");
                let up = lead.lines[0].baseline - 0.5 * cap(lead);
                let (mut up, mut down) = (up, lead.height - up);
                if let (Some(_), Some(name)) = (value, name) {
                    if cos >= 0.0 {
                        up += name.height;
                    } else {
                        down += name.height;
                    }
                }
                let w = value.map_or(0.0, |t| t.width).max(name.map_or(0.0, |t| t.width));
                (w, up, down)
            };
            // The ring is as large as the plot, less the room its words need: the
            // largest radius at which each slice's stay inside the plot, on every side.
            let mut outer = 0.5 * (right - left).min(bottom - top);
            for ((&(start, end), text), name) in turns.iter().zip(&values).zip(&names) {
                let name = name.as_ref().map(|(_, t)| t);
                if text.is_none() && name.is_none() {
                    continue;
                }
                let mid = 0.5 * (start + end) * core::f32::consts::TAU;
                let (sin, cos) = (libm::sinf(mid), libm::cosf(mid));
                let align = side(sin);
                let (w, up, down) = block(text.as_ref(), name, cos);
                let mut reach = f32::INFINITY;
                if sin > 0.0 {
                    reach = reach.min((right - cx - (1.0 - align) * w) / sin);
                }
                if sin < 0.0 {
                    reach = reach.min((cx - left - align * w) / -sin);
                }
                if cos > 0.0 {
                    reach = reach.min((cy - top - up) / cos);
                }
                if cos < 0.0 {
                    reach = reach.min((bottom - cy - down) / -cos);
                }
                let lead = text.as_ref().or(name).expect("a value or a name");
                outer = outer.min(reach - offset(cos, lead));
            }
            let outer = outer.max(1.0);
            for (((r, value), &(start, end)), name) in rows.iter().zip(values).zip(&turns).zip(&mut names) {
                let shape = Shape::Arc { cx, cy, inner: outer * hole, outer, start, end };
                let mid = 0.5 * (start + end) * core::f32::consts::TAU;
                let (align, cos) = (side(libm::sinf(mid)), libm::cosf(mid));
                // Where a value's line box stands, and so where its name goes.
                let mut stands: Option<(f32, f32)> = None;
                if let Some(text) = value {
                    let value = ValueLabel {
                        value: r.y,
                        below: false,
                        offset: offset(cos, &text),
                        align,
                        drop: 0.5 * cap(&text),
                    };
                    let [ax, baseline] = value.anchor(&shape);
                    let origin = [ax - align * text.width, baseline - text.lines[0].baseline];
                    stands = Some((origin[1], origin[1] + text.height));
                    out.labels.push(Label::new(r.key.clone(), origin, text, Some(value)));
                }
                if let Some((color, text)) = name.take() {
                    let at = ValueLabel {
                        value: r.y,
                        below: false,
                        offset: offset(cos, &text),
                        align,
                        drop: 0.5 * cap(&text),
                    };
                    let [ax, baseline] = at.anchor(&shape);
                    let y = match stands {
                        Some((top, _)) if cos >= 0.0 => top - text.height,
                        Some((_, foot)) => foot,
                        None => baseline - text.lines[0].baseline,
                    };
                    let origin = [ax - align * text.width, y];
                    let baseline = y + text.lines[0].baseline;
                    let swatch =
                        RoundRect { x: origin[0], y: baseline, w: 0.0, h: 0.0, top_radius: 0.0, bottom_radius: 0.0 };
                    let label = Label::new(r.label.clone(), origin, text, None);
                    out.legend.push(LegendEntry { key: r.label.clone(), swatch, color, label });
                }
                let place = Stack { key: String::new(), from: start, to: end };
                out.marks.push(Mark { key: r.key.clone(), shape, color: color_of(r), stack: Some(place) });
            }
        }
    }
    // Names at the series' ends (`direct`): each beside the plot (a scatter's beside its
    // series' last point), the middle of its cap height level with where its series ends
    // across (a line's last point, its last dot or bar, the middle of a stack's last span
    // or segment), nudged apart as value labels are and kept inside the chart. Each is a legend entry with no swatch, so it
    // moves, recolors, and fades as entries do.
    if at_ends {
        let mut names: Vec<(String, Color)> = Vec::new();
        let mut placed: Vec<Label> = Vec::new();
        // In a column `lead` past the farthest end of any series.
        let reach = |shape: &Shape| match *shape {
            Shape::Dot { x, r, .. } => x + r,
            Shape::Bar(b) => b.x + b.w,
            other => other.center_x(),
        };
        let column = out.marks.iter().map(|m| reach(&m.shape)).fold(left, f32::max) + lead;
        for slot in &mut legend_texts {
            let Some((key, color, text)) = slot.take() else { continue };
            let end = (rows.iter().zip(&out.marks))
                .filter(|(r, _)| r.series.as_deref() == Some(key.as_str()))
                .map(|(_, m)| m.shape)
                .max_by(|a, b| a.center_x().total_cmp(&b.center_x()));
            let Some(end) = end else { continue };
            let y = match end {
                Shape::Dot { y, .. } => y,
                Shape::Span { top, base, .. } => 0.5 * (top + base),
                Shape::Bar(r) => 0.5 * (r.top() + r.bottom()),
                Shape::Arc { cy, .. } => cy,
            };
            // A scatter's series end where their points do, apart from one another: each
            // name stands beside its own last point.
            let x = if kind == Kind::Scatter { reach(&end) + lead } else { column };
            let origin = [x, y + 0.5 * cap(&text) - text.lines[0].baseline];
            names.push((key.clone(), color));
            placed.push(Label::new(key, origin, text, None));
        }
        nudge(&mut placed, 0.25 * gap);
        let (high, low) =
            (placed.iter().map(ink)).fold((f32::INFINITY, f32::NEG_INFINITY), |(h, l), b| (h.min(b[1]), l.max(b[3])));
        let shift = if high < 0.0 {
            -high
        } else if low > size[1] {
            (size[1] - low).max(-high)
        } else {
            0.0
        };
        for ((key, color), mut label) in names.into_iter().zip(placed) {
            label.origin[1] += shift;
            let baseline = label.origin[1] + label.text.lines[0].baseline;
            let swatch =
                RoundRect { x: label.origin[0], y: baseline, w: 0.0, h: 0.0, top_radius: 0.0, bottom_radius: 0.0 };
            out.legend.push(LegendEntry { key, swatch, color, label });
        }
    }
    // Category labels under each band, or x ticks along a continuous x, each centered on
    // its band or tick but inside the plot's sides.
    let center = |i: usize| match &x_scale {
        Some(s) => x_ticks.get(i).map_or(left, |(v, _)| s.map(*v)),
        None => left + (i as f32 + 0.5) * band,
    };
    let at = |i: usize, text: &TextLayout| (center(i) - 0.5 * text.width).clamp(left, (right - text.width).max(left));
    // The labels at a stride of `k` from the first, each long where its year (a time's
    // day) is not the last one kept's.
    let pick = |k: usize| -> Vec<(usize, bool)> {
        let mut last = None;
        (0..x_texts.len())
            .step_by(k)
            .map(|i| {
                let period = periods.get(i).copied().flatten();
                let long = period.is_some() && period != last && x_longs.get(i).is_some_and(Option::is_some);
                last = period;
                (i, long)
            })
            .collect()
    };
    let text = |&(i, long): &(usize, bool)| match long {
        true => x_longs[i].as_ref().unwrap_or(&x_texts[i].1),
        false => &x_texts[i].1,
    };
    // Neighbors less than `space` apart.
    let crowded = |kept: &[(usize, bool)], space: f32| -> Vec<(usize, usize)> {
        (kept.windows(2))
            .filter(|w| {
                let (a, b) = (text(&w[0]), text(&w[1]));
                at(w[0].0, a) + a.width + space > at(w[1].0, b)
            })
            .map(|w| (w[0].0, w[1].0))
            .collect()
    };
    // Where an ordered axis's labels (dates, numbers) come within a space of each other,
    // it keeps every k-th from the first, at the smallest stride that clears them. A
    // text axis keeps every category, and reports those that overlap (W310).
    let n = x_texts.len();
    let ordered = matches!(table.types[xc], ColumnType::Date | ColumnType::Number);
    let kept = match (&x_scale, ordered) {
        (Some(_), _) => pick(1),
        (None, true) => (strides(date_labels.as_ref().map(|d| d.unit)).take_while(|&k| k < n))
            .map(&pick)
            .find(|kept| crowded(kept, gap).is_empty())
            .unwrap_or_else(|| pick(n.max(1))),
        (None, false) => {
            let kept = pick(1);
            let keys = |(a, b): (usize, usize)| (x_texts[a].0.clone(), x_texts[b].0.clone());
            out.crowded = crowded(&kept, 0.25 * gap).into_iter().map(keys).collect();
            kept
        }
    };
    let mut shorts: Vec<Option<(String, TextLayout)>> = x_texts.into_iter().map(Some).collect();
    let mut longs = x_longs;
    for (i, long) in kept {
        let Some((key, short)) = shorts[i].take() else { continue };
        let text = match long.then(|| longs.get_mut(i).and_then(Option::take)).flatten() {
            Some(text) => text,
            None => short,
        };
        let origin = [at(i, &text), y_scale.range[0] + gap - text.trimmed(TextBox::Cap).0];
        out.ticks.push(Label::new(key, origin, text, None));
    }
    // A value label does not cover another mark. A dot's that would goes under its dot;
    // a bar's wider than its bar leans off the mark, starting or ending at the bar's
    // edge; one that still would hides, unless the chart asked for its values, which
    // reports it (W310).
    let apart = 0.25 * gap;
    let collide = labels.and_then(|l| l.get("collide")).and_then(Value::as_str);
    let boxes: Vec<(String, Shape, [f32; 4])> = (out.marks.iter())
        .filter_map(|m| match m.shape {
            Shape::Dot { x, y, r } if r > 0.0 => Some((m.key.clone(), m.shape, [x - r, y - r, x + r, y + r])),
            Shape::Bar(b) if b.h > 0.0 => Some((m.key.clone(), m.shape, [b.x, b.top(), b.x + b.w, b.bottom()])),
            _ => None,
        })
        .collect();
    let covered = |l: &Label| -> Option<String> {
        (boxes.iter()).find(|(k, _, b)| *k != l.key && near(ink(l), *b, apart)).map(|(k, ..)| k.clone())
    };
    let mut covering: Vec<(String, String)> = Vec::new();
    for l in &mut out.labels {
        let Some(mark) = covered(l) else { continue };
        let own = boxes.iter().find(|(k, ..)| *k == l.key).map(|(_, s, _)| *s);
        if let (Some(Shape::Dot { y, r, .. }), Some(v)) = (own, l.value)
            && !v.below
        {
            let mut under = l.clone();
            let first = &under.text.lines[0];
            let offset = gap + first.cap_height.unwrap_or(first.ascent);
            under.origin[1] = y + r + offset - first.baseline;
            under.value = Some(ValueLabel { below: true, offset, drop: 0.0, ..v });
            if covered(&under).is_none() {
                *l = under;
                continue;
            }
        }
        if let (Some(Shape::Bar(b)), Some(v)) = (own, l.value)
            && l.text.width > b.w
        {
            // Away from the mark it covers first: starting at the bar's left edge, or
            // ending at its right.
            let width = l.text.width;
            let starts = (0.5 * b.w / width, b.x);
            let ends = (1.0 - 0.5 * b.w / width, b.x + b.w - width);
            let before =
                boxes.iter().find(|(k, ..)| *k == mark).is_some_and(|(.., m)| m[0] + m[2] < 2.0 * b.center_x());
            let leaned = (if before { [starts, ends] } else { [ends, starts] }).into_iter().find_map(|(align, x)| {
                let mut leaned = l.clone();
                leaned.origin[0] = x;
                leaned.value = Some(ValueLabel { align, ..v });
                (x >= left && x + width <= right && covered(&leaned).is_none()).then_some(leaned)
            });
            if let Some(leaned) = leaned {
                *l = leaned;
                continue;
            }
        }
        covering.push((l.key.clone(), mark));
    }
    if !chosen || collide == Some("hide") {
        out.labels.retain(|l| !covering.iter().any(|(k, _)| *k == l.key));
    } else {
        out.covers = covering;
    }
    // Value labels that overlap one another: hidden or nudged apart as `labels.collide`
    // says, else reported (W310); values the chart did not ask for hide.
    match collide {
        None if !chosen => hide(&mut out.labels, apart),
        None => out.collisions = collisions(&out.labels, apart),
        Some("hide") => hide(&mut out.labels, apart),
        Some("nudge") => nudge(&mut out.labels, apart),
        Some(other) => return Err(EngineError::Layout(format!("labels.collide `{other}`: expected hide or nudge"))),
    }

    // Annotations. An x is a category's band (its start, middle, and end), or a point
    // along a continuous x.
    let x_at = |v: &Scalar| -> Result<[f32; 3], EngineError> {
        match &x_scale {
            Some(s) => {
                let at = v.position(temporal).ok_or_else(|| {
                    let wants = if temporal { "a date in ISO 8601" } else { "a number" };
                    EngineError::Layout(format!("an annotation's x `{}` must be {wants}", v.label()))
                })?;
                let x = s.map(at);
                Ok([x, x, x])
            }
            None => {
                let label = v.label();
                let i = categories.iter().position(|(k, _)| *k == label).ok_or_else(|| {
                    EngineError::Data(format!("an annotation stands at `{label}`, no category of `{x_field}`"))
                })?;
                let start = left + i as f32 * band;
                Ok([start, start + 0.5 * band, start + band])
            }
        }
    };
    // Whether an annotation's `x` and `series` pick out a datum.
    let picks = |note: &Annotation, r: &Row| -> bool {
        let x = note.at.x.as_ref().is_none_or(|p| {
            p.values().into_iter().any(|v| match (&x_scale, r.x) {
                (Some(_), Some(x)) => v.position(temporal) == Some(x),
                _ => v.label() == r.category,
            })
        });
        let series = (note.at.series.as_ref())
            .is_none_or(|p| p.values().into_iter().any(|v| r.series.as_deref() == Some(v.label().as_str())));
        x && series
    };
    // Every value an annotation names is in the data.
    for (k, note) in notes.iter().enumerate() {
        for (axis, place) in [("x", &note.at.x), ("series", &note.at.series)] {
            for v in place.iter().flat_map(Place::values) {
                let found = match axis {
                    "x" if note.kind != AnnotationKind::Highlight => x_at(v).map(|_| true)?,
                    "x" => rows.iter().any(|r| match (&x_scale, r.x) {
                        (Some(_), Some(x)) => v.position(temporal) == Some(x),
                        _ => v.label() == r.category,
                    }),
                    _ => rows.iter().any(|r| r.series.as_deref() == Some(v.label().as_str())),
                };
                if !found {
                    return Err(EngineError::Data(format!("annotation {k}: no datum has {axis} `{}`", v.label())));
                }
            }
        }
    }
    // Where each rule across the plot stands: a callout's text rises past one it would
    // cross.
    let levels: Vec<f32> = (notes.iter())
        .filter(|n| n.kind == AnnotationKind::Rule)
        .filter_map(|n| n.at.y.as_ref())
        .map(|p| to_y(p.values()[0].position(false).expect("checked: y is a number")))
        .collect();
    let mut seen: Vec<(AnnotationKind, &str)> = Vec::new();
    for (note, text) in notes.iter().zip(note_texts) {
        if note.kind == AnnotationKind::Highlight {
            continue;
        }
        // Keyed by kind, axis, and place among the chart's annotations of both.
        let axis = if note.kind != AnnotationKind::Callout && note.at.y.is_some() { "y" } else { "x" };
        let n = seen.iter().filter(|s| **s == (note.kind, axis)).count();
        seen.push((note.kind, axis));
        let name = match note.kind {
            AnnotationKind::Rule => "rule",
            AnnotationKind::Band => "band",
            _ => "callout",
        };
        let mut out_note =
            Note { key: format!("{name}\u{1f}{axis}\u{1f}{n}"), band: None, rule: None, gaps: Vec::new(), label: None };
        let first_y = |p: &Place| p.values()[0].position(false).expect("checked: y is a number");
        // Where its text goes: its top-left corner.
        let origin: Option<[f32; 2]> = match note.kind {
            AnnotationKind::Rule => match (&note.at.x, &note.at.y) {
                // Across the plot, its text over the rule at the plot's start, its
                // descenders clear of it.
                (_, Some(p)) => {
                    let y = to_y(first_y(p));
                    out_note.rule =
                        Some(Rule { from: [left, y], to: [right, y], width: note_width, color: rule_color });
                    text.as_ref().map(|t| [left, y - 0.5 * gap - t.lines[0].descent - t.lines[0].baseline])
                }
                // Up the plot, its text beside the rule's top, after it unless it would
                // pass the plot's end.
                (Some(p), None) => {
                    let [_, x, _] = x_at(p.values()[0])?;
                    out_note.rule =
                        Some(Rule { from: [x, top], to: [x, bottom], width: note_width, color: rule_color });
                    text.as_ref().map(|t| {
                        let after = x + 0.5 * gap;
                        let x0 = if after + t.width <= right { after } else { x - 0.5 * gap - t.width };
                        [x0, top - (t.lines[0].baseline - cap(t))]
                    })
                }
                (None, None) => unreachable!("checked: a rule stands at x or y"),
            },
            // A box across the plot, its text inside its top-left corner.
            AnnotationKind::Band => {
                let rect = match (&note.at.x, &note.at.y) {
                    (_, Some(p)) => {
                        let ends: Vec<f32> =
                            p.values().iter().map(|v| to_y(v.position(false).unwrap_or(0.0))).collect();
                        let (y0, y1) = (ends[0].min(ends[1]), ends[0].max(ends[1]));
                        [left, y0, right - left, y1 - y0]
                    }
                    (Some(p), None) => {
                        let ends = p.values();
                        let (a, b) = (x_at(ends[0])?, x_at(ends[1])?);
                        let (x0, x1) = (a[0].min(b[0]), a[2].max(b[2]));
                        [x0, top, x1 - x0, bottom - top]
                    }
                    (None, None) => unreachable!("checked: a band spans x or y"),
                };
                out_note.band = Some((rect, band_color));
                // Its text never sits on a rule either: it moves down past one.
                text.as_ref().map(|t| {
                    let (c, descent) = (cap(t), t.lines[0].descent);
                    let mut top = rect[1] + 0.5 * gap;
                    for _ in 0..levels.len() {
                        match levels.iter().find(|&&y| y > top - gap && y < top + c + descent + gap) {
                            Some(&y) => top = y + gap,
                            None => break,
                        }
                    }
                    [rect[0] + 0.5 * gap, top - (t.lines[0].baseline - c)]
                })
            }
            // A leader up from its point, or from its mark's value end and clear of the
            // mark's value label (down from a bar below the baseline), to its text,
            // centered but inside the plot's sides, over every mark, line, and value label
            // under it.
            _ => {
                let x = note.at.x.as_ref().expect("checked: a callout stands at an x");
                let (ax, ay, below) = match &note.at.y {
                    Some(p) => (x_at(x.values()[0])?[1], to_y(first_y(p)), false),
                    None => {
                        let hits: Vec<usize> = (0..rows.len()).filter(|&i| picks(note, &rows[i])).collect();
                        let [i] = hits[..] else {
                            return Err(EngineError::Data(format!(
                                "a callout at `{}` stands on {} marks; give it `at.series` or `at.y`",
                                x.values()[0].label(),
                                hits.len()
                            )));
                        };
                        let mark = &out.marks[i];
                        let (ax, end, below) = match mark.shape {
                            Shape::Bar(r) if rows[i].y < 0.0 => (r.center_x(), r.bottom(), true),
                            Shape::Bar(r) => (r.center_x(), r.top(), false),
                            Shape::Dot { x, y, r } => (x, y - r, false),
                            Shape::Span { x, top, .. } => (x, top, false),
                            Shape::Arc { .. } => unreachable!("checked: a donut takes no callouts"),
                        };
                        let end = match out.labels.iter().find(|l| l.key == mark.key) {
                            Some(l) if below => end.max(ink(l)[3]),
                            Some(l) => end.min(ink(l)[1]),
                            None => end,
                        };
                        (ax, end, below)
                    }
                };
                let x0 = text.as_ref().map_or(ax, |t| (ax - 0.5 * t.width).clamp(left, (right - t.width).max(left)));
                let x1 = x0 + text.as_ref().map_or(0.0, |t| t.width);
                let under = |a: f32, b: f32| b >= x0 && a <= x1;
                // Over a bar below the baseline, nothing stands under it; over the rest,
                // the highest mark, line, value label, or annotation's text before it
                // that its text would cross.
                let mut clear = match below {
                    true => ay,
                    false => (out.marks.iter())
                        .filter_map(|m| match m.shape {
                            Shape::Bar(r) => under(r.x, r.x + r.w).then_some(r.top()),
                            Shape::Dot { x, y, r } => under(x - r, x + r).then_some(y - r),
                            Shape::Span { x, top, .. } => under(x, x).then_some(top),
                            Shape::Arc { .. } => None,
                        })
                        .chain(out.labels.iter().map(ink).filter(|b| under(b[0], b[2])).map(|b| b[1]))
                        .chain(
                            (out.notes.iter().filter_map(|n| n.label.as_ref()).map(ink))
                                .filter(|b| under(b[0], b[2]))
                                .map(|b| b[1]),
                        )
                        .chain(out.paths.iter().flat_map(|p| {
                            // A line or an area's top: its vertices under the text, and
                            // where it crosses the text's ends.
                            let mut at: Vec<[f32; 2]> = (p.marks.iter())
                                .filter_map(|k| out.marks.iter().find(|m| m.key == *k))
                                .map(|m| m.shape.point())
                                .collect();
                            at.sort_by(|a, b| a[0].total_cmp(&b[0]));
                            let cross = |x: f32| {
                                at.windows(2).find(|w| w[0][0] <= x && x <= w[1][0]).map(|w| {
                                    let t = if w[1][0] > w[0][0] { (x - w[0][0]) / (w[1][0] - w[0][0]) } else { 0.0 };
                                    w[0][1] + t * (w[1][1] - w[0][1])
                                })
                            };
                            let inside: Vec<f32> = at.iter().filter(|q| under(q[0], q[0])).map(|q| q[1]).collect();
                            inside.into_iter().chain(cross(x0)).chain(cross(x1))
                        }))
                        .fold(ay, f32::min),
                };
                // Its text never sits on a rule: it moves on past one it would cross,
                // its leader crossing the rule instead.
                if let Some(t) = text.as_ref() {
                    let (c, descent) = (cap(t), t.lines[0].descent);
                    let span = |clear: f32| match below {
                        true => [clear + 3.0 * gap, clear + 3.0 * gap + c + descent],
                        false => [clear - 3.0 * gap - c, clear - 3.0 * gap + descent],
                    };
                    for _ in 0..levels.len() {
                        let [a, b] = span(clear);
                        match levels.iter().find(|&&y| y > a - gap && y < b + gap) {
                            Some(&y) => clear = y,
                            None => break,
                        }
                    }
                }
                let dir = if below { 1.0 } else { -1.0 };
                let (from, to) = ([ax, ay + dir * 0.5 * gap], [ax, clear + dir * 2.5 * gap]);
                out_note.rule = Some(Rule { from, to, width: note_width, color: rule_color });
                text.as_ref().map(|t| {
                    let first = &t.lines[0];
                    match below {
                        true => [x0, clear + 3.0 * gap - (first.baseline - cap(t))],
                        // Its descenders clear of the leader's end.
                        false => [x0, clear - 3.0 * gap - first.descent - first.baseline],
                    }
                })
            }
        };
        out_note.label = origin.zip(text).map(|(at, t)| Label::new(out_note.key.clone(), at, t, None));
        out.notes.push(out_note);
    }
    // A rule or a leader breaks where it would cross text, value labels, names, and the
    // other annotations' words alike, half a space unit clear of it either side.
    let texts: Vec<(String, [f32; 4])> = (out.labels.iter())
        .chain(out.notes.iter().filter_map(|n| n.label.as_ref()))
        .map(|l| (l.key.clone(), text_box(l)))
        .chain(out.legend.iter().map(|e| (format!("legend\u{1f}{}", e.key), text_box(&e.label))))
        .collect();
    for note in &mut out.notes {
        let Some(rule) = &note.rule else { continue };
        let own = |k: &str| note.label.as_ref().is_some_and(|l| l.key == k);
        note.gaps =
            (texts.iter()).filter(|(k, _)| !own(k)).filter_map(|(k, b)| crossing(rule, k, *b, 0.5 * gap)).collect();
    }
    // A highlight colors what it picks out in the signal color: its marks, and a line,
    // an area, or a legend entry all of whose marks it picks (a direct name with it). It
    // dims the rest: marks, their value labels, a line or an area with none of its marks
    // picked, and a legend entry whose marks are none of them. Words dim half as far as
    // marks, so the context stays legible.
    let highlights: Vec<&Annotation> = notes.iter().filter(|n| n.kind == AnnotationKind::Highlight).collect();
    if !highlights.is_empty() {
        let signal = theme.color(charts.and_then(|c| c.signal.as_deref()).unwrap_or("accent"))?;
        let lit: Vec<&str> =
            rows.iter().filter(|r| highlights.iter().any(|h| picks(h, r))).map(|r| r.key.as_str()).collect();
        let dim = |Color([r, g, b, a]): Color| Color([r, g, b, (f32::from(a) * dimmed).round() as u8]);
        let words = (1.0 + dimmed) / 2.0;
        for m in &mut out.marks {
            m.color = if lit.contains(&m.key.as_str()) { signal } else { dim(m.color) };
        }
        for l in out.labels.iter_mut().filter(|l| !lit.contains(&l.key.as_str())) {
            l.opacity = words;
        }
        for p in &mut out.paths {
            let picked = p.marks.iter().filter(|k| lit.contains(&k.as_str())).count();
            if picked == 0 {
                p.color = dim(p.color);
            } else if picked == p.marks.len() {
                p.color = signal;
            }
        }
        for e in &mut out.legend {
            let entry = |r: &&Row| if donut { r.label == e.key } else { r.series.as_deref() == Some(e.key.as_str()) };
            let (picked, all) = (rows.iter().filter(entry))
                .fold((0, 0), |(p, n), r| (p + usize::from(lit.contains(&r.key.as_str())), n + 1));
            if picked == 0 {
                e.color = dim(e.color);
                e.label.opacity = words;
            } else if picked == all {
                // A direct name takes the signal with its series.
                if direct {
                    for run in &mut e.label.text.runs {
                        run.color = signal;
                    }
                }
                e.color = signal;
            }
        }
    }
    Ok(out)
}

/// The gap `rule` leaves where it crosses text `key` (its box), `pad` clear of the text
/// along it; `None` where it passes by.
fn crossing(rule: &Rule, key: &str, text: [f32; 4], pad: f32) -> Option<Gap> {
    let level = (rule.to[0] - rule.from[0]).abs() >= (rule.to[1] - rule.from[1]).abs();
    // Along the rule, and across it.
    let (along, across) = if level { (0, 1) } else { (1, 0) };
    let reach = 0.5 * rule.width + 0.5 * pad;
    let gap = Gap {
        key: key.to_string(),
        along: [text[along] - pad, text[along + 2] + pad],
        across: [text[across] - reach, text[across + 2] + reach],
    };
    let (lo, hi) = (rule.from[along].min(rule.to[along]), rule.from[along].max(rule.to[along]));
    let at = rule.from[across];
    (at > gap.across[0] && at < gap.across[1] && gap.along[0] < hi && gap.along[1] > lo).then_some(gap)
}

/// A label's box as drawn: its advance across, from its cap height to its last line's
/// descent.
fn text_box(l: &Label) -> [f32; 4] {
    let (first, last) = (&l.text.lines[0], &l.text.lines[l.text.lines.len() - 1]);
    let top = l.origin[1] + first.baseline - first.cap_height.unwrap_or(first.ascent);
    [l.origin[0], top, l.origin[0] + l.text.width, l.origin[1] + last.baseline + last.descent]
}

/// A label's box: its advance across, and its cap height down to its baseline.
fn ink(l: &Label) -> [f32; 4] {
    let first = &l.text.lines[0];
    let baseline = l.origin[1] + first.baseline;
    let cap = first.cap_height.unwrap_or(first.ascent);
    [l.origin[0], baseline - cap, l.origin[0] + l.text.width, baseline]
}

/// Two boxes closer than `apart` both ways.
fn near(a: [f32; 4], b: [f32; 4], apart: f32) -> bool {
    a[0] < b[2] + apart && b[0] < a[2] + apart && a[1] < b[3] + apart && b[1] < a[3] + apart
}

/// Each pair of labels closer than `apart`, by key, in label order.
fn collisions(labels: &[Label], apart: f32) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (i, a) in labels.iter().enumerate() {
        for b in &labels[i + 1..] {
            if near(ink(a), ink(b), apart) {
                out.push((a.key.clone(), b.key.clone()));
            }
        }
    }
    out
}

/// Keep the labels of the largest values that touch no label kept before them (the
/// earlier in data order on a tie); hide the rest.
fn hide(labels: &mut Vec<Label>, apart: f32) {
    let size = |l: &Label| l.value.map_or(0.0, |v| v.value.abs());
    let mut order: Vec<usize> = (0..labels.len()).collect();
    order.sort_by(|&i, &j| size(&labels[j]).total_cmp(&size(&labels[i])).then(i.cmp(&j)));
    let mut kept: Vec<usize> = Vec::new();
    for i in order {
        if !kept.iter().any(|&k| near(ink(&labels[k]), ink(&labels[i]), apart)) {
            kept.push(i);
        }
    }
    let mut i = 0;
    labels.retain(|_| {
        i += 1;
        kept.contains(&(i - 1))
    });
}

/// Move labels that overlap up and down, as little as they can in all (least squares),
/// keeping their order: labels whose spans across overlap form a column, and each run
/// of a column that would touch moves as a block centered on where its labels want to
/// be. A nudged label rides its mark at its new height.
fn nudge(labels: &mut [Label], apart: f32) {
    let boxes: Vec<[f32; 4]> = labels.iter().map(ink).collect();
    // Columns: labels whose spans across overlap, transitively.
    let mut across: Vec<usize> = (0..labels.len()).collect();
    across.sort_by(|&i, &j| boxes[i][0].total_cmp(&boxes[j][0]).then(i.cmp(&j)));
    let mut columns: Vec<(Vec<usize>, f32)> = Vec::new();
    for i in across {
        match columns.last_mut() {
            Some((column, right)) if boxes[i][0] < *right + apart => {
                column.push(i);
                *right = right.max(boxes[i][2]);
            }
            _ => columns.push((vec![i], boxes[i][2])),
        }
    }
    for (mut column, _) in columns {
        column.sort_by(|&i, &j| boxes[i][1].total_cmp(&boxes[j][1]).then(i.cmp(&j)));
        // Blocks of labels set edge to edge: their members, and where the block's top
        // goes. Each member wants its top where it is.
        let mut blocks: Vec<(Vec<usize>, f32)> = Vec::new();
        for i in column {
            blocks.push((vec![i], boxes[i][1]));
            while blocks.len() > 1 {
                let (members, top) = &blocks[blocks.len() - 2];
                let bottom = top + members.iter().map(|&m| boxes[m][3] - boxes[m][1] + apart).sum::<f32>();
                let (_, next) = &blocks[blocks.len() - 1];
                if bottom <= *next {
                    break;
                }
                let (tail, _) = blocks.pop().expect("two blocks");
                let (head, _) = blocks.pop().expect("two blocks");
                let members: Vec<usize> = head.into_iter().chain(tail).collect();
                // The top that moves the members least in all: the mean of each one's
                // wanted top less its place in the block.
                let mut at = 0.0;
                let mut want = 0.0;
                for &m in &members {
                    want += boxes[m][1] - at;
                    at += boxes[m][3] - boxes[m][1] + apart;
                }
                let top = want / members.len() as f32;
                blocks.push((members, top));
            }
        }
        for (members, top) in blocks {
            let mut at = top;
            for m in members {
                let dy = at - boxes[m][1];
                labels[m].origin[1] += dy;
                if let Some(v) = labels[m].value.as_mut() {
                    v.drop += dy;
                }
                at += boxes[m][3] - boxes[m][1] + apart;
            }
        }
    }
}

/// The color `t` of the way along `stops`, mixed in Oklab between the two around it.
fn along(stops: &[Color], t: f32) -> Color {
    if stops.len() == 1 {
        return stops[0];
    }
    let at = t * (stops.len() - 1) as f32;
    let i = (at.floor() as usize).min(stops.len() - 2);
    crate::sample::mix(stops[i], stops[i + 1], at - i as f32)
}
