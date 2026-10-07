//! Bars across (SPEC §3.7, PLAN 1.29): a `bar` or a `stackedBar` with `orient:
//! horizontal`. The categories run down the side, a band each, their names read across in
//! a gutter at the plot's left; the bars run across from a baseline at zero, right for a
//! value above it and left for one below; each value stands past its bar's end. The value
//! axis, when it shows, runs under the plot. Everything else is the chart as it stands up:
//! the same data, keys, colors, legend, labels, and annotations, turned.

use super::super::{
    AxisTick, ChartLayout, Gap, Label, LegendEntry, Mark, MarkPlace, Note, Numerals, RoundRect, Rule, Shape, Stack,
    ValueLabel,
};
use super::{DateUnit, Kind, Row, clear_along, collisions, crossing, hide, ink, near, nudge, strides, text_box};
use crate::EngineError;
use crate::scale::LinearScale;
use crate::text::TextLayout;
use crate::theme::TextBox;
use scaena_core::displaylist::Color;
use scaena_core::model::values::{Annotation, AnnotationKind, Place, Scalar};

/// What a chart that runs across is laid out from: its data, its text set, and its styles,
/// as the chart compiler reads them whichever way its bars run.
pub(in crate::charts) struct Parts<'a> {
    pub size: [f32; 2],
    pub kind: Kind,
    pub rows: Vec<Row>,
    /// Each row's color.
    pub colors: Vec<Color>,
    /// Each category's identity and how it prints.
    pub categories: Vec<(String, String)>,
    pub series: Vec<String>,
    /// A stacked bar's totals, by category.
    pub totals: Vec<(String, f64)>,
    /// Each row's value label, where it prints one.
    pub values: Vec<Option<TextLayout>>,
    pub numerals: Option<Numerals>,
    /// The value axis's bounds.
    pub domain: [f64; 2],
    /// The value axis's ticks: each value, its key, and its label where the axis shows.
    pub ticks: Vec<(f64, String, Option<TextLayout>)>,
    /// Gridlines between the categories (`axes.x.gridlines`) and at the value axis's ticks
    /// (`axes.y.gridlines`).
    pub grid: [bool; 2],
    /// The axes' titles: `x`, the category axis's, and `y`, the value axis's.
    pub titles: Vec<(&'a str, TextLayout)>,
    /// Where the legend stands: `top`, `bottom`, `right`, or `none`.
    pub legend_place: &'a str,
    pub legend_title: Option<TextLayout>,
    pub legend_texts: Vec<(String, Color, TextLayout)>,
    /// Each category's name as it prints short, and long where it could begin a run of its
    /// year (dates).
    pub names: Vec<(String, TextLayout)>,
    pub longs: Vec<Option<TextLayout>>,
    pub periods: Vec<Option<i64>>,
    /// The unit an ordered axis of dates steps by, which its names thin by.
    pub unit: Option<DateUnit>,
    /// The categories are dates or numbers, which thin where they crowd.
    pub ordered: bool,
    pub notes: Vec<Annotation>,
    pub note_texts: Vec<Option<TextLayout>>,
    pub collide: Option<&'a str>,
    /// The chart asked for its values (`labels.show` set).
    pub chosen: bool,
    pub source: &'a str,
    pub x_field: &'a str,
    pub style: Style,
}

/// The theme's chart styles a chart across reads.
pub(in crate::charts) struct Style {
    pub gap: f32,
    pub corner: f32,
    pub bar_gap: f32,
    pub group_gap: f32,
    pub axis: (f32, Color),
    pub grid: (f32, Color),
    /// An annotation's stroke width, its rules' color, and its bands'.
    pub note: (f32, Color, Color),
    pub dimmed: f32,
    /// The signal color, where a highlight spends it.
    pub signal: Option<Color>,
}

fn cap(t: &TextLayout) -> f32 {
    t.lines.first().map_or(0.0, |l| l.cap_height.unwrap_or(l.ascent))
}

fn below_cap(t: &TextLayout) -> f32 {
    t.lines.first().map_or(0.0, |l| t.height - (l.baseline - cap(t)))
}

