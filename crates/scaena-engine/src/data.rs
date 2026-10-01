//! Data sources (SPEC §3.10): CSV or JSON files in the bundle, or inline rows, typed
//! by the deck's `schema`. The engine never reads files: the caller hands it the
//! bundle's data files as bytes ([`DataFiles`]), the way it hands over fonts.

use crate::EngineError;
use scaena_core::Deck;
use serde_json::Value;
use std::collections::BTreeMap;

/// A bundle's data files by bundle path (`data/q3.csv`), as the deck's sources name them.
#[derive(Debug, Clone, Default)]
pub struct DataFiles(BTreeMap<String, Vec<u8>>);

impl DataFiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: impl Into<String>, bytes: Vec<u8>) {
        self.0.insert(path.into(), bytes);
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.0.get(path).map(Vec::as_slice)
    }
}

/// One cell, typed by the source's schema.
#[derive(Debug, Clone, PartialEq)]
pub enum Datum {
    Number(f64),
    Text(String),
    Bool(bool),
    Null,
}

impl Datum {
    /// How a category or key reads: text as is, numbers in Rust's shortest form.
    pub fn label(&self) -> String {
        match self {
            Datum::Number(n) => format_number(*n),
            Datum::Text(s) => s.clone(),
            Datum::Bool(b) => b.to_string(),
            Datum::Null => String::new(),
        }
    }
}

/// A number with no format string (SPEC §3.7 `format` is PLAN 1.9): integers without
/// a decimal point, everything else in Rust's shortest round-trip form. Both are the
/// same on every platform.
pub fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

/// How many decimal places [`format_number`] shows for `n`.
pub fn decimals(n: f64) -> usize {
    let s = format_number(n);
    s.find('.').map_or(0, |dot| s.len() - dot - 1)
}

/// `n` rounded to `places` decimals, as a counting label shows it: an integer when
/// `places` is 0, and no sign on a value that rounds to zero. Fixed-precision
/// formatting is correctly rounded, so it reads the same on every platform.
pub fn format_fixed(n: f64, places: usize) -> String {
    let s = if places == 0 { format!("{}", n.round() as i64) } else { format!("{n:.places$}") };
    match s.strip_prefix('-') {
        Some(zero) if zero.bytes().all(|b| b == b'0' || b == b'.') => zero.to_string(),
        _ => s,
    }
}

/// Rows of a source, columns in source order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Datum>>,
}

impl Table {
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }
}

/// Load the deck's data source `name` (a chart's `"@name"` without the `@`).
pub fn load(deck: &Deck, files: &DataFiles, name: &str) -> Result<Table, EngineError> {
    let source = deck.data.get(name).ok_or_else(|| EngineError::Data(format!("unknown data source `@{name}`")))?;
    let schema = source.schema.clone().unwrap_or_default();
    if source.parse.is_some() {
        return Err(EngineError::NotImplemented("data parse options — PLAN 1.9"));
    }
    let typed = |column: &str, raw: Value| -> Result<Datum, EngineError> {
        let bad = |what: &str| EngineError::Data(format!("`@{name}`: column `{column}`: {what}"));
        Ok(match (schema.get(column).map(String::as_str), raw) {
            (_, Value::Null) => Datum::Null,
            (Some("number"), Value::Number(n)) => Datum::Number(n.as_f64().ok_or_else(|| bad("not a number"))?),
            (Some("number"), Value::String(s)) if s.trim().is_empty() => Datum::Null,
            (Some("number"), Value::String(s)) => {
                Datum::Number(s.trim().parse().map_err(|_| bad(&format!("`{s}` is not a number")))?)
            }
            (Some("boolean"), Value::Bool(b)) => Datum::Bool(b),
            (Some("boolean"), Value::String(s)) => match s.trim() {
                "true" => Datum::Bool(true),
                "false" => Datum::Bool(false),
                other => return Err(bad(&format!("`{other}` is not true or false"))),
            },
            (Some("date"), _) => return Err(EngineError::NotImplemented("date columns — PLAN 1.9")),
            (Some("string") | None, Value::String(s)) => Datum::Text(s),
            (Some("string") | None, other) => Datum::Text(other.to_string()),
            (Some(kind), other) => return Err(bad(&format!("`{other}` does not fit schema type `{kind}`"))),
        })
    };
    let records: Records = match &source.source {
        Value::String(path) => {
            let bytes = files.get(path).ok_or_else(|| {
                EngineError::Data(format!("`@{name}`: `{path}` was not handed to the engine (DataFiles)"))
            })?;
            let text = std::str::from_utf8(bytes).map_err(|_| EngineError::Data(format!("`{path}` is not UTF-8")))?;
            if path.ends_with(".csv") {
                csv(text).map_err(|e| EngineError::Data(format!("`{path}`: {e}")))?
            } else if path.ends_with(".json") {
                let rows: Value =
                    serde_json::from_str(text).map_err(|e| EngineError::Data(format!("`{path}`: {e}")))?;
                json_rows(&rows).map_err(|e| EngineError::Data(format!("`{path}`: {e}")))?
            } else {
                return Err(EngineError::Data(format!("`{path}`: expected a .csv or .json file")));
            }
        }
        Value::Object(o) if o.contains_key("inline") => {
            json_rows(&o["inline"]).map_err(|e| EngineError::Data(format!("`@{name}` inline: {e}")))?
        }
        other => return Err(EngineError::Data(format!("`@{name}`: unsupported source {other}"))),
    };
    let columns = records.first().map(|(c, _)| c.clone()).unwrap_or_default();
    let mut rows = Vec::with_capacity(records.len());
    for (header, values) in records {
        let row = header.iter().zip(values).map(|(c, v)| typed(c, v)).collect::<Result<Vec<_>, _>>()?;
        rows.push(row);
    }
    Ok(Table { columns, rows })
}

