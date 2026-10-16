//! The weather chart (SPEC §3.7, ADR-0030 decision 19, PLAN O.2b): a forecast's periods keyed by
//! time, each its name, its sky's icon, and its figures, in columns across the box, or in rows down
//! one taller than wide, and one period alone drawn large. Hours step by 1, 2, 3, or 6 to fit.
//! Units convert at layout, so a change of units fetches nothing. A highlight sets its period on a
//! band. The chart reads as a table: each part a cell, the sky's icon one whose layer says the sky
//! in words.

use super::{ChartKind, ChartLayout, Ctx, Icon, Label, Note};
use crate::EngineError;
use crate::data::{self, ColumnType, Datum};
use crate::text::{TextLayout, TextSpec};
use crate::theme::Numeric;
use scaena_core::document::Props;
use scaena_core::format::{DateFormat, Locale, MINUS};
use scaena_core::model::nodes::{Weather, WeatherPart};
use scaena_core::model::theme::{IconWeight, TableText};
use scaena_core::model::values::AnnotationKind;
use scaena_core::weather::Units;
use serde_json::Value;

/// What a period shows when the chart does not say.
pub const SHOWN: [WeatherPart; 5] =
    [WeatherPart::Period, WeatherPart::Sky, WeatherPart::High, WeatherPart::Low, WeatherPart::Chance];

/// The hours a forecast by the hour may step by to fit, least first.
pub const STEPS: [usize; 4] = [1, 2, 3, 6];

/// The chance of rain from which a drop is drawn beside it, in percent.
const DROP_FROM: f64 = 10.0;

/// A part's name, as a cell's key ends.
fn part_name(part: WeatherPart) -> &'static str {
    match part {
        WeatherPart::Period => "period",
        WeatherPart::Sky => "sky",
        WeatherPart::Words => "words",
        WeatherPart::High => "high",
        WeatherPart::Low => "low",
        WeatherPart::Chance => "chance",
        WeatherPart::Wind => "wind",
    }
}

/// One period, read from its row.
struct Period {
    /// Its key: the period as ISO 8601 writes it.
    key: String,
    /// Its name as drawn: the weekday, or the hour.
    name: String,
    code: Option<&'static scaena_core::weather::Code>,
    day: bool,
    high: Option<f64>,
    low: Option<f64>,
    chance: Option<f64>,
    wind: Option<f64>,
}

/// A part laid out before it is placed: its text, or an icon of its size.
enum Laid {
    Text(TextLayout),
    /// A sky's icon, by its name and words; a chance's drop beside its text.
    Icon {
        name: &'static str,
        words: String,
    },
    Chance {
        text: TextLayout,
        drop: bool,
    },
}

impl Laid {
    fn width(&self, icon: f32) -> f32 {
        match self {
            Laid::Text(t) => t.width,
            Laid::Icon { .. } => icon,
            Laid::Chance { text, drop } => text.width + if *drop { 0.9 * text.height } else { 0.0 },
        }
    }

    fn height(&self, icon: f32) -> f32 {
        match self {
            Laid::Text(t) | Laid::Chance { text: t, .. } => t.height,
            Laid::Icon { .. } => icon,
        }
    }
}

