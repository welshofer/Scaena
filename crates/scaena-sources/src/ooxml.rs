//! An Office Open XML package: a zip of XML parts, each part's relationships beside it in
//! `_rels/`, read as Vzestup's deck import reads one, by local names.

use crate::SourceError;
use crate::xml::{self, Element};
use std::io::{Cursor, Read};

/// What an Office file may unpack to, at most, as a bundle's zip is bounded (the 20261024 plan's
/// SEC-1, `scaena_store::UNZIPPED`, of which this crate, depending on none of ours, keeps a twin):
/// a part past its bound, or the parts read past theirs, is refused, never read on into memory.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    /// Parts, at most.
    pub entries: usize,
    /// Bytes in one part, at most.
    pub part: u64,
    /// Bytes in all the parts read, at most.
    pub total: u64,
}

/// The bounds every Office file is read within.
pub const BOUNDS: Bounds = Bounds { entries: 10_000, part: 64 << 20, total: 1 << 30 };

/// A relationship: what it is (its type's last word) and the part or the address it names.
#[derive(Debug, Clone, PartialEq)]
pub struct Rel {
    pub id: String,
    pub kind: String,
    pub target: String,
}

pub struct Package {
    zip: zip::ZipArchive<Cursor<Vec<u8>>>,
    most: Bounds,
    /// The bytes the parts read so far unpacked to.
    read: u64,
    /// Why the last part refused was refused: it unpacked past its bound, or the parts past theirs.
    refused: Option<String>,
}

impl Package {
    pub fn open(bytes: &[u8]) -> Result<Package, SourceError> {
        Package::within(bytes, BOUNDS)
    }

    /// The package `bytes` holds, read within `most`.
    pub fn within(bytes: &[u8], most: Bounds) -> Result<Package, SourceError> {
        let zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec()))
            .map_err(|e| SourceError::Unreadable(format!("it is not a zip of Office's parts: {e}")))?;
        if zip.len() > most.entries {
            return Err(SourceError::Unreadable(format!(
                "it holds {} parts, more than an Office file is read with ({})",
                zip.len(),
                most.entries
            )));
        }
        Ok(Package { zip, most, read: 0, refused: None })
    }

    /// The part at `path`, as text; none where the package has no such part, or where it unpacks
    /// past what it may (the part's bound, or what the parts read before it leave).
    pub fn text(&mut self, path: &str) -> Option<String> {
        let file = self.zip.by_name(path).ok()?;
        let room = self.most.part.min(self.most.total - self.read);
        let mut bytes = Vec::new();
        file.take(room + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 > room {
            self.refused = Some(match room < self.most.part {
                true => format!("its parts unpack past {} bytes, more than an Office file is read to", self.most.total),
                false => format!("its {path} unpacks past {} bytes, more than a part is read to", self.most.part),
            });
            return None;
        }
        self.read += bytes.len() as u64;
        Some(crate::decode(&bytes))
    }

    /// The part at `path`, as XML.
    pub fn xml(&mut self, path: &str) -> Option<Element> {
        xml::parse(&self.text(path)?).ok()
    }

    /// The part at `path`, as XML, which the file is read for: refused with `missing` where the
    /// package has no such part, and naming the part where it does not read.
    pub fn part(&mut self, path: &str, missing: &str) -> Result<Element, SourceError> {
        let text =
            self.text(path).ok_or_else(|| SourceError::Unreadable(self.refused.take().unwrap_or(missing.into())))?;
        xml::parse(&text).map_err(|e| SourceError::Unreadable(format!("its {path} does not read: {e}")))
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

    /// A package of `parts`, each a path and its bytes.
    fn package(parts: &[(&str, Vec<u8>)]) -> Vec<u8> {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (path, bytes) in parts {
            zip.start_file(*path, deflated).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    /// An Office file unpacks only within its bounds (the 20261024 plan's SEC-1, for sources): too
    /// many parts, a part that inflates past its bound, or parts read past theirs, refused.
    #[test]
    fn a_package_that_unpacks_past_its_bounds_is_refused() {
        let most = Bounds { entries: 3, part: 1000, total: 1500 };
        let many: Vec<(&str, Vec<u8>)> = ["a", "b", "c", "d"].iter().map(|n| (*n, vec![])).collect();
        let refused = Package::within(&package(&many), most).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(refused.contains("it holds 4 parts"), "{refused}");
        let bytes =
            package(&[("word/document.xml", vec![b' '; 1001]), ("a.xml", vec![b' '; 800]), ("b.xml", vec![b' '; 800])]);
        let mut pkg = Package::within(&bytes, most).unwrap();
        let refused = pkg.part("word/document.xml", "missing").unwrap_err().to_string();
        assert!(refused.contains("its word/document.xml unpacks past 1000 bytes"), "{refused}");
        assert_eq!(pkg.text("a.xml").map(|t| t.len()), Some(800));
        assert_eq!(pkg.text("b.xml"), None, "past the bytes in all");
        let refused = pkg.part("b.xml", "missing").unwrap_err().to_string();
        assert!(refused.contains("its parts unpack past 1500 bytes"), "{refused}");
    }

    #[test]
    fn a_target_resolves_from_its_parts_directory() {
        assert_eq!(resolve("ppt/slides", "../notesSlides/notesSlide1.xml"), "ppt/notesSlides/notesSlide1.xml");
        assert_eq!(resolve("ppt", "slides/slide1.xml"), "ppt/slides/slide1.xml");
        assert_eq!(resolve("ppt/slides", "/ppt/charts/chart1.xml"), "ppt/charts/chart1.xml");
    }
}
