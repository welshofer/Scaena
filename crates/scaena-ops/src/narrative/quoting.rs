//! A made deck's figures tied to the data they came from (G1, ADR-0019): where a figure a slide
//! writes is the value of exactly one cell of a sheet the deck keeps from its sources, written as
//! a format writes it, it becomes a run that quotes that cell, so a data edit or a refreshed source
//! sets it again and W427 finds one a file changed behind the deck's back.
//!
//! Only an unambiguous match is quoted: one cell of all the kept sheets, whose value is the
//! written figure exactly, with no rounding the figure hides. The words stay as they were, byte
//! for byte: the quoted run's text is the figure as the model wrote it, which the quote's own
//! value is checked to give.

use scaena_core::Deck;
use scaena_core::data::{self, ColumnType, Datum, SourceFiles, Table};
use scaena_core::format::{Locale, NumberFormat};
use scaena_core::model::values::{Quote, QuoteKey, QuoteRow};
use serde_json::{Map, Value, json};

/// A cell a figure may quote: its source's id, its row as a quote names it, its column, and the
/// words its column's name is made of.
struct Cell<'t> {
    source: &'t str,
    row: QuoteRow,
    column: &'t str,
    words: Vec<String>,
    value: f64,
}

/// Whether `context`, the words a slide shows beside a figure, names what `column` measures: at
/// least half of the words of its name ("free cash flow" for `free_cash_flow`), or its initials
/// ("net revenue retention" for `nrr`). A word written in kana or Han is found where it stands;
/// a Latin one only as a word.
fn names(column: &str, context: &str) -> bool {
    let context = context.to_lowercase();
    let latin: Vec<&str> = context.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let parts: Vec<String> =
        column.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect();
    let said = |w: &str| {
        if !w.is_ascii() {
            return context.contains(w);
        }
        let one = w.strip_suffix('s').unwrap_or(w);
        latin.iter().any(|l| *l == w || *l == one || l.strip_suffix('s') == Some(one))
    };
    let initials = |w: &str| {
        (2..=5).contains(&w.len())
            && w.chars().all(|c| c.is_ascii_alphabetic())
            && latin.windows(w.len()).any(|run| run.iter().zip(w.chars()).all(|(l, c)| l.starts_with(c)))
    };
    let hits = parts.iter().filter(|w| said(w)).count();
    !parts.is_empty() && (hits * 2 >= parts.len() || parts.iter().any(|w| initials(w)))
}

/// A figure found in a text: where it stands (bytes), what it says, and how it is written.
#[derive(Debug, Clone, PartialEq)]
struct Figure {
    start: usize,
    end: usize,
    /// The number as written, without grouping: `48.2`.
    number: f64,
    /// Decimals written.
    decimals: usize,
    /// Significant digits written.
    significant: usize,
    group: bool,
    currency: bool,
    percent: bool,
    /// A compact suffix's power of a thousand (K 1, M 2, B 3, T 4).
    tier: Option<i32>,
    /// Where the number itself stands, without a currency sign or a suffix.
    digits: (usize, usize),
}

/// One way a figure may be a cell's value: the span of the text it quotes, the format that writes
/// the cell as that span, and the value the span says.
struct Reading {
    span: (usize, usize),
    format: String,
    value: f64,
    /// Whether the span alone says enough to be trusted on its value: a marker, decimals, or
    /// three significant digits; else a unit word after it must be one its column names.
    telling: bool,
}

