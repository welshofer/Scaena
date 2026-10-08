//! Cells copied from a spreadsheet (PLAN 2.96). Numbers, Excel, and Google Sheets put a range
//! on the clipboard as text: a row to a line, its cells split by tabs, and a cell that holds a
//! tab, a line break, or a quote in quotes. Pasted on the canvas, the rows become a data
//! source, the first row naming its columns.
//!
//! A sheet shows numbers formatted (`$1,234.50`, `12.5%`, `(1,200)`), and that is what it
//! copies. A column whose cells are all numbers that one format writes, as `docs/spec/format.md`
//! reads it in the deck's language, is written plainly in the source's file (`1234.5`,
//! `0.125`) and keeps that format, so a table of it prints each cell as it was copied and a
//! chart plots it. Any other column is text, even where its cells read as numbers (`007`).

use super::{ColumnType, delimited, infer};
use crate::format::{Locale, MINUS, NumberFormat};
use crate::inserts::slug;
use indexmap::IndexMap;
use serde::Serialize;

/// Cells read as a data source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cells {
    /// A name for the source and its file, made from the first columns' names.
    pub name: String,
    /// The columns, as the first row names them: an empty name is `column 2`, and a name the
    /// row has already is `name 2`.
    pub columns: Vec<String>,
    /// Each column's type, as the source declares it: `number` where its cells are numbers in
    /// one format, else what they read as (`string`, `date`, `boolean`), never `number`.
    pub schema: IndexMap<String, String>,
    /// For each column, the format that prints its numbers as they were copied: none for text,
    /// or for numbers written as no format writes them.
    pub formats: Vec<Option<String>>,
    /// How many rows there are under the first.
    pub rows: usize,
    /// The source's file: RFC 4180, the numbers written plainly.
    pub csv: String,
}

/// `text` as cells, read in `locale`: at least two rows of at least two cells, each row with as
/// many. None where it is not such, so it pastes as text.
pub fn read(text: &str, locale: &Locale) -> Option<Cells> {
    if !text.contains('\t') {
        return None;
    }
    let rows = delimited(text.trim_end_matches(['\r', '\n']), '\t').ok()?;
    let width = rows.first()?.len();
    if rows.len() < 2 || width < 2 || rows.iter().any(|r| r.len() != width) {
        return None;
    }
    let (header, body) = rows.split_first()?;
    let columns = names(header);
    let cells: Vec<Vec<&str>> = body.iter().map(|r| r.iter().map(|c| c.trim()).collect()).collect();
    let read: Vec<Option<(Option<String>, Vec<String>)>> =
        (0..width).map(|c| numbers(&cells.iter().map(|r| r[c]).collect::<Vec<_>>(), locale)).collect();
    let mut csv = String::new();
    line(&mut csv, columns.iter().map(String::as_str));
    for (i, row) in cells.iter().enumerate() {
        line(&mut csv, (0..width).map(|c| read[c].as_ref().map_or(row[c], |(_, plain)| &plain[i])));
    }
    // What the rest read as, as `data_attach` types a file; a column of text never a number.
    let inferred = infer("cells.csv", csv.as_bytes()).ok()?;
    let schema = (columns.iter().zip(&inferred.types).zip(&read))
        .map(|((column, kind), numbers)| {
            let kind = match (numbers, kind) {
                (Some(_), _) => ColumnType::Number,
                (None, ColumnType::Number) => ColumnType::String,
                (None, &kind) => kind,
            };
            (column.clone(), kind.name().to_string())
        })
        .collect();
    let name = slug(&columns.iter().take(3).cloned().collect::<Vec<_>>().join(" "), "cells");
    let formats = read.into_iter().map(|r| r.and_then(|(format, _)| format)).collect();
    Some(Cells { name, columns, schema, formats, rows: body.len(), csv })
}

/// The columns' names: each cell of the first row, its spaces and line breaks one space; an
/// empty one `column N`, and one the row has already `name 2`, `name 3`, ….
fn names(header: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(header.len());
    for (i, cell) in header.iter().enumerate() {
        let words = cell.split_whitespace().collect::<Vec<_>>().join(" ");
        let base = if words.is_empty() { format!("column {}", i + 1) } else { words };
        let name = (1..)
            .map(|n| if n == 1 { base.clone() } else { format!("{base} {n}") })
            .find(|n| !out.contains(n))
            .expect("some number is free");
        out.push(name);
    }
    out
}

