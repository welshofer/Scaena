//! Saving a bundle (SPEC §3.1, PLAN 1.4).

use crate::crdt::{DeckDoc, Edit};
use crate::subset::subset;
use crate::{Bundle, Files, HISTORY, StoreError};
use scaena_core::Deck;
use scaena_core::document::{Manifest, NodeType};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;

/// How [`Bundle::save`] writes.
#[derive(Debug, Clone)]
pub struct SaveOptions {
    /// Subset fonts to what the deck can draw (SPEC §3.1). Off keeps each font whole.
    pub subset_fonts: bool,
    /// When this save happens, in RFC 3339: the manifest's `modified`, and its `created`
    /// for a bundle saved the first time. The store reads no clock; its caller says.
    pub now: String,
    /// Start keeping history (`history/deck.loro`, SPEC §8) if the bundle keeps none. One
    /// that does keeps it either way.
    pub history: bool,
}

/// What a save wrote.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Saved {
    /// Files now named by their content: (old path, new path).
    pub renamed: Vec<(String, String)>,
    /// Fonts subset: (path, bytes before, bytes after).
    pub subset: Vec<(String, usize, usize)>,
    pub manifest: Manifest,
}

/// A save, done in memory: the files [`Bundle::save`] writes, for a caller that keeps them
/// itself, as a page does (PLAN 2.4).
#[derive(Debug, Clone)]
pub struct Saving {
    /// Every file of the saved bundle, by its path inside it.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Files of the bundle as it was that the save renamed or rewrote: one saved in place
    /// drops those `files` does not hold.
    pub replaced: BTreeSet<String>,
    pub saved: Saved,
}

/// Font files, by extension.
const FONT_EXTENSIONS: [&str; 4] = ["ttf", "otf", "woff", "woff2"];

impl Bundle {
    /// Write the bundle to `to` as SPEC §3.1 lays it out:
    /// - `deck.json` in canonical form, and the theme file the deck names in the same form;
    /// - each font the deck or its theme names, subset to what the deck can draw, at
    ///   `fonts/<family>-<hash>.<ext>`, and each image at `assets/<sha256>.<ext>`, with
    ///   every reference to them rewritten, a beat's evidence among them;
    /// - every other file of the bundle as it is (from a bare deck file, whose directory is
    ///   not a bundle of its own, only its data and the font licenses beside its fonts);
    /// - its history, with the save recorded in it, if it keeps one or `opts` starts one;
    /// - `manifest.json`.
    ///
    /// `to` is a directory, or a zip when it ends in `.scaena`. A directory must be absent,
    /// empty, or this bundle's own; saving in place removes the files it renamed.
    pub fn save(&self, to: &Path, opts: &SaveOptions) -> Result<Saved, StoreError> {
        let Saving { files, replaced, saved } = self.saving(opts)?;
        if to.extension().and_then(|e| e.to_str()) == Some("scaena") && !to.is_dir() {
            std::fs::write(to, zip(&files)?)?;
        } else {
            self.write_dir(to, &files, &replaced)?;
        }
        Ok(saved)
    }

    /// What [`Bundle::save`] writes, as files in memory.
    pub fn saving(&self, opts: &SaveOptions) -> Result<Saving, StoreError> {
        let subset = |font: &str, bytes: &[u8], chars: &BTreeSet<char>| {
            subset(bytes, chars).map_err(|e| StoreError::Subset(font.to_string(), e))
        };
        self.saving_with(opts, |deck| self.recorded(deck, opts.history), subset)
    }

    /// What a save subsets (SPEC §3.1): every character the deck can draw, and each font
    /// file, by its path, that it keeps those of. A caller that subsets elsewhere, as a page
    /// does in a module of its own (PLAN 2.4), subsets these and hands them to
    /// [`Bundle::saving_with`].
    pub fn subsetting(&self) -> Result<(BTreeSet<char>, Vec<String>), StoreError> {
        let theme: Option<Value> = match &self.deck.theme {
            Some(Value::String(path)) => Some(serde_json::from_slice(&self.read(path)?)?),
            other => other.clone(),
        };
        let fonts = font_files(&self.deck, theme.as_ref()).into_iter().map(|(file, _, _)| file).collect();
        Ok((self.drawable_chars()?, fonts))
    }

    /// The bundle's history with `deck` saved in it, by the bundle's author; begun if
    /// `begin`, where the bundle keeps none.
    fn recorded(&self, deck: &Deck, begin: bool) -> Result<Option<Vec<u8>>, StoreError> {
        let edit = Edit { message: Some("save"), ..Edit::by(&self.author) };
        match self.record(deck, &BTreeMap::new(), &edit)? {
            Some(bytes) => Ok(Some(bytes)),
            None if begin => {
                // The deck, and the data it is drawn from (ADR-0014).
                let begun = Edit { message: Some("history begins"), ..Edit::by(&self.author) };
                Ok(Some(DeckDoc::begin(deck, &self.data_files(deck), &begun)?.save()?))
            }
            None => Ok(None),
        }
    }

