//! A made deck's figures tied to the data they came from (G1, ADR-0019): where a figure a slide
//! writes is the value of exactly one cell of a sheet the deck keeps from its sources, written as
//! a format writes it, it becomes a run that quotes that cell, so a data edit or a refreshed source
//! sets it again and W427 finds one a file changed behind the deck's back.
//!
//! A wrong quote is worse than none, so only an unambiguous match is quoted:
//!
//! - the cell's value is the figure as written, exactly, no rounding hidden, and the quote's
//!   format writes it as written;
//! - no other cell of the kept sheets holds that value;
//! - the cell's column shares a word with the words around the figure (its line, or the label of
//!   the card it heads), so a value that happens to be one cell's is not taken for it;
//! - no unit conflicts: a column of percents quotes no figure in dollars, and one of dollars no
//!   percent.
//!
//! The words stay as they were, byte for byte: the quoted run's text is the figure as the model
//! wrote it, which the quote's own value is checked to give.

use scaena_core::Deck;
use scaena_core::data::{self, ColumnType, Datum, SourceFiles, Table};
use scaena_core::format::{Locale, NumberFormat};
use scaena_core::model::values::{Quote, QuoteKey, QuoteRow};
use serde_json::{Map, Value, json};

/// A cell a figure may quote: its source's id, its row as a quote names it, its column, the
/// words its column's name is made of, and its value.
struct Cell<'t> {
    source: &'t str,
    row: QuoteRow,
    column: &'t str,
    words: Vec<String>,
    value: f64,
}

/// Words of a column's name that say its unit, not what it measures: they name nothing.
const UNITS: &[&str] = &["pct", "percent", "usd", "musd", "kusd", "busd", "eur", "gbp", "k", "m", "mm", "bn", "num"];

/// A figure found in a text: where it stands (bytes), what it says, and how it is written.
#[derive(Debug, Clone, PartialEq)]
struct Figure {
    start: usize,
    end: usize,
    /// The number as written, without grouping: `48.2`.
    number: f64,
    decimals: usize,
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
    /// Whether the span says enough to stand on its value: a mark, decimals, or three
    /// significant digits that are not a year; else its column must name the word after it, or
    /// the label of the card it is alone in.
    telling: bool,
}

/// Whether `text` at byte `at` starts with a digit.
fn digit_at(text: &str, at: usize) -> bool {
    text.as_bytes().get(at).is_some_and(u8::is_ascii_digit)
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
        let (mut group, mut point, mut decimals) = (false, false, 0);
        while i < bytes.len() {
            let rest = &text[i..];
            if bytes[i].is_ascii_digit() {
                decimals += usize::from(point);
                i += 1;
            } else if !point && rest.starts_with(locale.group) && digit_at(text, i + locale.group.len()) {
                group = true;
                i += locale.group.len();
            } else if !point && rest.starts_with(locale.decimal) && digit_at(text, i + locale.decimal.len()) {
                point = true;
                i += locale.decimal.len();
            } else {
                break;
            }
        }
        let digits_end = i;
        let plain = text[digits_start..digits_end].replace(locale.group, "").replace(locale.decimal, ".");
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
        if text[end..].chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        let digits = (digits_start, digits_end);
        out.push(Figure { start, end, number, decimals, significant, group, currency, percent, tier, digits });
    }
    out
}

/// Whether `f` reads as a year (1900 to 2100, four digits, no marks).
fn year(f: &Figure) -> bool {
    f.decimals == 0 && !f.group && f.significant <= 4 && (1900.0..=2100.0).contains(&f.number)
}

