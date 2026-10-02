//! The chart compiler (SPEC §3.7): a chart node's resolved props and its data to marks,
//! labels, axes, and a legend, once per snapshot. Every v1 kind is a list of keyed
//! marks, one per datum, so a transition carries each datum to its next value whatever
//! the kind; lines and areas are paths through their series' marks.

use super::{
    AxisTick, CategoryFormat, ChartLayout, Ctx, Label, LegendEntry, Mark, Numerals, RoundRect, Rule, SeriesPath, Shape,
    ValueLabel, typeset_minus,
};
use crate::EngineError;
use crate::data::{self, ColumnType, Datum};
use crate::scale::{self, LinearScale};
use crate::text::{TextLayout, TextSpec};
use crate::theme::{Numeric, TextBox};
use scaena_core::displaylist::Color;
use scaena_core::document::Props;
use scaena_core::format::{DateFormat, DateTime, Locale, MINUS, NumberFormat};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// The v1 kinds (SPEC §3.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bar,
    StackedBar,
    Line,
    Area,
    Scatter,
    Dot,
    Donut,
}

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
    y: f64,
    /// Its series, if the chart has one.
    series: Option<String>,
    /// What a numeric color encoding reads.
    shade: Option<f64>,
    size: Option<f64>,
    key: String,
}

type Encoding<'a> = Option<&'a Map<String, Value>>;

fn field<'a>(e: Encoding<'a>) -> Option<&'a str> {
    e.and_then(|e| e.get("field")).and_then(Value::as_str)
}

/// A datum as a number: a number, or a date in seconds.
fn number(d: &Datum) -> Option<f64> {
    match d {
        Datum::Number(n) => Some(*n),
        Datum::Date(t) => Some(t.0 as f64),
        _ => None,
    }
}