/// A cell's number, as it shows it.
struct Shown {
    /// Its value written plainly: a `.` for the point, no grouping, a percent as its fraction.
    plain: String,
    value: f64,
    parens: bool,
    currency: bool,
    percent: bool,
    group: bool,
    /// Digits after the point, as the cell shows it.
    places: usize,
}

/// A column of cells as numbers that one format writes: that format (none where no format
/// writes them so), and each cell's number written plainly, empty where the cell is. None where
/// a cell is not a number, where its cells show different formats, or where nothing is in it.
fn numbers(column: &[&str], locale: &Locale) -> Option<(Option<String>, Vec<String>)> {
    let shown: Vec<Option<Shown>> = (column.iter())
        .map(|c| if c.is_empty() { Some(None) } else { shown(c, locale).map(Some) })
        .collect::<Option<_>>()?;
    let seen: Vec<&Shown> = shown.iter().flatten().collect();
    let first = seen.first()?;
    if seen.iter().any(|s| s.currency != first.currency || s.percent != first.percent) {
        return None;
    }
    let (parens, group) = (seen.iter().any(|s| s.parens), seen.iter().any(|s| s.group));
    let places = seen.iter().map(|s| s.places).max().unwrap_or(0);
    let trim = seen.iter().any(|s| s.places != places);
    let writes = |format: &NumberFormat| {
        (column.iter().zip(&shown)).all(|(c, s)| s.as_ref().is_none_or(|s| alike(&format.format(s.value, locale), c)))
    };
    let plain = (shown.iter()).map(|s| s.as_ref().map_or(String::new(), |s| s.plain.clone())).collect();
    if !(parens || group || first.currency || first.percent) && writes(&NumberFormat::plain()) {
        return Some((None, plain));
    }
    let kind = match (first.percent, places) {
        (true, _) => "%",
        (false, 0) => "d",
        (false, _) => "f",
    };
    let format = format!(
        "{}{}{}{}{}{kind}",
        if parens { "(" } else { "" },
        if first.currency { "$" } else { "" },
        if group { "," } else { "" },
        if kind == "d" { String::new() } else { format!(".{places}") },
        if trim { "~" } else { "" },
    );
    writes(&NumberFormat::parse(&format).ok()?).then_some((Some(format), plain))
}

/// Whether a format's text is the cell's: alike but for the minus sign, which a sheet writes as
/// a hyphen, and spaces, which it may leave out or write as others.
fn alike(printed: &str, cell: &str) -> bool {
    let bare = |s: &str| {
        s.chars().filter(|c| !c.is_whitespace()).map(|c| if c == MINUS { '-' } else { c }).collect::<String>()
    };
    bare(printed) == bare(cell)
}