/// Lay a chart out across.
pub(in crate::charts) fn layout(parts: Parts) -> Result<ChartLayout, EngineError> {
    let Parts {
        size,
        kind,
        rows,
        colors,
        categories,
        series,
        totals,
        values,
        numerals,
        domain: [lo, hi],
        ticks,
        grid: [x_grid, y_grid],
        titles,
        legend_place,
        legend_title,
        legend_texts,
        names,
        longs,
        periods,
        unit,
        ordered,
        notes,
        note_texts,
        collide,
        chosen,
        source,
        x_field,
        style,
    } = parts;
    let Style {
        gap,
        corner,
        bar_gap,
        group_gap,
        axis,
        grid,
        note: (note_width, rule_color, band_color),
        dimmed,
        signal,
    } = style;
    let n = categories.len();
    let title_height = |key: &str| titles.iter().find(|(k, _)| *k == key).map_or(0.0, |(_, t)| t.height + gap);
    let (x_title, y_title) = (title_height("x"), title_height("y"));

    // The legend wraps across the chart's width, or stands in a column at its right, as a
    // chart that stands up places it.
    let swatch = legend_texts.iter().map(|(.., t)| cap(t)).fold(0.0_f32, f32::max);
    let beside = legend_place == "right";
    let indent = match (&legend_title, beside) {
        (Some(t), false) => t.width + 2.0 * gap,
        _ => 0.0,
    };
    let mut legend_rows: Vec<Vec<usize>> = Vec::new();
    let mut used = f32::INFINITY;
    for (i, (.., t)) in legend_texts.iter().enumerate() {
        let w = swatch + 0.5 * gap + t.width;
        if beside || used + 2.0 * gap + w > size[0] - indent || legend_rows.is_empty() {
            legend_rows.push(Vec::new());
            used = -2.0 * gap;
        }
        legend_rows.last_mut().expect("a row").push(i);
        used += 2.0 * gap + w;
    }
    let legend_lead = usize::from(beside && legend_title.is_some());
    let legend_line = (legend_texts.iter().map(|(.., t)| t).chain(&legend_title)).map(|t| t.height).fold(0.0, f32::max);
    let legend_baseline =
        (legend_texts.iter().map(|(.., t)| t).chain(&legend_title)).map(|t| t.lines[0].baseline).fold(0.0, f32::max);
    let legend_height = match legend_rows.is_empty() || beside {
        true => 0.0,
        false => legend_rows.len() as f32 * legend_line + gap,
    };
    let legend_width = match (beside, legend_texts.is_empty()) {
        (true, false) => {
            let widest = legend_texts.iter().map(|(.., t)| t.width).fold(0.0_f32, f32::max);
            (swatch + 0.5 * gap + widest).max(legend_title.as_ref().map_or(0.0, |t| t.width)) + 2.0 * gap
        }
        _ => 0.0,
    };

    // Down the chart: the category axis's title and the legend over the plot, room for a
    // rule's words over the first category, then the plot; under it the value axis's labels,
    // its title, and a legend at the foot.
    let rule_room = (notes.iter().zip(&note_texts))
        .filter(|(n, _)| n.kind == AnnotationKind::Rule && n.at.y.is_none())
        .filter_map(|(_, t)| t.as_ref().map(|t| 0.5 * gap + cap(t) + t.lines[0].descent))
        .fold(0.0_f32, f32::max);
    let legend_top = if legend_place == "top" { legend_height } else { 0.0 };
    let top = x_title + legend_top + rule_room;
    let tick_below = ticks.iter().filter_map(|(.., l)| l.as_ref()).map(below_cap).fold(0.0_f32, f32::max);
    let mut bottom = size[1];
    if tick_below > 0.0 {
        bottom -= gap + tick_below;
    }
    bottom -= y_title;
    if legend_place == "bottom" {
        bottom -= legend_height;
    }
    if bottom - top <= 0.0 || n == 0 {
        return Err(EngineError::Layout(format!("chart cell {}×{} cu leaves no room to plot", size[0], size[1])));
    }
    let band = (bottom - top) / n as f32;
    let middle = |i: usize| top + (i as f32 + 0.5) * band;

    // The names down the side, each with the middle of its cap height level with its band's
    // middle. Dates and numbers that would come within a space of each other keep every k-th
    // from the first, at the smallest stride that clears them; text keeps every name, and
    // names that overlap are reported (W310).
    let pick = |k: usize| -> Vec<(usize, bool)> {
        let mut last = None;
        (0..names.len())
            .step_by(k)
            .map(|i| {
                let period = periods.get(i).copied().flatten();
                let long = period.is_some() && period != last && longs.get(i).is_some_and(Option::is_some);
                last = period;
                (i, long)
            })
            .collect()
    };
    let text = |&(i, long): &(usize, bool)| match long {
        true => longs[i].as_ref().unwrap_or(&names[i].1),
        false => &names[i].1,
    };
    // Where a name stands down: from its cap height to its descent.
    let span = |i: usize, t: &TextLayout| {
        let line = &t.lines[0];
        let baseline = middle(i) + 0.5 * cap(t);
        [baseline - cap(t), baseline + line.descent]
    };
    let crowded = |kept: &[(usize, bool)], space: f32| -> Vec<(usize, usize)> {
        (kept.windows(2))
            .filter(|w| span(w[0].0, text(&w[0]))[1] + space > span(w[1].0, text(&w[1]))[0])
            .map(|w| (w[0].0, w[1].0))
            .collect()
    };
    let mut crowds = Vec::new();
    let kept = match ordered {
        true => (strides(unit).take_while(|&k| k < names.len()))
            .map(&pick)
            .find(|kept| crowded(kept, gap).is_empty())
            .unwrap_or_else(|| pick(names.len().max(1))),
        false => {
            let kept = pick(1);
            let keys = |(a, b): (usize, usize)| (names[a].0.clone(), names[b].0.clone());
            crowds = crowded(&kept, 0.25 * gap).into_iter().map(keys).collect();
            kept
        }
    };
    let gutter = kept.iter().map(|k| text(k).width).fold(0.0_f32, f32::max);

    // Across the chart: the names' gutter, room for the values past the bars below zero,
    // the plot, room for the values and callouts past the bars above it, and a legend at
    // the right.
    let room = |below: bool| -> f32 {
        (rows.iter().zip(&values))
            .filter(|(r, v)| v.is_some() && (r.y < 0.0) == below)
            .filter_map(|(_, v)| v.as_ref().map(|t| t.width + gap))
            .fold(0.0_f32, f32::max)
    };
    let callout_room = (notes.iter().zip(&note_texts))
        .filter(|(n, _)| n.kind == AnnotationKind::Callout)
        .filter_map(|(_, t)| t.as_ref().map(|t| t.width + 3.0 * gap))
        .fold(0.0_f32, f32::max);
    let left = gutter + if gutter > 0.0 { gap } else { 0.0 } + room(true);
    let right = size[0] - legend_width - room(false) - callout_room;
    if right - left <= 0.0 {
        return Err(EngineError::Layout(format!("chart cell {}×{} cu leaves no room to plot", size[0], size[1])));
    }

    let x_scale = LinearScale { domain: [lo, hi], range: [left, right] };
    let to_x = |v: f64| x_scale.map(v);
    let base = to_x(0.0_f64.clamp(lo, hi));
    let zero_ruled = lo <= 0.0 && 0.0 <= hi;
    let mut out = ChartLayout {
        kind,
        horizontal: true,
        base,
        baseline: zero_ruled.then_some(Rule { from: [base, top], to: [base, bottom], width: axis.0, color: axis.1 }),
        marks: Vec::with_capacity(rows.len()),
        ticks: Vec::new(),
        labels: Vec::new(),
        numerals,
        paths: Vec::new(),
        y_scale: x_scale,
        plot: [left, top, right - left, bottom - top],
        clip: None,
        y_axis: Vec::with_capacity(ticks.len()),
        titles: Vec::new(),
        legend: Vec::new(),
        x_grid: Vec::new(),
        collisions: Vec::new(),
        crowded: crowds,
        covers: Vec::new(),
        notes: Vec::new(),
        source: source.to_string(),
        rows: rows.iter().map(|r| (r.key.clone(), r.from.clone())).collect(),
        places: (rows.iter())
            .map(|r| (r.key.clone(), MarkPlace { x: r.place.clone(), value: r.y, series: r.series.clone() }))
            .collect(),
        // Each category's band down the chart, where an annotation dragged there stands.
        categories: (categories.iter().enumerate())
            .map(|(i, (k, _))| {
                let place =
                    rows.iter().find(|r| r.category == *k).map_or_else(|| Scalar::Text(k.clone()), |r| r.place.clone());
                (place, [top + i as f32 * band, top + (i + 1) as f32 * band])
            })
            .collect(),
        highlights: Vec::new(),
    };

    // The value axis: each tick's label under the plot, centered on it but inside the plot's
    // sides; a gridline up the plot at each, but the one the baseline rules.
    for (v, key, label) in ticks {
        let x = to_x(v);
        let rule = (y_grid && !(zero_ruled && x == base)).then_some(Rule {
            from: [x, top],
            to: [x, bottom],
            width: grid.0,
            color: grid.1,
        });
        let label = label.map(|text| {
            let x0 = (x - 0.5 * text.width).clamp(left, (right - text.width).max(left));
            Label::new(key.clone(), [x0, bottom + gap - text.trimmed(TextBox::Cap).0], text, None)
        });
        out.y_axis.push(AxisTick { key, value: v, rule, label });
    }
    // Between the categories' bands, each keyed by the category after it.
    if x_grid {
        for (i, (key, _)) in categories.iter().enumerate().skip(1) {
            let y = top + i as f32 * band;
            let rule = Rule { from: [left, y], to: [right, y], width: grid.0, color: grid.1 };
            out.x_grid.push(AxisTick { key: key.clone(), value: i as f64, rule: Some(rule), label: None });
        }
    }
    // The category axis's title over the names at the chart's left; the value axis's
    // centered under its labels.
    let under = if legend_place == "bottom" { legend_height } else { 0.0 };
    for (key, text) in titles {
        let origin = match key {
            "x" => [0.0, 0.0],
            _ => [left + 0.5 * (right - left) - 0.5 * text.width, size[1] - under - text.height],
        };
        out.titles.push(Label::new(key, origin, text, None));
    }
    // The legend: a swatch its cap height square on each name's baseline, over the plot
    // under the category axis's title, at the chart's foot, or in a column at its right.
    let legend_y = match legend_place {
        "bottom" => size[1] - legend_height + gap,
        "right" => top,
        _ => x_title,
    };
    let legend_x = if beside { size[0] - legend_width + 2.0 * gap } else { 0.0 };
    if let Some(text) = legend_title {
        let origin = [legend_x, legend_y + legend_baseline - text.lines[0].baseline];
        out.titles.push(Label::new("legend", origin, text, None));
    }
    let mut entries: Vec<Option<(String, Color, TextLayout)>> = legend_texts.into_iter().map(Some).collect();
    for (r, line) in legend_rows.iter().enumerate() {
        let mut x0 = legend_x + indent;
        for &i in line {
            let (key, color, text) = entries[i].take().expect("each entry once");
            let baseline = legend_y + (r + legend_lead) as f32 * legend_line + legend_baseline;
            let square = RoundRect::rounded(x0, baseline - swatch, swatch, swatch, corner.min(0.5 * swatch));
            let origin = [x0 + swatch + 0.5 * gap, baseline - text.lines[0].baseline];
            x0 = origin[0] + text.width + 2.0 * gap;
            out.legend.push(LegendEntry {
                key: key.clone(),
                swatch: square,
                color,
                label: Label::new(key, origin, text, None),
            });
        }
    }

    // Marks: across from the baseline, or from the stack before them; each a band's
    // `1 − barGap` thick, a series sharing it one under another.
    let groups = if kind == Kind::Bar { series.len().max(1) } else { 1 };
    let mut stacks: Vec<(f64, f64)> = vec![(0.0, 0.0); n];
    for (i, ((r, value), color)) in rows.iter().zip(values).zip(&colors).enumerate() {
        let c = categories.iter().position(|(k, _)| *k == r.category).unwrap_or(0);
        let thick = band * (1.0 - bar_gap);
        let (cy, h) = match (groups, &r.series) {
            (1, _) | (_, None) => (middle(c), thick),
            (g, Some(s)) => {
                let slot = thick / g as f32;
                let k = series.iter().position(|x| x == s).unwrap_or(0);
                (middle(c) - 0.5 * thick + (k as f32 + 0.5) * slot, slot * (1.0 - group_gap))
            }
        };
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
            (to, to_x(from), Some(Stack { key, from: to_x(from), to: to_x(to) }))
        } else {
            (r.y, base, None)
        };
        let at = to_x(to);
        let (x0, x1) = (at.min(foot), at.max(foot));
        let below = at < foot;
        // Square on its foot, rounded at its free end; a stack rounds only its outermost
        // segment each way.
        let outermost = kind != Kind::StackedBar
            || rows.iter().rposition(|o| o.category == r.category && (o.y >= 0.0) == (r.y >= 0.0)) == Some(i);
        let radius = if outermost { corner.min(0.5 * h).min(x1 - x0) } else { 0.0 };
        let (top_radius, bottom_radius) = if below { (0.0, radius) } else { (radius, 0.0) };
        let bar = RoundRect { x: x0, y: cy - 0.5 * h, w: x1 - x0, h, top_radius, bottom_radius, across: true };
        let shape = Shape::Bar(bar);
        // Its value a space past its free end, the middle of its cap height level with its
        // middle.
        if let Some(text) = value {
            let v = match kind {
                Kind::StackedBar => totals.iter().find(|(k, _)| *k == r.category).map_or(r.y, |t| t.1),
                _ => r.y,
            };
            let at = ValueLabel {
                value: v,
                below,
                offset: if below { -gap } else { gap },
                align: if below { 1.0 } else { 0.0 },
                drop: 0.5 * cap(&text),
            };
            let [ax, baseline] = at.anchor(&shape);
            let origin = [ax - at.align * text.width, baseline - text.lines[0].baseline];
            out.labels.push(Label::new(r.key.clone(), origin, text, Some(at)));
        }
        out.marks.push(Mark { key: r.key.clone(), shape, color: *color, stack: place });
    }
    // The names, right-aligned in their gutter.
    let mut shorts: Vec<Option<(String, TextLayout)>> = names.into_iter().map(Some).collect();
    let mut longs = longs;
    for (i, long) in kept {
        let Some((key, short)) = shorts[i].take() else { continue };
        let text = match long.then(|| longs.get_mut(i).and_then(Option::take)).flatten() {
            Some(text) => text,
            None => short,
        };
        let origin = [gutter - text.width, middle(i) + 0.5 * cap(&text) - text.lines[0].baseline];
        out.ticks.push(Label::new(key, origin, text, None));
    }

    // A value label covers no other mark: one that would hides, unless the chart asked for
    // its values, which reports it (W310). Labels that overlap hide, nudge, or are reported
    // as `labels.collide` says.
    let apart = 0.25 * gap;
    let boxes: Vec<(String, [f32; 4])> = (out.marks.iter())
        .filter_map(|m| match m.shape {
            Shape::Bar(b) if b.w > 0.0 => Some((m.key.clone(), [b.x, b.y, b.x + b.w, b.y + b.h])),
            _ => None,
        })
        .collect();
    let covering: Vec<(String, String)> = (out.labels.iter())
        .filter_map(|l| {
            let mark = boxes.iter().find(|(k, b)| *k != l.key && near(ink(l), *b, apart))?;
            Some((l.key.clone(), mark.0.clone()))
        })
        .collect();
    if !chosen || collide == Some("hide") {
        out.labels.retain(|l| !covering.iter().any(|(k, _)| *k == l.key));
    } else {
        out.covers = covering;
    }
    match collide {
        None if !chosen => hide(&mut out.labels, apart),
        None => out.collisions = collisions(&out.labels, apart),
        Some("hide") => hide(&mut out.labels, apart),
        Some("nudge") => nudge(&mut out.labels, apart),
        Some(other) => return Err(EngineError::Layout(format!("labels.collide `{other}`: expected hide or nudge"))),
    }

    notes_across(&mut out, &rows, &categories, (&notes, note_texts), [top, bottom], band, gap, x_field)?;
    finish_notes(&mut out, (note_width, rule_color, band_color), gap);
    lit(&mut out, &rows, &notes, signal, dimmed);
    Ok(out)
}