/// Compile a chart node's resolved props for a cell `size` wide and high.
pub fn compile(cx: &mut Ctx, props: &Props, size: [f32; 2]) -> Result<ChartLayout, EngineError> {
    let kind = Kind::parse(props.get("kind"))?;
    for (key, task) in
        [("dataTransform", "chart data transforms — PLAN 1.9"), ("annotations", "chart annotations — PLAN 1.9")]
    {
        if props.contains_key(key) {
            return Err(EngineError::NotImplemented(task));
        }
    }
    let encoding = |name: &str| props.get(name).and_then(Value::as_object);
    let required = |name: &str| encoding(name).ok_or_else(|| EngineError::Layout(format!("chart has no `{name}`")));
    let (x, y) = (required("x")?, required("y")?);
    let (series_enc, color_enc, size_enc) = (encoding("series"), encoding("color"), encoding("sizeEncoding"));
    // Axes: categories (or x ticks) under the plot by default; the value axis, its
    // gridlines, and titles when asked for.
    let axes = props.get("axes");
    let setting = |name: &str, key: &str| axes.and_then(|a| a.get(name)).and_then(|a| a.get(key));
    let flag = |name: &str, key: &str, default: bool| setting(name, key).and_then(Value::as_bool).unwrap_or(default);
    let donut = kind == Kind::Donut;
    let (x_show, y_show, y_grid) = (
        flag("x", "show", !donut) && !donut,
        flag("y", "show", false) && !donut,
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
        .map(|f| CategoryFormat::parse(f, table.types[xc], x_field))
        .transpose()?;
    let series_col = field(series_enc).map(col).transpose()?;
    let color_col = field(color_enc).map(col).transpose()?;
    let size_col = field(size_enc).map(col).transpose()?;
    let key_col = props.get("key").and_then(Value::as_str).map(col).transpose()?;
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
        let label = match &x_format {
            Some(f) => f.print(&row[xc], locale),
            None => row[xc].label(),
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
            y: v,
            series,
            shade,
            size,
            key,
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
    let mut series: Vec<String> = Vec::new();
    for r in &rows {
        if !categories.iter().any(|(k, _)| *k == r.category) {
            categories.push((r.category.clone(), r.label.clone()));
        }
        if let Some(s) = &r.series
            && !series.contains(s)
        {
            series.push(s.clone());
        }
    }

    // Theme: chart styles, all tokens.
    let theme = cx.theme;
    let charts = theme.charts.as_ref();
    let axis = charts.and_then(|c| c.axis.as_ref());
    let axis_width = theme.stroke(axis.and_then(|a| a.stroke.as_deref()).unwrap_or("hairline"))?;
    let axis_color = theme.color(axis.and_then(|a| a.color.as_deref()).unwrap_or("onSurfaceMuted"))?;
    let corner = charts.and_then(|c| c.corner_radius).unwrap_or(0.0) as f32;
    let bar_gap = charts.and_then(|c| c.bar_gap).unwrap_or(0.2) as f32;
    let group_gap = charts.and_then(|c| c.group_gap).unwrap_or(0.1) as f32;
    let point_radius = charts.and_then(|c| c.point_radius).unwrap_or(0.0) as f32;
    let dot_radius = charts.and_then(|c| c.dot_radius).unwrap_or(8.0) as f32;
    let hole = charts.and_then(|c| c.donut_hole).unwrap_or(0.6).clamp(0.0, 0.99) as f32;
    let line_width = theme.stroke(charts.and_then(|c| c.stroke_width.as_deref()).unwrap_or("thin"))?;
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
    if flag("x", "gridlines", false) && !continuous {
        return Err(EngineError::NotImplemented(
            "gridlines across a category axis need a continuous x scale — PLAN 1.9",
        ));
    }
    let x_grid = flag("x", "gridlines", false) && continuous;

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
        let index = match (&r.series, donut) {
            (Some(s), _) => series.iter().position(|x| x == s).unwrap_or(0),
            (None, true) => categories.iter().position(|(k, _)| *k == r.category).unwrap_or(0),
            (None, false) => 0,
        };
        palette[index % palette.len()]
    };
    // The legend: one entry per series, or per slice of a donut, when there are two
    // or more and the chart does not say `none`.
    let legend_place = match props.get("legend") {
        None => "top",
        Some(Value::String(place)) => match place.as_str() {
            "auto" => "top",
            "top" => "top",
            "bottom" => "bottom",
            "right" => "right",
            "none" => "none",
            other => {
                return Err(EngineError::Layout(format!(
                    "legend `{other}`: expected auto, top, bottom, right, or none"
                )));
            }
        },
        Some(_) => return Err(EngineError::NotImplemented("a legend given as an object — PLAN 1.9")),
    };
    let entries: Vec<(String, Color)> = if donut {
        categories.iter().map(|(k, l)| (l.clone(), color_of(rows.iter().find(|r| r.category == *k).unwrap()))).collect()
    } else {
        series
            .iter()
            .map(|s| (s.clone(), color_of(rows.iter().find(|r| r.series.as_ref() == Some(s)).unwrap())))
            .collect()
    };
    let entries = if legend_place != "none" && entries.len() > 1 { entries } else { Vec::new() };

    // Text first: the plot is what the labels leave.
    let mut set = |text: String, role: &str| -> Result<TextLayout, EngineError> {
        let spec = TextSpec { numeric: Some(Numeric::TabularLining), ..TextSpec::plain(theme.text_role(role)?, text) };
        cx.text.layout(cx.fonts, theme, &spec, f32::INFINITY)
    };
    let show = labels.and_then(|l| l.get("show")).and_then(Value::as_str).unwrap_or("none");
    if !matches!(show, "all" | "ends" | "none") {
        return Err(EngineError::Layout(format!("labels.show `{show}`: expected all, ends, or none")));
    }
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
        let text = typeset_minus(value_format.format(v, locale), minus);
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
    let domain = y.get("domain").and_then(Value::as_array);
    let bound = |i: usize| domain.and_then(|d| d.get(i)).and_then(Value::as_f64);
    let lo = bound(0).unwrap_or(if kind.zero_based() { min.min(0.0) } else { min });
    let hi = bound(1).unwrap_or(max).max(lo + f64::EPSILON);
    let ruled = y_show || y_grid;
    let (lo, hi) =
        if ruled { scale::nice(lo, hi, tick_count, [bound(0).is_none(), bound(1).is_none()]) } else { (lo, hi) };
    let tick_values = if ruled { scale::ticks(lo, hi, tick_count) } else { Vec::new() };
    let step = scale::tick_step(lo, hi, tick_count);
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
        let (a, b) =
            rows.iter().filter_map(|r| r.x).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
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
    let x_texts: Vec<(String, TextLayout)> = match (x_show, continuous) {
        (false, _) => Vec::new(),
        (true, false) => categories
            .iter()
            .map(|(k, l)| Ok((k.clone(), set(l.clone(), &tick_role)?)))
            .collect::<Result<_, EngineError>>()?,
        (true, true) => x_ticks
            .iter()
            .map(|(_, t)| Ok((t.clone(), set(t.clone(), &tick_role)?)))
            .collect::<Result<_, EngineError>>()?,
    };
    let titles: Vec<(&str, TextLayout)> = [("y", &y_title), ("x", &x_title)]
        .into_iter()
        .filter_map(|(k, t)| t.as_ref().map(|t| (k, t)))
        .map(|(k, t)| Ok((k, set(t.clone(), &title_role)?)))
        .collect::<Result<_, EngineError>>()?;
    let legend_texts: Vec<(String, Color, TextLayout)> = entries
        .iter()
        .map(|(t, c)| Ok((t.clone(), *c, set(t.clone(), &legend_role)?)))
        .collect::<Result<_, EngineError>>()?;

    // The plot is what the text leaves.
    let cap = |t: &TextLayout| t.lines.first().map_or(0.0, |l| l.cap_height.unwrap_or(l.ascent));
    let below_cap = |t: &TextLayout| t.lines.first().map_or(0.0, |l| t.height - (l.baseline - cap(t)));
    let title_height = |key: &str| titles.iter().find(|(k, _)| *k == key).map_or(0.0, |(_, t)| t.height + gap);
    let (title_y, title_x) = (title_height("y"), title_height("x"));
    // The legend wraps across the chart's width, or stands in a column at its right.
    let mut legend_rows: Vec<Vec<usize>> = Vec::new();
    let swatch = legend_texts.iter().map(|(.., t)| cap(t)).fold(0.0_f32, f32::max);
    let beside = legend_place == "right";
    {
        let mut used = f32::INFINITY;
        for (i, (.., t)) in legend_texts.iter().enumerate() {
            let w = swatch + 0.5 * gap + t.width;
            if beside || used + 2.0 * gap + w > size[0] || legend_rows.is_empty() {
                legend_rows.push(Vec::new());
                used = -2.0 * gap;
            }
            legend_rows.last_mut().unwrap().push(i);
            used += 2.0 * gap + w;
        }
    }
    let legend_line = legend_texts.iter().map(|(.., t)| t.height).fold(0.0_f32, f32::max);
    let legend_height = match legend_rows.is_empty() || beside {
        true => 0.0,
        false => legend_rows.len() as f32 * legend_line + gap,
    };
    // A legend at the right takes its widest entry and two spaces from the plot.
    let legend_width = match beside && !legend_texts.is_empty() {
        true => swatch + 0.5 * gap + legend_texts.iter().map(|(.., t)| t.width).fold(0.0_f32, f32::max) + 2.0 * gap,
        false => 0.0,
    };
    let right = size[0] - legend_width;
    let label_room = values.iter().flatten().map(cap).fold(0.0_f32, f32::max);
    let tick_cap = tick_labels.iter().filter_map(|(.., l)| l.as_ref()).map(cap).fold(0.0_f32, f32::max);
    let legend_top = if legend_place == "top" { legend_height } else { 0.0 };
    // Above the plot: the value axis's title, the legend, then room for value labels
    // over the tallest mark or half a tick label's cap over the top gridline.
    let top = title_y + legend_top + (if label_room > 0.0 { label_room + gap } else { 0.0 }).max(0.5 * tick_cap);
    let mut bottom = size[1];
    if x_show {
        bottom = bottom - gap - x_texts.iter().map(|(_, t)| below_cap(t)).fold(0.0_f32, f32::max);
    }
    bottom -= title_x;
    if legend_place == "bottom" {
        bottom -= legend_height;
    }
    // Beside it, a gutter as wide as the widest value-axis label.
    let left = tick_labels.iter().filter_map(|(.., l)| l.as_ref()).map(|l| l.width + gap).fold(0.0_f32, f32::max);
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

    let mut out = ChartLayout {
        base,
        baseline: (!donut).then_some(Rule {
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
        clipped: left > 0.0 || legend_width > 0.0,
        y_axis: Vec::with_capacity(tick_labels.len()),
        titles: Vec::new(),
        legend: Vec::new(),
        x_grid: Vec::new(),
    };
    for (v, key, label) in tick_labels {
        let y = to_y(v);
        // The baseline already rules the line it sits on.
        let rule = (y_grid && y != base).then_some(Rule {
            from: [left, y],
            to: [right, y],
            width: grid_width,
            color: grid_color,
        });
        // Right-aligned in the gutter, the middle of its cap height on the tick.
        let label = label.map(|text| {
            let first = &text.lines[0];
            let origin = [left - gap - text.width, y + 0.5 * cap(&text) - first.baseline];
            Label { key: key.clone(), origin, text, value: None }
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
    for (key, text) in titles {
        // The value axis's title above the plot at the chart's left; the category
        // axis's title centered under the category labels.
        let under = if legend_place == "bottom" { legend_height } else { 0.0 };
        let origin = match key {
            "y" => [0.0, 0.0],
            _ => [left + 0.5 * (right - left) - 0.5 * text.width, size[1] - under - text.height],
        };
        out.titles.push(Label { key: key.to_string(), origin, text, value: None });
    }
    // Legend entries: a swatch, its cap height square, on the label's baseline; above
    // the plot under the value axis's title, at the chart's foot, or in a column right
    // of the plot from its top.
    let legend_y = match legend_place {
        "bottom" => size[1] - legend_height + gap,
        "right" => top,
        _ => title_y,
    };
    let legend_x = if beside { right + 2.0 * gap } else { 0.0 };
    let mut legend_texts: Vec<Option<(String, Color, TextLayout)>> = legend_texts.into_iter().map(Some).collect();
    for (r, line) in legend_rows.iter().enumerate() {
        let mut x0 = legend_x;
        for &i in line {
            let (key, color, text) = legend_texts[i].take().expect("each entry once");
            let first = &text.lines[0];
            let baseline = legend_y + r as f32 * legend_line + first.baseline;
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
                label: Label { key, origin, text, value: None },
            });
        }
    }

    // Marks.
    // A value label over its mark (under a bar below its foot), centered on `cx`.
    let label_at =
        |out: &mut ChartLayout, value: Option<TextLayout>, key: &str, shape: &Shape, cx: f32, v: f64, below: bool| {
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
            // Centered on its mark, unless that would put it past the plot's side.
            let x0 = (cx - 0.5 * text.width).clamp(left, (right - text.width).max(left));
            let align = if x0 == cx - 0.5 * text.width { 0.5 } else { (cx - x0) / text.width.max(f32::EPSILON) };
            let value = ValueLabel { value: v, below, offset, align, drop: 0.0 };
            out.labels.push(Label { key: key.to_string(), origin: [x0, origin_y], text, value: Some(value) });
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
                // A stacked segment stands on the stack under it, a bar on the baseline.
                let (to, foot, grows) = if kind == Kind::StackedBar {
                    let stack = &mut stacks[c];
                    let from = if r.y >= 0.0 { stack.0 } else { stack.1 };
                    let to = from + r.y;
                    if r.y >= 0.0 {
                        stack.0 = to;
                    } else {
                        stack.1 = to;
                    }
                    (to, to_y(from), Some(to_y(from)))
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
                label_at(&mut out, value, &r.key, &shape, cx, v, below);
                out.marks.push(Mark { key: r.key.clone(), shape, color: color_of(r), base: grows });
            }
        }
        Kind::Line | Kind::Area | Kind::Dot | Kind::Scatter => {
            let stack_areas = kind == Kind::Area && !series.is_empty();
            let mut stacks: Vec<f64> = vec![0.0; categories.len()];
            let size_max = rows.iter().filter_map(|r| r.size).fold(0.0_f64, f64::max);
            for (r, value) in rows.iter().zip(values) {
                let x = center_of(r);
                let shape = match kind {
                    Kind::Area => {
                        let c = categories.iter().position(|(k, _)| *k == r.category).unwrap_or(0);
                        let from = if stack_areas { stacks[c] } else { 0.0_f64.clamp(lo, hi) };
                        let to = if stack_areas { from + r.y } else { r.y };
                        if stack_areas {
                            stacks[c] = to;
                        }
                        Shape::Span { x, top: to_y(to), base: to_y(from) }
                    }
                    Kind::Scatter => {
                        let r_ = match (r.size, size_max > 0.0) {
                            (Some(s), true) => dot_radius * (s.max(0.0) / size_max).sqrt() as f32,
                            _ => dot_radius,
                        };
                        Shape::Dot { x, y: to_y(r.y), r: r_ }
                    }
                    Kind::Dot => Shape::Dot { x, y: to_y(r.y), r: dot_radius },
                    _ => Shape::Dot { x, y: to_y(r.y), r: point_radius },
                };
                let grows = match shape {
                    Shape::Span { base, .. } => Some(base),
                    _ => None,
                };
                label_at(&mut out, value, &r.key, &shape, x, r.y, false);
                out.marks.push(Mark { key: r.key.clone(), shape, color: color_of(r), base: grows });
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
                    });
                }
            }
        }
        Kind::Donut => {
            let total: f64 = rows.iter().map(|r| r.y).sum();
            let label_h = values.iter().flatten().map(|t| t.height).fold(0.0_f32, f32::max);
            let outer =
                (0.5 * (right - left).min(bottom - top) - if label_h > 0.0 { label_h + gap } else { 0.0 }).max(1.0);
            let (cx, cy) = (left + 0.5 * (right - left), top + 0.5 * (bottom - top));
            let mut at = 0.0_f64;
            for (r, value) in rows.iter().zip(values) {
                let start = if total > 0.0 { (at / total) as f32 } else { 0.0 };
                at += r.y;
                let end = if total > 0.0 { (at / total) as f32 } else { 0.0 };
                let shape = Shape::Arc { cx, cy, inner: outer * hole, outer, start, end };
                if let Some(text) = value {
                    // Outside the slice's middle, on its side of the donut, the middle of
                    // its cap height level with the point.
                    let mid = 0.5 * (start + end) * core::f32::consts::TAU;
                    let sin = libm::sinf(mid);
                    let align = if sin > 0.05 {
                        0.0
                    } else if sin < -0.05 {
                        1.0
                    } else {
                        0.5
                    };
                    let value = ValueLabel { value: r.y, below: false, offset: gap, align, drop: 0.5 * cap(&text) };
                    let [ax, baseline] = value.anchor(&shape);
                    let origin = [ax - align * text.width, baseline - text.lines[0].baseline];
                    out.labels.push(Label { key: r.key.clone(), origin, text, value: Some(value) });
                }
                out.marks.push(Mark { key: r.key.clone(), shape, color: color_of(r), base: None });
            }
        }
    }
    // Category labels under each band, or x ticks along a continuous x.
    for (key, text) in x_texts {
        let x = match (&x_scale, x_ticks.iter().find(|(_, t)| *t == key)) {
            (Some(s), Some((v, _))) => s.map(*v),
            _ => {
                let i = categories.iter().position(|(k, _)| *k == key).unwrap_or(0);
                left + (i as f32 + 0.5) * band
            }
        };
        // Centered under its tick, but inside the plot's sides.
        let x0 = (x - 0.5 * text.width).clamp(left, (right - text.width).max(left));
        let origin = [x0, y_scale.range[0] + gap - text.trimmed(TextBox::Cap).0];
        out.ticks.push(Label { key, origin, text, value: None });
    }
    Ok(out)
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