/// The ways `f` may be written by a format: the whole figure, its marks with it, and the number
/// alone, its marks left as words, for a column that holds the number as written (`48.2` of
/// "$48.2M" in `arr_musd`): a format writes no literal unit.
fn readings(f: &Figure) -> Vec<Reading> {
    let group = if f.group { "," } else { "" };
    let currency = if f.currency { "$" } else { "" };
    let whole = (f.start, f.end);
    let mut out = Vec::new();
    if f.percent {
        let format = format!("{currency}{group}.{}%", f.decimals);
        out.push(Reading { span: whole, format, value: f.number / 100.0, telling: true });
    } else if let Some(tier) = f.tier {
        for trim in ["~", ""] {
            let format = format!("{currency}{group}.{}{trim}k", f.significant.max(1));
            out.push(Reading { span: whole, format, value: f.number * 1000f64.powi(tier), telling: true });
        }
    } else if f.currency {
        let format = format!("{currency}{group}.{}f", f.decimals);
        out.push(Reading { span: whole, format, value: f.number, telling: true });
    }
    let marked = f.currency || f.percent || f.tier.is_some();
    let telling = f.decimals > 0 || marked || (f.significant >= 3 && !year(f));
    out.push(Reading { span: f.digits, format: format!("{group}.{}f", f.decimals), value: f.number, telling });
    out
}

/// The words of `text`, lowercase, each plural's singular beside it.
fn words_of(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        let w = w.to_lowercase();
        if let Some(one) = w.strip_suffix('s').filter(|s| s.len() > 2) {
            out.push(one.to_string());
        }
        out.push(w);
    }
    out
}

/// Whether `context`, the words around a figure, names what `cell`'s column measures: a word of
/// its name not a unit's, or its initials (`nrr` for "net revenue retention"). A word written in
/// kana or Han is found where it stands; a Latin one only as a word.
fn names(cell: &Cell, context: &str) -> bool {
    let lower = context.to_lowercase();
    let latin = words_of(&lower);
    let words: Vec<&str> = lower.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let said = |w: &String| if w.is_ascii() { latin.contains(w) } else { lower.contains(w.as_str()) };
    let initials = |w: &String| {
        (2..=5).contains(&w.len())
            && w.chars().all(|c| c.is_ascii_alphabetic())
            && words.windows(w.len()).any(|run| run.iter().zip(w.chars()).all(|(l, c)| l.starts_with(c)))
    };
    cell.words.iter().filter(|w| !UNITS.contains(&w.as_str())).any(|w| said(w) || initials(w))
}

/// Whether a figure so written may be `cell`'s: a column of percents quotes no amount in a
/// currency or with a compact suffix, and one of a currency no percent.
fn fits(cell: &Cell, f: &Figure) -> bool {
    let has = |units: &[&str]| cell.words.iter().any(|w| units.contains(&w.as_str()));
    let percents = has(&["pct", "percent", "rate"]);
    let money = has(&["usd", "musd", "kusd", "busd", "eur", "gbp", "dollar"]);
    !(percents && (f.currency || f.tier.is_some()) || money && f.percent)
}