/// Whether an annotation's `x` and `series` pick out a row.
fn picks(note: &Annotation, r: &Row) -> bool {
    let x = note.at.x.as_ref().is_none_or(|p| p.values().into_iter().any(|v| v.label() == r.category));
    let series = (note.at.series.as_ref())
        .is_none_or(|p| p.values().into_iter().any(|v| r.series.as_deref() == Some(v.label().as_str())));
    x && series
}

/// The annotations of a chart across: a rule or a band of values runs up the plot, one of
/// categories across it; a callout's leader runs on past its mark's value to its words.
#[allow(clippy::too_many_arguments)]
fn notes_across(
    out: &mut ChartLayout,
    rows: &[Row],
    categories: &[(String, String)],
    (notes, texts): (&[Annotation], Vec<Option<TextLayout>>),
    [top, bottom]: [f32; 2],
    band: f32,
    gap: f32,
    x_field: &str,
) -> Result<(), EngineError> {
    let [left, _, width, _] = out.plot;
    let right = left + width;
    let scale = out.y_scale;
    let to_x = |v: f64| scale.map(v);
    // A category's band down the plot: its top, middle, and foot.
    let y_at = |v: &Scalar| -> Result<[f32; 3], EngineError> {
        let label = v.label();
        let i = categories.iter().position(|(k, _)| *k == label).ok_or_else(|| {
            EngineError::Data(format!("an annotation stands at `{label}`, no category of `{x_field}`"))
        })?;
        let start = top + i as f32 * band;
        Ok([start, start + 0.5 * band, start + band])
    };
    // Every value an annotation names is in the data.
    for (k, note) in notes.iter().enumerate() {
        for (axis, place) in [("x", &note.at.x), ("series", &note.at.series)] {
            for v in place.iter().flat_map(Place::values) {
                let found = match axis {
                    "x" => rows.iter().any(|r| v.label() == r.category),
                    _ => rows.iter().any(|r| r.series.as_deref() == Some(v.label().as_str())),
                };
                if !found {
                    return Err(EngineError::Data(format!("annotation {k}: no datum has {axis} `{}`", v.label())));
                }
            }
        }
    }
    let value_of = |p: &Place| p.values()[0].position(false).expect("checked: y is a number");
    // Where each rule up the plot stands: a callout's words move on past one they would
    // cross.
    let levels: Vec<f32> = (notes.iter())
        .filter(|n| n.kind == AnnotationKind::Rule)
        .filter_map(|n| n.at.y.as_ref())
        .map(|p| to_x(value_of(p)))
        .collect();
    let mut seen: Vec<(AnnotationKind, &str)> = Vec::new();
    for (index, (note, text)) in notes.iter().zip(texts).enumerate() {
        if note.kind == AnnotationKind::Highlight {
            continue;
        }
        let axis = if note.kind != AnnotationKind::Callout && note.at.y.is_some() { "y" } else { "x" };
        let n = seen.iter().filter(|s| **s == (note.kind, axis)).count();
        seen.push((note.kind, axis));
        let name = match note.kind {
            AnnotationKind::Rule => "rule",
            AnnotationKind::Band => "band",
            _ => "callout",
        };
        let mut placed = Note {
            key: format!("{name}\u{1f}{axis}\u{1f}{n}"),
            index,
            kind: note.kind,
            text: note.text.clone(),
            band: None,
            rule: None,
            gaps: Vec::new(),
            label: None,
        };
        let origin: Option<[f32; 2]> = match note.kind {
            AnnotationKind::Rule => match (&note.at.x, &note.at.y) {
                // Up the plot at a value, its words beside the rule's top, after it unless
                // that would pass the plot's end.
                (_, Some(p)) => {
                    let x = to_x(value_of(p));
                    placed.rule = Some(Rule { from: [x, top], to: [x, bottom], width: 0.0, color: Color([0; 4]) });
                    text.as_ref().map(|t| {
                        let after = x + 0.5 * gap;
                        let x0 = if after + t.width <= right { after } else { x - 0.5 * gap - t.width };
                        [x0, top - (t.lines[0].baseline - cap(t))]
                    })
                }
                // Across the plot through a category's middle, its words over the rule, clear
                // of the marks and words behind them.
                (Some(p), None) => {
                    let [_, y, _] = y_at(p.values()[0])?;
                    placed.rule = Some(Rule { from: [left, y], to: [right, y], width: 0.0, color: Color([0; 4]) });
                    text.as_ref().map(|t| {
                        let up = y - 0.5 * gap - t.lines[0].descent - t.lines[0].baseline;
                        let (a, b) = (up + t.lines[0].baseline - cap(t), y);
                        let beside = |y0: f32, y1: f32| y0 < b && y1 > a;
                        let behind: Vec<[f32; 2]> = (out.marks.iter())
                            .filter_map(|m| match m.shape {
                                Shape::Bar(r) => beside(r.y, r.y + r.h).then_some([r.x, r.x + r.w]),
                                _ => None,
                            })
                            .chain(
                                (out.labels.iter().map(ink))
                                    .chain(out.notes.iter().filter_map(|n| n.label.as_ref()).map(ink))
                                    .filter(|q| beside(q[1], q[3]))
                                    .map(|q| [q[0], q[2]]),
                            )
                            .collect();
                        [clear_along(left, right, t.width, &behind, 0.5 * gap), up]
                    })
                }
                (None, None) => unreachable!("checked: a rule stands at x or y"),
            },
            AnnotationKind::Band => {
                let rect = match (&note.at.x, &note.at.y) {
                    (_, Some(p)) => {
                        let ends: Vec<f32> =
                            p.values().iter().map(|v| to_x(v.position(false).unwrap_or(0.0))).collect();
                        let (x0, x1) = (ends[0].min(ends[1]), ends[0].max(ends[1]));
                        [x0, top, x1 - x0, bottom - top]
                    }
                    (Some(p), None) => {
                        let ends = p.values();
                        let (a, b) = (y_at(ends[0])?, y_at(ends[1])?);
                        let (y0, y1) = (a[0].min(b[0]), a[2].max(b[2]));
                        [left, y0, right - left, y1 - y0]
                    }
                    (None, None) => unreachable!("checked: a band spans x or y"),
                };
                placed.band = Some((rect, Color([0; 4])));
                // Its words inside its top-left corner, moved on past a rule up the plot
                // they would cross.
                text.as_ref().map(|t| {
                    let mut x0 = rect[0] + 0.5 * gap;
                    for _ in 0..levels.len() {
                        match levels.iter().find(|&&x| x > x0 - gap && x < x0 + t.width + gap) {
                            Some(&x) => x0 = x + gap,
                            None => break,
                        }
                    }
                    [x0, rect[1] + 0.5 * gap - (t.lines[0].baseline - cap(t))]
                })
            }
            // A leader on from its point, or from its mark's value end and past its value,
            // to its words, which stand past everything in their way, the middle of their
            // cap height level with the leader.
            _ => {
                let x = note.at.x.as_ref().expect("checked: a callout stands at an x");
                let (ax, ay, below) = match &note.at.y {
                    Some(p) => (to_x(value_of(p)), y_at(x.values()[0])?[1], false),
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
                        let Shape::Bar(r) = mark.shape else { unreachable!("a chart across draws bars") };
                        let below = rows[i].y < 0.0;
                        let end = if below { r.x } else { r.x + r.w };
                        let end = match out.labels.iter().find(|l| l.key == mark.key) {
                            Some(l) if below => end.min(ink(l)[0]),
                            Some(l) => end.max(ink(l)[2]),
                            None => end,
                        };
                        (end, r.y + 0.5 * r.h, below)
                    }
                };
                let height = text.as_ref().map_or(0.0, |t| cap(t) + t.lines[0].descent);
                let (y0, y1) = (ay - 0.5 * height - 0.5 * gap, ay + 0.5 * height + 0.5 * gap);
                let beside = |a: f32, b: f32| a < y1 && b > y0;
                // Past every mark, value, and earlier annotation's words beside its own.
                let edges = (out.marks.iter())
                    .filter_map(|m| match m.shape {
                        Shape::Bar(r) => beside(r.y, r.y + r.h).then_some([r.x, r.x + r.w]),
                        _ => None,
                    })
                    .chain(
                        (out.labels.iter().map(ink))
                            .chain(out.notes.iter().filter_map(|n| n.label.as_ref()).map(ink))
                            .filter(|q| beside(q[1], q[3]))
                            .map(|q| [q[0], q[2]]),
                    );
                let mut clear = match below {
                    true => edges.map(|e| e[0]).fold(ax, f32::min),
                    false => edges.map(|e| e[1]).fold(ax, f32::max),
                };
                // Its words never sit on a rule up the plot: they move on past one.
                if let Some(t) = text.as_ref() {
                    for _ in 0..levels.len() {
                        let [a, b] = match below {
                            true => [clear - 3.0 * gap - t.width, clear - 3.0 * gap],
                            false => [clear + 3.0 * gap, clear + 3.0 * gap + t.width],
                        };
                        match levels.iter().find(|&&x| x > a - gap && x < b + gap) {
                            Some(&x) => clear = x,
                            None => break,
                        }
                    }
                }
                let dir = if below { -1.0 } else { 1.0 };
                let (from, to) = ([ax + dir * 0.5 * gap, ay], [clear + dir * 2.5 * gap, ay]);
                placed.rule = Some(Rule { from, to, width: 0.0, color: Color([0; 4]) });
                text.as_ref().map(|t| {
                    let x0 = if below { clear - 3.0 * gap - t.width } else { clear + 3.0 * gap };
                    [x0, ay + 0.5 * cap(t) - t.lines[0].baseline]
                })
            }
        };
        placed.label = origin.zip(text).map(|(at, t)| Label::new(placed.key.clone(), at, t, None));
        out.notes.push(placed);
    }
    Ok(())
}

