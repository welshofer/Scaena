//! # scaena-sources
//!
//! What a deck is grounded in, read on the device (ADR-0026, PLAN G.3, G.4): Markdown, text,
//! and RTF; CSV and Excel workbooks, each sheet's rows also a table a chart can plot; Word's
//! `.docx`; PowerPoint's `.pptx`, its slides' words, notes, and charts' figures as text and
//! nothing else (invariant 8); a web page's words; a PDF, handed to the model as itself; and,
//! fetched by a plan the client carries out ([`fetch`]), a company's filings on SEC's EDGAR,
//! a GitHub repository, and a web page. Then the sources packed for Narrato's prompt as Vzestup
//! packs them: `=== SOURCE: name ===` before each, the whole held to Narrato's million bytes, a
//! source cut short rather than dropped.
//!
//! It reads bytes and nothing else: no file system, no network, no clock, so the browser loads
//! it as a module of its own.

pub mod docx;
pub mod fetch;
pub mod github;
pub mod html;
pub mod ooxml;
pub mod pptx;
pub mod rtf;
pub mod sec;
pub mod xlsx;
pub mod xml;

pub use xlsx::Sheet;

/// The most bytes the sources may take in the prompt (Vzestup's `SOURCE_CONTENT_CAP`).
pub const SOURCE_CONTENT_CAP: usize = 1_000_000;

/// What a source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Markdown,
    Rtf,
    Csv,
    Sheets,
    Word,
    Slides,
    Html,
    Json,
    /// A PDF, which the model reads as itself.
    Pdf,
    /// A company's filings on SEC's EDGAR.
    Sec,
    /// A GitHub repository's README, documentation, and manifests.
    GitHub,
    /// A report deep research wrote, its sources listed.
    Research,
}

impl Kind {
    /// What it is, in a word or two.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Markdown => "markdown",
            Kind::Rtf => "rtf",
            Kind::Csv => "csv",
            Kind::Sheets => "workbook",
            Kind::Word => "word",
            Kind::Slides => "slides",
            Kind::Html => "web page",
            Kind::Json => "json",
            Kind::Pdf => "pdf",
            Kind::Sec => "sec filings",
            Kind::GitHub => "github repository",
            Kind::Research => "deep research",
        }
    }
}

/// A source read: its name, what it is, its text for the prompt, and, for a workbook or a
/// CSV, its sheets; a PDF's bytes for the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub name: String,
    pub kind: Kind,
    pub text: String,
    pub sheets: Vec<Sheet>,
    pub pdf: Option<Vec<u8>>,
}

/// Why a source cannot be read.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SourceError {
    #[error("{0}")]
    Unreadable(String),
    /// Vzestup's words for an Office file protected by a password or a sensitivity label.
    #[error(
        "This document appears to be protected (encrypted or carries a sensitivity label) and can't be read. Remove the protection or save an unprotected copy, then re-upload."
    )]
    Protected,
    #[error("{0} is an older Office file: save it as {1} and give that")]
    Legacy(String, &'static str),
    #[error("{0}: a file of this kind is not read")]
    Kind(String),
}

/// The OLE2 compound file's signature, which wraps an Office file encrypted or labeled.
const OLE: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];

/// `bytes` as text: UTF-8 (its byte order mark left out), or UTF-16 where a mark says so.
pub fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xef, 0xbb, 0xbf, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xff, 0xfe, rest @ ..] => String::from_utf16_lossy(
            &rest.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect::<Vec<_>>(),
        ),
        [0xfe, 0xff, rest @ ..] => String::from_utf16_lossy(
            &rest.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect::<Vec<_>>(),
        ),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// The extension `bytes` would be read by, where they say what they are: a PDF, or an Office
/// file by the part it holds.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"%PDF") {
        return Some("pdf");
    }
    ooxml::kind(bytes)
}

/// The source `name`, whose bytes are `bytes`, read by its extension.
pub fn read(name: &str, bytes: &[u8]) -> Result<Source, SourceError> {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let source = |kind, text: String| Source { name: name.to_string(), kind, text, sheets: Vec::new(), pdf: None };
    if matches!(ext.as_str(), "docx" | "xlsx" | "xlsm" | "pptx") && bytes.starts_with(&OLE) {
        return Err(SourceError::Protected);
    }
    match ext.as_str() {
        "md" | "markdown" | "mdown" | "mkd" => Ok(source(Kind::Markdown, decode(bytes))),
        "txt" | "text" | "" => Ok(source(Kind::Text, decode(bytes))),
        "json" => Ok(source(Kind::Json, decode(bytes))),
        "rtf" => Ok(source(Kind::Rtf, rtf::read(&decode(bytes)))),
        "html" | "htm" | "xhtml" => Ok(source(Kind::Html, html::read(&decode(bytes)))),
        "csv" | "tsv" => {
            let text = decode(bytes);
            let sheet =
                Sheet { name: name.to_string(), rows: csv_rows(&text, if ext == "tsv" { b'\t' } else { b',' }) };
            Ok(Source { sheets: vec![sheet], ..source(Kind::Csv, text) })
        }
        "xlsx" | "xlsm" => {
            let sheets = xlsx::sheets(bytes)?;
            Ok(Source { text: xlsx::text(&sheets), sheets, ..source(Kind::Sheets, String::new()) })
        }
        "docx" => Ok(source(Kind::Word, docx::read(bytes)?)),
        "pptx" => Ok(source(Kind::Slides, pptx::read(bytes)?)),
        "pdf" => {
            if !bytes.starts_with(b"%PDF") {
                return Err(SourceError::Unreadable(format!("{name} is not a PDF")));
            }
            Ok(Source { pdf: Some(bytes.to_vec()), ..source(Kind::Pdf, String::new()) })
        }
        "doc" => Err(SourceError::Legacy(name.into(), ".docx")),
        "xls" => Err(SourceError::Legacy(name.into(), ".xlsx")),
        "ppt" => Err(SourceError::Legacy(name.into(), ".pptx")),
        _ => match std::str::from_utf8(bytes) {
            Ok(text) => Ok(source(Kind::Text, text.to_string())),
            Err(_) => Err(SourceError::Kind(name.into())),
        },
    }
}

