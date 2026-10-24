//! An Excel workbook as grounding: each sheet under its name, a row a line, its cells apart by
//! tabs, as Vzestup asks its transcription to set a spreadsheet; and each sheet's rows, which a
//! chart can plot.

use crate::SourceError;
use crate::ooxml::Package;
use crate::xml::Element;

/// A sheet: its name and its rows, each cell's text as the sheet holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<String>>,
}

impl Sheet {
    /// The sheet as a CSV a data source reads (SPEC §3.10): its first row that holds anything is
    /// its header, each column named (`column 3` where its header is empty, a name taken twice
    /// numbered), each row as wide as the header, and a row that holds nothing left out. None
    /// where no row stands under the header.
    pub fn csv(&self) -> Option<Vec<u8>> {
        let filled = |r: &&Vec<String>| r.iter().any(|c| !c.trim().is_empty());
        let mut rows = self.rows.iter().filter(filled);
        let header = rows.next()?;
        let body: Vec<&Vec<String>> = rows.collect();
        if body.is_empty() {
            return None;
        }
        let held = |r: &Vec<String>, c: usize| r.get(c).is_some_and(|v| !v.trim().is_empty());
        let width = body.iter().map(|r| r.len()).chain([header.len()]).max().unwrap_or(0);
        let used = (0..width).rev().find(|&c| held(header, c) || body.iter().any(|r| held(r, c))).map_or(0, |c| c + 1);
        let mut names: Vec<String> = Vec::new();
        for c in 0..used {
            let own = header.get(c).map(|h| h.trim()).filter(|h| !h.is_empty());
            let base = own.map_or_else(|| format!("column {}", c + 1), str::to_string);
            let (mut name, mut n) = (base.clone(), 2);
            while names.contains(&name) {
                name = format!("{base} {n}");
                n += 1;
            }
            names.push(name);
        }
        let mut w = csv::Writer::from_writer(Vec::new());
        w.write_record(&names).ok()?;
        for r in body {
            w.write_record((0..used).map(|c| r.get(c).map_or("", |v| v.trim()))).ok()?;
        }
        w.into_inner().ok()
    }
}

/// The most rows of a sheet read: a sheet past it is cut there.
const MOST_ROWS: usize = 20_000;

/// The most cells of a sheet read, each row as wide as its last cell: a sheet past it is cut
/// there, at the row that reaches it.
const MOST_CELLS: usize = 1 << 21;

/// The most columns a sheet has, as Excel's own: A to XFD.
const MOST_COLUMNS: usize = 16_384;

/// `bytes`, an `.xlsx`, as its sheets.
pub fn sheets(bytes: &[u8]) -> Result<Vec<Sheet>, SourceError> {
    let mut pkg = Package::open(bytes)?;
    let workbook = pkg
        .xml("xl/workbook.xml")
        .ok_or_else(|| SourceError::Unreadable("it has no xl/workbook.xml: not a workbook".into()))?;
    let rels = pkg.rels("xl/workbook.xml");
    let strings: Vec<String> =
        pkg.xml("xl/sharedStrings.xml").map(|sst| sst.all("si").map(rich).collect()).unwrap_or_default();
    let mut out = Vec::new();
    for sheet in workbook.child("sheets").into_iter().flat_map(|s| s.all("sheet")) {
        let name = sheet.attr("name").unwrap_or("Sheet").to_string();
        let Some(target) = sheet.rel_id().and_then(|id| rels.iter().find(|r| r.id == id)).map(|r| r.target.clone())
        else {
            continue;
        };
        let Some(doc) = pkg.xml(&target) else { continue };
        out.push(Sheet { name, rows: rows(&doc, &strings)? });
    }
    Ok(out)
}

/// `bytes`, an `.xlsx`, as grounding text.
pub fn read(bytes: &[u8]) -> Result<String, SourceError> {
    Ok(text(&sheets(bytes)?))
}