/// `cell` as a number written in `locale`, or none: an optional sign, or parentheses about it;
/// the currency symbol where `locale` writes it; digits, grouped in threes or not; a decimal
/// point and digits; and the percent sign.
fn shown(cell: &str, locale: &Locale) -> Option<Shown> {
    let spaced = |s: &str| s.replace(['\u{a0}', '\u{202f}'], " ");
    let text = spaced(cell);
    let mut s = text.trim();
    let parens = s.starts_with('(') && s.ends_with(')') && s.len() > 2;
    if parens {
        s = s[1..s.len() - 1].trim();
    }
    let minus = s.strip_prefix(['-', MINUS]);
    if parens && minus.is_some() {
        return None;
    }
    let negative = parens || minus.is_some();
    s = minus.unwrap_or(s);
    let (pre, post) = (spaced(locale.currency.0), spaced(locale.currency.1));
    let mut currency = false;
    if let Some(rest) = Some(pre.trim()).filter(|p| !p.is_empty()).and_then(|p| s.strip_prefix(p)) {
        (s, currency) = (rest.trim_start(), true);
    }
    if let Some(rest) = Some(post.trim()).filter(|p| !p.is_empty()).and_then(|p| s.strip_suffix(p)) {
        (s, currency) = (rest.trim_end(), true);
    }
    let sign = spaced(locale.percent);
    let percent = match s.strip_suffix(sign.trim()) {
        Some(rest) => {
            s = rest.trim_end();
            true
        }
        None => false,
    };
    let (int, frac) = match s.split_once(locale.decimal) {
        Some((int, frac)) if !frac.is_empty() && frac.bytes().all(|b| b.is_ascii_digit()) => (int, frac),
        Some(_) => return None,
        None => (s, ""),
    };
    let separator = spaced(locale.group);
    let group = !separator.is_empty() && int.contains(separator.as_str());
    let mut parts: Vec<&str> = if group { int.split(separator.as_str()).collect() } else { vec![int] };
    let head = parts.remove(0);
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    if !digits(head) || (group && (head.len() > 3 || parts.iter().any(|p| p.len() != 3 || !digits(p)))) {
        return None;
    }
    let int: String = std::iter::once(head).chain(parts).collect();
    let plain = plainly(negative, &int, frac, if percent { 2 } else { 0 });
    let value = plain.parse::<f64>().ok().filter(|v| v.is_finite())?;
    Some(Shown { plain, value, parens, currency, percent, group, places: frac.len() })
}

/// The number `int`.`frac`, divided by ten `shift` times, written plainly: no leading zeros but
/// one before the point, no trailing zeros after it, and no point with nothing after it.
fn plainly(negative: bool, int: &str, frac: &str, shift: usize) -> String {
    // With `shift` zeros before them, the point goes where the integer's digits end.
    let digits: String = format!("{}{int}{frac}", "0".repeat(shift));
    let (whole, part) = digits.split_at(int.len());
    let whole = whole.trim_start_matches('0');
    let part = part.trim_end_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let zero = whole == "0" && part.is_empty();
    let sign = if negative && !zero { "-" } else { "" };
    if part.is_empty() { format!("{sign}{whole}") } else { format!("{sign}{whole}.{part}") }
}