    /// [`Bundle::saving`], with the history the save writes made by `history` from the deck
    /// as saved, and each font subset by `subset` (its path, its bytes, and the characters
    /// to keep) where `opts` subsets them.
    ///
    /// `history` gives the history's bytes, or `None` to carry the bundle's history as it
    /// is. A caller that keeps no CRDT, as a page does (PLAN 2.4), carries it: the next save
    /// that records takes in the deck as a change by `fs`, as it does a `deck.json` edited by
    /// hand (SPEC §8).
    pub fn saving_with(
        &self,
        opts: &SaveOptions,
        history: impl FnOnce(&Deck) -> Result<Option<Vec<u8>>, StoreError>,
        mut subset: impl FnMut(&str, &[u8], &BTreeSet<char>) -> Result<Vec<u8>, StoreError>,
    ) -> Result<Saving, StoreError> {
        let mut deck = self.deck.clone();
        let mut theme: Option<(String, Value)> = match &deck.theme {
            Some(Value::String(path)) => Some((path.clone(), serde_json::from_slice(&self.read(path)?)?)),
            _ => None,
        };
        let chars = self.drawable_chars()?;
        let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut replaced: BTreeSet<String> = BTreeSet::from([self.deck_file.clone(), "manifest.json".into()]);
        let mut renamed = Vec::new();
        let mut subsets = Vec::new();

        // Fonts: the deck's, then any other its theme's families name.
        let fonts = font_files(&deck, theme.as_ref().map(|(_, v)| v).or(deck.theme.as_ref()));
        let mut names: BTreeMap<String, String> = BTreeMap::new();
        for (old, family, italic) in fonts {
            let bytes = self.read(&old)?;
            let bytes_out = if opts.subset_fonts {
                let smaller = subset(&old, &bytes, &chars)?;
                subsets.push((old.clone(), bytes.len(), smaller.len()));
                smaller
            } else {
                bytes
            };
            // `Inter-…` for the family's own face, `Inter-Italic-…` for its italic (PLAN 2.40).
            let face = if italic { "-Italic" } else { "" };
            let hash = &sha256(&bytes_out)[..16];
            let new = format!("fonts/{}{face}-{hash}.{}", slug(&family), extension(&old).unwrap_or("ttf"));
            replaced.insert(old.clone());
            out.insert(new.clone(), bytes_out);
            names.insert(old, new);
        }
        for font in &mut deck.fonts {
            if let Some(new) = names.get(&font.file) {
                font.file = new.clone();
            }
        }
        if let Some(Value::Object(_)) = deck.theme {
            rename_theme_fonts(deck.theme.as_mut(), &names);
        }
        if let Some((_, value)) = &mut theme {
            rename_theme_fonts(Some(value), &names);
        }

        // Images, by their content.
        let mut sources: Vec<&mut Value> = Vec::new();
        let images: BTreeSet<String> =
            deck.nodes.iter().filter(|(_, n)| n.node_type == NodeType::Image).map(|(id, _)| id.clone()).collect();
        for (_, node) in deck.nodes.iter_mut().filter(|(id, _)| images.contains(*id)) {
            sources.extend(node.props.get_mut("src"));
        }
        for state in &mut deck.states {
            for (_, delta) in state.props.iter_mut().filter(|(id, _)| images.contains(*id)) {
                sources.extend(delta.get_mut("src"));
            }
        }
        for src in sources {
            let Value::String(old) = src else { continue };
            let new = match names.get(old.as_str()) {
                Some(new) => new.clone(),
                None => {
                    let bytes = self.read(old)?;
                    let new = format!("assets/{}.{}", sha256(&bytes), extension(old).unwrap_or("bin"));
                    replaced.insert(old.clone());
                    out.insert(new.clone(), bytes);
                    names.insert(old.clone(), new.clone());
                    new
                }
            };
            *old = new;
        }
        // A beat cites a file by its path, so it follows the file.
        for beat in deck.spine.iter_mut().flat_map(|s| &mut s.sections).flat_map(|s| &mut s.beats) {
            for cited in &mut beat.evidence {
                if let Some(new) = names.get(cited.as_str()) {
                    cited.clone_from(new);
                }
            }
        }
        renamed.extend(names.iter().filter(|(old, new)| old != new).map(|(o, n)| (o.clone(), n.clone())));

        // The history, with this save in it: files renamed are a change to the deck.
        if let Some(bytes) = history(&deck)? {
            replaced.insert(HISTORY.into());
            out.insert(HISTORY.into(), bytes);
        }

        // The theme file, and everything else as it is.
        if let Some((path, value)) = &theme {
            out.insert(path.clone(), (serde_json::to_string_pretty(value)? + "\n").into_bytes());
            replaced.insert(path.clone());
        }
        for rel in self.carried()? {
            if !replaced.contains(&rel) && !out.contains_key(&rel) {
                out.insert(rel.clone(), self.read(&rel)?);
            }
        }

        let deck_json = (deck.to_json()? + "\n").into_bytes();
        let created = self
            .read("manifest.json")
            .ok()
            .and_then(|m| serde_json::from_slice::<Manifest>(&m).ok())
            .map_or_else(|| opts.now.clone(), |m| m.created);
        let manifest = Manifest {
            scaena: scaena_core::FORMAT_VERSION.into(),
            deck: sha256(&deck_json),
            files: out.iter().map(|(path, bytes)| (path.clone(), sha256(bytes))).collect(),
            created,
            modified: opts.now.clone(),
        };
        out.insert("deck.json".into(), deck_json);
        out.insert("manifest.json".into(), (serde_json::to_string_pretty(&manifest)? + "\n").into_bytes());
        Ok(Saving { files: out, replaced, saved: Saved { renamed, subset: subsets, manifest } })
    }

