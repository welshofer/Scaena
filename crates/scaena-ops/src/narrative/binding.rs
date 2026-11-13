//! A made chart bound to the data source its slide names (agent dogfood): a slide's
//! `chartData.sourceId` that names one of the deck's sources, by its id or its file's name, else
//! its `sourceRef` that does (dogfood 9), draws from it, so an edit of the data sets the chart
//! again. Only where the source holds the slide's
//! categories and its figures as written: a chart the file says otherwise stays on its own rows,
//! so a made deck never draws a figure its writer did not write (DF7-1), and why is said.

use scaena_core::Deck;
use scaena_core::data::{ColumnType, Datum, SourceFiles, Table};
use scaena_core::inserts::slug;
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

/// The words of a series' or a column's name that say what it measures, its unit's aside, as
/// quoting reads a column's (agent dogfood 2): "On time" and `on_time_pct` alike, `ontime`. A
/// currency's letters before its sign are its unit too: "Receita (R$ mi)" measures "Receita"
/// (W1's dogfood 11). What it is measured per is its unit too, "je Tonne" as `_t`, and a German
/// name's umlauts are read as a file's name writes them: "Gebühr je Tonne" measures `gebuehr`,
/// as `gebuehr_eur_t` does (W1 dogfood 13). A Romance name's accents are read as a file's name
/// drops them, and the word that joins two of its words is no word of what it measures: "Baisse
/// de température" measures `baisse_temperature_c`, as "Taux de chômage" does `taux_chomage` (W1's
/// dogfood 19). A temperature's unit is its unit too, `_c` and "(°C)".
fn measured(name: &str) -> String {
    const PER: [&str; 5] = ["je", "pro", "per", "par", "por"];
    const MASSES: [&str; 6] = ["t", "kg", "c", "degc", "celsius", "f"];
    const JOINING: [&str; 21] = [
        "de", "du", "des", "d", "la", "le", "les", "l", "of", "the", "del", "da", "do", "di", "dos", "das", "e", "el",
        "los", "las", "il",
    ];
    let name = ["R$", "r$", "US$", "us$"].iter().fold(name.to_string(), |n, sign| n.replace(sign, " brl "));
    let name = (name.to_lowercase().chars()).fold(String::new(), |mut out, c| {
        match c {
            '\u{e4}' => out.push_str("ae"),
            '\u{f6}' => out.push_str("oe"),
            '\u{fc}' => out.push_str("ue"),
            '\u{df}' => out.push_str("ss"),
            '\u{153}' => out.push_str("oe"),
            '\u{e6}' => out.push_str("ae"),
            '\u{e0}'..='\u{e3}' | '\u{e5}' => out.push('a'),
            '\u{e7}' => out.push('c'),
            '\u{e8}'..='\u{eb}' => out.push('e'),
            '\u{ec}'..='\u{ef}' => out.push('i'),
            '\u{f1}' => out.push('n'),
            '\u{f2}'..='\u{f5}' => out.push('o'),
            '\u{f9}'..='\u{fb}' => out.push('u'),
            '\u{fd}' | '\u{ff}' => out.push('y'),
            c => out.push(c),
        }
        out
    });
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let mut out = String::new();
    let mut at = 0;
    while at < words.len() {
        let w = words[at];
        // "je Tonne", "per capita": the word after it is what it is measured per.
        if PER.contains(&w) && at + 1 < words.len() && at > 0 {
            at += 2;
            continue;
        }
        // "Baisse de température": a word that joins two others measures nothing.
        if JOINING.contains(&w) && at > 0 && at + 1 < words.len() {
            at += 1;
            continue;
        }
        if !super::quoting::UNITS.contains(&w) && !(MASSES.contains(&w) && at > 0) {
            out.push_str(w);
        }
        at += 1;
    }
    out
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

/// The data sources a chart's `named` source may be: the one called so, else the one sheet the
/// deck keeps by that sheet's own name, else each sheet the deck keeps from the workbook named,
/// whose id is the workbook's name and the sheet's (`navette-mensuel` for "navette.xlsx"), in the
/// deck's order. A model names the file it was given, never the id Scaena kept a sheet under
/// (W1's dogfood 10).
pub fn sources_named(deck: &Deck, named: &str) -> Vec<String> {
    if let Some(id) = source_named(deck, named) {
        return vec![id];
    }
    if stem(named).chars().count() < 3 {
        return Vec::new();
    }
    // A sheet by its own name, as the workbook shows it ("Annuel" for `velo-2025-annuel`), where
    // one sheet the deck keeps is called so (agent dogfood 4).
    let own = format!("-{}", slug(named.trim(), "data"));
    let sheets: Vec<String> = deck.data.keys().filter(|id| id.ends_with(&own)).cloned().collect();
    if sheets.len() == 1 {
        return sheets;
    }
    // The ids `keeping` makes of a workbook's sheets: the slug of its name, its extension off,
    // then each sheet's (`navette-mensuel`), so a source of another file whose name begins alike
    // (`navettes`) is never claimed.
    let file = named.trim().rsplit_once('/').map_or(named.trim(), |(_, file)| file);
    let base = slug(file.rsplit_once('.').map_or(file, |(stem, _)| stem), "data");
    let sheet = format!("{base}-");
    deck.data.keys().filter(|id| **id == base || id.starts_with(&sheet)).cloned().collect()
}

/// `node` bound to the first of `ids` that holds its slide's rows, as [`bind`] binds it: that
/// source's id and the column it typed as dates, if any; else why not, each source saying its
/// own where there are several, and `node` as it was.
pub fn bind_first(
    node: &mut Value,
    chart: &ChartData,
    deck: &Deck,
    files: &dyn SourceFiles,
    ids: &[String],
) -> Result<(String, Option<String>), String> {
    let mut whys = Vec::new();
    for id in ids {
        let mut tried = node.clone();
        match bind(&mut tried, chart, deck, files, id) {
            Ok(dated) => {
                *node = tried;
                return Ok((id.clone(), dated));
            }
            Err(why) => whys.push(if ids.len() == 1 { why } else { format!("`{id}`: {why}") }),
        }
    }
    Err(whys.join("; "))
}

/// How a cell is a category as a slide writes it: not, wholly, or by its month's name alone, which
/// a year the column also holds would name again (dogfood 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Named {
    No,
    Whole,
    /// A month named without its year: by its `short` name ("Jan"), its `long` ("January"), or
    /// both, where the two are one ("May").
    Month {
        short: bool,
        long: bool,
    },
    /// A year alone ("2021"), naming a date on its first day, as a column of years typed `date`
    /// holds it (agent dogfood 2).
    Year,
}

