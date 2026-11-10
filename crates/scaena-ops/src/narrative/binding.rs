//! A made chart bound to the data source its slide names (agent dogfood): a slide's
//! `chartData.sourceId` that names one of the deck's sources, by its id or its file's name, draws
//! from it, so an edit of the data sets the chart again. Only where the source holds the slide's
//! categories and its figures as written: a chart the file says otherwise stays on its own rows,
//! so a made deck never draws a figure its writer did not write (DF7-1), and why is said.

use scaena_core::Deck;
use scaena_core::data::{ColumnType, Datum, SourceFiles, Table};
use scaena_narrato::ChartData;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A chart that names a data source and was not bound to it, and why (agent dogfood): the slide,
/// by its number from 1, the source it named, and what kept it on its own rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Unbound {
    pub slide: usize,
    pub source: String,
    pub why: String,
}

/// A name as an id would be made of it: letters and digits alone, in lower case, its file's
/// extension off: `Trails Budget.csv`, `trails-budget`, and `trails_budget` alike.
fn stem(name: &str) -> String {
    let name = name.trim();
    let name = name.rsplit_once('/').map_or(name, |(_, file)| file);
    let name = name.rsplit_once('.').filter(|(b, e)| !b.is_empty() && e.len() <= 5).map_or(name, |(b, _)| b);
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The deck's data source `named` names: by its id, else by the file it is read from.
pub fn source_named(deck: &Deck, named: &str) -> Option<String> {
    let wanted = stem(named);
    if wanted.is_empty() {
        return None;
    }
    let by_id = deck.data.keys().find(|id| stem(id) == wanted);
    let by_file = || {
        deck.data.iter().find_map(|(id, decl)| {
            let file = serde_json::to_value(decl).ok()?.get("source")?.as_str()?.to_string();
            (stem(&file) == wanted).then(|| id.clone())
        })
    };
    by_id.cloned().or_else(by_file)
}

/// Whether `cell` is `category` as a slide writes it: its words, or its number, or its date.
fn is(cell: &Datum, category: &str) -> bool {
    let category = category.trim();
    match cell {
        Datum::Text(s) => s.trim() == category,
        Datum::Number(n) => category.replace(',', "").parse::<f64>().is_ok_and(|c| (c - n).abs() < 1e-9),
        Datum::Null => false,
        other => other.label() == category,
    }
}

/// `chart`'s node, built on its own rows, bound to the source `id` of `deck`, read by `files`:
/// its x the column that holds the categories, its y the column each series names, else the one
/// number column, several folded into `series` and `value`. Each figure the slide writes must be
/// the file's, to the places the slide shows; else why not.
/// `Ok(Some(column))` where the source's `column` is to be typed `date` in its schema, as the
/// chart's own rows type their years, so a line along years runs on time bound as it does on its
/// own rows (G.18).
pub fn bind(
    node: &mut Value,
    chart: &ChartData,
    deck: &Deck,
    files: &dyn SourceFiles,
    id: &str,
) -> Result<Option<String>, String> {
    let table =
        scaena_core::data::load(deck, files, id).map_err(|e| format!("its source `{id}` does not read: {e}"))?;
    let x = (0..table.columns.len())
        .find(|&c| {
            chart.categories.iter().all(|cat| table.rows.iter().any(|r| is(&r[c], cat)))
                && table.rows.iter().all(|r| chart.categories.iter().any(|cat| is(&r[c], cat)))
        })
        .ok_or_else(|| format!("no column of `{id}` holds the slide's categories ({})", chart.categories.join(", ")))?;
    let numbers: Vec<usize> =
        (0..table.columns.len()).filter(|&c| c != x && table.types[c] == ColumnType::Number).collect();
    let several = chart.series.len() > 1;
    let mut columns = Vec::new();
    for series in &chart.series {
        let named = series.name.as_deref().map(stem).filter(|n| !n.is_empty());
        let column = match named.and_then(|n| numbers.iter().copied().find(|&c| stem(&table.columns[c]) == n)) {
            Some(c) => c,
            None if !several && numbers.len() == 1 => numbers[0],
            None => {
                return Err(format!(
                    "no number column of `{id}` is named for the series {}",
                    series.name.as_deref().map_or("with no name".to_string(), |n| format!("\u{201c}{n}\u{201d}"))
                ));
            }
        };
        columns.push(column);
    }
    agrees(chart, &table, x, &columns, id)?;
    let names: Vec<&str> = columns.iter().map(|&c| table.columns[c].as_str()).collect();
    node["data"] = json!(format!("@{id}"));
    node["x"]["field"] = json!(table.columns[x]);
    let mut dated = None;
    if node["x"]["type"] == "temporal" && table.types[x] != ColumnType::Date {
        if years(deck, files, id, &table, x) {
            dated = Some(table.columns[x].clone());
        } else {
            node["x"]["type"] = json!("ordinal");
        }
    }
    let mut steps: Vec<Value> = Vec::new();
    if several {
        steps.push(json!({ "fold": names, "as": ["series", "value"] }));
    } else {
        node["y"]["field"] = json!(names[0]);
    }
    let y = node["y"]["field"].as_str().unwrap_or("value").to_string();
    for step in node.get("dataTransform").and_then(Value::as_array).into_iter().flatten() {
        // A sort the chart asks by its value sorts the value bound.
        let mut step = step.clone();
        if let Some(by) = step.get("sort").and_then(Value::as_str).map(str::to_string) {
            let desc = by.starts_with('-');
            if by.trim_start_matches('-') == "value" {
                step["sort"] = json!(format!("{}{y}", if desc { "-" } else { "" }));
            }
        }
        steps.push(step);
    }
    if steps.is_empty() {
        node.as_object_mut().map(|o| o.remove("dataTransform"));
    } else {
        node["dataTransform"] = json!(steps);
    }
    // Where it reads only some rows, the chart's own words say so no more than they did.
    Ok(dated)
}

/// Whether the source `id`'s column `x` holds years alone, four digits each, as the chart's own
/// rows do when its line runs on time (G.18), and reads as dates typed so: where nothing else in
/// the deck reads the source, so no table of it is set otherwise, and it declares no `parse`.
fn years(deck: &Deck, files: &dyn SourceFiles, id: &str, table: &Table, x: usize) -> bool {
    let year = |cell: &Datum| match cell {
        Datum::Number(n) => n.fract() == 0.0 && (1000.0..=9999.0).contains(n),
        Datum::Text(s) => s.trim().len() == 4 && s.trim().bytes().all(|b| b.is_ascii_digit()) && s.trim() >= "1000",
        _ => false,
    };
    if table.rows.len() < 2 || !table.rows.iter().all(|r| year(&r[x])) {
        return false;
    }
    let read = format!("\"@{id}\"");
    if serde_json::to_string(&deck.nodes).is_ok_and(|nodes| nodes.contains(&read)) {
        return false;
    }
    let column = &table.columns[x];
    let Some(decl) = deck.data.get(id) else { return false };
    if decl.parse.as_ref().is_some_and(|p| p.contains_key(column)) {
        return false;
    }
    let mut typed = deck.clone();
    let Some(decl) = typed.data.get_mut(id) else { return false };
    decl.schema.get_or_insert_with(Default::default).insert(column.clone(), "date".into());
    scaena_core::data::load(&typed, files, id).is_ok_and(|t| t.types[x] == ColumnType::Date)
}

/// Whether each figure the slide writes is the file's, to the places the slide shows.
fn agrees(chart: &ChartData, table: &Table, x: usize, columns: &[usize], id: &str) -> Result<(), String> {
    for (k, category) in chart.categories.iter().enumerate() {
        let rows: Vec<&Vec<Datum>> = table.rows.iter().filter(|r| is(&r[x], category)).collect();
        for (series, &column) in chart.series.iter().zip(columns) {
            let Some(Some(written)) = series.values.get(k) else { continue };
            let places = places(*written);
            let found = rows.iter().find_map(|r| match r[column] {
                Datum::Number(n) => Some(n),
                _ => None,
            });
            match found {
                Some(n) if (round(n, places) - written).abs() < 0.5 * 10f64.powi(-(places as i32)) => {}
                Some(n) => {
                    return Err(format!(
                        "the slide writes {written} for {category}, where `{id}`'s `{}` holds {n}",
                        table.columns[column]
                    ));
                }
                None => return Err(format!("`{id}` gives no `{}` for {category}", table.columns[column])),
            }
        }
    }
    Ok(())
}

/// The decimal places a figure is written to, two at most.
fn places(v: f64) -> usize {
    (0..=2usize).find(|d| (v * 10f64.powi(*d as i32)).fract().abs() < 1e-9).unwrap_or(2)
}

fn round(v: f64, places: usize) -> f64 {
    let k = 10f64.powi(places as i32);
    (v * k).round() / k
}