    /// Every character the deck can draw: every string in it, and every character of its
    /// data files.
    fn drawable_chars(&self) -> Result<BTreeSet<char>, StoreError> {
        let mut chars = BTreeSet::new();
        strings(&serde_json::to_value(&self.deck)?, &mut chars);
        for (_, bytes) in self.read_data()? {
            chars.extend(String::from_utf8_lossy(&bytes).chars());
        }
        chars.retain(|c| !c.is_control());
        Ok(chars)
    }

    /// The files a save carries over as they are: all of a bundle's, or, from a bare deck
    /// file, its data files and the licenses beside its fonts.
    fn carried(&self) -> Result<Vec<String>, StoreError> {
        let all = self.files.list()?;
        if self.deck_file == "deck.json" {
            return Ok(all);
        }
        let data: Vec<String> = self.read_data()?.into_iter().map(|(path, _)| path).collect();
        let licenses = all
            .iter()
            .filter(|p| p.starts_with("fonts/") && extension(p).is_none_or(|e| !FONT_EXTENSIONS.contains(&e)));
        Ok(data.into_iter().chain(licenses.cloned()).collect())
    }

    fn write_dir(
        &self,
        to: &Path,
        out: &BTreeMap<String, Vec<u8>>,
        replaced: &BTreeSet<String>,
    ) -> Result<(), StoreError> {
        let in_place = matches!(&self.files, Files::Dir(root) if same_dir(root, to)) && self.deck_file == "deck.json";
        if to.exists() && !in_place && std::fs::read_dir(to)?.next().is_some() {
            return Err(StoreError::Occupied(to.to_path_buf()));
        }
        for (rel, bytes) in out {
            let path = to.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, bytes)?;
        }
        if in_place {
            for rel in replaced.iter().filter(|rel| !out.contains_key(*rel)) {
                std::fs::remove_file(to.join(rel))?;
            }
        }
        Ok(())
    }
}

impl Bundle {
    /// Write `files` (bundle path → bytes) into the bundle where it is, each replacing what
    /// is there: in its directory, or into its zip, which is rewritten. Nothing else changes;
    /// a save ([`Bundle::save`]) is what puts a bundle in canonical form.
    pub fn write(&self, files: &BTreeMap<String, Vec<u8>>) -> Result<(), StoreError> {
        match &self.files {
            Files::Dir(root) => {
                for (rel, bytes) in files {
                    let path = crate::inside(root, rel)?;
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(path, bytes)?;
                }
                Ok(())
            }
            Files::Zip(entries) => {
                let mut all = (**entries).clone();
                for (rel, bytes) in files {
                    all.insert(crate::normal(rel)?, bytes.clone());
                }
                std::fs::write(&self.root, zip(&all)?)?;
                Ok(())
            }
        }
    }
}