/// The figures of `text` a sheet's cell may give, in order.
fn figures(text: &str, locale: &Locale) -> Vec<Figure> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        // A figure starts at a word's start: not inside "Q3" or "FY26". Words written in kana or
        // Han stand against their figures with no space, so only a Latin letter or a digit joins.
        let before = text[..i].chars().next_back();
        if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_') {
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.' || bytes[i] == b',') {
                i += 1;
            }
            continue;
        }
        let (prefix, currency) = match locale.currency.0 {
            sign if !sign.is_empty() && text[..i].ends_with(sign) => (sign.len(), true),
            _ => (0, false),
        };
        let start = i - prefix;
        let digits_start = i;
        let mut group = false;
        let mut seen_point = false;
        let mut decimals = 0;
        while i < bytes.len() {
            let c = &text[i..];
            if bytes[i].is_ascii_digit() {
                if seen_point {
                    decimals += 1;
                }
                i += 1;
            } else if !seen_point
                && c.starts_with(locale.group)
                && c[locale.group.len()..].as_bytes().first().is_some_and(u8::is_ascii_digit)
            {
                group = true;
                i += locale.group.len();
            } else if !seen_point
                && c.starts_with(locale.decimal)
                && c[locale.decimal.len()..].as_bytes().first().is_some_and(u8::is_ascii_digit)
            {
                seen_point = true;
                i += locale.decimal.len();
            } else {
                break;
            }
        }
        let digits_end = i;
        let plain: String = text[digits_start..digits_end].replace(locale.group, "").replace(locale.decimal, ".");
        let Ok(number) = plain.parse::<f64>() else { continue };
        let significant = plain.trim_start_matches(['0', '.']).chars().filter(char::is_ascii_digit).count();
        let rest = &text[i..];
        let (suffix, percent, tier) = if rest.starts_with(locale.percent) {
            (locale.percent.len(), true, None)
        } else if let Some((k, s)) = (locale.compact.iter().enumerate()).find(|(_, s)| rest.starts_with(**s)) {
            (s.len(), false, Some(k as i32 + 1))
        } else {
            (0, false, None)
        };
        let end = i + suffix;
        // A figure ends at a word's end; a CJK counter (万, 件) may follow it.
        let after = text[end..].chars().next();
        if after.is_some_and(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        out.push(Figure {
            start,
            end,
            number,
            decimals,
            significant,
            group,
            currency,
            percent,
            tier,
            digits: (digits_start, digits_end),
        });
    }
    out
}

/// The ways `f` may be written by a format: the whole figure, its marks with it, and the
/// number alone, its marks left as words, for a column that holds the number as written.
fn readings(f: &Figure) -> Vec<Reading> {
    let group = if f.group { "," } else { "" };
    let currency = if f.currency { "$" } else { "" };
    let marked = f.currency || f.percent || f.tier.is_some();
    let bare_telling = f.decimals > 0 || (f.significant >= 3 && !year(f));
    let mut out = Vec::new();
    if f.percent {
        out.push(Reading {
            span: (f.start, f.end),
            format: format!("{currency}{group}.{}%", f.decimals),
            value: f.number / 100.0,
            telling: true,
        });
    } else if let Some(tier) = f.tier {
        let scale = 1000f64.powi(tier);
        for trim in ["~", ""] {
            out.push(Reading {
                span: (f.start, f.end),
                format: format!("{currency}{group}.{}{trim}k", f.significant.max(1)),
                value: f.number * scale,
                telling: true,
            });
        }
    } else if f.currency {
        out.push(Reading {
            span: (f.start, f.end),
            format: format!("{currency}{group}.{}f", f.decimals),
            value: f.number,
            telling: true,
        });
    }
    out.push(Reading {
        span: f.digits,
        format: format!("{group}.{}f", f.decimals),
        value: f.number,
        telling: bare_telling || (marked && f.decimals > 0),
    });
    out
}

/// Whether `f` reads as a year (1900 to 2100, four digits, no marks): a date, which a cell of
/// numbers is not taken to give unless its column names the word after it.
fn year(f: &Figure) -> bool {
    f.decimals == 0 && !f.group && f.significant <= 4 && (1900.0..=2100.0).contains(&f.number)
}

/// The words a column's name is made of, lowercase, a plural's singular beside it.
fn words_of(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        let w = w.to_lowercase();
        if let Some(one) = w.strip_suffix('s').filter(|s| s.len() > 2) {
            out.push(one.to_string());
        }
        out.push(w);
    }
    out
}