/// Rows as (column names, values), each in source order.
type Records = Vec<(Vec<String>, Vec<Value>)>;

/// RFC 4180: a header row, then records; fields may be quoted, with `""` for a quote.
fn csv(text: &str) -> Result<Records, String> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let (mut record, mut field) = (Vec::new(), String::new());
    let (mut quoted, mut chars) = (false, text.trim_start_matches('\u{FEFF}').chars().peekable());
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => record.push(std::mem::take(&mut field)),
            (false, '\r') if chars.peek() == Some(&'\n') => {}
            (false, '\n' | '\r') => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            (false, c) => field.push(c),
        }
    }
    if quoted {
        return Err("unterminated quoted field".into());
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    let mut records = records.into_iter();
    let header = records.next().ok_or("no header row")?;
    records
        .enumerate()
        .map(|(i, r)| {
            if r.len() != header.len() {
                return Err(format!("row {}: {} fields, header has {}", i + 2, r.len(), header.len()));
            }
            Ok((header.clone(), r.into_iter().map(Value::String).collect()))
        })
        .collect()
}

/// An array of objects; each row keeps its own key order.
fn json_rows(rows: &Value) -> Result<Records, String> {
    let rows = rows.as_array().ok_or("expected an array of objects")?;
    rows.iter()
        .map(|r| {
            let o = r.as_object().ok_or("expected an array of objects")?;
            Ok((o.keys().cloned().collect(), o.values().cloned().collect()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(data: &str) -> Deck {
        let json = format!(
            r#"{{"scaena": "0.1", "canvas": {{"width": 1920, "height": 1080}}, "data": {data}, "nodes": {{}}, "states": []}}"#
        );
        Deck::from_json(&json).unwrap()
    }

    #[test]
    fn csv_rows_are_typed_by_the_schema() {
        let deck = deck(r#"{"q": {"source": "data/q.csv", "schema": {"quarter": "string", "rev": "number"}}}"#);
        let mut files = DataFiles::new();
        files.insert("data/q.csv", b"quarter,rev,note\r\nQ1,1.5,\"a, \"\"b\"\"\"\nQ2,,plain\n".to_vec());
        let t = load(&deck, &files, "q").unwrap();
        assert_eq!(t.columns, ["quarter", "rev", "note"]);
        assert_eq!(t.rows[0], [Datum::Text("Q1".into()), Datum::Number(1.5), Datum::Text("a, \"b\"".into())]);
        assert_eq!(t.rows[1][1], Datum::Null);
    }

    #[test]
    fn inline_rows_and_bad_values_say_where() {
        let deck = deck(r#"{"q": {"source": {"inline": [{"k": "a", "v": 2}]}, "schema": {"v": "number"}}}"#);
        let t = load(&deck, &DataFiles::new(), "q").unwrap();
        assert_eq!(t.rows, [vec![Datum::Text("a".into()), Datum::Number(2.0)]]);
        let deck = deck_with_bad();
        let mut files = DataFiles::new();
        files.insert("d.csv", b"v\nseven\n".to_vec());
        let err = load(&deck, &files, "q").unwrap_err().to_string();
        assert!(err.contains("`seven` is not a number"), "{err}");
        assert!(load(&deck, &DataFiles::new(), "q").unwrap_err().to_string().contains("was not handed"));
    }

    fn deck_with_bad() -> Deck {
        deck(r#"{"q": {"source": "d.csv", "schema": {"v": "number"}}}"#)
    }

    #[test]
    fn numbers_read_the_same_everywhere() {
        assert_eq!(format_number(24.0), "24");
        assert_eq!(format_number(-3.0), "-3");
        assert_eq!(format_number(1.25), "1.25");
        assert_eq!(format_number(0.1 + 0.2), "0.30000000000000004");
        assert_eq!((decimals(24.0), decimals(1.25), decimals(-0.5)), (0, 2, 1));
        assert_eq!(
            (format_fixed(18.5, 0), format_fixed(-0.4, 0), format_fixed(1.005, 1)),
            ("19".into(), "0".into(), "1.0".into())
        );
        assert_eq!((format_fixed(-0.04, 1), format_fixed(-0.06, 1)), ("0.0".into(), "-0.1".into()));
    }
}