/// The weather chart of `props` in a box of `size`.
pub fn compile(cx: &mut Ctx, props: &Props, size: [f32; 2]) -> Result<ChartLayout, EngineError> {
    let spec: Weather = match props.get("weather") {
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| EngineError::Layout(format!("`weather`: {e}")))?,
        None => Weather { fields: None, units: None, show: None, every: None, band: None },
    };
    let fields = spec.fields.clone().unwrap_or_default();
    let shown: Vec<WeatherPart> = match &spec.show {
        Some(show) if !show.is_empty() => show.clone(),
        _ => SHOWN.to_vec(),
    };
    let source = props.get("data").and_then(Value::as_str).and_then(|d| d.strip_prefix('@'));
    let source = source.ok_or_else(|| EngineError::Layout("chart has no `data`".into()))?;
    let (table, _) = data::traced(data::load(cx.deck, cx.data, source)?, props.get("dataTransform"))?;
    let fetched = cx.deck.data.get(source).and_then(|d| d.fetched());
    let forecast = fetched.as_ref().and_then(|f| f.forecast.as_ref());
    let lang = cx.deck.meta.as_ref().and_then(|m| m.lang.as_deref());
    let locale = Locale::of(lang);

    // The columns each part reads: the chart's, else the forecast's own.
    let period_field = props.get("x").and_then(|x| x.get("field")).and_then(Value::as_str);
    let period_field = period_field
        .or_else(|| ["date", "time"].into_iter().find(|f| table.column(f).is_some()))
        .ok_or_else(|| EngineError::Data("a weather chart reads its periods from `date` or `time`".into()))?;
    let pc = table.column(period_field).ok_or_else(|| EngineError::Data(format!("no column `{period_field}`")))?;
    if table.types[pc] != ColumnType::Date {
        return Err(EngineError::Data(format!("`{period_field}` must hold dates to key a forecast's periods")));
    }
    let hourly = forecast.map(|f| f.period == scaena_core::weather::Period::Hour).unwrap_or_else(|| {
        table.rows.iter().any(|r| matches!(r[pc], Datum::Date(t) if t.0.rem_euclid(86_400) != 0))
            || period_field == "time"
    });
    let column = |given: &Option<String>, default: &str| -> Option<usize> {
        match given {
            Some(f) => table.column(f),
            None => table.column(default),
        }
    };
    let sky_col = column(&fields.sky, "sky");
    let high_col = column(&fields.high, if hourly { "temperature" } else { "high" });
    let low_col = column(&fields.low, "low");
    let chance_col = column(&fields.chance, "rain");
    let wind_col = column(&fields.wind, "wind");
    let day_col = column(&fields.day, "day");
    for (given, name) in [
        (&fields.sky, "sky"),
        (&fields.high, "high"),
        (&fields.low, "low"),
        (&fields.chance, "chance"),
        (&fields.wind, "wind"),
        (&fields.day, "day"),
    ] {
        if let Some(f) = given
            && table.column(f).is_none()
        {
            return Err(EngineError::Data(format!("`weather.fields.{name}` names no column: `{f}`")));
        }
    }

    // Units: the view's, converted from the source's at layout.
    let came = forecast.map_or(Units::Metric, |f| f.units);
    let units = spec.units.unwrap_or(came);
    let temperature = |v: f64| match (came, units) {
        (Units::Metric, Units::Us) => v * 9.0 / 5.0 + 32.0,
        (Units::Us, Units::Metric) => (v - 32.0) * 5.0 / 9.0,
        _ => v,
    };
    let speed = |v: f64| match (came, units) {
        (Units::Metric, Units::Us) => v / 1.609_344,
        (Units::Us, Units::Metric) => v * 1.609_344,
        _ => v,
    };
    let number = |row: &[Datum], c: Option<usize>| match c.map(|c| &row[c]) {
        Some(Datum::Number(v)) if v.is_finite() => Some(*v),
        _ => None,
    };
    let name_format = DateFormat::parse(if hourly { "%-I %p" } else { "%a" })
        .map_err(|e| EngineError::Layout(format!("a period's name: {e}")))?;
    let mut periods: Vec<Period> = Vec::new();
    for row in &table.rows {
        let Datum::Date(t) = row[pc] else { continue };
        let code =
            match number(row, sky_col) {
                Some(c) => Some(scaena_core::weather::code(c).ok_or_else(|| {
                    EngineError::Data(format!("`{c}` is not a WMO weather code Open-Meteo documents"))
                })?),
                None => None,
            };
        let day = match day_col.map(|c| &row[c]) {
            Some(Datum::Bool(b)) => *b,
            Some(Datum::Number(n)) => *n != 0.0,
            _ => true,
        };
        periods.push(Period {
            key: row[pc].label(),
            name: name_format.format(t, locale),
            code,
            day,
            high: number(row, high_col).map(temperature),
            low: number(row, low_col).map(temperature),
            chance: number(row, chance_col),
            wind: number(row, wind_col).map(speed),
        });
    }

    // Theme: the chart's roles, in the surface's colors.
    let theme = cx.theme;
    let charts = theme.charts.as_ref();
    let tick_role = charts.and_then(|c| c.axis.as_ref()).and_then(|a| a.role.clone()).unwrap_or_else(|| "label".into());
    let label_role =
        charts.and_then(|c| c.label.as_ref()).and_then(|l| l.role.clone()).unwrap_or_else(|| "label".into());
    // The theme's forecast look (PLAN O.2f): each part's role and color, the icons', and the band's.
    let look = charts.and_then(|c| c.weather.clone()).unwrap_or_default();
    let text = |t: &Option<TableText>, role: &str, color: &str| -> (String, String) {
        let t = t.as_ref();
        let role = t.and_then(|t| t.role.clone()).unwrap_or_else(|| role.into());
        (role, t.and_then(|t| t.color.clone()).unwrap_or_else(|| color.into()))
    };
    let period_look = text(&look.period, &tick_role, "onSurface");
    let high_look = text(&look.high, &label_role, "onSurface");
    let low_look = text(&look.low, &label_role, "onSurfaceMuted");
    let chance_look = text(&look.chance, &tick_role, "onSurfaceMuted");
    let caption_look = text(&look.caption, &tick_role, "onSurfaceMuted");
    let icon_look = look.icon.clone().unwrap_or_default();
    let strong = theme.color(icon_look.color.as_deref().unwrap_or("onSurface"))?;
    let fill = icon_look.weight == Some(IconWeight::Fill);
    let muted = theme.color(&chance_look.1)?;
    let gap = theme.tokens.space.unit as f32;
    let band = theme.color(spec.band.as_deref().or(look.band.as_deref()).unwrap_or("surface-2"))?;

    // Which periods are drawn: every hour that fits, stepping by 1, 2, 3, or 6.
    let across = size[0] >= size[1];
    let lone = periods.len() == 1;
    let unit = |part: WeatherPart| -> &(String, String) {
        match part {
            WeatherPart::Period | WeatherPart::Sky => &period_look,
            WeatherPart::High => &high_look,
            WeatherPart::Low => &low_look,
            WeatherPart::Chance => &chance_look,
            WeatherPart::Words | WeatherPart::Wind => &caption_look,
        }
    };

    // Each part's text, at a scale `k` of its role's size.
    let mut set = |text: String, role: &str, color: &str, k: f32| -> Result<TextLayout, EngineError> {
        let mut role = theme.text_role(role)?;
        role.size *= k;
        role.color = Some(color.to_string());
        let spec = TextSpec { numeric: Some(Numeric::TabularLining), ..TextSpec::plain(role, text) };
        cx.text.layout(cx.fonts, theme, &spec, f32::INFINITY)
    };
    let degrees = |v: f64| {
        let r = v.round();
        let r = if r == 0.0 { 0.0 } else { r };
        let text = format!("{}°", r.abs() as i64);
        if r < 0.0 { format!("{MINUS}{text}") } else { text }
    };
    let wind_unit = match units {
        Units::Metric => "km/h",
        Units::Us => "mph",
    };
    let mut lay = |p: &Period, part: WeatherPart, k: f32| -> Result<Option<Laid>, EngineError> {
        let (role, color) = unit(part).clone();
        Ok(match part {
            WeatherPart::Period => Some(Laid::Text(set(p.name.clone(), &role, &color, k)?)),
            WeatherPart::Sky => p.code.map(|c| Laid::Icon { name: c.sky.icon(p.day), words: c.said(lang).into() }),
            WeatherPart::Words => match p.code {
                Some(c) => Some(Laid::Text(set(c.said(lang).into(), &role, &color, k)?)),
                None => None,
            },
            WeatherPart::High => match p.high {
                Some(v) => Some(Laid::Text(set(degrees(v), &role, &color, if lone { 2.0 * k } else { k })?)),
                None => None,
            },
            WeatherPart::Low => match p.low {
                Some(v) => Some(Laid::Text(set(degrees(v), &role, &color, k)?)),
                None => None,
            },
            WeatherPart::Chance => match p.chance {
                Some(v) => Some(Laid::Chance {
                    text: set(format!("{}%", v.round() as i64), &role, &color, k)?,
                    drop: v >= DROP_FROM,
                }),
                None => None,
            },
            WeatherPart::Wind => match p.wind {
                Some(v) => Some(Laid::Text(set(format!("{} {wind_unit}", v.round() as i64), &role, &color, k)?)),
                None => None,
            },
        })
    };

    // Each period's parts at the roles' sizes, to measure what a column needs.
    // The widest part and the tallest, and the widest row of a period's parts side by side, an
    // icon twice the tallest part's height.
    let mut measure = |k: f32, every: &[&Period]| -> Result<(f32, f32, f32), EngineError> {
        let (mut wide, mut tall, mut row) = (0.0f32, 0.0f32, 0.0f32);
        for p in every {
            let mut parts = Vec::new();
            for &part in &shown {
                if let Some(l) = lay(p, part, k)? {
                    wide = wide.max(l.width(0.0));
                    tall = tall.max(l.height(0.0));
                    parts.push(l);
                }
            }
            let across: f32 = parts.iter().map(|l| l.width(2.0 * tall)).sum::<f32>() + gap * parts.len() as f32;
            row = row.max(across);
        }
        Ok((wide, tall, row))
    };
    let every: Vec<&Period> = periods.iter().collect();
    let (wide, tall, _) = measure(1.0, &every)?;
    let step = match spec.every {
        Some(n) if hourly => n.max(1) as usize,
        _ if !hourly || lone => 1,
        _ => {
            let n = periods.len();
            let fits = |s: usize| {
                let count = n.div_ceil(s) as f32;
                if across { count * (wide + 2.0 * gap) <= size[0] } else { count * (tall + gap) <= size[1] }
            };
            STEPS.into_iter().find(|&s| fits(s)).unwrap_or(STEPS[STEPS.len() - 1])
        }
    };
    let drawn: Vec<&Period> = periods.iter().step_by(step).collect();
    let count = drawn.len().max(1) as f32;

    // A column, or a row, too narrow for its parts sets them smaller (W312 says so).
    let (cell_w, cell_h) = if across || lone { (size[0] / count, size[1]) } else { (size[0], size[1] / count) };
    let (wide, tall, row) = measure(1.0, &drawn)?;
    let k = if lone {
        1.0
    } else if across {
        ((cell_w - gap) / wide.max(1.0)).min(1.0)
    } else {
        (cell_h / (tall + gap).max(1.0)).min(cell_w / row.max(1.0)).min(1.0)
    };

    let mut out = ChartLayout::empty(ChartKind::Weather, source.to_string());
    out.plot = [0.0, 0.0, size[0], size[1]];
    // A highlight: its period on a band.
    let notes: Vec<scaena_core::model::values::Annotation> = match props.get("annotations") {
        None | Some(Value::Null) => Vec::new(),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| EngineError::Layout(format!("`annotations`: {e}")))?,
    };
    let lit: Vec<(usize, String)> = notes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == AnnotationKind::Highlight)
        .flat_map(|(i, n)| highlighted(n).into_iter().map(move |k| (i, k)))
        .collect();

    for (r, p) in drawn.iter().enumerate() {
        let origin = if across || lone { [r as f32 * cell_w, 0.0] } else { [0.0, r as f32 * cell_h] };
        let mut parts: Vec<(WeatherPart, Laid)> = Vec::new();
        for &part in &shown {
            if let Some(l) = lay(p, part, k)? {
                parts.push((part, l));
            }
        }
        for (i, key) in &lit {
            // A day by its date; an hour by its time, or every hour of a date named.
            if *key == p.key || (hourly && p.key.starts_with(key.as_str()) && key.len() == 10) {
                out.notes.push(Note {
                    key: format!("highlight\u{1f}{i}"),
                    index: *i,
                    kind: AnnotationKind::Highlight,
                    text: None,
                    band: Some(([origin[0], origin[1], cell_w, cell_h], band)),
                    rule: None,
                    gaps: Vec::new(),
                    label: None,
                });
                out.highlights.push((*i, vec![p.key.clone()]));
            }
        }
        // The icon: as large as the cell leaves it, and twice the type's height at most.
        let texts: f32 =
            parts.iter().filter(|(_, l)| !matches!(l, Laid::Icon { .. })).map(|(_, l)| l.height(0.0)).sum();
        let pieces = parts.len() as f32;
        let icon = if lone {
            (0.6 * cell_h).min(0.4 * cell_w)
        } else if across {
            let room = cell_h - texts - gap * (pieces + 1.0);
            room.min(0.6 * cell_w).min(3.5 * tall * k).max(0.0)
        } else {
            (0.8 * cell_h).min(3.5 * tall * k).max(0.0)
        };
        let row = r as u32 + 1;
        let place =
            |out: &mut ChartLayout, c: usize, part: WeatherPart, l: Laid, at: [f32; 2]| -> Result<(), EngineError> {
                let key = format!("{}\u{1f}{}", p.key, part_name(part));
                let cell = Some([row, c as u32]);
                match l {
                    Laid::Text(text) => {
                        let mut label = Label::new(key, at, text, None);
                        label.cell = cell;
                        out.ticks.push(label);
                    }
                    Laid::Icon { name, words } => {
                        let outline = Icon::outline(name, fill)?;
                        out.icons.push(Icon { key, cell, at, size: icon, name, outline, color: strong, words });
                    }
                    Laid::Chance { text, drop } => {
                        let d = 0.9 * text.height;
                        let mut label =
                            Label::new(key.clone(), [at[0] + if drop { d } else { 0.0 }, at[1]], text, None);
                        label.cell = cell;
                        if drop {
                            let size = 0.8 * d;
                            let y = at[1] + (label.text.height - size) / 2.0;
                            out.icons.push(Icon {
                                key: format!("{key}\u{1f}drop"),
                                cell: None,
                                at: [at[0], y],
                                size,
                                name: "drop",
                                outline: Icon::outline("drop", fill)?,
                                color: muted,
                                words: String::new(),
                            });
                        }
                        out.ticks.push(label);
                    }
                }
                Ok(())
            };
        if lone {
            // One period alone, drawn large: its icon at the left, its parts stacked beside it.
            let mut rest = Vec::new();
            let mut sky = None;
            for (c, (part, l)) in parts.into_iter().enumerate() {
                match l {
                    Laid::Icon { .. } => sky = Some((c, part, l)),
                    l => rest.push((c, part, l)),
                }
            }
            let heights: f32 = rest.iter().map(|(_, _, l)| l.height(icon)).sum::<f32>() + gap * rest.len() as f32;
            let left = if sky.is_some() { icon + 2.0 * gap } else { 0.0 };
            let x0 = ((cell_w - left - rest.iter().map(|(_, _, l)| l.width(icon)).fold(0.0, f32::max)) / 2.0).max(0.0);
            if let Some((c, part, l)) = sky {
                place(&mut out, c, part, l, [x0, (cell_h - icon) / 2.0])?;
            }
            let mut y = (cell_h - heights) / 2.0;
            for (c, part, l) in rest {
                let h = l.height(icon);
                place(&mut out, c, part, l, [x0 + left, y])?;
                y += h + gap;
            }
        } else if across {
            // A column: its parts stacked, each centered across it.
            let heights: f32 = parts.iter().map(|(_, l)| l.height(icon)).sum::<f32>() + gap * (pieces - 1.0).max(0.0);
            let mut y = origin[1] + (cell_h - heights) / 2.0;
            for (c, (part, l)) in parts.into_iter().enumerate() {
                let (w, h) = (l.width(icon), l.height(icon));
                place(&mut out, c, part, l, [origin[0] + (cell_w - w) / 2.0, y])?;
                y += h + gap;
            }
        } else {
            // A row: its parts across it, the period's name at the start, the rest spread.
            let widths: Vec<f32> = parts.iter().map(|(_, l)| l.width(icon)).collect();
            let spare = (cell_w - widths.iter().sum::<f32>()).max(0.0) / pieces.max(1.0);
            let mut x = origin[0];
            for (c, (part, l)) in parts.into_iter().enumerate() {
                let (w, h) = (l.width(icon), l.height(icon));
                place(&mut out, c, part, l, [x, origin[1] + (cell_h - h) / 2.0])?;
                x += w + spare;
            }
        }
    }
    Ok(out)
}

/// The periods a highlight names: its `at.x`, a value or several, as ISO 8601 writes them.
fn highlighted(note: &scaena_core::model::values::Annotation) -> Vec<String> {
    use scaena_core::model::values::Scalar;
    let values = note.at.x.as_ref().map(|x| x.values()).unwrap_or_default();
    values
        .into_iter()
        .filter_map(|v| match v {
            Scalar::Text(t) => Some(t.clone()),
            Scalar::Number(_) => None,
        })
        .collect()
}