/// A CSV's rows, each cell as written; a row that does not read as CSV kept whole.
fn csv_rows(text: &str, delimiter: u8) -> Vec<Vec<String>> {
    let mut reader =
        csv::ReaderBuilder::new().has_headers(false).flexible(true).delimiter(delimiter).from_reader(text.as_bytes());
    reader.records().filter_map(Result::ok).map(|r| r.iter().map(str::to_string).collect()).collect()
}

/// The sources packed for the prompt, and how far each got.
#[derive(Debug, Clone, PartialEq)]
pub struct Packed {
    pub text: String,
    /// Each source by its name: the bytes it took, and whether it was cut short.
    pub each: Vec<(String, usize, bool)>,
    pub truncated: bool,
}

/// `sources`, each a name and its text, packed as Vzestup's `assembleSourceContent` packs them:
/// `=== SOURCE: name ===`, the text, a blank line; in order, the first given the room first,
/// the whole held to `most` bytes, a source cut short where it runs over, with the room for
/// less than a useful slice said and left out.
pub fn pack(sources: &[(String, String)], most: usize) -> Packed {
    let mut text = String::new();
    let mut each = Vec::new();
    let mut truncated = false;
    for (name, body) in sources {
        let body = body.trim();
        if body.is_empty() {
            continue;
        }
        let header = format!("=== SOURCE: {name} ===\n");
        let footer = "\n\n";
        let remaining = most.saturating_sub(text.len());
        if remaining <= header.len() + 200 {
            truncated = true;
            each.push((name.clone(), 0, true));
            continue;
        }
        let (body, cut) = truncate(body, remaining - header.len() - footer.len());
        truncated |= cut;
        let piece = format!("{header}{body}{footer}");
        each.push((name.clone(), piece.len(), cut));
        text.push_str(&piece);
    }
    Packed { text, each, truncated }
}

/// `text` held to `most` bytes of UTF-8, cut at a character with `…[truncated]` after it (its
/// room within `most`), as Vzestup's `truncateUtf8` cuts it.
pub fn truncate(text: &str, most: usize) -> (String, bool) {
    if text.len() <= most {
        return (text.to_string(), false);
    }
    let marker = "\n\u{2026}[truncated]";
    let mut end = most.saturating_sub(marker.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (format!("{}{marker}", &text[..end]), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_pack_as_vzestup_packs_them() {
        let p = pack(&[("brief.md".into(), "# Plan\nShip it.".into()), ("empty.txt".into(), "  ".into())], 1000);
        assert_eq!(p.text, "=== SOURCE: brief.md ===\n# Plan\nShip it.\n\n");
        assert!(!p.truncated);
        let long = "é".repeat(1000);
        let p = pack(&[("a.txt".into(), long.clone()), ("b.txt".into(), "late".into())], 600);
        assert!(p.truncated && p.text.len() <= 600, "{}", p.text.len());
        assert!(p.text.contains("\u{2026}[truncated]"));
        assert_eq!(p.each[1], ("b.txt".to_string(), 0, true));
        assert_eq!(truncate("abc", 10), ("abc".to_string(), false));
    }

    #[test]
    fn each_kind_reads_by_its_extension() {
        let md = read("notes.md", b"\xef\xbb\xbf# Notes\nOne").unwrap();
        assert_eq!((md.kind, md.text.as_str()), (Kind::Markdown, "# Notes\nOne"));
        let csv = read("q3.csv", b"region,revenue\nGulf,4.1\n").unwrap();
        assert_eq!(csv.sheets[0].rows, vec![vec!["region", "revenue"], vec!["Gulf", "4.1"]]);
        assert!(matches!(read("deck.ppt", b"x"), Err(SourceError::Legacy(_, ".pptx"))));
        assert_eq!(read("locked.docx", &OLE), Err(SourceError::Protected));
        assert!(read("doc.pdf", b"not a pdf").is_err());
        assert_eq!(read("doc.pdf", b"%PDF-1.7 ...").unwrap().pdf.as_deref(), Some(&b"%PDF-1.7 ..."[..]));
        assert!(matches!(read("x.bin", &[0xff, 0x00, 0xfe, 0x80]), Err(SourceError::Kind(_))));
        let utf16: Vec<u8> = [0xff, 0xfe].into_iter().chain("Hi".encode_utf16().flat_map(u16::to_le_bytes)).collect();
        assert_eq!(read("u.txt", &utf16).unwrap().text, "Hi");
    }
}
