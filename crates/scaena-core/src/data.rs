//! Data sources (SPEC §3.10): CSV or JSON files in the bundle, or inline rows, typed by
//! the source's `schema` and read with its `parse` formats. Core reads no files: the
//! caller hands over the bytes ([`SourceFiles`]), as it hands the engine fonts.
//! Validation reads sources here for E103, and the engine for charts and tables.

use crate::Deck;
use crate::format::{self, DateFormat, DateTime, Locale};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::BTreeMap;

/// The bundle's files, by bundle path (`data/q3.csv`), as far as data needs them.
pub trait SourceFiles {
    fn bytes(&self, path: &str) -> Option<Cow<'_, [u8]>>;
}

impl SourceFiles for BTreeMap<String, Vec<u8>> {
    fn bytes(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        self.get(path).map(|b| Cow::Borrowed(b.as_slice()))
    }
}

/// One cell, typed by the source's schema.
#[derive(Debug, Clone, PartialEq)]
pub enum Datum {
    Number(f64),
    Text(String),
    Bool(bool),
    Date(DateTime),
    Null,
}

impl Datum {
    /// How a category or key reads with no format: text as is, numbers in d3's default
    /// form, dates in ISO 8601 (with the time only when it is not midnight).
    pub fn label(&self) -> String {
        match self {
            Datum::Number(n) => format::format_number(*n, Locale::of(None)),
            Datum::Text(s) => s.clone(),
            Datum::Bool(b) => b.to_string(),
            Datum::Date(t) => {
                let spec = if t.0.rem_euclid(86_400) == 0 { "%Y-%m-%d" } else { "%Y-%m-%dT%H:%M:%S" };
                DateFormat::parse(spec).expect("ISO formats parse").format(*t, Locale::of(None))
            }
            Datum::Null => String::new(),
        }
    }
}

/// What a column holds, from the source's `schema` (`string` when it says nothing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Number,
    String,
    Boolean,
    Date,
}

impl ColumnType {
    fn of(schema: Option<&str>) -> Result<ColumnType, String> {
        Ok(match schema {
            None | Some("string") => ColumnType::String,
            Some("number") => ColumnType::Number,
            Some("boolean") => ColumnType::Boolean,
            Some("date") => ColumnType::Date,
            Some(other) => return Err(format!("schema type `{other}`: expected number, string, date, or boolean")),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            ColumnType::Number => "number",
            ColumnType::String => "string",
            ColumnType::Boolean => "boolean",
            ColumnType::Date => "date",
        }
    }
}

/// Rows of a source, columns in source order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub columns: Vec<String>,
    pub types: Vec<ColumnType>,
    pub rows: Vec<Vec<Datum>>,
}

impl Table {
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }
}

/// Why a source did not load.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DataError {
    #[error("unknown data source `@{0}`")]
    Unknown(String),
    #[error("`@{name}`: `{path}` was not handed over")]
    Missing { name: String, path: String },
    #[error("{0}")]
    Bad(String),
}