/// The word that follows `end` in `text`, past spaces and a colon, lowercase, its plural's
/// singular too.
fn word_after(text: &str, end: usize) -> Vec<String> {
    let rest = text[end..].trim_start_matches([' ', '\u{a0}', ':']);
    let word: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
    if word.is_empty() {
        return Vec::new();
    }
    words_of(&word)
}

/// The cells of number columns in `tables` (each a kept source's id and its table), each with its
/// row named by the first column of words whose values tell its rows apart, else by its index.
fn cells<'t>(tables: &'t [(String, Table)]) -> Vec<Cell<'t>> {
    let mut out = Vec::new();
    for (source, table) in tables {
        let key = (0..table.columns.len()).find(|&c| {
            table.types[c] == ColumnType::String && {
                let labels: Vec<String> = table.rows.iter().map(|r| r[c].label()).collect();
                labels.iter().all(|l| !l.trim().is_empty())
                    && labels.iter().enumerate().all(|(i, l)| !labels[..i].contains(l))
            }
        });
        for (r, row) in table.rows.iter().enumerate() {
            let named = match key {
                Some(k) => {
                    QuoteRow::Keys([(table.columns[k].clone(), QuoteKey::Text(row[k].label()))].into_iter().collect())
                }
                None => QuoteRow::Index(r as i64),
            };
            for (c, datum) in row.iter().enumerate() {
                let Datum::Number(value) = datum else { continue };
                if Some(c) == key || !value.is_finite() {
                    continue;
                }
                out.push(Cell {
                    source,
                    row: named.clone(),
                    column: &table.columns[c],
                    words: words_of(&table.columns[c]),
                    value: *value,
                });
            }
        }
    }
    out
}

/// Whether `a` and `b` are one value, as a figure written and a cell read may be: equal, or
/// apart by what binary fractions leave (`0.41` against `41 / 100`).
fn same(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
}

/// What a text node's words become: the runs, each figure that quotes a cell its own run with its
/// quote. `runs` are the node's own; `None` where nothing in it quotes.
fn quoted_runs(
    runs: &[Value],
    context: &str,
    cells: &[Cell],
    locale: &Locale,
    deck: &Deck,
    files: &dyn SourceFiles,
) -> Option<(Vec<Value>, usize)> {
    let mut out = Vec::new();
    let mut count = 0;
    for run in runs {
        let Some(text) = run.get("text").and_then(Value::as_str) else {
            out.push(run.clone());
            continue;
        };
        // A run that links or quotes already is the deck's own.
        if run.get("quote").is_some() || run.get("link").is_some() {
            out.push(run.clone());
            continue;
        }
        let mut at = 0;
        let mut pieces: Vec<Value> = Vec::new();
        let piece = |words: &str| {
            let mut r = run.clone();
            r["text"] = json!(words);
            r
        };
        for f in figures(text, locale) {
            let Some((span, quote)) = quote_of(text, context, &f, cells, locale, deck, files) else { continue };
            if span.0 < at {
                continue;
            }
            if span.0 > at {
                pieces.push(piece(&text[at..span.0]));
            }
            let mut q = piece(&text[span.0..span.1]);
            q["quote"] = serde_json::to_value(&quote).expect("a quote is JSON");
            pieces.push(q);
            count += 1;
            at = span.1;
        }
        if at == 0 {
            out.push(run.clone());
            continue;
        }
        if at < text.len() {
            pieces.push(piece(&text[at..]));
        }
        out.extend(pieces);
    }
    // Never a word changed: the runs say what they said.
    let said = |rs: &[Value]| rs.iter().filter_map(|r| r.get("text").and_then(Value::as_str)).collect::<String>();
    (count > 0 && said(&out) == said(runs)).then_some((out, count))
}

