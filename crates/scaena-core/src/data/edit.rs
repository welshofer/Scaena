//! A data source edited in place (PLAN 2.55, ADR-0014): a cell set, a row added, a row taken
//! away, each value checked by its column's type, as a chart reads it (SPEC §3.10).
//!
//! A file is edited byte by byte. A cell rewrites that field's text alone. In a CSV it is quoted
//! where it was or where it has to be. In a JSON array of objects it is the member's value.
//! Every other byte stays: the header, the quoting, the line endings, the layout. A row added
//! copies the row before it: its line ending, its quoting, its layout. A row taken away takes
//! its line, or its element and one comma, with it. Rows written inline are the deck's, and their
//! edit is a JSON Patch of `/data/<name>/source/inline` (SPEC §7.3).

use super::{ColumnType, DateFormat, Datum, Locale, SourceFiles, cell};
use crate::Deck;
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::ops::Range;

/// An edit of a data source's rows. Row 0 is the first after the header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum RowEdit {
    /// `column` of row `row`, set to `value` as typed: a number, `true` or `false`, a date as the
    /// column's `parse` format or ISO 8601 reads it, or any text. Empty, or null, is nothing.
    Set {
        row: usize,
        column: String,
        #[serde(deserialize_with = "typed")]
        #[schemars(schema_with = "typed_schema")]
        value: String,
    },
    /// A row added at `row`, the rows from there on after it; at the end without it. Each column
    /// is its value in `values`, as typed, or nothing.
    Add {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        row: Option<usize>,
        #[serde(default, deserialize_with = "typed_values")]
        #[schemars(schema_with = "typed_values_schema")]
        values: IndexMap<String, String>,
    },
    /// Row `row` taken away.
    Remove { row: usize },
}

/// A value as typed: text, or a JSON number or boolean taken as its text; null as nothing.
fn typed<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    text(Value::deserialize(d)?).map_err(serde::de::Error::custom)
}

/// Each column's value as typed, as [`typed`] takes it.
fn typed_values<'de, D: serde::Deserializer<'de>>(d: D) -> Result<IndexMap<String, String>, D::Error> {
    let values = IndexMap::<String, Value>::deserialize(d)?;
    values.into_iter().map(|(column, v)| Ok((column, text(v).map_err(serde::de::Error::custom)?))).collect()
}

fn text(value: Value) -> Result<String, String> {
    match value {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Null => Ok(String::new()),
        other => Err(format!("a cell is text, a number, true or false, or null, not {other}")),
    }
}

fn typed_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "type": ["string", "number", "boolean", "null"] })
}

fn typed_values_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": { "type": ["string", "number", "boolean", "null"] }
    })
}

/// A source as an editor shows it: its columns, each one's type, and each row's cells as
/// written. Edits name a row by its place here, from 0, and a column by its name.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Sheet {
    pub columns: Vec<SheetColumn>,
    /// Each row's cells, in the columns' order: a value as the source writes it, text without its
    /// quotes, and nothing as empty.
    pub rows: Vec<Vec<String>>,
    /// Each cell its column does not read, and why: what a chart that reads the source refuses
    /// (E103), until an edit sets the cell to what it reads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<CellProblem>,
}

/// A column of a sheet.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct SheetColumn {
    pub name: String,
    /// How the source's `schema` types it (`string` where it says nothing): what a value set in
    /// it must read as.
    #[serde(rename = "type")]
    pub kind: ColumnType,
}

/// A cell of a sheet that its column's type does not read.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct CellProblem {
    pub row: usize,
    pub column: String,
    pub why: String,
}

/// Source `name` as a [`Sheet`]: a file that reads as a table, each cell as written, and those
/// its column does not read; why not, where it does not read as a table.
pub fn sheet(deck: &Deck, files: &dyn SourceFiles, name: &str) -> Result<Sheet, String> {
    let raw = raw(deck, files, name)?;
    let reader = Reader::new(deck, name);
    let mut problems = Vec::new();
    for (r, row) in raw.rows.iter().enumerate() {
        for ((column, &kind), (_, value)) in raw.columns.iter().zip(&raw.types).zip(row) {
            if let Err(why) = reader.read(column, kind, value.clone()) {
                problems.push(CellProblem { row: r, column: column.clone(), why });
            }
        }
    }
    let columns =
        raw.columns.iter().zip(&raw.types).map(|(name, &kind)| SheetColumn { name: name.clone(), kind }).collect();
    let rows = raw.rows.into_iter().map(|row| row.into_iter().map(|(text, _)| text).collect()).collect();
    Ok(Sheet { columns, rows, problems })
}

/// A source's rows as written, none yet read as its column's type.
struct Raw {
    columns: Vec<String>,
    types: Vec<ColumnType>,
    /// Each row's cells, in the columns' order: as written, and as the reader takes it.
    rows: Vec<Vec<(String, Value)>>,
}