/// The deck's data source `name` (a chart's `"@name"` without the `@`), typed.
pub fn load(deck: &Deck, files: &dyn SourceFiles, name: &str) -> Result<Table, DataError> {
    let source = deck.data.get(name).ok_or_else(|| DataError::Unknown(name.to_string()))?;
    let at = |what: String| DataError::Bad(format!("`@{name}`: {what}"));
    let schema = source.schema.clone().unwrap_or_default();
    let locale = Locale::of(deck.meta.as_ref().and_then(|m| m.lang.as_deref()));
    // Each date column's reader: its `parse` format, else ISO 8601.
    let mut parse: BTreeMap<&str, DateFormat> = BTreeMap::new();
    for (column, spec) in source.parse.iter().flatten() {
        if schema.get(column).map(String::as_str) != Some("date") {
            return Err(at(format!("`parse` names `{column}`, which the schema does not type as a date")));
        }
        let f = DateFormat::parse(spec).map_err(|e| at(format!("`parse.{column}`: {e}")))?;
        parse.insert(column, f);
    }
    let typed = |column: &str, kind: ColumnType, raw: Value| -> Result<Datum, DataError> {
        let bad = |what: &str| at(format!("column `{column}`: {what}"));
        Ok(match (kind, raw) {
            (_, Value::Null) => Datum::Null,
            (ColumnType::Number, Value::Number(n)) => Datum::Number(n.as_f64().ok_or_else(|| bad("not a number"))?),
            (ColumnType::Number | ColumnType::Date, Value::String(s)) if s.trim().is_empty() => Datum::Null,
            (ColumnType::Number, Value::String(s)) => {
                Datum::Number(s.trim().parse().map_err(|_| bad(&format!("`{s}` is not a number")))?)
            }
            (ColumnType::Boolean, Value::Bool(b)) => Datum::Bool(b),
            (ColumnType::Boolean, Value::String(s)) => match s.trim() {
                "true" => Datum::Bool(true),
                "false" => Datum::Bool(false),
                other => return Err(bad(&format!("`{other}` is not true or false"))),
            },
            (ColumnType::Date, Value::String(s)) => Datum::Date(match parse.get(column) {
                Some(f) => f.read(&s, locale).map_err(|e| bad(&e.to_string()))?,
                None => format::read_iso(&s).map_err(|e| bad(&format!("{e}; give the column a `parse` format")))?,
            }),
            (ColumnType::String, Value::String(s)) => Datum::Text(s),
            (ColumnType::String, other) => Datum::Text(other.to_string()),
            (kind, other) => return Err(bad(&format!("`{other}` does not fit schema type `{}`", kind.name()))),
        })
    };
    let records: Records = match &source.source {
        Value::String(path) => {
            let bytes =
                files.bytes(path).ok_or_else(|| DataError::Missing { name: name.to_string(), path: path.clone() })?;
            let text = std::str::from_utf8(&bytes).map_err(|_| at(format!("`{path}` is not UTF-8")))?;
            if path.ends_with(".csv") {
                csv(text).map_err(|e| at(format!("`{path}`: {e}")))?
            } else if path.ends_with(".json") {
                let rows: Value = serde_json::from_str(text).map_err(|e| at(format!("`{path}`: {e}")))?;
                json_rows(&rows).map_err(|e| at(format!("`{path}`: {e}")))?
            } else {
                return Err(at(format!("`{path}`: expected a .csv or .json file")));
            }
        }
        Value::Object(o) if o.contains_key("inline") => {
            json_rows(&o["inline"]).map_err(|e| at(format!("inline: {e}")))?
        }
        other => return Err(at(format!("unsupported source {other}"))),
    };
    let columns = records.first().map(|(c, _)| c.clone()).unwrap_or_else(|| schema.keys().cloned().collect());
    let types = columns
        .iter()
        .map(|c| ColumnType::of(schema.get(c).map(String::as_str)).map_err(|e| at(format!("`{c}`: {e}"))))
        .collect::<Result<Vec<_>, _>>()?;
    let mut rows = Vec::with_capacity(records.len());
    for (header, values) in records {
        let row = header
            .iter()
            .zip(values)
            .map(|(c, v)| {
                let kind = ColumnType::of(schema.get(c).map(String::as_str)).map_err(|e| at(format!("`{c}`: {e}")))?;
                typed(c, kind, v)
            })
            .collect::<Result<Vec<_>, _>>()?;
        rows.push(row);
    }
    Ok(Table { columns, types, rows })
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
            r#"{{"scaena": "0.9", "canvas": {{"width": 1920, "height": 1080}}, "data": {data}, "nodes": {{}}, "states": []}}"#
        );
        Deck::from_json(&json).unwrap()
    }

    fn files(path: &str, bytes: &str) -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([(path.to_string(), bytes.as_bytes().to_vec())])
    }

    #[test]
    fn csv_rows_are_typed_by_the_schema() {
        let deck = deck(r#"{"q": {"source": "data/q.csv", "schema": {"quarter": "string", "rev": "number"}}}"#);
        let t =
            load(&deck, &files("data/q.csv", "quarter,rev,note\r\nQ1,1.5,\"a, \"\"b\"\"\"\nQ2,,plain\n"), "q").unwrap();
        assert_eq!(t.columns, ["quarter", "rev", "note"]);
        assert_eq!(t.types, [ColumnType::String, ColumnType::Number, ColumnType::String]);
        assert_eq!(t.rows[0], [Datum::Text("Q1".into()), Datum::Number(1.5), Datum::Text("a, \"b\"".into())]);
        assert_eq!(t.rows[1][1], Datum::Null);
    }

    #[test]
    fn inline_rows_and_bad_values_say_where() {
        let deck1 = deck(r#"{"q": {"source": {"inline": [{"k": "a", "v": 2}]}, "schema": {"v": "number"}}}"#);
        let t = load(&deck1, &BTreeMap::new(), "q").unwrap();
        assert_eq!(t.rows, [vec![Datum::Text("a".into()), Datum::Number(2.0)]]);
        let deck2 = deck(r#"{"q": {"source": "d.csv", "schema": {"v": "number"}}}"#);
        let err = load(&deck2, &files("d.csv", "v\nseven\n"), "q").unwrap_err().to_string();
        assert!(err.contains("`seven` is not a number"), "{err}");
        let missing = load(&deck2, &BTreeMap::new(), "q").unwrap_err();
        assert!(matches!(missing, DataError::Missing { .. }), "{missing}");
    }

    #[test]
    fn dates_read_in_iso_or_with_their_parse_format() {
        let deck1 = deck(
            r#"{"q": {"source": "d.csv", "schema": {"day": "date", "month": "date"}, "parse": {"month": "%b %Y"}}}"#,
        );
        let t = load(&deck1, &files("d.csv", "day,month\n2025-03-05,Mar 2025\n2025-04,Apr 2025\n"), "q").unwrap();
        assert_eq!(t.types, [ColumnType::Date, ColumnType::Date]);
        assert_eq!(t.rows[0][0], Datum::Date(DateTime::ymd(2025, 3, 5).unwrap()));
        assert_eq!(t.rows[1][0], Datum::Date(DateTime::ymd(2025, 4, 1).unwrap()));
        assert_eq!(t.rows[0][1], Datum::Date(DateTime::ymd(2025, 3, 1).unwrap()));
        assert_eq!(t.rows[0][0].label(), "2025-03-05");
        let err = load(&deck1, &files("d.csv", "day,month\nlast Tuesday,Mar 2025\n"), "q").unwrap_err().to_string();
        assert!(err.contains("column `day`") && err.contains("ISO 8601") && err.contains("parse"), "{err}");
        // Month names read in the deck's language.
        let de = r#"{"scaena": "0.9", "meta": {"lang": "de-DE"}, "canvas": {"width": 1920, "height": 1080},
            "data": {"q": {"source": {"inline": [{"m": "März 2025"}]}, "schema": {"m": "date"}, "parse": {"m": "%B %Y"}}},
            "nodes": {}, "states": []}"#;
        let t = load(&Deck::from_json(de).unwrap(), &BTreeMap::new(), "q").unwrap();
        assert_eq!(t.rows[0][0], Datum::Date(DateTime::ymd(2025, 3, 1).unwrap()));
        let deck2 = deck(r#"{"q": {"source": {"inline": []}, "schema": {"v": "number"}, "parse": {"v": "%Y"}}}"#);
        assert!(load(&deck2, &BTreeMap::new(), "q").unwrap_err().to_string().contains("does not type as a date"));
    }
}