/// The cell `f` in `text` quotes, and the span of `text` the quote's run takes; none where no
/// cell's value is the figure, or more than one is, or the slide's words beside it (`context`) do
/// not name what the cell's column measures, or the quote would not write it as written.
#[allow(clippy::too_many_arguments)]
fn quote_of(
    text: &str,
    context: &str,
    f: &Figure,
    cells: &[Cell],
    locale: &Locale,
    deck: &Deck,
    files: &dyn SourceFiles,
) -> Option<((usize, usize), Quote)> {
    let mut found: Vec<((usize, usize), &Cell, String)> = Vec::new();
    for reading in readings(f) {
        let span = &text[reading.span.0..reading.span.1];
        let after = word_after(text, reading.span.1);
        let Ok(format) = NumberFormat::parse(&reading.format) else { continue };
        for cell in cells {
            if !same(cell.value, reading.value) || format.format(cell.value, locale) != span {
                continue;
            }
            // A figure that says little by itself is a cell's only where its column names the
            // word after it: "61 new logos", "11 days".
            if !reading.telling && !after.iter().any(|w| cell.words.contains(w)) {
                continue;
            }
            found.push((reading.span, cell, reading.format.clone()));
        }
    }
    // One cell, however many ways it reads: the widest span, its marks with it.
    let first = found.first()?;
    let one = |c: &Cell| c.source == first.1.source && c.column == first.1.column && c.row == first.1.row;
    if !found.iter().all(|(_, c, _)| one(c)) {
        return None;
    }
    let (span, cell, format) = found.iter().rev().max_by_key(|(s, _, _)| s.1 - s.0)?;
    // And a value is the cell's only where the slide says what the cell measures: $1.2M at risk
    // is not a quarter's free cash flow of $1.2M.
    if !names(cell.column, context) {
        return None;
    }
    let quote = Quote {
        data: format!("@{}", cell.source),
        data_transform: None,
        row: Some(cell.row.clone()),
        column: cell.column.to_string(),
        format: Some(format.clone()),
    };
    // The quote, read as every patch reads it, writes the figure as written.
    let gives = scaena_core::quotes::value(deck, files, &quote).ok()?;
    (gives == text[span.0..span.1]).then_some((*span, quote))
}