/// Source `name`'s rows as written: its columns as the reader names them (SPEC §3.10), a CSV's
/// its header's and JSON's its rows' keys, else the schema's; why not, where its file does not
/// read as a table.
fn raw(deck: &Deck, files: &dyn SourceFiles, name: &str) -> Result<Raw, String> {
    let source = deck.data.get(name).ok_or_else(|| format!("unknown data source `@{name}`"))?;
    let at = |what: String| format!("`@{name}`: {what}");
    let schema = source.schema.clone().unwrap_or_default();
    // Each row's cells by its keys, for JSON: key, as written, as read.
    let mut keyed: Vec<Vec<(String, String, Value)>> = Vec::new();
    let mut header = None;
    let mut csv_rows = Vec::new();
    match &source.source {
        Value::String(path) => {
            let bytes = files.bytes(path).ok_or_else(|| at(format!("`{path}` was not handed over")))?;
            let text = std::str::from_utf8(&bytes).map_err(|_| at(format!("`{path}` is not UTF-8")))?;
            if path.ends_with(".csv") {
                let (names, records) = super::csv_rows(text).map_err(|e| at(format!("`{path}`: {e}")))?;
                for (i, record) in records.iter().enumerate() {
                    if record.len() != names.len() {
                        let n = record.len();
                        return Err(at(format!("`{path}`: row {}: {n} fields, header has {}", i + 2, names.len())));
                    }
                }
                csv_rows = records;
                header = Some(names);
            } else if path.ends_with(".json") {
                let array = array(text).map_err(|e| at(format!("`{path}`: {e}")))?;
                for element in &array.elements {
                    keyed.push(
                        (element.members.iter())
                            .map(|m| {
                                let json = &text[m.value.clone()];
                                (m.key.clone(), written(json), serde_json::from_str(json).unwrap_or(Value::Null))
                            })
                            .collect(),
                    );
                }
            } else {
                return Err(at(format!("`{path}`: expected a .csv or .json file")));
            }
        }
        Value::Object(o) if o.contains_key("inline") => {
            let rows = o["inline"].as_array().ok_or_else(|| at("inline: expected an array of objects".into()))?;
            for row in rows {
                let row = row.as_object().ok_or_else(|| at("inline: expected an array of objects".into()))?;
                keyed.push(row.iter().map(|(k, v)| (k.clone(), written(&v.to_string()), v.clone())).collect());
            }
        }
        other => return Err(at(format!("unsupported source {other}"))),
    }
    let (columns, rows) = match header {
        Some(header) => {
            let rows = csv_rows.into_iter().map(|r| r.into_iter().map(|f| (f.clone(), Value::String(f))).collect());
            (header, rows.collect())
        }
        None => {
            // The first row's keys, then any a later row names first; with no rows, the schema's.
            let mut columns: Vec<String> = Vec::new();
            for (key, ..) in keyed.iter().flatten() {
                if !columns.contains(key) {
                    columns.push(key.clone());
                }
            }
            if keyed.is_empty() {
                columns = schema.keys().cloned().collect();
            }
            let cell = |row: &[(String, String, Value)], column: &str| {
                // A key twice: the reader reads the last.
                (row.iter().rev().find(|(key, ..)| key == column))
                    .map_or((String::new(), Value::Null), |(_, text, value)| (text.clone(), value.clone()))
            };
            let rows = keyed.iter().map(|row| columns.iter().map(|c| cell(row, c)).collect()).collect();
            (columns, rows)
        }
    };
    let types = (columns.iter())
        .map(|c| ColumnType::of(schema.get(c).map(String::as_str)).map_err(|e| at(format!("`{c}`: {e}"))))
        .collect::<Result<_, _>>()?;
    Ok(Raw { columns, types, rows })
}

/// How a source's cells are read (SPEC §3.10): a date by its column's `parse` format, else as
/// ISO 8601, in the deck's locale.
struct Reader<'d> {
    parse: IndexMap<&'d str, Result<DateFormat, String>>,
    locale: &'static Locale,
}

