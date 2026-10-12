//! An Office Open XML package: a zip of XML parts, each part's relationships beside it in
//! `_rels/`, read as Vzestup's deck import reads one, by local names.

use crate::SourceError;
use crate::xml::{self, Element};
use std::io::{Cursor, Read};

/// The most a part may unpack to: a zip that unpacks past it is not read further.
const PART_LIMIT: u64 = 64 << 20;

/// A relationship: what it is (its type's last word) and the part or the address it names.
#[derive(Debug, Clone, PartialEq)]
pub struct Rel {
    pub id: String,
    pub kind: String,
    pub target: String,
}

pub struct Package {
    zip: zip::ZipArchive<Cursor<Vec<u8>>>,
}

impl Package {
    pub fn open(bytes: &[u8]) -> Result<Package, SourceError> {
        let zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec()))
            .map_err(|e| SourceError::Unreadable(format!("it is not a zip of Office's parts: {e}")))?;
        Ok(Package { zip })
    }

    /// The part at `path`, as text; none where the package has no such part.
    pub fn text(&mut self, path: &str) -> Option<String> {
        let file = self.zip.by_name(path).ok()?;
        let mut bytes = Vec::new();
        file.take(PART_LIMIT).read_to_end(&mut bytes).ok()?;
        Some(crate::decode(&bytes))
    }

    /// The part at `path`, as XML.
    pub fn xml(&mut self, path: &str) -> Option<Element> {
        xml::parse(&self.text(path)?).ok()
    }

    /// The relationships of the part at `path`, each target a path in the package (or an
    /// address, where it is external).
    pub fn rels(&mut self, path: &str) -> Vec<Rel> {
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
        let rels_path = if dir.is_empty() { format!("_rels/{name}.rels") } else { format!("{dir}/_rels/{name}.rels") };
        let Some(doc) = self.xml(&rels_path) else { return Vec::new() };
        doc.all("Relationship")
            .filter_map(|r| {
                let target = r.attr("Target")?;
                let external = r.attr("TargetMode") == Some("External");
                Some(Rel {
                    id: r.attr("Id")?.to_string(),
                    kind: r.attr("Type").unwrap_or_default().rsplit('/').next().unwrap_or_default().to_string(),
                    target: if external { target.to_string() } else { resolve(dir, target) },
                })
            })
            .collect()
    }

    /// The document's title, from its core properties.
    pub fn title(&mut self) -> Option<String> {
        let core = self.xml("docProps/core.xml")?;
        Some(core.child("title")?.text().trim().to_string()).filter(|t| !t.is_empty())
    }
}

/// What Office file `bytes` is, by the part it holds: `docx`, `xlsx`, or `pptx`.
pub fn kind(bytes: &[u8]) -> Option<&'static str> {
    let zip = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let has = |part: &str| zip.file_names().any(|n| n == part);
    [("word/document.xml", "docx"), ("xl/workbook.xml", "xlsx"), ("ppt/presentation.xml", "pptx")]
        .into_iter()
        .find_map(|(part, ext)| has(part).then_some(ext))
}

/// `target`, a relationship's target, as a path in the package from the part in `dir`.
pub fn resolve(dir: &str, target: &str) -> String {
    if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_string();
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            seg => parts.push(seg),
        }
    }
    parts.join("/")
}

/// The words of each paragraph (`a:p`) of a text body, its runs and fields in order.
pub fn paragraphs(body: &Element) -> Vec<String> {
    body.all("p")
        .map(|p| {
            let mut line = String::new();
            for e in p.elements() {
                match e.local() {
                    "r" | "fld" => line.push_str(&e.child("t").map(|t| t.text()).unwrap_or_default()),
                    "br" => line.push('\n'),
                    _ => {}
                }
            }
            line
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_resolves_from_its_parts_directory() {
        assert_eq!(resolve("ppt/slides", "../notesSlides/notesSlide1.xml"), "ppt/notesSlides/notesSlide1.xml");
        assert_eq!(resolve("ppt", "slides/slide1.xml"), "ppt/slides/slide1.xml");
        assert_eq!(resolve("ppt/slides", "/ppt/charts/chart1.xml"), "ppt/charts/chart1.xml");
    }
}