impl Bundle {
    /// Take `paths` out of the bundle (PLAN 2.59): out of its directory, or its zip written
    /// again without them; and out of its manifest's list of files, where it keeps one, so the
    /// manifest names no file the bundle lacks.
    pub fn remove(&self, paths: &[String]) -> Result<(), StoreError> {
        let manifest = match self.files.read("manifest.json") {
            Ok(bytes) => {
                let mut manifest: Manifest = serde_json::from_slice(&bytes)?;
                manifest.files.retain(|path, _| !paths.contains(path));
                Some((serde_json::to_string_pretty(&manifest)? + "\n").into_bytes())
            }
            Err(_) => None,
        };
        match &self.files {
            Files::Dir(root) => {
                for rel in paths {
                    std::fs::remove_file(crate::inside(root, rel)?)?;
                }
                if let Some(bytes) = manifest {
                    std::fs::write(crate::inside(root, "manifest.json")?, bytes)?;
                }
                Ok(())
            }
            Files::Zip(entries) => {
                let mut all = (**entries).clone();
                for rel in paths {
                    all.remove(&crate::normal(rel)?);
                }
                if let Some(bytes) = manifest {
                    all.insert("manifest.json".into(), bytes);
                }
                std::fs::write(&self.root, zip(&all)?)?;
                Ok(())
            }
        }
    }
}

/// `files` as a `.scaena` zip's bytes: sorted by path, each deflated and dated 1980-01-01,
/// so the same bundle zips to the same bytes.
pub fn zip(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, StoreError> {
    let zip_error = |source| StoreError::Zip { path: "deck.scaena".into(), source };
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    for (rel, bytes) in files {
        zip.start_file(rel.as_str(), options).map_err(zip_error)?;
        zip.write_all(bytes)?;
    }
    Ok(zip.finish().map_err(zip_error)?.into_inner())
}

/// The font files a save writes, as (file, family name, whether it is the family's italic):
/// the deck's, then any other its theme's families name.
fn font_files(deck: &Deck, theme: Option<&Value>) -> Vec<(String, String, bool)> {
    let mut fonts: Vec<(String, String, bool)> =
        (deck.fonts.iter()).map(|f| (f.file.clone(), f.family.clone(), f.style.as_deref() == Some("italic"))).collect();
    for (file, family, italic) in theme_families(theme) {
        if !fonts.iter().any(|(f, _, _)| *f == file) {
            fonts.push((file, family, italic));
        }
    }
    fonts
}

/// The (file, family name, whether it is the italic) of each family in a theme, and of its
/// italic face (PLAN 2.40), in theme order.
fn theme_families(theme: Option<&Value>) -> Vec<(String, String, bool)> {
    let families = theme.and_then(|t| t.get("type")).and_then(|t| t.get("families")).and_then(Value::as_object);
    let mut out = Vec::new();
    for family in families.into_iter().flat_map(|f| f.values()) {
        let Some(name) = family.get("family").and_then(Value::as_str) else { continue };
        for (face, italic) in [(Some(family), false), (family.get("italic"), true)] {
            if let Some(file) = face.and_then(|f| f.get("file")).and_then(Value::as_str) {
                out.push((file.to_string(), name.to_string(), italic));
            }
        }
    }
    out
}

/// A theme's family files, and its families' italic faces', renamed by `names`.
fn rename_theme_fonts(theme: Option<&mut Value>, names: &BTreeMap<String, String>) {
    let rename = |face: &mut Value| {
        if let Some(Value::String(file)) = face.get_mut("file")
            && let Some(new) = names.get(file.as_str())
        {
            *file = new.clone();
        }
    };
    let families =
        theme.and_then(|t| t.get_mut("type")).and_then(|t| t.get_mut("families")).and_then(Value::as_object_mut);
    for family in families.into_iter().flat_map(|f| f.values_mut()) {
        if let Some(italic) = family.get_mut("italic") {
            rename(italic);
        }
        rename(family);
    }
}

/// Every character of every string in `value`.
fn strings(value: &Value, chars: &mut BTreeSet<char>) {
    match value {
        Value::String(s) => chars.extend(s.chars()),
        Value::Array(items) => items.iter().for_each(|v| strings(v, chars)),
        Value::Object(map) => map.values().for_each(|v| strings(v, chars)),
        _ => {}
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Where a file added to a bundle goes, by what it is (PLAN 2.4): a font under `fonts/`
/// and a data file under `data/`, by its own name; anything else, an image above all,
/// under `assets/`, named by its content as a save names it.
pub fn place(name: &str, bytes: &[u8]) -> String {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match extension(file).map(str::to_ascii_lowercase).as_deref() {
        Some(ext) if FONT_EXTENSIONS.contains(&ext) => format!("fonts/{file}"),
        Some("csv" | "tsv" | "json") => format!("data/{file}"),
        ext => format!("assets/{}.{}", sha256(bytes), ext.unwrap_or("bin")),
    }
}

/// A family name as a file name: its letters and digits.
fn slug(family: &str) -> String {
    let slug: String = family.chars().filter(char::is_ascii_alphanumeric).collect();
    if slug.is_empty() { "font".into() } else { slug }
}

fn extension(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next()?;
    name.rsplit_once('.').map(|(_, ext)| ext).filter(|e| !e.is_empty())
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