/// Whether `cell` is `category` as a slide writes it: its words, or its number, or its date.
fn is(cell: &Datum, category: &str) -> bool {
    named(cell, category) != Named::No
}

/// How `cell` is `category` as a slide writes it: its words, or its number, or its date, which a
/// slide writes as the file does, by its year and month ("2026-01"), or by its month's name, short
/// or long in any language Scaena formats, with its year after it or none ("Jan", "January 2026").
/// A month is a month's first day, as a monthly file's "2026-01" reads.
fn named(cell: &Datum, category: &str) -> Named {
    let category = category.trim();
    let whole = |yes: bool| if yes { Named::Whole } else { Named::No };
    match cell {
        Datum::Text(s) => whole(s.trim() == category),
        Datum::Number(n) => whole(category.replace(',', "").parse::<f64>().is_ok_and(|c| (c - n).abs() < 1e-9)),
        Datum::Null => Named::No,
        Datum::Date(t) => {
            if cell.label() == category {
                return Named::Whole;
            }
            let c = t.civil();
            if (c.day, c.hour, c.minute, c.second) != (1, 0, 0, 0) {
                return Named::No;
            }
            let number = |s: &str| s.bytes().all(|b| b.is_ascii_digit()).then(|| s.parse::<i64>().ok()).flatten();
            if category.len() == 4 && number(category).is_some() {
                let yes = c.month == 1 && number(category) == Some(c.year);
                return if yes { Named::Year } else { Named::No };
            }
            if let Some((y, m)) = category.split_once('-')
                && y.len() == 4
                && (1..=2).contains(&m.len())
            {
                return whole(number(y) == Some(c.year) && number(m) == Some(i64::from(c.month)));
            }
            let mut words = category.split_whitespace();
            let (Some(name), year, None) = (words.next(), words.next(), words.next()) else { return Named::No };
            let name = name.trim_end_matches(['.', ',']).to_lowercase();
            let month = |names: [&str; 12]| names.iter().position(|n| n.to_lowercase() == name);
            let locales = scaena_core::format::LOCALES.iter();
            // A month's name cut short as a chart's axis cuts it, its first three letters or more where
            // they begin no other month of its language ("Mär" for März beside the locale's "Mrz",
            // "Sept"; R1's agent dogfood 13), reads as its short name.
            let cut = |names: [&str; 12]| {
                let starts = |n: &&str| {
                    let n = n.to_lowercase();
                    name.chars().count() >= 3 && n.starts_with(&name) && n != name
                };
                let mut found = names.iter().enumerate().filter(|(_, n)| starts(n));
                match (found.next(), found.next()) {
                    (Some((k, _)), None) => Some(k),
                    _ => None,
                }
            };
            let long = locales.clone().find_map(|l| month(l.months));
            let short = (locales.clone().find_map(|l| month(l.short_months)))
                .or_else(|| long.is_none().then(|| locales.clone().find_map(|l| cut(l.months))).flatten());
            let Some(index) = short.or(long) else { return Named::No };
            if index + 1 != c.month as usize {
                return Named::No;
            }
            match year {
                None => Named::Month { short: short.is_some(), long: long.is_some() },
                Some(y) => whole(y.len() == 4 && number(y) == Some(c.year)),
            }
        }
        other => whole(other.label() == category),
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
    // The column that holds every category: each row of it one of them, else each category in
    // one row alone, the rest of the file's rows left out by a filter (W1's dogfood 17: a slide
    // charts 2021–2025 of a file that runs from 2016).
    let holds = |c: usize| chart.categories.iter().all(|cat| table.rows.iter().any(|r| is(&r[c], cat)));
    let only = |c: usize| table.rows.iter().all(|r| chart.categories.iter().any(|cat| is(&r[c], cat)));
    let once = |c: usize| chart.categories.iter().all(|cat| table.rows.iter().filter(|r| is(&r[c], cat)).count() == 1);
    let x = (0..table.columns.len())
        .find(|&c| holds(c) && only(c))
        .or_else(|| (0..table.columns.len()).find(|&c| holds(c) && once(c)))
        .ok_or_else(|| missing(chart, &table, id))?;
    let some = !only(x);
    // A month named without its year names one row, else the chart would read another year's.
    let months: Vec<Named> = (chart.categories.iter())
        .map(|cat| {
            table.rows.iter().map(|r| named(&r[x], cat)).find(|n| matches!(n, Named::Month { .. })).unwrap_or(Named::No)
        })
        .collect();
    for (cat, month) in chart.categories.iter().zip(&months) {
        let rows = table.rows.iter().filter(|r| is(&r[x], cat)).count();
        if *month != Named::No && rows > 1 {
            return Err(format!(
                "`{cat}` names {rows} rows of `{id}`'s `{}`, a month in more than one year",
                table.columns[x]
            ));
        }
    }
    let numbers: Vec<usize> =
        (0..table.columns.len()).filter(|&c| c != x && table.types[c] == ColumnType::Number).collect();
    let several = chart.series.len() > 1;
    let mut columns = Vec::new();
    for series in &chart.series {
        let named = series.name.as_deref().map(stem).filter(|n| !n.is_empty());
        let by_name = named.as_ref().and_then(|n| numbers.iter().copied().find(|&c| stem(&table.columns[c]) == *n));
        // Else by what it measures, its unit's words aside: "On time" for `on_time_pct`.
        let by_measure = || {
            let wanted = series.name.as_deref().map(measured).filter(|n| !n.is_empty())?;
            let mut found = numbers.iter().copied().filter(|&c| measured(&table.columns[c]) == wanted);
            let one = found.next();
            one.filter(|_| found.next().is_none())
        };
        // Else by its words, the one number column whose name holds every word of the series'
        // and more: "Planted" for `trees_planted` (W1's dogfood 17).
        let by_words = || {
            let wanted = series.name.as_deref().map(words_of).filter(|w| !w.is_empty())?;
            let mut found = (numbers.iter().copied())
                .filter(|c| !columns.contains(c))
                .filter(|&c| wanted.iter().all(|w| words_of(&table.columns[c]).contains(w)));
            let one = found.next();
            one.filter(|_| found.next().is_none())
        };
        let column = match by_name.or_else(by_measure).or_else(by_words) {
            Some(c) => c,
            None if !several && numbers.len() == 1 => numbers[0],
            None => {
                return Err(format!(
                    "no number column of `{id}` is named for the series {}; its number columns are {}",
                    series.name.as_deref().map_or("with no name".to_string(), |n| format!("\u{201c}{n}\u{201d}")),
                    listed(&numbers.iter().map(|&c| &table.columns[c]).collect::<Vec<_>>())
                ));
            }
        };
        columns.push(column);
    }
    agrees(chart, &table, x, &columns, id)?;
    let names: Vec<&str> = columns.iter().map(|&c| table.columns[c].as_str()).collect();
    node["data"] = json!(format!("@{id}"));
    // What it marks by a category marks the row bound, as the column reads it: a year a date
    // column holds, "2025", is its `2025-01-01` (agent dogfood 2), as lint reads `at.x`.
    let read_as = |category: &str| -> Option<String> {
        let row = table.rows.iter().find(|r| is(&r[x], category))?;
        Some(row[x].label()).filter(|label| label != category)
    };
    // What it marks by a series marks the column that series is bound to, as the fold names it:
    // "Median" lit is `median_minutes` (R1's agent dogfood 11).
    if several {
        for note in node.get_mut("annotations").and_then(Value::as_array_mut).into_iter().flatten() {
            let Some(at) = note.pointer_mut("/at/series") else { continue };
            let bound = (chart.series.iter().zip(&names))
                .find(|(s, _)| s.name.as_deref().is_some_and(|n| Some(n) == at.as_str()))
                .map(|(_, column)| *column);
            if let Some(column) = bound {
                *at = json!(column);
            }
        }
    }
    for note in node.get_mut("annotations").and_then(Value::as_array_mut).into_iter().flatten() {
        let Some(at) = note.pointer_mut("/at/x") else { continue };
        let places: Vec<&mut Value> = match at {
            Value::Array(ends) => ends.iter_mut().collect(),
            one => vec![one],
        };
        for place in places {
            if let Some(label) = place.as_str().and_then(read_as) {
                *place = json!(label);
            }
        }
    }
    node["x"]["field"] = json!(table.columns[x]);
    // Its years print as the slide wrote them, a date column's first days by their year alone.
    if node["x"].get("format").is_none() && !months.is_empty() {
        let years = (chart.categories.iter()).all(|cat| table.rows.iter().any(|r| named(&r[x], cat) == Named::Year));
        if years {
            node["x"]["format"] = json!("%Y");
        }
    }
    // Its months print as the slide wrote them, each by its name alone.
    if node["x"].get("format").is_none()
        && !months.is_empty()
        && months.iter().all(|m| matches!(m, Named::Month { .. }))
    {
        if months.iter().all(|m| matches!(m, Named::Month { short: true, .. })) {
            node["x"]["format"] = json!("%b");
        } else if months.iter().all(|m| matches!(m, Named::Month { long: true, .. })) {
            node["x"]["format"] = json!("%B");
        }
    }
    let mut dated = None;
    if node["x"]["type"] == "temporal" && table.types[x] != ColumnType::Date {
        if years(deck, files, id, &table, x) {
            dated = Some(table.columns[x].clone());
            // Its years typed as dates, what it marks by a year marks that year's first day,
            // as lint then reads its rows (R1's ferry).
            let first_day = |place: &mut Value| {
                if let Some(y) = place.as_str().filter(|y| y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit())) {
                    *place = json!(format!("{y}-01-01"));
                }
            };
            for note in node.get_mut("annotations").and_then(Value::as_array_mut).into_iter().flatten() {
                match note.pointer_mut("/at/x") {
                    Some(Value::Array(ends)) => ends.iter_mut().for_each(first_day),
                    Some(one) => first_day(one),
                    None => {}
                }
            }
        } else {
            node["x"]["type"] = json!("ordinal");
        }
    }
    let mut steps: Vec<Value> = Vec::new();
    if some {
        steps.push(json!({ "filter": kept(chart, &table, x, dated.is_some()) }));
    }
    if several {
        steps.push(json!({ "fold": names, "as": ["series", "value"] }));
    } else {
        node["y"]["field"] = json!(names[0]);
    }
    let y = node["y"]["field"].as_str().unwrap_or("value").to_string();
    // Bound to dates, the chart runs in time: a sort by its value, which bars of names with no
    // order of their own take, is dropped (R1's agent dogfood 12: Seville's months, May to
    // September, were drawn longest first).
    let in_time = table.types[x] == ColumnType::Date || dated.is_some();
    for step in node.get("dataTransform").and_then(Value::as_array).into_iter().flatten() {
        // A sort the chart asks by its value sorts the value bound.
        let mut step = step.clone();
        if let Some(by) = step.get("sort").and_then(Value::as_str).map(str::to_string) {
            let desc = by.starts_with('-');
            if by.trim_start_matches('-') == "value" {
                if in_time {
                    continue;
                }
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

/// Why no column of the source `id` holds `chart`'s categories, by the column that holds most of
/// them: the categories it lacks, or those it holds in more than one row, else its columns.
fn missing(chart: &ChartData, table: &Table, id: &str) -> String {
    let all = chart.categories.join(", ");
    let held = |c: usize| chart.categories.iter().filter(|cat| table.rows.iter().any(|r| is(&r[c], cat))).count();
    let best = (0..table.columns.len()).filter(|&c| held(c) > 0).max_by_key(|&c| (held(c), usize::MAX - c));
    let Some(c) = best else {
        return format!(
            "no column of `{id}` holds the slide's categories ({all}); its columns are {}",
            listed(&table.columns.iter().collect::<Vec<_>>())
        );
    };
    let column = &table.columns[c];
    let lacks: Vec<&str> = (chart.categories.iter())
        .filter(|cat| !table.rows.iter().any(|r| is(&r[c], cat)))
        .map(String::as_str)
        .collect();
    if !lacks.is_empty() {
        return format!(
            "`{id}`'s `{column}` holds the slide's categories ({all}) but no row of it is {}",
            lacks.join(" or ")
        );
    }
    let twice: Vec<&str> = (chart.categories.iter())
        .filter(|cat| table.rows.iter().filter(|r| is(&r[c], cat)).count() > 1)
        .map(String::as_str)
        .collect();
    format!(
        "`{id}`'s `{column}` holds the slide's categories ({all}), {} in more than one row, so which it charts is not known",
        twice.join(", ")
    )
}

/// The filter that keeps the rows of `table` whose column `x` is one of `chart`'s categories, by
/// their cells as the column reads them, `dated` where the bind types a column of years `date`:
/// each by its own cell, the exact set the slide names, so the chart keeps to its words if the
/// file gains a row between them.
fn kept(chart: &ChartData, table: &Table, x: usize, dated: bool) -> String {
    let column = &table.columns[x];
    let field = if column.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && column.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    {
        column.clone()
    } else {
        format!("`{}`", column.replace('`', ""))
    };
    let rows: Vec<&Datum> =
        (table.rows.iter().map(|r| &r[x])).filter(|cell| chart.categories.iter().any(|cat| is(cell, cat))).collect();
    let number = |cell: &Datum| match cell {
        Datum::Number(n) => Some(*n),
        Datum::Text(t) if dated => t.trim().parse::<f64>().ok(),
        _ => None,
    };
    let read = if dated { format!("year({field})") } else { field.clone() };
    let print = |n: f64| scaena_core::data::Datum::Number(n).label();
    let one = |cell: &Datum| match cell {
        Datum::Date(t) => {
            let c = t.civil();
            format!("(year({field}) == {} && month({field}) == {} && day({field}) == {})", c.year, c.month, c.day)
        }
        Datum::Text(t) if !dated => format!("{field} == '{}'", t.replace('\\', "\\\\").replace('\'', "\\'")),
        other => format!("{read} == {}", number(other).map_or_else(|| other.label(), print)),
    };
    rows.iter().map(|c| one(c)).collect::<Vec<_>>().join(" || ")
}

/// The words of a series' or a column's name, lowercase, its unit's aside: `trees_planted`'s
/// `trees` and `planted`.
fn words_of(name: &str) -> Vec<String> {
    (name.to_lowercase().split(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty() && !super::quoting::UNITS.contains(w))
        .map(str::to_string)
        .collect()
}

/// `names` as a reader lists them: "`a`, `b`, and `c`", or "none".
fn listed(names: &[&String]) -> String {
    let quoted: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
    match quoted.as_slice() {
        [] => "none".into(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use scaena_core::format::{Civil, DateTime};
    use scaena_narrato::answer::Series;
    use std::collections::BTreeMap;

    fn month(year: i64, month: u32) -> Datum {
        Datum::Date(DateTime::from_civil(Civil { year, month, day: 1, hour: 0, minute: 0, second: 0 }).unwrap())
    }

    /// A slide writes a monthly file's months as people do, by their names, with a year or none,
    /// in any language Scaena formats, or as the file does (dogfood 9).
    #[test]
    fn a_month_is_named_as_a_slide_writes_it() {
        let jan = month(2026, 1);
        for (written, how) in [
            ("2026-01-01", Named::Whole),
            ("2026-01", Named::Whole),
            ("Jan 2026", Named::Whole),
            ("January 2026", Named::Whole),
            ("Jan", Named::Month { short: true, long: false }),
            ("jan.", Named::Month { short: true, long: false }),
            ("January", Named::Month { short: false, long: true }),
            ("Januar", Named::Month { short: false, long: true }),
            ("Feb", Named::No),
            ("Jan 2025", Named::No),
            ("2025-01", Named::No),
            ("Jan 26", Named::No),
            ("Q1", Named::No),
        ] {
            assert_eq!(named(&jan, written), how, "{written}");
        }
        assert_eq!(named(&month(2026, 5), "May"), Named::Month { short: true, long: true });
        // A month's name cut short as an axis cuts it, where no other month of its language begins
        // so (R1's agent dogfood 13: a German chart's "Mär" beside the locale's "Mrz").
        assert_eq!(named(&month(2026, 3), "Mär"), Named::Month { short: true, long: false });
        assert_eq!(named(&month(2026, 9), "Sept"), Named::Month { short: true, long: false });
        assert_eq!(named(&month(2026, 6), "Juni"), Named::Month { short: false, long: true });
        assert_eq!(named(&month(2026, 3), "Mä"), Named::No, "two letters name no month");
        assert_eq!(named(&month(2026, 1), "Ju"), Named::No);
        // A day within the month is not the month.
        let fifteenth = Datum::Date(
            DateTime::from_civil(Civil { year: 2026, month: 1, day: 15, hour: 0, minute: 0, second: 0 }).unwrap(),
        );
        assert_eq!(named(&fifteenth, "Jan"), Named::No);
    }

    fn deck_with(csv: &str) -> (Deck, BTreeMap<String, Vec<u8>>) {
        let mut doc: Value = serde_json::from_str(include_str!("../../../../docs/examples/revenue.deck.json")).unwrap();
        doc["data"]["sunday-visits"] = json!({
            "source": "data/sunday-visits.csv",
            "schema": { "month": "date", "sunday_visits": "number", "weekday_average": "number" }
        });
        let files = BTreeMap::from([("data/sunday-visits.csv".to_string(), csv.as_bytes().to_vec())]);
        (Deck::from_json(&doc.to_string()).unwrap(), files)
    }

    fn line(values: &[f64]) -> (Value, ChartData) {
        let node = json!({
            "type": "chart", "chart": "line", "data": "@own",
            "x": { "field": "category", "type": "ordinal" },
            "y": { "field": "value", "type": "quantitative" }
        });
        let chart = ChartData {
            categories: ["Jan", "Feb", "Mar", "Apr", "May", "Jun"].map(String::from).to_vec(),
            series: vec![Series {
                name: Some("Sunday visits".into()),
                values: values.iter().map(|v| Some(*v)).collect(),
            }],
            unit: None,
            source_id: None,
        };
        (node, chart)
    }

    const SUNDAYS: &str = "month,sunday_visits,weekday_average\n2026-01,1840,1210\n2026-02,1920,1190\n2026-03,2105,1230\n2026-04,2260,1250\n2026-05,2390,1240\n2026-06,2475,1260\n";

    /// Dogfood 9's chart: "Jan" to "Jun" over a file whose months are dates binds to it, its
    /// months printed by their short names as the slide wrote them, so an edit of the file draws
    /// the chart again.
    #[test]
    fn a_chart_of_month_names_binds_to_a_file_of_dated_months() {
        let (deck, files) = deck_with(SUNDAYS);
        let (mut node, chart) = line(&[1840.0, 1920.0, 2105.0, 2260.0, 2390.0, 2475.0]);
        assert_eq!(bind(&mut node, &chart, &deck, &files, "sunday-visits"), Ok(None));
        assert_eq!(
            (node["data"].clone(), node["x"]["field"].clone(), node["x"]["format"].clone(), node["y"]["field"].clone()),
            (json!("@sunday-visits"), json!("month"), json!("%b"), json!("sunday_visits"))
        );
        // What it marks by a month marks that month's row as the date column reads it, so the
        // deck still validates (Route 9's bar chart on main after #1045, E103).
        let (mut node, chart) = line(&[1840.0, 1920.0, 2105.0, 2260.0, 2390.0, 2475.0]);
        node["annotations"] = json!([{ "kind": "highlight", "at": { "x": "Jun" } }]);
        bind(&mut node, &chart, &deck, &files, "sunday-visits").unwrap();
        assert_eq!(node["annotations"][0]["at"]["x"], json!("2026-06-01"));
        // A figure the file says otherwise still keeps the chart on its own rows.
        let (mut node, chart) = line(&[1840.0, 1920.0, 2105.0, 2260.0, 2390.0, 2500.0]);
        let why = bind(&mut node, &chart, &deck, &files, "sunday-visits").unwrap_err();
        assert!(why.contains("2500") && why.contains("2475"), "{why}");
    }

    /// A chart bound to a date column runs in time: the sort by value that bars of names with no
    /// order of their own take is dropped (R1's agent dogfood 12: Seville's months, bound to the
    /// pilot's dates, were drawn longest first).
    #[test]
    fn a_chart_bound_to_dates_is_never_sorted_by_its_values() {
        let (deck, files) = deck_with(SUNDAYS);
        let (mut node, chart) = line(&[1840.0, 1920.0, 2105.0, 2260.0, 2390.0, 2475.0]);
        node["chart"] = json!("bar");
        node["dataTransform"] = json!([{ "sort": "-value" }]);
        assert_eq!(bind(&mut node, &chart, &deck, &files, "sunday-visits"), Ok(None));
        assert_eq!(node["x"]["field"], json!("month"));
        assert!(node.get("dataTransform").is_none(), "{:#}", node["dataTransform"]);
    }

    /// A month named without its year that a file holds in two years names neither: the chart
    /// keeps its own rows, saying why.
    #[test]
    fn a_month_a_file_holds_in_two_years_binds_nothing() {
        let two = "month,sunday_visits,weekday_average\n2025-01,1700,1200\n2025-02,1750,1180\n2025-03,1800,1220\n2025-04,1810,1240\n2025-05,1830,1235\n2025-06,1835,1250\n2026-01,1840,1210\n2026-02,1920,1190\n2026-03,2105,1230\n2026-04,2260,1250\n2026-05,2390,1240\n2026-06,2475,1260\n";
        let (deck, files) = deck_with(two);
        let (mut node, chart) = line(&[1840.0, 1920.0, 2105.0, 2260.0, 2390.0, 2475.0]);
        let why = bind(&mut node, &chart, &deck, &files, "sunday-visits").unwrap_err();
        assert!(why.contains("holds the slide's categories") || why.contains("more than one year"), "{why}");
        assert_eq!(node["data"], json!("@own"));
    }

    const FERRY: &str = "year,riders,on_time_pct,fare_revenue_kusd,midday_turnaways\n2021,412000,91.2,1648,1900\n2022,468000,90.4,1872,3100\n2023,521000,89.8,2084,4800\n2024,566000,88.1,2264,7200\n2025,604000,86.9,2416,9800\n";

    /// R1's agent dogfood 2: `ferry.csv`, its `year` typed `date` as a first bind types it, or
    /// as `data_attach` reads it before, a number.
    fn ferry(year_dated: bool) -> (Deck, BTreeMap<String, Vec<u8>>) {
        let mut doc: Value = serde_json::from_str(include_str!("../../../../docs/examples/revenue.deck.json")).unwrap();
        let year = if year_dated { "date" } else { "number" };
        doc["data"]["ferry"] = json!({ "source": "data/ferry.csv", "schema": {
            "year": year, "riders": "number", "on_time_pct": "number",
            "fare_revenue_kusd": "number", "midday_turnaways": "number"
        } });
        let files = BTreeMap::from([("data/ferry.csv".to_string(), FERRY.as_bytes().to_vec())]);
        (Deck::from_json(&doc.to_string()).unwrap(), files)
    }

    fn by_year(chart: &str, series: &str, values: &[f64]) -> (Value, ChartData) {
        let node = json!({
            "type": "chart", "chart": chart, "data": "@own",
            "x": { "field": "category", "type": "ordinal" },
            "y": { "field": "value", "type": "quantitative" }
        });
        let chart = ChartData {
            categories: ["2021", "2022", "2023", "2024", "2025"].map(String::from).to_vec(),
            series: vec![Series { name: Some(series.into()), values: values.iter().map(|v| Some(*v)).collect() }],
            unit: None,
            source_id: Some("ferry".into()),
        };
        (node, chart)
    }

    /// W1's dogfood 17: a slide charts Planted and Removed for 2021–2025 from `canopy.csv`, which
    /// runs from 2016 and names its series `trees_planted` and `trees_removed`. Its years are the
    /// column's that holds each one once, the file's other rows filtered out, and each series the
    /// one number column whose name holds its words. A category the file lacks is said by name.
    #[test]
    fn a_chart_of_some_years_binds_the_rows_it_names() {
        const CANOPY: &str = "year,canopy_pct,trees_planted,trees_removed\n2016,31.0,800,900\n2017,30.6,820,940\n2018,30.1,790,980\n2019,29.7,850,1010\n2020,29.2,880,1050\n2021,28.9,1200,1000\n2022,28.7,1450,980\n2023,28.6,1700,950\n2024,28.8,2100,900\n2025,29.1,2500,880\n";
        let mut doc: Value = serde_json::from_str(include_str!("../../../../docs/examples/revenue.deck.json")).unwrap();
        doc["data"]["canopy"] = json!({ "source": "data/canopy.csv", "schema": {
            "year": "number", "canopy_pct": "number", "trees_planted": "number", "trees_removed": "number"
        } });
        let files = BTreeMap::from([("data/canopy.csv".to_string(), CANOPY.as_bytes().to_vec())]);
        let deck = Deck::from_json(&doc.to_string()).unwrap();
        let chart = |categories: &[&str]| ChartData {
            categories: categories.iter().map(|c| c.to_string()).collect(),
            series: vec![
                Series { name: Some("Planted".into()), values: [1200., 1450., 1700., 2100., 2500.].map(Some).to_vec() },
                Series { name: Some("Removed".into()), values: [1000., 980., 950., 900., 880.].map(Some).to_vec() },
            ],
            unit: None,
            source_id: Some("canopy".into()),
        };
        let node = || {
            json!({
                "type": "chart", "chart": "bar", "data": "@own",
                "x": { "field": "category", "type": "ordinal" },
                "y": { "field": "value", "type": "quantitative" }
            })
        };
        let mut bound = node();
        bind(&mut bound, &chart(&["2021", "2022", "2023", "2024", "2025"]), &deck, &files, "canopy").unwrap();
        assert_eq!(bound["data"], json!("@canopy"));
        // The series its headline lights marks the column bound to it, as the fold names it, so
        // lint finds it among the series (R1's agent dogfood 11).
        let mut lit = node();
        lit["annotations"] = json!([
            { "kind": "highlight", "at": { "series": "Planted" } },
            { "kind": "highlight", "at": { "series": "Shrubs" } }
        ]);
        bind(&mut lit, &chart(&["2021", "2022", "2023", "2024", "2025"]), &deck, &files, "canopy").unwrap();
        assert_eq!(lit["annotations"][0]["at"]["series"], json!("trees_planted"));
        // A series no series of the slide names is left as written, for lint's E103 to name the
        // column meant (R1's dogfood 10).
        assert_eq!(lit["annotations"][1]["at"]["series"], json!("Shrubs"));
        assert_eq!(bound["x"]["field"], json!("year"));
        assert_eq!(
            bound["dataTransform"],
            json!([
                { "filter": "year == 2021 || year == 2022 || year == 2023 || year == 2024 || year == 2025" },
                { "fold": ["trees_planted", "trees_removed"], "as": ["series", "value"] }
            ])
        );
        let mut doc = doc.clone();
        doc["nodes"]["bars"] = bound.clone();
        let props: scaena_core::document::Props = serde_json::from_value(bound.clone()).unwrap();
        let read = scaena_core::data::read(&Deck::from_json(&doc.to_string()).unwrap(), &files, &props);
        assert_eq!(read.map(|t| t.rows.len()).ok(), Some(10), "five years, two series each");
        // Years apart too.
        let mut apart = node();
        let mut two = chart(&["2021", "2023"]);
        two.series.iter_mut().for_each(|s| s.values = vec![s.values[0], s.values[2]]);
        bind(&mut apart, &two, &deck, &files, "canopy").unwrap();
        assert_eq!(apart["dataTransform"][0], json!({ "filter": "year == 2021 || year == 2023" }));
        // A year the file lacks is said by name.
        let mut late = node();
        let mut on = chart(&["2022", "2023", "2024", "2025", "2026"]);
        on.series.iter_mut().for_each(|s| s.values.rotate_left(1));
        let why = bind(&mut late, &on, &deck, &files, "canopy").unwrap_err();
        assert!(
            why.contains("`canopy`'s `year` holds the slide's categories") && why.ends_with("but no row of it is 2026"),
            "{why}"
        );
        assert_eq!(late["data"], json!("@own"));
    }

    /// A year a slide writes names a date column's first day of that year, once a first bind
    /// has typed the column `date` (agent dogfood 2, B2): the later charts bind too, printing
    /// their years as written.
    #[test]
    fn a_year_names_a_date_column_of_years() {
        assert_eq!(named(&month(2021, 1), "2021"), Named::Year);
        assert_eq!(named(&month(2021, 2), "2021"), Named::No);
        assert_eq!(named(&month(2022, 1), "2021"), Named::No);
        let (deck, files) = ferry(true);
        let (mut node, chart) = by_year("line", "Riders", &[412000.0, 468000.0, 521000.0, 566000.0, 604000.0]);
        assert_eq!(bind(&mut node, &chart, &deck, &files, "ferry"), Ok(None));
        assert_eq!(
            (node["x"]["field"].clone(), node["x"]["format"].clone(), node["y"]["field"].clone()),
            (json!("year"), json!("%Y"), json!("riders"))
        );
        // What it marks by a year marks that year's row as the column reads it.
        let (mut node, chart) = by_year("bar", "Riders", &[412000.0, 468000.0, 521000.0, 566000.0, 604000.0]);
        node["annotations"] = json!([
            { "kind": "highlight", "at": { "x": "2025" } },
            { "kind": "band", "at": { "x": ["2022", "2025"] }, "text": "since 2022" }
        ]);
        bind(&mut node, &chart, &deck, &files, "ferry").unwrap();
        assert_eq!(node["annotations"][0]["at"]["x"], json!("2025-01-01"));
        assert_eq!(node["annotations"][1]["at"]["x"], json!(["2022-01-01", "2025-01-01"]));
        // A number column of years reads them as written.
        let (deck, files) = ferry(false);
        let (mut node, chart) = by_year("bar", "Riders", &[412000.0, 468000.0, 521000.0, 566000.0, 604000.0]);
        node["annotations"] = json!([{ "kind": "highlight", "at": { "x": "2025" } }]);
        bind(&mut node, &chart, &deck, &files, "ferry").unwrap();
        assert_eq!(node["annotations"][0]["at"]["x"], json!("2025"));
    }

    /// A line along years that types its number column of years as dates marks its years as
    /// that column then reads them, each year's first day (R1's ferry, after #1056).
    #[test]
    fn a_line_that_types_its_years_as_dates_marks_them_so() {
        let (deck, files) = ferry(false);
        let (mut node, chart) = by_year("line", "Riders", &[412000.0, 468000.0, 521000.0, 566000.0, 604000.0]);
        node["x"]["type"] = json!("temporal");
        node["annotations"] = json!([
            { "kind": "highlight", "at": {} },
            { "kind": "rule", "at": { "x": "2021" }, "text": "2021" },
            { "kind": "band", "at": { "x": ["2022", "2025"] }, "text": "since 2022" }
        ]);
        assert_eq!(bind(&mut node, &chart, &deck, &files, "ferry"), Ok(Some("year".into())));
        assert_eq!(node["annotations"][1]["at"]["x"], json!("2021-01-01"));
        assert_eq!(node["annotations"][2]["at"]["x"], json!(["2022-01-01", "2025-01-01"]));
        assert_eq!(node["annotations"][1]["text"], json!("2021"), "its words as written");
    }

    /// A series named by what it measures binds to the column named so with its unit's words
    /// (agent dogfood 2): "On time" to `on_time_pct`, whether the years are dates or numbers.
    #[test]
    fn a_series_binds_to_its_column_with_its_units_words_aside() {
        for dated in [true, false] {
            let (deck, files) = ferry(dated);
            let (mut node, chart) = by_year("line", "On time", &[91.2, 90.4, 89.8, 88.1, 86.9]);
            let bound = bind(&mut node, &chart, &deck, &files, "ferry");
            assert!(bound.is_ok(), "dated {dated}: {bound:?}");
            assert_eq!(node["y"]["field"], json!("on_time_pct"), "dated {dated}");
        }
        assert_eq!(measured("On-time %"), "ontime");
        assert_eq!(measured("fare_revenue_kusd"), "farerevenue");
    }

    /// A series no number column is named for says the number columns the file has, so an agent
    /// can name one from the result alone (agent dogfood 2).
    #[test]
    fn a_series_named_for_no_column_says_the_columns_there_are() {
        let (deck, files) = ferry(true);
        let (mut node, chart) = by_year("bar", "Turned away", &[1900.0, 3100.0, 4800.0, 7200.0, 9800.0]);
        let why = bind(&mut node, &chart, &deck, &files, "ferry").unwrap_err();
        assert_eq!(
            why,
            "no number column of `ferry` is named for the series \u{201c}Turned away\u{201d}; its number columns are \
             `riders`, `on_time_pct`, `fare_revenue_kusd`, and `midday_turnaways`"
        );
        assert_eq!(node["data"], json!("@own"));
        let (mut node, mut chart) = by_year("bar", "Riders", &[412000.0, 468000.0, 521000.0, 566000.0, 604000.0]);
        chart.categories = ["FY21", "FY22", "FY23", "FY24", "FY25"].map(String::from).to_vec();
        let why = bind(&mut node, &chart, &deck, &files, "ferry").unwrap_err();
        assert!(
            why.ends_with(
                "its columns are `year`, `riders`, `on_time_pct`, `fare_revenue_kusd`, and `midday_turnaways`"
            ),
            "{why}"
        );
    }

    const UMSATZ: &str = "monat,umsatz_teur,neukunden,abwanderung_pct\nJanuar,412,38,2.1\nFebruar,398,31,2.4\nM\u{e4}rz,455,44,2.0\nApril,471,47,1.9\nMai,502,52,1.7\nJuni,538,61,1.5\n";

    /// R1's agent dogfood 3: a German file's columns carry German accounts' unit words
    /// (`umsatz_teur`), and its series bind by what they measure, "Umsatz" to `umsatz_teur` and
    /// "Abwanderung" to `abwanderung_pct`.
    #[test]
    fn a_german_series_binds_to_its_column_with_its_units_words_aside() {
        let mut doc: Value = serde_json::from_str(include_str!("../../../../docs/examples/revenue.deck.json")).unwrap();
        doc["data"]["umsatz"] = json!({ "source": "data/umsatz.csv", "schema": {
            "monat": "string", "umsatz_teur": "number", "neukunden": "number", "abwanderung_pct": "number"
        } });
        let deck = Deck::from_json(&doc.to_string()).unwrap();
        let files = BTreeMap::from([("data/umsatz.csv".to_string(), UMSATZ.as_bytes().to_vec())]);
        for (series, values, column) in [
            ("Umsatz", [412.0, 398.0, 455.0, 471.0, 502.0, 538.0], "umsatz_teur"),
            ("Abwanderung", [2.1, 2.4, 2.0, 1.9, 1.7, 1.5], "abwanderung_pct"),
        ] {
            let mut node = json!({
                "type": "chart", "chart": "line", "data": "@own",
                "x": { "field": "category", "type": "ordinal" },
                "y": { "field": "value", "type": "quantitative" }
            });
            let chart = ChartData {
                categories: ["Januar", "Februar", "M\u{e4}rz", "April", "Mai", "Juni"].map(String::from).to_vec(),
                series: vec![Series { name: Some(series.into()), values: values.iter().map(|v| Some(*v)).collect() }],
                unit: None,
                source_id: Some("umsatz".into()),
            };
            let bound = bind(&mut node, &chart, &deck, &files, "umsatz");
            assert!(bound.is_ok(), "{series}: {bound:?}");
            assert_eq!(node["y"]["field"], json!(column), "{series}");
        }
        assert_eq!(measured("umsatz_teur"), "umsatz");
        assert_eq!(measured("Gebühr je Tonne"), "gebuehr");
        assert_eq!(measured("gebuehr_eur_t"), "gebuehr");
        // Folded both ways: a file that keeps its umlauts and a slide that writes them out.
        assert_eq!(measured("gebühr_eur_t"), measured("Gebuehr je Tonne"));
        assert_eq!(measured("Straße"), measured("strasse"));
        assert_eq!(measured("Restmüll (t)"), "restmuell");
        assert_eq!(measured("Umsatz (Mio. EUR)"), "umsatz");
    }

    const RESULTATS: &str = "quartier,toitures_m2,baisse_temperature_c,part_surface_pct\nConfluence,184500,2.4,11.2\nPart-Dieu,97300,1.6,6.8\nCroix-Rousse,61200,1.1,4.3\nGerland,142800,1.9,8.9\nVaise,38400,0.7,3.7\n";

    /// W1's dogfood 19: a French paper's results, its columns written without accents, its
    /// slide's series with them and with the word that joins them: "Baisse de température" was
    /// drawn from the slide's own rows, unbound, beside a file that held them.
    #[test]
    fn a_french_series_binds_to_its_column_its_accents_and_joining_words_aside() {
        let mut doc: Value = serde_json::from_str(include_str!("../../../../docs/examples/revenue.deck.json")).unwrap();
        doc["data"]["resultats"] = json!({ "source": "data/resultats.csv", "schema": {
            "quartier": "string", "toitures_m2": "number", "baisse_temperature_c": "number", "part_surface_pct": "number"
        } });
        let deck = Deck::from_json(&doc.to_string()).unwrap();
        let files = BTreeMap::from([("data/resultats.csv".to_string(), RESULTATS.as_bytes().to_vec())]);
        for (series, values, column) in [
            ("Baisse de température", [2.4, 1.6, 1.1, 1.9, 0.7], "baisse_temperature_c"),
            ("Part de la surface (%)", [11.2, 6.8, 4.3, 8.9, 3.7], "part_surface_pct"),
        ] {
            let mut node = json!({
                "type": "chart", "chart": "bar", "data": "@own",
                "x": { "field": "category", "type": "ordinal" },
                "y": { "field": "value", "type": "quantitative" }
            });
            let chart = ChartData {
                categories: ["Confluence", "Part-Dieu", "Croix-Rousse", "Gerland", "Vaise"].map(String::from).to_vec(),
                series: vec![Series { name: Some(series.into()), values: values.iter().map(|v| Some(*v)).collect() }],
                unit: None,
                source_id: Some("resultats".into()),
            };
            let bound = bind(&mut node, &chart, &deck, &files, "resultats");
            assert!(bound.is_ok(), "{series}: {bound:?}");
            assert_eq!(node["y"]["field"], json!(column), "{series}");
        }
        assert_eq!(measured("Taux de chômage"), measured("taux_chomage"));
        assert_eq!(measured("Œuvres reçues"), measured("oeuvres_recues"));
        // A word that joins nothing is a word of the name: "De" alone, "Part de" at its end.
        assert_eq!(measured("Part de"), "partde");
        assert_eq!(measured("Baisse de température (°C)"), "baissetemperature");
    }

    /// W1 dogfood 13: a German file writes its umlauts out and its unit per tonne as `_t`, and a
    /// slide's "Gebühr je Tonne" binds to `gebuehr_eur_t`, never left on its own rows.
    #[test]
    fn a_german_series_per_tonne_binds_to_its_column_written_without_umlauts() {
        let mut doc: Value = serde_json::from_str(include_str!("../../../../docs/examples/revenue.deck.json")).unwrap();
        doc["data"]["abfall"] = json!({ "source": "data/abfall.csv", "schema": {
            "jahr": "number", "restmuell_t": "number", "gebuehr_eur_t": "number"
        } });
        let deck = Deck::from_json(&doc.to_string()).unwrap();
        let csv = "jahr,restmuell_t,gebuehr_eur_t\n2021,182000,48\n2022,186500,52\n2023,189000,57\n2024,191500,63\n2025,194000,71\n";
        let files = BTreeMap::from([("data/abfall.csv".to_string(), csv.as_bytes().to_vec())]);
        for (series, values, column) in [
            ("Gebühr je Tonne", [48.0, 52.0, 57.0, 63.0, 71.0], "gebuehr_eur_t"),
            ("Restmüll", [182000.0, 186500.0, 189000.0, 191500.0, 194000.0], "restmuell_t"),
        ] {
            let (mut node, mut chart) = by_year("line", series, &values);
            chart.source_id = Some("abfall".into());
            let bound = bind(&mut node, &chart, &deck, &files, "abfall");
            assert!(bound.is_ok(), "{series}: {bound:?}");
            assert_eq!(node["y"]["field"], json!(column), "{series}");
        }
    }

    /// W1's dogfood 11: a Brazilian sheet's columns carry the real and its magnitude in their
    /// names ("Receita (R$ mi)"), and a series binds by what it measures, "Receita" to it, where
    /// the sheet has two number columns and so a name must choose. Spanish's and Italian's
    /// accounts write theirs alike ("Ventas (millones)", "Ricavi (mln)").
    #[test]
    fn a_portuguese_series_binds_to_its_column_with_its_units_words_aside() {
        let mut doc: Value = serde_json::from_str(include_str!("../../../../docs/examples/revenue.deck.json")).unwrap();
        doc["data"]["padaria-mensal"] = json!({ "source": "data/padaria-mensal.csv", "schema": {
            "M\u{ea}s": "string", "Receita (R$ mi)": "number", "Pedidos": "number"
        } });
        let deck = Deck::from_json(&doc.to_string()).unwrap();
        let csv = "M\u{ea}s,Receita (R$ mi),Pedidos\njan.,1.2,8400\nfev.,1.1,7900\nmar.,1.5,9800\n";
        let files = BTreeMap::from([("data/padaria-mensal.csv".to_string(), csv.as_bytes().to_vec())]);
        for (series, values, column) in
            [("Receita", [1.2, 1.1, 1.5], "Receita (R$ mi)"), ("Pedidos", [8400.0, 7900.0, 9800.0], "Pedidos")]
        {
            let mut node = json!({
                "type": "chart", "chart": "bar", "data": "@own",
                "x": { "field": "category", "type": "ordinal" },
                "y": { "field": "value", "type": "quantitative" }
            });
            let chart = ChartData {
                categories: ["jan.", "fev.", "mar."].map(String::from).to_vec(),
                series: vec![Series { name: Some(series.into()), values: values.iter().map(|v| Some(*v)).collect() }],
                unit: None,
                source_id: Some("padaria-mensal".into()),
            };
            let bound = bind(&mut node, &chart, &deck, &files, "padaria-mensal");
            assert!(bound.is_ok(), "{series}: {bound:?}");
            assert_eq!(node["y"]["field"], json!(column), "{series}");
        }
        assert_eq!(measured("Receita (R$ mi)"), "receita");
        assert_eq!(measured("Faturamento em US$ bi"), "faturamentoem");
        assert_eq!(measured("Ventas (millones)"), "ventas");
        assert_eq!(measured("Ricavi (mln)"), "ricavi");
        // A word that only begins with a unit's letters stays whole: "Milhas" counts miles.
        assert_eq!(measured("Milhas"), "milhas");
    }
}