/// A CSV record of `cells`, each quoted where it holds a comma, a quote, or a line break.
fn line<'a>(csv: &mut String, cells: impl Iterator<Item = &'a str>) {
    for (i, cell) in cells.enumerate() {
        if i > 0 {
            csv.push(',');
        }
        if cell.contains([',', '"', '\n', '\r']) {
            csv.push('"');
            csv.push_str(&cell.replace('"', "\"\""));
            csv.push('"');
        } else {
            csv.push_str(cell);
        }
    }
    csv.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en() -> &'static Locale {
        Locale::of(Some("en-US"))
    }

    /// A sheet's figures, as Google Sheets copies them: each column of numbers written plainly,
    /// with the format that prints it as it was.
    #[test]
    fn a_sheets_figures_keep_their_formats() {
        let text = "Region\tRevenue\tShare\tUnits\tChange\r\nNorth\t$1,234.50\t12.5%\t1,200\t(40)\r\nSouth\t$987.00\t8%\t950\t15\r\n";
        let cells = read(text, en()).unwrap();
        assert_eq!(cells.columns, ["Region", "Revenue", "Share", "Units", "Change"]);
        assert_eq!(cells.name, "region-revenue-share");
        assert_eq!(cells.rows, 2);
        let formats: Vec<Option<&str>> = cells.formats.iter().map(Option::as_deref).collect();
        assert_eq!(formats, [None, Some("$,.2f"), Some(".1~%"), Some(",d"), Some("(d")]);
        assert_eq!(
            cells.csv,
            "Region,Revenue,Share,Units,Change\nNorth,1234.5,0.125,1200,-40\nSouth,987,0.08,950,15\n"
        );
        let types: Vec<&str> = cells.schema.values().map(String::as_str).collect();
        assert_eq!(types, ["string", "number", "number", "number", "number"]);
        // Each format prints each cell as it was copied.
        let printed = NumberFormat::parse("$,.2f").unwrap().format(1234.5, en());
        assert_eq!(printed, "$1,234.50");
        assert_eq!(NumberFormat::parse(".1~%").unwrap().format(0.08, en()), "8%");
        assert_eq!(NumberFormat::parse("(d").unwrap().format(-40.0, en()), "(40)");
    }

    /// Numbers that no format needs to write as they were stay unformatted; a column whose
    /// cells only read as numbers (a code with its zeros, a mix of currencies) stays text, and
    /// so does a date, though one in ISO 8601 reads as a date.
    #[test]
    fn what_is_not_one_format_stays_text() {
        let text = "Code\tScore\tPrice\tWhen\tNote\n007\t3.5\t$4\t2024-03-01\tfine\n012\t12\t€5\t2024-04-01\t\"tabs\tand \"\"quotes\"\"\"\n";
        let cells = read(text, en()).unwrap();
        let formats: Vec<Option<&str>> = cells.formats.iter().map(Option::as_deref).collect();
        assert_eq!(formats, [None, None, None, None, None]);
        let types: Vec<&str> = cells.schema.values().map(String::as_str).collect();
        assert_eq!(types, ["string", "number", "string", "date", "string"]);
        assert_eq!(
            cells.csv,
            "Code,Score,Price,When,Note\n007,3.5,$4,2024-03-01,fine\n012,12,€5,2024-04-01,\"tabs\tand \"\"quotes\"\"\"\n"
        );
    }

    /// A German sheet groups with a point and writes its currency after: read by its language.
    #[test]
    fn cells_read_in_the_decks_language() {
        let de = Locale::of(Some("de-DE"));
        let cells = read("Monat\tUmsatz\nJan\t1.234,50 €\nFeb\t987,00 €\n", de).unwrap();
        assert_eq!(cells.formats[1].as_deref(), Some("$,.2f"));
        assert_eq!(cells.csv, "Monat,Umsatz\nJan,1234.5\nFeb,987\n");
        assert_eq!(NumberFormat::parse("$,.2f").unwrap().format(1234.5, de), "1.234,50\u{a0}€");
        // In English, the same cells are text.
        assert_eq!(read("Monat\tUmsatz\nJan\t1.234,50 €\n", en()).unwrap().formats[1], None);
    }

    /// Names: an empty one is the column's place, and one again is numbered.
    #[test]
    fn columns_are_named_by_the_first_row() {
        let cells = read("\tValue\tValue\tRevenue\n(USD)\nA\t1\t2\t3\n", en());
        // A quoted line break in a name is not a row: here the second line is a row of one cell.
        assert_eq!(cells, None);
        let cells = read("\tValue\tValue\t\"Revenue\n(USD)\"\nA\t1\t2\t3\n", en()).unwrap();
        assert_eq!(cells.columns, ["column 1", "Value", "Value 2", "Revenue (USD)"]);
        assert_eq!(cells.name, "column-1-value-value-2");
    }

    /// Text that is not a sheet's rows pastes as text: one cell to a row, rows of different
    /// widths, or one row alone.
    #[test]
    fn text_that_is_not_cells_is_none() {
        assert_eq!(read("just words", en()), None);
        assert_eq!(read("fn main() {\n\tprintln!();\n}\n", en()), None);
        assert_eq!(read("a\tb\nc\n", en()), None);
        assert_eq!(read("a\tb\n", en()), None);
        assert_eq!(read("a\tb\nc\td\n", en()).map(|c| c.rows), Some(1));
    }

    /// Empty cells are nothing, in a column of numbers as in one of text; a negative that is
    /// zero has no sign.
    #[test]
    fn empty_cells_are_nothing() {
        let cells = read("Item\tCost\nA\t$1,000\nB\t\nC\t$0\n", en()).unwrap();
        assert_eq!(cells.formats[1].as_deref(), Some("$,d"));
        assert_eq!(cells.csv, "Item,Cost\nA,1000\nB,\nC,0\n");
        assert_eq!(plainly(true, "0", "00", 0), "0");
        assert_eq!(plainly(false, "12", "5", 2), "0.125");
        assert_eq!(plainly(false, "1250", "", 2), "12.5");
        assert_eq!(plainly(true, "3", "", 2), "-0.03");
    }
}