/// The cells of number columns in `tables` (each a kept source's id and its table), each with its
/// row named by the first column of words whose values tell its rows apart, else by its index.
fn cells(tables: &[(String, Table)]) -> Vec<Cell<'_>> {
    let mut out = Vec::new();
    for (source, table) in tables {
        let key = (0..table.columns.len()).find(|&c| {
            let labels: Vec<String> = table.rows.iter().map(|r| r[c].label()).collect();
            table.types[c] == ColumnType::String
                && labels.iter().all(|l| !l.trim().is_empty())
                && labels.iter().enumerate().all(|(i, l)| !labels[..i].contains(l))
        });
        for (r, row) in table.rows.iter().enumerate() {
            let named = match key {
                Some(k) => {
                    let keys = [(table.columns[k].clone(), QuoteKey::Text(row[k].label()))];
                    QuoteRow::Keys(keys.into_iter().collect())
                }
                None => QuoteRow::Index(r as i64),
            };
            for (c, datum) in row.iter().enumerate() {
                let Datum::Number(value) = datum else { continue };
                if Some(c) != key && value.is_finite() {
                    let column = table.columns[c].as_str();
                    out.push(Cell { source, row: named.clone(), column, words: words_of(column), value: *value });
                }
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

/// The line of `text` that holds byte `at`: a list's item, a paragraph.
fn line_of(text: &str, at: usize) -> &str {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    &text[start..end]
}

/// Where the figures of a text are read: the deck, its files, its locale, and the kept cells.
struct Reader<'a> {
    deck: &'a Deck,
    files: &'a dyn SourceFiles,
    locale: &'a Locale,
    cells: &'a [Cell<'a>],
}

impl Reader<'_> {
    /// The cell `f` in `text` quotes, and the span of `text` its run takes, `beside` the words of
    /// the card it is in; none where no cell's value is the figure, more than one is, the words
    /// around it do not name what the cell measures, or the quote would not write it as written.
    fn quote_of(&self, text: &str, f: &Figure, beside: &str) -> Option<((usize, usize), Quote)> {
        let mut found: Vec<((usize, usize), &Cell, String, bool)> = Vec::new();
        for reading in readings(f) {
            let span = &text[reading.span.0..reading.span.1];
            let Ok(format) = NumberFormat::parse(&reading.format) else { continue };
            for cell in self.cells {
                if same(cell.value, reading.value) && format.format(cell.value, self.locale) == span {
                    found.push((reading.span, cell, reading.format.clone(), reading.telling));
                }
            }
        }
        // One cell holds the value, however many ways it reads: else no quote at all.
        let first = found.first()?;
        let one = |c: &Cell| c.source == first.1.source && c.column == first.1.column && c.row == first.1.row;
        if !found.iter().all(|(_, c, _, _)| one(c)) {
            return None;
        }
        let (span, cell, format, telling) = found.iter().rev().max_by_key(|(s, ..)| s.1 - s.0)?;
        // The words around it name what the cell measures: its line, or the card it heads where
        // it stands alone. A figure that says little by itself needs the word right after it.
        let line = line_of(text, f.start);
        let alone = line.trim() == &text[f.start..f.end];
        let near = match (alone, telling) {
            (true, _) => beside.to_string(),
            (false, true) => line.to_string(),
            (false, false) => text[span.1..].split_whitespace().next().unwrap_or_default().to_string(),
        };
        if !names(cell, &near) || !fits(cell, f) {
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
        let gives = scaena_core::quotes::value(self.deck, self.files, &quote).ok()?;
        (gives == text[span.0..span.1]).then_some((*span, quote))
    }

    /// What a text's `runs` become, `beside` the words of the card it is in: each figure that
    /// quotes a cell its own run with its quote; `None` where nothing in it quotes.
    fn quoted(&self, runs: &[Value], beside: &str) -> Option<Vec<Value>> {
        let mut out = Vec::new();
        let mut count = 0;
        for run in runs {
            let text = run.get("text").and_then(Value::as_str);
            // A run that links or quotes already is the deck's own.
            let (Some(text), None, None) = (text, run.get("quote"), run.get("link")) else {
                out.push(run.clone());
                continue;
            };
            let piece = |words: &str| {
                let mut r = run.clone();
                r["text"] = json!(words);
                r
            };
            let mut at = 0;
            for f in figures(text, self.locale) {
                let Some((span, quote)) = self.quote_of(text, &f, beside) else { continue };
                if span.0 < at {
                    continue;
                }
                if span.0 > at {
                    out.push(piece(&text[at..span.0]));
                }
                let mut q = piece(&text[span.0..span.1]);
                q["quote"] = serde_json::to_value(&quote).expect("a quote is JSON");
                out.push(q);
                count += 1;
                at = span.1;
            }
            if at == 0 {
                out.push(run.clone());
            } else if at < text.len() {
                out.push(piece(&text[at..]));
            }
        }
        // Never a word changed: the runs say what they said.
        let said = |rs: &[Value]| rs.iter().filter_map(|r| r.get("text").and_then(Value::as_str)).collect::<String>();
        (count > 0 && said(&out) == said(runs)).then_some(out)
    }
}

/// The words a text's props say: its `text`, or its runs' texts in turn.
fn said(props: &Value) -> String {
    match (props.get("text").and_then(Value::as_str), props.get("runs").and_then(Value::as_array)) {
        (Some(text), _) => text.to_string(),
        (None, Some(runs)) => runs.iter().filter_map(|r| r["text"].as_str()).collect(),
        _ => String::new(),
    }
}

/// The card a node is in on its slide: its id less the part it plays (`first-title` and `first`
/// are one card's figure and label).
fn card(id: &str) -> &str {
    const PARTS: &[&str] = &["-title", "-figure", "-label", "-value", "-number", "-caption", "-name"];
    PARTS.iter().find_map(|p| id.strip_suffix(p)).unwrap_or(id)
}

/// The words beside node `id` in `state`: the rest of its card's texts.
fn beside(doc: &Value, state: &Value, id: &str, texts: &[String]) -> String {
    let props = state["props"].as_object();
    (props.into_iter().flatten())
        .filter(|(other, _)| other.as_str() != id && texts.contains(other) && card(other) == card(id))
        .map(
            |(other, p)| {
                if p.get("text").is_some() || p.get("runs").is_some() { said(p) } else { said(&doc["nodes"][other]) }
            },
        )
        .collect::<Vec<_>>()
        .join("\n")
}

/// A text's props as runs: none for code, or for props that say no words.
fn runs_of(props: &Value) -> Option<Vec<Value>> {
    if props.get("code").is_some() {
        return None;
    }
    match (props.get("text").and_then(Value::as_str), props.get("runs").and_then(Value::as_array)) {
        (Some(text), _) => Some(vec![json!({ "text": text })]),
        (None, Some(runs)) => Some(runs.clone()),
        _ => None,
    }
}

/// `doc`, a deck as JSON, with each figure its texts write that is the value of one cell of the
/// sources named `sources` (ids of its `data`) a run that quotes it, `files` the bundle's: how
/// many were quoted, by source.
pub fn quoting(doc: &mut Value, files: &dyn SourceFiles, sources: &[String]) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = sources.iter().map(|s| (s.clone(), 0)).collect();
    let Ok(deck) = Deck::from_json(&doc.to_string()) else { return counts };
    let tables: Vec<(String, Table)> =
        sources.iter().filter_map(|id| data::load(&deck, files, id).ok().map(|t| (id.clone(), t))).collect();
    let cells = cells(&tables);
    if cells.is_empty() {
        return counts;
    }
    let locale = Locale::of(deck.meta.as_ref().and_then(|m| m.lang.as_deref()));
    let reader = Reader { deck: &deck, files, locale, cells: &cells };
    let texts: Vec<String> = (doc["nodes"].as_object().into_iter().flatten())
        .filter(|(_, n)| n["type"] == "text" && n.get("code").is_none())
        .map(|(id, _)| id.clone())
        .collect();
    let states = doc["states"].as_array().cloned().unwrap_or_default();
    let mut changes: Vec<(Option<usize>, String, Vec<Value>)> = Vec::new();
    for id in &texts {
        // A text's own words, beside its card's in each state that shows it.
        let shown = states.iter().filter(|s| s["props"].get(id).is_some());
        let cards = shown.map(|s| beside(doc, s, id, &texts)).collect::<Vec<_>>().join("\n");
        if let Some(runs) = runs_of(&doc["nodes"][id])
            && let Some(quoted) = reader.quoted(&runs, &cards)
        {
            changes.push((None, id.clone(), quoted));
        }
    }
    for (k, state) in states.iter().enumerate() {
        for id in &texts {
            if let Some(runs) = state["props"].get(id).and_then(runs_of)
                && let Some(quoted) = reader.quoted(&runs, &beside(doc, state, id, &texts))
            {
                changes.push((Some(k), id.clone(), quoted));
            }
        }
    }
    for (state, id, runs) in changes {
        for r in &runs {
            let source = r.get("quote").and_then(|q| q["data"].as_str()).and_then(|d| d.strip_prefix('@'));
            if let Some(c) = counts.iter_mut().find(|(s, _)| Some(s.as_str()) == source) {
                c.1 += 1;
            }
        }
        let props = match state {
            None => doc["nodes"][&id].as_object_mut(),
            Some(k) => doc["states"][k]["props"][&id].as_object_mut(),
        };
        if let Some(props) = props {
            set_runs(props, runs);
        }
    }
    counts
}