/// Highlights: what they pick takes the signal color, and the rest dims, as on a chart that
/// stands up.
fn lit(out: &mut ChartLayout, rows: &[Row], notes: &[Annotation], signal: Option<Color>, dimmed: f32) {
    let highlights: Vec<&Annotation> = notes.iter().filter(|n| n.kind == AnnotationKind::Highlight).collect();
    out.highlights = (notes.iter().enumerate())
        .filter(|(_, n)| n.kind == AnnotationKind::Highlight)
        .map(|(i, h)| (i, rows.iter().filter(|r| picks(h, r)).map(|r| r.key.clone()).collect()))
        .collect();
    let Some(signal) = signal.filter(|_| !highlights.is_empty()) else { return };
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
    for e in &mut out.legend {
        let (picked, all) = (rows.iter().filter(|r| r.series.as_deref() == Some(e.key.as_str())))
            .fold((0, 0), |(p, n), r| (p + usize::from(lit.contains(&r.key.as_str())), n + 1));
        if picked == 0 {
            e.color = dim(e.color);
            e.label.opacity = words;
        } else if picked == all {
            e.color = signal;
        }
    }
}

/// A rule, a leader, or a band in the annotations' stroke and colors, each rule breaking
/// where it would cross words.
fn finish_notes(out: &mut ChartLayout, (width, rule, band): (f32, Color, Color), gap: f32) {
    for note in &mut out.notes {
        if let Some(r) = note.rule.as_mut() {
            r.width = width;
            r.color = rule;
        }
        if let Some((_, c)) = note.band.as_mut() {
            *c = band;
        }
    }
    let texts: Vec<(String, [f32; 4])> = (out.labels.iter())
        .chain(out.notes.iter().filter_map(|n| n.label.as_ref()))
        .map(|l| (l.key.clone(), text_box(l)))
        .chain(out.legend.iter().map(|e| (format!("legend\u{1f}{}", e.key), text_box(&e.label))))
        .collect();
    for note in &mut out.notes {
        let Some(rule) = &note.rule else { continue };
        let own = |k: &str| note.label.as_ref().is_some_and(|l| l.key == k);
        let gaps: Vec<Gap> =
            (texts.iter()).filter(|(k, _)| !own(k)).filter_map(|(k, b)| crossing(rule, k, *b, 0.5 * gap)).collect();
        note.gaps = gaps;
    }
}
