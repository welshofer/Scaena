//! A Word file as grounding: its paragraphs in order, a heading marked by its level, a list's
//! item by a dash, a table's rows a line each, its cells apart by tabs. Deleted text is left
//! out; inserted text is in.

use crate::SourceError;
use crate::ooxml::Package;
use crate::xml::Element;

/// `bytes`, a `.docx`, as grounding text.
pub fn read(bytes: &[u8]) -> Result<String, SourceError> {
    let mut pkg = Package::open(bytes)?;
    let doc = pkg.part("word/document.xml", "it has no word/document.xml: not a Word document")?;
    let body = doc.child("body").ok_or_else(|| SourceError::Unreadable("its document has no body".into()))?;
    let mut out = Vec::new();
    if let Some(title) = pkg.title() {
        out.push(format!("# {title}"));
    }
    blocks(body, &mut out);
    Ok(out.join("\n").trim().to_string())
}

/// The paragraphs and tables of `parent`, each a line or more, a content control opened.
fn blocks(parent: &Element, out: &mut Vec<String>) {
    for e in parent.elements() {
        match e.local() {
            "p" => {
                let line = paragraph(e);
                if !line.trim().is_empty() {
                    out.push(line);
                }
            }
            "tbl" => {
                for row in e.all("tr") {
                    let cells: Vec<String> = row
                        .all("tc")
                        .map(|cell| {
                            let mut lines = Vec::new();
                            blocks(cell, &mut lines);
                            lines.join(" ").trim().to_string()
                        })
                        .collect();
                    if cells.iter().any(|c| !c.is_empty()) {
                        out.push(cells.join("\t"));
                    }
                }
                out.push(String::new());
            }
            "sdt" => {
                if let Some(content) = e.child("sdtContent") {
                    blocks(content, out);
                }
            }
            _ => {}
        }
    }
}

/// A paragraph's words, marked as a heading or a list's item where it is one.
fn paragraph(p: &Element) -> String {
    let mut words = String::new();
    runs(p, &mut words);
    let words = words.trim_end().to_string();
    let props = p.child("pPr");
    let style = props.and_then(|pp| pp.child("pStyle")).and_then(|s| s.attr("val")).unwrap_or_default();
    let level = match style {
        "Title" => Some(1),
        s => s
            .strip_prefix("Heading")
            .or_else(|| s.strip_prefix("heading"))
            .and_then(|n| n.trim().parse::<usize>().ok())
            .or_else(|| {
                props
                    .and_then(|pp| pp.child("outlineLvl"))
                    .and_then(|o| o.attr("val"))
                    .and_then(|v| v.parse::<usize>().ok())
                    .map(|l| l + 1)
            }),
    };
    match level {
        Some(level) if !words.is_empty() => format!("{} {words}", "#".repeat(level.clamp(1, 6))),
        _ if props.is_some_and(|pp| pp.child("numPr").is_some()) && !words.is_empty() => format!("- {words}"),
        _ => words,
    }
}

/// The text of a paragraph's runs, in order: a tab as a tab, a break as a line, a deletion
/// left out, a field's instructions left out.
fn runs(e: &Element, out: &mut String) {
    for child in e.elements() {
        match child.local() {
            "t" => out.push_str(&child.text()),
            "tab" => out.push('\t'),
            "br" | "cr" => out.push('\n'),
            "noBreakHyphen" => out.push('-'),
            "del" | "delText" | "instrText" | "pPr" | "rPr" => {}
            _ => runs(child, out),
        }
    }
}