/// `props` saying its words as `runs`.
fn set_runs(props: &mut Map<String, Value>, runs: Vec<Value>) {
    props.remove("text");
    props.insert("runs".into(), Value::Array(runs));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A deck of the texts `words` (by id, all on one slide), with `csv` kept as the source `kpi`.
    fn deck(words: &[(&str, &str)], csv: &str, lang: Option<&str>) -> (Value, BTreeMap<String, Vec<u8>>) {
        let nodes: Map<String, Value> = words
            .iter()
            .map(|(id, t)| (id.to_string(), json!({ "type": "text", "role": "body", "text": t })))
            .collect();
        let props: Map<String, Value> = words.iter().map(|(id, _)| (id.to_string(), json!({}))).collect();
        // Typed as `data_attach` types a sheet: its first column words, the rest numbers.
        let header = csv.lines().next().unwrap().split(',');
        let schema: Map<String, Value> =
            header.enumerate().map(|(i, c)| (c.to_string(), json!(if i == 0 { "string" } else { "number" }))).collect();
        let mut doc = json!({
            "scaena": "0.38",
            "canvas": { "width": 1920, "height": 1080 },
            "theme": "theme.json",
            "data": { "kpi": { "source": "data/kpi.csv", "schema": schema } },
            "nodes": nodes,
            "states": [{ "id": "s", "props": props }]
        });
        if let Some(lang) = lang {
            doc["meta"] = json!({ "lang": lang });
        }
        (doc, BTreeMap::from([("data/kpi.csv".to_string(), csv.as_bytes().to_vec())]))
    }

    /// What each text becomes over `csv`: its quotes, each its words and its quote.
    fn quotes(words: &[(&str, &str)], csv: &str, lang: Option<&str>) -> Vec<(String, Value)> {
        let (mut doc, files) = deck(words, csv, lang);
        quoting(&mut doc, &files, &["kpi".into()]);
        let mut out = Vec::new();
        for (id, was) in words {
            let node = &doc["nodes"][id];
            assert_eq!(said(node), *was, "the words stay as written");
            for r in node["runs"].as_array().into_iter().flatten().filter(|r| r.get("quote").is_some()) {
                out.push((r["text"].as_str().unwrap().to_string(), r["quote"].clone()));
            }
        }
        out
    }

    /// The pass-6 QBR's sheet (`dogfood/sources/northwind-q3.csv`), written-scale numbers.
    const NORTHWIND: &str = "quarter,arr_musd,net_new_arr_enterprise_musd,net_new_arr_midmarket_musd,\
net_new_arr_smb_musd,churn_smb_pct,churn_midmarket_pct,nrr_pct,gross_margin_pct,new_logos,new_logos_plan,fcf_musd,\
logo_churn_pct
Q3 FY25,33.9,1.9,0.8,0.4,3.1,1.4,109,74.9,58,60,0.6,2.0
Q4 FY25,36.8,2.0,0.6,0.3,3.3,1.3,110,75.6,63,62,0.9,2.0
Q1 FY26,40.1,2.2,1.2,0.3,3.6,1.2,111,76.3,66,64,1.2,2.1
Q2 FY26,44.2,2.6,1.2,0.3,4.0,1.2,112,76.3,64,66,1.4,2.1
Q3 FY26,48.2,2.7,1.1,0.2,4.9,1.1,114,78.4,61,70,1.9,2.8
";

    fn one(words: &str) -> Vec<(String, Value)> {
        quotes(&[("t", words)], NORTHWIND, None)
    }

    #[test]
    fn a_figure_in_its_columns_scale_quotes_the_number_and_leaves_its_marks_as_words() {
        let q = quotes(&[("first-title", "$48.2M"), ("first", "ARR, up 9% on Q2")], NORTHWIND, None);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].0, "48.2", "the $ and M stay words");
        let quote = json!({ "data": "@kpi", "row": { "quarter": "Q3 FY26" }, "column": "arr_musd", "format": ".1f" });
        assert_eq!(q[0].1, quote);
        let q = quotes(&[("third-title", "78.4%"), ("third", "Gross margin, up 2.1 points")], NORTHWIND, None);
        assert_eq!((q[0].0.as_str(), q[0].1["column"].as_str()), ("78.4", Some("gross_margin_pct")));
        let q = quotes(&[("fourth-title", "61"), ("fourth", "New logos, 9 short of plan")], NORTHWIND, None);
        assert_eq!((q[0].0.as_str(), q[0].1["column"].as_str()), ("61", Some("new_logos")));
        let q = quotes(&[("sixth-title", "2.8%"), ("sixth", "Logo churn, up from 2.1%")], NORTHWIND, None);
        assert_eq!(q.len(), 1, "2.1 is four cells' value: {q:?}");
        assert_eq!(q[0].1["column"], json!("logo_churn_pct"));
        let q = one("Enterprise added $2.7M of the $4.0M net new ARR");
        assert_eq!(q.len(), 1);
        assert_eq!((q[0].0.as_str(), q[0].1["column"].as_str()), ("2.7", Some("net_new_arr_enterprise_musd")));
        let q = one("SMB churn climbed to 4.9% while mid-market churn fell");
        assert_eq!((q[0].0.as_str(), q[0].1["column"].as_str()), ("4.9", Some("churn_smb_pct")));
        let q = quotes(&[("second-title", "114%"), ("second", "Net revenue retention")], NORTHWIND, None);
        assert_eq!(q[0].1["column"], json!("nrr_pct"), "a column named by its initials");
    }

    #[test]
    fn a_value_one_cell_happens_to_hold_stays_words_where_nothing_around_it_names_the_cell() {
        // $4.0M is a sum no cell holds; 4.0 is Q2 FY26's SMB churn, the only 4.0 in the sheet.
        let q = one("Enterprise added $2.7M of the $4.0M net new ARR");
        assert!(q.iter().all(|(w, _)| w != "4.0"), "{q:?}");
        // 0.4 is Q3 FY25's SMB net new ARR, the only 0.4.
        assert!(one("Cons: margin dilution of 0.4 points").is_empty());
        assert!(one("Margin: −0.4 pts").is_empty());
    }

    #[test]
    fn a_value_several_cells_hold_stays_words() {
        // 1.1 is Q3 FY26's mid-market net new ARR and its mid-market churn; 1.2 four cells'.
        assert!(one("Mid-market was steady at $1.1M").is_empty());
        assert!(one("$1.2M of ARR at risk from downgrades").is_empty());
        // 9% holds no cell.
        assert!(one("Up 9% QoQ").is_empty());
    }

    #[test]
    fn a_column_of_percents_quotes_no_amount_and_one_of_dollars_no_percent() {
        let csv = "quarter,churn_smb_pct,net_new_arr_smb_musd\nQ3,4.9,0.2\n";
        assert!(quotes(&[("t", "SMB churn cost $4.9M")], csv, None).is_empty());
        assert!(quotes(&[("t", "SMB net new ARR fell 0.2%")], csv, None).is_empty());
        assert_eq!(quotes(&[("t", "SMB churn was 4.9%")], csv, None).len(), 1);
    }

    #[test]
    fn a_figure_written_in_the_cells_own_scale_quotes_it_whole() {
        let csv = "quarter,arr,nrr\nQ2,44200000,1.12\nQ3,48200000,1.14\n";
        let q = quotes(&[("t", "ARR reached $48.2M; net revenue retention 114%")], csv, None);
        assert_eq!((q[0].0.as_str(), q[0].1["format"].as_str()), ("$48.2M", Some("$.3~k")));
        assert_eq!((q[1].0.as_str(), q[1].1["format"].as_str()), ("114%", Some(".0%")));
        let q = quotes(&[("t", "Market size: $1.1T")], "segment,size\nGlobal,1100000000000\n", None);
        assert_eq!(q[0].0, "$1.1T");
    }

    #[test]
    fn a_unit_word_or_a_cjk_counter_stays_words() {
        let days = "team,lead_time_days\nOps,11\nSales,4\n";
        let q = quotes(&[("t", "Lead time is down to 11 days.")], days, None);
        assert_eq!(q[0].0, "11");
        assert!(quotes(&[("t", "11 reasons to switch")], days, None).is_empty());
        let csv = "region,売上\nTokyo,41.2\nOsaka,18.6\n";
        let q = quotes(&[("t", "売上は41.2万ドルに達した")], csv, Some("ja"));
        assert_eq!((q[0].0.as_str(), q[0].1["format"].as_str()), ("41.2", Some(".1f")));
    }

    #[test]
    fn a_name_a_year_or_a_rounded_figure_stays_words() {
        assert!(one("Q3 FY26 closed in 2026").is_empty());
        let q = quotes(&[("t", "A 41% share of the market")], "quarter,share\nQ3,0.4123\n", None);
        assert!(q.is_empty(), "41% hides the rounding of 41.23%");
    }
}