/// `doc`, a deck as JSON, with each figure its texts write that is the value of one cell of the
/// sources named `sources` (ids of its `data`) a run that quotes it, `files` the bundle's: how
/// many were quoted, by source.
pub fn quoting(doc: &mut Value, files: &dyn SourceFiles, sources: &[String]) -> Vec<(String, usize)> {
    let Ok(deck) = Deck::from_json(&doc.to_string()) else { return Vec::new() };
    let tables: Vec<(String, Table)> =
        sources.iter().filter_map(|id| data::load(&deck, files, id).ok().map(|t| (id.clone(), t))).collect();
    let cells = cells(&tables);
    let mut counts: Vec<(String, usize)> = sources.iter().map(|s| (s.clone(), 0)).collect();
    if cells.is_empty() {
        return counts;
    }
    let locale = Locale::of(deck.meta.as_ref().and_then(|m| m.lang.as_deref()));
    let texts: Vec<String> = (doc["nodes"].as_object().into_iter().flatten())
        .filter(|(_, n)| n["type"] == "text" && n.get("code").is_none())
        .map(|(id, _)| id.clone())
        .collect();
    // The words each state shows, and each text's: those of the states that show it.
    let said = |props: &Value| -> String {
        match (props.get("text").and_then(Value::as_str), props.get("runs").and_then(Value::as_array)) {
            (Some(text), _) => text.to_string(),
            (None, Some(runs)) => runs.iter().filter_map(|r| r["text"].as_str()).collect(),
            _ => String::new(),
        }
    };
    let shown: Vec<String> =
        (doc["states"].as_array().into_iter().flatten())
            .map(|state| {
                let props = state["props"].as_object();
                (props.into_iter().flatten())
                    .filter(|(id, _)| texts.contains(id))
                    .map(|(id, p)| {
                        if p.get("text").is_some() || p.get("runs").is_some() {
                            said(p)
                        } else {
                            said(&doc["nodes"][id])
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect();
    let context_of = |id: &str| -> String {
        let states: Vec<&String> = (doc["states"].as_array().into_iter().flatten())
            .zip(&shown)
            .filter(|(state, _)| state["props"].get(id).is_some())
            .map(|(_, words)| words)
            .collect();
        match states.is_empty() {
            true => said(&doc["nodes"][id]),
            false => states.into_iter().map(String::as_str).collect::<Vec<_>>().join("\n"),
        }
    };
    let contexts: Vec<String> = texts.iter().map(|id| context_of(id)).collect();
    let mut tally = |runs: &[Value]| {
        for r in runs {
            if let Some(q) = r.get("quote").and_then(|q| q["data"].as_str()) {
                if let Some(c) = counts.iter_mut().find(|(s, _)| q.strip_prefix('@') == Some(s.as_str())) {
                    c.1 += 1;
                }
            }
        }
    };
    let mut quote_in = |props: &mut Map<String, Value>, context: &str| {
        if props.contains_key("code") {
            return;
        }
        let runs = match (props.get("text").and_then(Value::as_str), props.get("runs").and_then(Value::as_array)) {
            (Some(text), _) => vec![json!({ "text": text })],
            (None, Some(runs)) => runs.clone(),
            _ => return,
        };
        if let Some((quoted, _)) = quoted_runs(&runs, context, &cells, locale, &deck, files) {
            tally(&quoted);
            props.remove("text");
            props.insert("runs".into(), Value::Array(quoted));
        }
    };
    for (id, context) in texts.iter().zip(&contexts) {
        if let Some(props) = doc["nodes"][id].as_object_mut() {
            quote_in(props, context);
        }
    }
    for (state, context) in doc["states"].as_array_mut().into_iter().flatten().zip(&shown) {
        for id in &texts {
            if let Some(props) = state["props"].get_mut(id).and_then(Value::as_object_mut) {
                quote_in(props, context);
            }
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A deck of one text that says `words`, with `csv` kept as the source `kpi`, in `lang`.
    fn deck(words: &str, csv: &str, schema: Value, lang: Option<&str>) -> (Value, BTreeMap<String, Vec<u8>>) {
        let mut doc = json!({
            "scaena": "0.38",
            "canvas": { "width": 1920, "height": 1080 },
            "theme": "theme.json",
            "data": { "kpi": { "source": "data/kpi.csv", "schema": schema } },
            "nodes": { "t": { "type": "text", "role": "body", "text": words } },
            "states": [{ "id": "s", "props": { "t": {} } }]
        });
        if let Some(lang) = lang {
            doc["meta"] = json!({ "lang": lang });
        }
        let files = BTreeMap::from([("data/kpi.csv".to_string(), csv.as_bytes().to_vec())]);
        (doc, files)
    }

    /// What `words` become over `csv`: the runs, and how many quote.
    fn quote(words: &str, csv: &str, schema: Value, lang: Option<&str>) -> (Value, usize) {
        let (mut doc, files) = deck(words, csv, schema, lang);
        let counts = quoting(&mut doc, &files, &["kpi".into()]);
        let said: String = match &doc["nodes"]["t"]["runs"] {
            Value::Array(runs) => runs.iter().map(|r| r["text"].as_str().unwrap()).collect(),
            _ => doc["nodes"]["t"]["text"].as_str().unwrap().to_string(),
        };
        assert_eq!(said, words, "the words stay as written");
        (doc["nodes"]["t"].clone(), counts[0].1)
    }

    fn quotes(node: &Value) -> Vec<(String, Value)> {
        node["runs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r.get("quote").is_some())
            .map(|r| (r["text"].as_str().unwrap().to_string(), r["quote"].clone()))
            .collect()
    }

    const QUARTERS: &str = "quarter,arr,nrr,margin,new_logos\nQ2,44200000,1.12,0.763,58\nQ3,48200000,1.14,0.784,61\n";

    fn schema() -> Value {
        json!({ "quarter": "string", "arr": "number", "nrr": "number", "margin": "number", "new_logos": "number" })
    }

    #[test]
    fn a_percent_quotes_its_fraction() {
        let (node, n) = quote("Net revenue retention is 114% and gross margin 78.4%.", QUARTERS, schema(), None);
        assert_eq!(n, 2);
        let q = quotes(&node);
        assert_eq!(q[0].0, "114%");
        assert_eq!(q[0].1, json!({ "data": "@kpi", "row": { "quarter": "Q3" }, "column": "nrr", "format": ".0%" }));
        assert_eq!(
            (q[1].0.as_str(), q[1].1["column"].as_str(), q[1].1["format"].as_str()),
            ("78.4%", Some("margin"), Some(".1%"))
        );
    }

    #[test]
    fn a_magnitude_and_a_currency_quote_the_whole_figure() {
        let (node, _) = quote("ARR reached $48.2M, from $44.2M.", QUARTERS, schema(), None);
        let q = quotes(&node);
        assert_eq!(q.len(), 2);
        assert_eq!((q[0].0.as_str(), q[0].1["format"].as_str()), ("$48.2M", Some("$.3~k")));
        assert_eq!(q[1].1["row"], json!({ "quarter": "Q2" }));
        let (node, _) = quote(
            "Market size: $1.1T",
            "segment,size\nGlobal,1100000000000\n",
            json!({ "segment": "string", "size": "number" }),
            None,
        );
        assert_eq!(quotes(&node)[0].0, "$1.1T");
    }

    #[test]
    fn a_small_number_quotes_only_where_its_column_names_its_unit() {
        let (node, n) = quote("61 new logos this quarter", QUARTERS, schema(), None);
        assert_eq!((n, quotes(&node)[0].1["column"].as_str()), (1, Some("new_logos")));
        let (_, n) = quote("61 reasons to smile", QUARTERS, schema(), None);
        assert_eq!(n, 0, "a column that does not name the word says nothing of it");
        let days = "team,lead_time_days\nOps,11\nSales,4\n";
        let (node, n) =
            quote("Lead time is down to 11 days.", days, json!({ "team": "string", "lead_time_days": "number" }), None);
        assert_eq!((n, quotes(&node)[0].0.as_str()), (1, "11"));
        assert_eq!(node["runs"][2]["text"], json!(" days."), "the unit stays words");
    }

    #[test]
    fn a_cjk_counter_leaves_its_counter_as_words() {
        let csv = "region,売上\nTokyo,41.2\nOsaka,18.6\n";
        let (node, n) =
            quote("売上は41.2万ドルに達した", csv, json!({ "region": "string", "売上": "number" }), Some("ja"));
        assert_eq!(n, 1);
        let q = quotes(&node);
        assert_eq!((q[0].0.as_str(), q[0].1["format"].as_str()), ("41.2", Some(".1f")));
        assert_eq!(node["runs"][2]["text"], json!("万ドルに達した"));
    }

    #[test]
    fn an_ambiguous_or_rounded_figure_stays_words() {
        // 2.1% is two cells' value: left as written.
        let csv = "quarter,smb,enterprise\nQ2,0.021,0.009\nQ3,0.049,0.021\n";
        let (_, n) = quote(
            "Churn was 2.1% in enterprise.",
            csv,
            json!({ "quarter": "string", "smb": "number", "enterprise": "number" }),
            None,
        );
        assert_eq!(n, 0);
        // 41% of 0.4123 needs a rounding the figure hides.
        let csv = "quarter,share\nQ3,0.4123\n";
        let (_, n) = quote("A 41% share", csv, json!({ "quarter": "string", "share": "number" }), None);
        assert_eq!(n, 0);
        // $1.2M is one cell's value, but the slide does not say what that cell measures.
        let csv = "quarter,free_cash_flow\nQ1,1200000\nQ2,1500000\n";
        let (_, n) = quote(
            "A cheaper tier puts $1.2M at risk",
            csv,
            json!({ "quarter": "string", "free_cash_flow": "number" }),
            None,
        );
        assert_eq!(n, 0, "a value alone is no quote");
        // Q3 and FY26 are names, and 2026 a year no column names.
        let csv = "quarter,year\nQ3,2026\n";
        let (_, n) = quote("Q3 FY26 closed in 2026", csv, json!({ "quarter": "string", "year": "number" }), None);
        assert_eq!(n, 0);
    }
}