/// Sheets as grounding text: `## Sheet: name`, then each row.
pub fn text(sheets: &[Sheet]) -> String {
    let mut out = Vec::new();
    for sheet in sheets.iter().filter(|s| !s.rows.is_empty()) {
        out.push(format!("## Sheet: {}", sheet.name));
        out.extend(sheet.rows.iter().map(|r| r.join("\t")));
        out.push(String::new());
    }
    out.join("\n").trim().to_string()
}

/// A string item's text: its own, or its runs' (rich text), its phonetic reading left out.
fn rich(si: &Element) -> String {
    match si.child("t") {
        Some(t) => t.text(),
        None => si.all("r").filter_map(|r| r.child("t")).map(|t| t.text()).collect(),
    }
}

/// A cell reference's column, from 0: `C7` → 2. None where it names no column, and an error
/// where it names one past `XFD`, which no workbook has.
fn column(reference: &str) -> Result<Option<usize>, SourceError> {
    let letters = reference.bytes().take_while(u8::is_ascii_alphabetic);
    let mut n = 0usize;
    for b in letters {
        n = n * 26 + (b.to_ascii_uppercase() - b'A' + 1) as usize;
        if n > MOST_COLUMNS {
            return Err(SourceError::Unreadable(format!(
                "its cell {reference} is past column XFD, the last a sheet has"
            )));
        }
    }
    Ok(n.checked_sub(1))
}

/// A sheet's rows, each cell where its reference puts it, the empty cells at a row's end left out.
fn rows(doc: &Element, strings: &[String]) -> Result<Vec<Vec<String>>, SourceError> {
    let mut out = Vec::new();
    let mut held = 0usize;
    for row in doc.child("sheetData").into_iter().flat_map(|d| d.all("row")).take(MOST_ROWS) {
        let mut cells: Vec<String> = Vec::new();
        for c in row.all("c") {
            let at = match c.attr("r") {
                Some(r) => column(r)?.unwrap_or(cells.len()),
                None => cells.len(),
            };
            let v = c.child("v").map(|v| v.text()).unwrap_or_default();
            let text = match c.attr("t") {
                Some("s") => v.trim().parse::<usize>().ok().and_then(|i| strings.get(i)).cloned().unwrap_or_default(),
                Some("inlineStr") => c.child("is").map(rich).unwrap_or_default(),
                Some("b") => (if v.trim() == "1" { "TRUE" } else { "FALSE" }).to_string(),
                _ => v,
            };
            if at >= cells.len() {
                cells.resize(at + 1, String::new());
            }
            cells[at] = text;
        }
        while cells.last().is_some_and(String::is_empty) {
            cells.pop();
        }
        held += cells.len();
        if held > MOST_CELLS {
            break;
        }
        out.push(cells);
    }
    while out.last().is_some_and(Vec::is_empty) {
        out.pop();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_names_its_column() {
        assert_eq!(column("A1").unwrap(), Some(0));
        assert_eq!(column("C7").unwrap(), Some(2));
        assert_eq!(column("AA10").unwrap(), Some(26));
        assert_eq!(column("xfd3").unwrap(), Some(16_383));
        assert_eq!(column("7").unwrap(), None);
        // Past the last column a sheet has, however many letters: an error, never an overflow.
        assert!(column("XFE1").is_err());
        assert!(column(&"Z".repeat(40)).is_err());
    }

    #[test]
    fn a_sheet_is_a_csv_under_its_first_row() {
        let rows = |r: &[&[&str]]| r.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect();
        let sheet = Sheet {
            name: "Revenue".into(),
            rows: rows(&[&["", ""], &["Quarter", "Revenue", ""], &["Q1", "12.5"], &["", ""], &["Q2", "", "x", ""]]),
        };
        let csv = String::from_utf8(sheet.csv().unwrap()).unwrap();
        assert_eq!(csv, "Quarter,Revenue,column 3\nQ1,12.5,\nQ2,,x\n");
        let twice = Sheet { name: "s".into(), rows: rows(&[&["a", "a"], &["1", "2"]]) };
        assert_eq!(String::from_utf8(twice.csv().unwrap()).unwrap(), "a,a 2\n1,2\n");
        assert_eq!(Sheet { name: "s".into(), rows: rows(&[&["only", "a header"]]) }.csv(), None);
    }
}