impl<'d> Reader<'d> {
    fn new(deck: &'d Deck, name: &str) -> Reader<'d> {
        let source = &deck.data[name];
        let parse = (source.parse.iter().flatten())
            .map(|(column, spec)| {
                let format = DateFormat::parse(spec).map_err(|e| format!("`parse.{column}`: {e}")).and_then(|f| {
                    if f.misses_year() {
                        return Err(format!("`parse.{column}`: `{spec}` reads no year"));
                    }
                    Ok(f)
                });
                (column.as_str(), format)
            })
            .collect();
        Reader { parse, locale: Locale::of(deck.meta.as_ref().and_then(|m| m.lang.as_deref())) }
    }

    /// `value`, a cell of `column`, read as `kind`, as a chart reads it; why not, where it does
    /// not read, or is a number a chart cannot draw.
    fn read(&self, column: &str, kind: ColumnType, value: Value) -> Result<Datum, String> {
        let parse = match self.parse.get(column) {
            Some(Ok(format)) => Some(format),
            Some(Err(why)) => return Err(why.clone()),
            None => None,
        };
        let shown = match &value {
            Value::String(s) => s.trim().to_string(),
            other => other.to_string(),
        };
        match cell(column, kind, parse, self.locale, value)? {
            Datum::Number(n) if !n.is_finite() => Err(format!("column `{column}`: `{shown}` is not a finite number")),
            datum => Ok(datum),
        }
    }
}

/// A JSON value's text as a cell: a string's characters, nothing for null, and the rest as
/// written.
fn written(json: &str) -> String {
    match serde_json::from_str::<Value>(json) {
        Ok(Value::String(s)) => s,
        Ok(Value::Null) => String::new(),
        _ => json.to_string(),
    }
}

/// What an edit writes.
#[derive(Debug, Clone, PartialEq)]
pub enum Edited {
    /// The file at `path`, as `bytes`.
    File { path: String, bytes: Vec<u8> },
    /// The deck, by these JSON Patch operations: rows written inline are the deck's.
    Inline(Vec<Value>),
}

/// Source `name` with `edit` made: the file it writes, or the patch of rows written inline; why
/// not, where a row or a column is not the source's, or a value does not fit its column. A cell
/// that does not read elsewhere in the source is no reason not to: an edit may be what fixes it.
pub fn edit(deck: &Deck, files: &dyn SourceFiles, name: &str, edit: &RowEdit) -> Result<Edited, String> {
    let raw = raw(deck, files, name)?;
    let reader = Reader::new(deck, name);
    // Each value as its column reads it, or why not.
    let check = |column: &str, value: &str| -> Result<ColumnType, String> {
        let i =
            raw.columns.iter().position(|c| c == column).ok_or_else(|| {
                format!("`@{name}` has no column `{column}`: its columns are {}", raw.columns.join(", "))
            })?;
        let kind = raw.types[i];
        reader.read(column, kind, Value::String(value.to_string())).map_err(|e| format!("`@{name}`: {e}"))?;
        Ok(kind)
    };
    let rows = raw.rows.len();
    let count = if rows == 1 { "1 row".to_string() } else { format!("{rows} rows") };
    match edit {
        RowEdit::Set { row, .. } | RowEdit::Remove { row } if *row >= rows => {
            return Err(format!("`@{name}` has {count}: there is no row {row}"));
        }
        RowEdit::Add { row: Some(row), .. } if *row > rows => {
            return Err(format!("`@{name}` has {count}: a row goes in at 0 to {rows}, not {row}"));
        }
        _ => {}
    }
    let values: Vec<Setting> = match edit {
        RowEdit::Set { column, value, .. } => vec![(column.clone(), value.clone(), check(column, value)?)],
        RowEdit::Add { values, .. } => {
            let mut typed = Vec::with_capacity(values.len());
            for (column, value) in values {
                typed.push((column.clone(), value.clone(), check(column, value)?));
            }
            typed
        }
        RowEdit::Remove { .. } => Vec::new(),
    };
    match &deck.data[name].source {
        Value::String(path) => {
            let bytes = files.bytes(path).ok_or_else(|| format!("`{path}` was not handed over"))?;
            let text = std::str::from_utf8(&bytes).map_err(|_| format!("`{path}` is not UTF-8"))?;
            let edited = if path.ends_with(".csv") {
                csv(text, &raw.columns, edit, &values)?
            } else {
                json_file(text, &raw.columns, edit, &values)?
            };
            Ok(Edited::File { path: path.clone(), bytes: edited.into_bytes() })
        }
        _ => Ok(Edited::Inline(inline(name, &raw.columns, deck, edit, &values)?)),
    }
}

/// Why an edit stops where the file's rows, read again to edit them, are not the rows the reader
/// read: it never should be.
const UNREAD: &str = "the file reads otherwise than its rows were read: edit it in its file";

/// A value an edit sets: its column, the value as typed, and the column's type.
type Setting = (String, String, ColumnType);

/// A CSV value as a field: as typed, a number or a boolean without the space around it, quoted
/// where `quote` says it was or where it has to be.
fn csv_field(value: &str, kind: ColumnType, quote: bool) -> String {
    let value = if matches!(kind, ColumnType::Number | ColumnType::Boolean) { value.trim() } else { value };
    if quote || value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// A JSON value for a column of `kind`, as typed: a number or a boolean as JSON's own, a date or
/// text as a string; nothing as null, but an empty text as an empty string.
fn json_value(value: &str, kind: ColumnType) -> String {
    let trimmed = value.trim();
    match kind {
        ColumnType::String => Value::String(value.to_string()).to_string(),
        _ if trimmed.is_empty() => "null".to_string(),
        // The number as typed where JSON reads it so (`12.50`), else as its value.
        ColumnType::Number if serde_json::from_str::<serde_json::Number>(trimmed).is_ok() => trimmed.to_string(),
        ColumnType::Number => trimmed
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map_or_else(|| "null".to_string(), |n| n.to_string()),
        ColumnType::Boolean => trimmed.to_string(),
        ColumnType::Date => Value::String(value.to_string()).to_string(),
    }
}

/// A CSV record: each field's bytes (quotes and all), where it ends, and where its line ending
/// does. A blank line is a record of one empty field, which the reader passes over.
struct Record {
    fields: Vec<Range<usize>>,
    /// Its first byte.
    start: usize,
    /// Where its last field ends: its line ending, or the end of the file.
    end: usize,
    /// After its line ending; `end` where it has none.
    next: usize,
    blank: bool,
}

/// `text`'s records, as the reader reads them (`csv_rows`), and its line ending: the first
/// record's, else `\n`.
fn records(text: &str) -> (Vec<Record>, &'static str) {
    let bytes = text.as_bytes();
    // The reader passes over every byte-order mark at the start.
    let mut at = text.len() - text.trim_start_matches('\u{FEFF}').len();
    let mut records = Vec::new();
    let mut ending = None;
    while at < bytes.len() {
        let start = at;
        let mut fields = Vec::new();
        let mut field = at;
        let mut quoted = false;
        // The field's text as the reader decodes it: a quote opens a field only while it is empty.
        let mut decoded = String::new();
        let mut i = at;
        let (end, next) = loop {
            let Some(c) = text[i..].chars().next() else { break (i, i) };
            match (quoted, c) {
                (true, '"') if bytes.get(i + 1) == Some(&b'"') => {
                    decoded.push('"');
                    i += 2;
                    continue;
                }
                (true, '"') => quoted = false,
                (true, c) => decoded.push(c),
                (false, '"') if decoded.is_empty() => quoted = true,
                (false, ',') => {
                    fields.push(field..i);
                    field = i + 1;
                    decoded.clear();
                }
                (false, '\r') if bytes.get(i + 1) == Some(&b'\n') => {
                    ending.get_or_insert("\r\n");
                    break (i, i + 2);
                }
                (false, '\n') => {
                    ending.get_or_insert("\n");
                    break (i, i + 1);
                }
                (false, '\r') => {
                    ending.get_or_insert("\r");
                    break (i, i + 1);
                }
                (false, c) => decoded.push(c),
            }
            i += c.len_utf8();
        };
        fields.push(field..end);
        // A record of one empty field, as a blank line or `""` alone is: the reader passes over it.
        let blank = fields.len() == 1 && decoded.is_empty();
        records.push(Record { fields, start, end, next, blank });
        at = next;
    }
    (records, ending.unwrap_or("\n"))
}

/// The CSV `text` with `edit` made.
fn csv(text: &str, columns: &[String], edit: &RowEdit, values: &[Setting]) -> Result<String, String> {
    let (all, ending) = records(text);
    let read: Vec<&Record> = all.iter().filter(|r| !r.blank).collect();
    let (header, rows) = read.split_first().ok_or("no header row")?;
    let quoted = |record: &Record, i: usize| record.fields.get(i).is_some_and(|f| text[f.clone()].starts_with('"'));
    let index = |column: &str| columns.iter().position(|c| c == column).expect("checked against the columns");
    let mut out = String::with_capacity(text.len() + 64);
    match edit {
        RowEdit::Set { row, .. } => {
            let (column, value, kind) = values.first().ok_or("no value to set")?;
            let record = *rows.get(*row).ok_or(UNREAD)?;
            let i = index(column);
            let field = record.fields.get(i).ok_or(UNREAD)?.clone();
            out.push_str(&text[..field.start]);
            out.push_str(&csv_field(value, *kind, quoted(record, i)));
            out.push_str(&text[field.end..]);
        }
        RowEdit::Add { row, .. } => {
            let at = row.unwrap_or(rows.len());
            // The row before it, else the one it goes before, sets the quoting.
            let like = if at > 0 { rows.get(at - 1) } else { rows.first() };
            let line = (0..columns.len())
                .map(|i| {
                    let (value, kind) = values
                        .iter()
                        .find(|(c, ..)| index(c) == i)
                        .map_or(("", ColumnType::String), |(_, v, k)| (v.as_str(), *k));
                    csv_field(value, kind, like.is_some_and(|r| quoted(r, i)))
                })
                .collect::<Vec<_>>()
                .join(",");
            match rows.get(at) {
                // Before the row at its place.
                Some(next) => {
                    out.push_str(&text[..next.start]);
                    out.push_str(&line);
                    out.push_str(ending);
                    out.push_str(&text[next.start..]);
                }
                // After the last row, or the header.
                None => {
                    let last = rows.last().copied().unwrap_or(header);
                    if last.next > last.end {
                        out.push_str(&text[..last.next]);
                        out.push_str(&line);
                        out.push_str(ending);
                        out.push_str(&text[last.next..]);
                    } else {
                        out.push_str(&text[..last.end]);
                        out.push_str(ending);
                        out.push_str(&line);
                        out.push_str(&text[last.end..]);
                    }
                }
            }
        }
        RowEdit::Remove { row } => {
            let record = *rows.get(*row).ok_or(UNREAD)?;
            if record.next > record.end {
                out.push_str(&text[..record.start]);
                out.push_str(&text[record.next..]);
            } else {
                // The last line, with no ending: the ending before it goes, so none follows the rest.
                let before = if *row > 0 { rows[*row - 1] } else { header };
                out.push_str(&text[..before.end]);
                out.push_str(&text[record.end..]);
            }
        }
    }
    Ok(out)
}

/// A member of a JSON object: its key, and its value's bytes.
struct Member {
    key: String,
    key_start: usize,
    value: Range<usize>,
}

/// An element of the array: its bytes, and its members.
struct Element {
    span: Range<usize>,
    members: Vec<Member>,
}

/// A JSON array of objects, read where each part is: `[`'s place, and each element's.
struct Array {
    open: usize,
    elements: Vec<Element>,
}

/// Where JSON `text` from `at` stops being white space.
fn skip_space(text: &str, mut at: usize) -> usize {
    let bytes = text.as_bytes();
    while at < bytes.len() && matches!(bytes[at], b' ' | b'\t' | b'\n' | b'\r') {
        at += 1;
    }
    at
}

/// Where the JSON value at `at` ends: a string, an array, an object, or a scalar.
fn skip_value(text: &str, at: usize) -> Result<usize, String> {
    let bytes = text.as_bytes();
    match bytes.get(at) {
        Some(b'"') => {
            let mut i = at + 1;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => i += 2,
                    b'"' => return Ok(i + 1),
                    _ => i += 1,
                }
            }
            Err("a string does not end".into())
        }
        Some(b'[' | b'{') => {
            let mut depth = 0usize;
            let mut i = at;
            while i < bytes.len() {
                match bytes[i] {
                    b'"' => {
                        i = skip_value(text, i)?;
                        continue;
                    }
                    b'[' | b'{' => depth += 1,
                    b']' | b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(i + 1);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            Err("an array or an object does not end".into())
        }
        Some(_) => {
            let mut i = at;
            while i < bytes.len() && !matches!(bytes[i], b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r') {
                i += 1;
            }
            Ok(i)
        }
        None => Err("a value is missing".into()),
    }
}

/// The object at `at`, as `{` opens it.
fn object(text: &str, at: usize) -> Result<Element, String> {
    let bytes = text.as_bytes();
    if bytes.get(at) != Some(&b'{') {
        return Err("expected an array of objects".into());
    }
    let mut members = Vec::new();
    let mut i = skip_space(text, at + 1);
    if bytes.get(i) == Some(&b'}') {
        return Ok(Element { span: at..i + 1, members });
    }
    loop {
        let key_start = i;
        let key_end = skip_value(text, i)?;
        let key: String = serde_json::from_str(&text[key_start..key_end]).map_err(|e| e.to_string())?;
        i = skip_space(text, key_end);
        if bytes.get(i) != Some(&b':') {
            return Err("expected `:` after a key".into());
        }
        let value_start = skip_space(text, i + 1);
        let value_end = skip_value(text, value_start)?;
        members.push(Member { key, key_start, value: value_start..value_end });
        i = skip_space(text, value_end);
        match bytes.get(i) {
            Some(b',') => i = skip_space(text, i + 1),
            Some(b'}') => return Ok(Element { span: at..i + 1, members }),
            _ => return Err("expected `,` or `}` in an object".into()),
        }
    }
}

/// `text`, a JSON array of objects, read where each part is.
fn array(text: &str) -> Result<Array, String> {
    // Read whole first, so what follows reads only well-formed JSON.
    serde_json::from_str::<Value>(text).map_err(|e| e.to_string())?;
    let bytes = text.as_bytes();
    let open = skip_space(text, 0);
    if bytes.get(open) != Some(&b'[') {
        return Err("expected an array of objects".into());
    }
    let mut elements = Vec::new();
    let mut i = skip_space(text, open + 1);
    if bytes.get(i) == Some(&b']') {
        return Ok(Array { open, elements });
    }
    loop {
        let element = object(text, i)?;
        i = skip_space(text, element.span.end);
        elements.push(element);
        match bytes.get(i) {
            Some(b',') => i = skip_space(text, i + 1),
            Some(b']') => return Ok(Array { open, elements }),
            _ => return Err("expected `,` or `]` in the array".into()),
        }
    }
}

/// `element`, the object `text` holds from `element.span.start`, with member `key` set to `value`
/// (its JSON): its value rewritten where it has one, else added after its last member, laid out
/// as the members before it are.
fn set_member(text: &str, element: &Element, key: &str, value: &str) -> String {
    let span = element.span.clone();
    // A key twice: the reader reads the last.
    if let Some(member) = element.members.iter().rev().find(|m| m.key == key) {
        return format!("{}{}{}", &text[span.start..member.value.start], value, &text[member.value.end..span.end]);
    }
    let key = Value::String(key.to_string()).to_string();
    match element.members.as_slice() {
        [] => format!("{{{key}: {value}{}", &text[span.start + 1..span.end]),
        members => {
            let last = members.last().expect("a member");
            // What stands between a key and its value, and between two members, in this object.
            let colon = &text[last.key_start + key_len(text, last.key_start)..last.value.start];
            let between = match members {
                [.., before, last] => &text[before.value.end..last.key_start],
                _ => ", ",
            };
            format!(
                "{}{between}{key}{colon}{value}{}",
                &text[span.start..last.value.end],
                &text[last.value.end..span.end]
            )
        }
    }
}

/// The length of the JSON string at `at`.
fn key_len(text: &str, at: usize) -> usize {
    skip_value(text, at).map_or(0, |end| end - at)
}

/// The JSON file `text` with `edit` made.
fn json_file(text: &str, columns: &[String], edit: &RowEdit, values: &[Setting]) -> Result<String, String> {
    let array = array(text)?;
    let elements = &array.elements;
    let splice = |range: Range<usize>, with: &str| format!("{}{with}{}", &text[..range.start], &text[range.end..]);
    Ok(match edit {
        RowEdit::Set { row, .. } => {
            let (column, value, kind) = values.first().ok_or("no value to set")?;
            let element = elements.get(*row).ok_or(UNREAD)?;
            let object = set_member(text, element, column, &json_value(value, *kind));
            splice(element.span.clone(), &object)
        }
        RowEdit::Add { row, .. } => {
            let at = row.unwrap_or(elements.len());
            let value = |i: usize| {
                let column = &columns[i];
                values.iter().find(|(c, ..)| c == column).map_or("null".to_string(), |(_, v, k)| json_value(v, *k))
            };
            let like = if at > 0 { elements.get(at - 1) } else { elements.first() };
            let added = match like {
                // The row before it, its values each set, and laid out as it is.
                Some(like) => {
                    let mut row = text[like.span.clone()].to_string();
                    for (i, column) in columns.iter().enumerate() {
                        let element = object(&row, 0)?;
                        let has = element.members.iter().any(|m| &m.key == column);
                        let given = values.iter().any(|(c, ..)| c == column);
                        if has || given {
                            row = set_member(&row, &element, column, &value(i));
                        }
                    }
                    row
                }
                None => {
                    let members: Vec<String> = (0..columns.len())
                        .map(|i| format!("{}: {}", Value::String(columns[i].clone()), value(i)))
                        .collect();
                    format!("{{{}}}", members.join(", "))
                }
            };
            match (elements.get(at), elements.last()) {
                // Before the element at its place, with what stands before it.
                (Some(next), _) => {
                    let space = &text[space_before(text, next.span.start)..next.span.start];
                    splice(next.span.start..next.span.start, &format!("{added},{space}"))
                }
                // After the last.
                (None, Some(last)) => {
                    let space = &text[space_before(text, last.span.start)..last.span.start];
                    splice(last.span.end..last.span.end, &format!(",{space}{added}"))
                }
                // The first.
                (None, None) => splice(array.open + 1..array.open + 1, &added),
            }
        }
        RowEdit::Remove { row } => {
            let element = elements.get(*row).ok_or(UNREAD)?;
            match (elements.get(*row + 1), row.checked_sub(1).and_then(|r| elements.get(r))) {
                (Some(next), _) => splice(element.span.start..next.span.start, ""),
                (None, Some(before)) => splice(before.span.end..element.span.end, ""),
                // The only one, and the space before it.
                (None, None) => splice(space_before(text, element.span.start)..element.span.end, ""),
            }
        }
    })
}

/// Where the white space before `at` begins.
fn space_before(text: &str, at: usize) -> usize {
    let bytes = text.as_bytes();
    let mut i = at;
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t' | b'\n' | b'\r') {
        i -= 1;
    }
    i
}

/// A JSON Pointer's token for `s`.
fn token(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

/// Rows written inline in source `name`, `edit` made: the deck's patch that makes it.
fn inline(
    name: &str,
    columns: &[String],
    deck: &Deck,
    edit: &RowEdit,
    values: &[Setting],
) -> Result<Vec<Value>, String> {
    let rows = format!("/data/{}/source/inline", token(name));
    let typed = |value: &str, kind: ColumnType| -> Value {
        serde_json::from_str(&json_value(value, kind)).expect("a JSON value")
    };
    Ok(match edit {
        RowEdit::Set { row, .. } => {
            let (column, value, kind) = values.first().ok_or("no value to set")?;
            let held = deck.data[name].source["inline"][*row].get(column).is_some();
            let op = if held { "replace" } else { "add" };
            vec![json!({ "op": op, "path": format!("{rows}/{row}/{}", token(column)), "value": typed(value, *kind) })]
        }
        RowEdit::Add { row, .. } => {
            let object: serde_json::Map<String, Value> = columns
                .iter()
                .map(|column| {
                    let value = values.iter().find(|(c, ..)| c == column).map_or(Value::Null, |(_, v, k)| typed(v, *k));
                    (column.clone(), value)
                })
                .collect();
            let at = row.map_or("-".to_string(), |r| r.to_string());
            vec![json!({ "op": "add", "path": format!("{rows}/{at}"), "value": object })]
        }
        RowEdit::Remove { row } => vec![json!({ "op": "remove", "path": format!("{rows}/{row}") })],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Datum;
    use std::collections::BTreeMap;

    fn deck(data: &str) -> Deck {
        let json = format!(
            r#"{{"scaena": "{}", "canvas": {{"width": 1920, "height": 1080}}, "data": {data}, "nodes": {{}}, "states": []}}"#,
            crate::FORMAT_VERSION
        );
        Deck::from_json(&json).unwrap()
    }

    const CSV: &str =
        r#"{"q": {"source": "data/q.csv", "schema": {"quarter": "string", "rev": "number", "note": "string"}}}"#;

    /// Source `q`, held as `text` at `path`, with `edit` made: the file's text after.
    fn edited(data: &str, path: &str, text: &str, edit: RowEdit) -> Result<String, String> {
        let files = BTreeMap::from([(path.to_string(), text.as_bytes().to_vec())]);
        match super::edit(&deck(data), &files, "q", &edit)? {
            Edited::File { path: written, bytes } => {
                assert_eq!(written, path);
                // What an edit writes reads back.
                let after = BTreeMap::from([(path.to_string(), bytes.clone())]);
                crate::data::load(&deck(data), &after, "q").expect("the file reads after the edit");
                Ok(String::from_utf8(bytes).unwrap())
            }
            Edited::Inline(_) => panic!("a file source edits its file"),
        }
    }

    fn set(row: usize, column: &str, value: &str) -> RowEdit {
        RowEdit::Set { row, column: column.into(), value: value.into() }
    }

    fn add(row: Option<usize>, values: &[(&str, &str)]) -> RowEdit {
        RowEdit::Add { row, values: values.iter().map(|(c, v)| (c.to_string(), v.to_string())).collect() }
    }

    /// A cell rewrites its field alone: the byte-order mark, the line endings, every quote, and
    /// the fields around it stay as they were.
    #[test]
    fn a_cell_rewrites_its_field_alone() {
        let text = "\u{FEFF}quarter,rev,note\r\n\"Q1\",1.5,\"a, \"\"b\"\"\"\r\nQ2,,plain\r\n";
        let after = edited(CSV, "data/q.csv", text, set(1, "rev", " 2.25 ")).unwrap();
        assert_eq!(after, "\u{FEFF}quarter,rev,note\r\n\"Q1\",1.5,\"a, \"\"b\"\"\"\r\nQ2,2.25,plain\r\n");
        // Quoted where it was, and where it has to be.
        let after = edited(CSV, "data/q.csv", text, set(0, "quarter", "Q1 \"early\"")).unwrap();
        assert!(after.contains("\r\n\"Q1 \"\"early\"\"\",1.5,"), "{after:?}");
        let after = edited(CSV, "data/q.csv", text, set(1, "note", "one, two")).unwrap();
        assert!(after.ends_with("Q2,,\"one, two\"\r\n"), "{after:?}");
        // Emptied, a number is nothing.
        let after = edited(CSV, "data/q.csv", text, set(0, "rev", "")).unwrap();
        assert!(after.contains("\"Q1\",,\"a"), "{after:?}");
    }

    /// A row added copies the line ending and quoting of the row before it, wherever it goes, and
    /// a file that ends without a line ending still does.
    #[test]
    fn a_row_added_copies_the_row_before_it() {
        let text = "quarter,rev,note\r\n\"Q1\",1.5,x\r\n";
        let end = edited(CSV, "data/q.csv", text, add(None, &[("quarter", "Q2"), ("rev", "3")])).unwrap();
        assert_eq!(end, "quarter,rev,note\r\n\"Q1\",1.5,x\r\n\"Q2\",3,\r\n");
        let first = edited(CSV, "data/q.csv", text, add(Some(0), &[("quarter", "Q0")])).unwrap();
        assert_eq!(first, "quarter,rev,note\r\n\"Q0\",,\r\n\"Q1\",1.5,x\r\n");
        let open = edited(CSV, "data/q.csv", "quarter,rev,note\nQ1,1,x", add(None, &[("note", "y")])).unwrap();
        assert_eq!(open, "quarter,rev,note\nQ1,1,x\n,,y");
        let empty = edited(CSV, "data/q.csv", "quarter,rev,note\n", add(None, &[("rev", "4")])).unwrap();
        assert_eq!(empty, "quarter,rev,note\n,4,\n");
    }

    /// A row taken away takes its line with it; the last, with no line ending after it, takes the
    /// ending before it, so the file still ends without one.
    #[test]
    fn a_row_taken_away_takes_its_line() {
        let text = "quarter,rev,note\nQ1,1,\"two\nlines\"\nQ2,2,b\nQ3,3,c";
        let middle = edited(CSV, "data/q.csv", text, RowEdit::Remove { row: 0 }).unwrap();
        assert_eq!(middle, "quarter,rev,note\nQ2,2,b\nQ3,3,c");
        let last = edited(CSV, "data/q.csv", text, RowEdit::Remove { row: 2 }).unwrap();
        assert_eq!(last, "quarter,rev,note\nQ1,1,\"two\nlines\"\nQ2,2,b");
    }

    /// A value its column refuses is refused, saying why, as a chart reading it would; so is a row
    /// or a column the source does not have.
    #[test]
    fn what_a_column_refuses_is_refused_with_why() {
        let text = "quarter,rev,note\nQ1,1,x\n";
        let why = |edit| edited(CSV, "data/q.csv", text, edit).unwrap_err();
        assert!(why(set(0, "rev", "12x")).contains("`12x` is not a number"), "{}", why(set(0, "rev", "12x")));
        assert!(why(set(3, "rev", "1")).contains("has 1 row: there is no row 3"));
        assert!(why(set(0, "revenue", "1")).contains("no column `revenue`: its columns are quarter, rev, note"));
        assert!(why(add(Some(5), &[])).contains("a row goes in at 0 to 1, not 5"));
        let dated = r#"{"q": {"source": "data/q.csv", "schema": {"day": "date", "ok": "boolean"}}}"#;
        let refuse = |edit| edited(dated, "data/q.csv", "day,ok\n2026-01-02,true\n", edit).unwrap_err();
        assert!(refuse(set(0, "day", "soon")).contains("column `day`"), "{}", refuse(set(0, "day", "soon")));
        assert!(refuse(set(0, "ok", "yes")).contains("`yes` is not true or false"));
    }

    const JSON: &str = r#"{"q": {"source": "data/q.json", "schema": {"quarter": "string", "rev": "number"}}}"#;

    /// In a JSON file a cell is its member's value: the layout around it stays, and a member a row
    /// lacks is added after its last, laid out as the members before it are.
    #[test]
    fn a_json_cell_is_its_members_value() {
        let text = "[\n  {\"quarter\": \"Q1\", \"rev\": 1.50},\n  {\"quarter\": \"Q2\"}\n]\n";
        let after = edited(JSON, "data/q.json", text, set(0, "rev", "12.50")).unwrap();
        assert_eq!(after, "[\n  {\"quarter\": \"Q1\", \"rev\": 12.50},\n  {\"quarter\": \"Q2\"}\n]\n");
        let added = edited(JSON, "data/q.json", text, set(1, "rev", "3")).unwrap();
        assert_eq!(added, "[\n  {\"quarter\": \"Q1\", \"rev\": 1.50},\n  {\"quarter\": \"Q2\", \"rev\": 3}\n]\n");
        let text_value = edited(JSON, "data/q.json", text, set(1, "quarter", "Q2 \"late\"")).unwrap();
        assert!(text_value.contains(r#"{"quarter": "Q2 \"late\""}"#), "{text_value}");
        let emptied = edited(JSON, "data/q.json", text, set(0, "rev", "")).unwrap();
        assert!(emptied.contains(r#""rev": null}"#), "{emptied}");
    }

    /// A row added to a JSON file is the row before it, its values set, laid out where it goes as the
    /// rows around it are; one taken away takes one comma with it.
    #[test]
    fn json_rows_are_added_and_taken_away_in_its_layout() {
        let text = "[\n  {\n    \"quarter\": \"Q1\",\n    \"rev\": 1\n  }\n]";
        let end = edited(JSON, "data/q.json", text, add(None, &[("quarter", "Q2"), ("rev", "2")])).unwrap();
        assert_eq!(
            end,
            "[\n  {\n    \"quarter\": \"Q1\",\n    \"rev\": 1\n  },\n  {\n    \"quarter\": \"Q2\",\n    \"rev\": 2\n  }\n]"
        );
        let start = edited(JSON, "data/q.json", &end, add(Some(0), &[("quarter", "Q0")])).unwrap();
        assert!(
            start
                .starts_with("[\n  {\n    \"quarter\": \"Q0\",\n    \"rev\": null\n  },\n  {\n    \"quarter\": \"Q1\""),
            "{start}"
        );
        let gone = edited(JSON, "data/q.json", &end, RowEdit::Remove { row: 1 }).unwrap();
        assert_eq!(gone, text);
        let first_gone = edited(JSON, "data/q.json", &end, RowEdit::Remove { row: 0 }).unwrap();
        assert_eq!(first_gone, "[\n  {\n    \"quarter\": \"Q2\",\n    \"rev\": 2\n  }\n]");
        let none = edited(JSON, "data/q.json", "[]", add(None, &[("quarter", "Q1"), ("rev", "1")])).unwrap();
        assert_eq!(none, r#"[{"quarter": "Q1", "rev": 1}]"#);
    }

    /// A sheet shows each cell as written: a CSV's fields unquoted, a JSON file's values as the
    /// file writes them, a string's without its quotes, and null as nothing.
    #[test]
    fn a_sheet_shows_each_cell_as_written() {
        let files = |path: &str, text: &str| BTreeMap::from([(path.to_string(), text.as_bytes().to_vec())]);
        let csv =
            sheet(&deck(CSV), &files("data/q.csv", "quarter,rev,note\n\"Q1\",1.50,\"a, \"\"b\"\"\"\n"), "q").unwrap();
        assert_eq!(csv.rows, [["Q1", "1.50", "a, \"b\""]]);
        let typed: Vec<(&str, ColumnType)> = csv.columns.iter().map(|c| (c.name.as_str(), c.kind)).collect();
        assert_eq!(typed, [("quarter", ColumnType::String), ("rev", ColumnType::Number), ("note", ColumnType::String)]);
        // JSON's columns are its first row's keys, in its order.
        let json =
            sheet(&deck(JSON), &files("data/q.json", r#"[{"rev": 1.50, "quarter": "Q1"}, {"quarter": null}]"#), "q");
        assert_eq!(json.unwrap().rows, [["1.50", "Q1"], ["", ""]]);
        // A header and no rows: the header's columns, in its order.
        let empty = sheet(&deck(CSV), &files("data/q.csv", "note,quarter,rev\n"), "q").unwrap();
        let names: Vec<&str> = empty.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!((names, empty.rows.len()), (vec!["note", "quarter", "rev"], 0));
        let added =
            edited(CSV, "data/q.csv", "note,quarter,rev\n", add(None, &[("quarter", "Q1"), ("rev", "2")])).unwrap();
        assert_eq!(added, "note,quarter,rev\n,Q1,2\n");
        let inline = r#"{"q": {"source": {"inline": [{"k": "a", "v": 2.5}]}, "schema": {"v": "number"}}}"#;
        assert_eq!(sheet(&deck(inline), &BTreeMap::new(), "q").unwrap().rows, [["a", "2.5"]]);
    }

    /// A value may come as a JSON number or boolean, which is its text, or null, which is
    /// nothing; and an edit names no field it does not have.
    #[test]
    fn values_come_as_any_scalar() {
        let edit: RowEdit =
            serde_json::from_value(json!({ "op": "set", "row": 0, "column": "rev", "value": 4.25 })).unwrap();
        assert_eq!(edit, set(0, "rev", "4.25"));
        let edit: RowEdit =
            serde_json::from_value(json!({ "op": "add", "values": { "ok": true, "rev": null } })).unwrap();
        assert_eq!(edit, add(None, &[("ok", "true"), ("rev", "")]));
        let extra = json!({ "op": "remove", "row": 0, "column": "rev" });
        assert!(serde_json::from_value::<RowEdit>(extra).is_err());
        let nested = json!({ "op": "set", "row": 0, "column": "rev", "value": [1] });
        assert!(serde_json::from_value::<RowEdit>(nested).unwrap_err().to_string().contains("a cell is text"));
    }

    /// A cell its column does not read is shown, with why, and an edit elsewhere is made: the
    /// edit that sets it to what it reads is what fixes the source.
    #[test]
    fn a_cell_that_does_not_read_is_shown_and_fixed() {
        let text = "quarter,rev,note\nQ1,n/a,x\nQ2,2,y\n";
        let files = BTreeMap::from([("data/q.csv".to_string(), text.as_bytes().to_vec())]);
        let shown = sheet(&deck(CSV), &files, "q").unwrap();
        assert_eq!(shown.rows[0], ["Q1", "n/a", "x"]);
        assert_eq!((shown.problems.len(), shown.problems[0].row, shown.problems[0].column.as_str()), (1, 0, "rev"));
        assert!(shown.problems[0].why.contains("`n/a` is not a number"), "{:?}", shown.problems);
        let elsewhere = edited_unread(text, set(1, "note", "z")).unwrap();
        assert_eq!(elsewhere, "quarter,rev,note\nQ1,n/a,x\nQ2,2,z\n");
        let fixed = edited(CSV, "data/q.csv", text, set(0, "rev", "1")).unwrap();
        assert_eq!(fixed, "quarter,rev,note\nQ1,1,x\nQ2,2,y\n");
    }

    /// The CSV `text` as source `q` with `edit` made, not read back: it need not read.
    fn edited_unread(text: &str, edit: RowEdit) -> Result<String, String> {
        let files = BTreeMap::from([("data/q.csv".to_string(), text.as_bytes().to_vec())]);
        match super::edit(&deck(CSV), &files, "q", &edit)? {
            Edited::File { bytes, .. } => Ok(String::from_utf8(bytes).unwrap()),
            Edited::Inline(_) => panic!("a file source edits its file"),
        }
    }

    /// A number a chart cannot draw is refused.
    #[test]
    fn a_number_is_finite() {
        let why = edited(CSV, "data/q.csv", "quarter,rev,note\nQ1,1,x\n", set(0, "rev", "inf")).unwrap_err();
        assert!(why.contains("`inf` is not a finite number"), "{why}");
    }

    /// Rows written inline are the deck's: their edit is a JSON Patch, the values typed as the
    /// column reads them.
    #[test]
    fn inline_rows_are_edited_by_a_patch() {
        let data = r#"{"q": {"source": {"inline": [{"k": "a", "v": 2}]}, "schema": {"v": "number"}}}"#;
        let patch = |edit| match super::edit(&deck(data), &BTreeMap::new(), "q", &edit).unwrap() {
            Edited::Inline(ops) => ops,
            Edited::File { .. } => panic!("rows inline are the deck's"),
        };
        assert_eq!(
            patch(set(0, "v", "3.5")),
            [json!({ "op": "replace", "path": "/data/q/source/inline/0/v", "value": 3.5 })]
        );
        assert_eq!(
            patch(add(None, &[("k", "b")])),
            [json!({ "op": "add", "path": "/data/q/source/inline/-", "value": { "k": "b", "v": null } })]
        );
        assert_eq!(patch(RowEdit::Remove { row: 0 }), [json!({ "op": "remove", "path": "/data/q/source/inline/0" })]);
        let table = |text: &str| {
            let files = BTreeMap::from([("data/q.csv".to_string(), text.as_bytes().to_vec())]);
            crate::data::load(&deck(CSV), &files, "q").unwrap().rows
        };
        assert_eq!(table("quarter,rev,note\nQ1,2,x\n")[0][1], Datum::Number(2.0));
    }
}
