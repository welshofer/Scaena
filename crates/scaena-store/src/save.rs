//! Saving a bundle (SPEC §3.1, PLAN 1.4).

use crate::subset::subset;
use crate::{Bundle, Files, StoreError};
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

/// Font files, by extension.
const FONT_EXTENSIONS: [&str; 4] = ["ttf", "otf", "woff", "woff2"];

impl Bundle {
    /// Write the bundle to `to` as SPEC §3.1 lays it out:
    /// - `deck.json` in canonical form, and the theme file the deck names in the same form;
    /// - each font the deck or its theme names, subset to what the deck can draw, at
    ///   `fonts/<family>-<hash>.<ext>`, and each image at `assets/<sha256>.<ext>`, with
    ///   every reference to them rewritten;
    /// - every other file of the bundle as it is (from a bare deck file, whose directory is
    ///   not a bundle of its own, only its data and the font licenses beside its fonts);
    /// - `manifest.json`.
    ///
    /// `to` is a directory, or a zip when it ends in `.scaena`. A directory must be absent,
    /// empty, or this bundle's own; saving in place removes the files it renamed.
    pub fn save(&self, to: &Path, opts: &SaveOptions) -> Result<Saved, StoreError> {
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
        let mut fonts: Vec<(String, String)> = deck.fonts.iter().map(|f| (f.file.clone(), f.family.clone())).collect();
        let theme_value = theme.as_ref().map(|(_, v)| v).or(deck.theme.as_ref());
        for (file, family) in theme_families(theme_value) {
            if !fonts.iter().any(|(f, _)| *f == file) {
                fonts.push((file, family));
            }
        }
        let mut names: BTreeMap<String, String> = BTreeMap::new();
        for (old, family) in fonts {
            let bytes = self.read(&old)?;
            let bytes_out = if opts.subset_fonts {
                let smaller = subset(&bytes, &chars).map_err(|e| StoreError::Subset(old.clone(), e))?;
                subsets.push((old.clone(), bytes.len(), smaller.len()));
                smaller
            } else {
                bytes
            };
            let new =
                format!("fonts/{}-{}.{}", slug(&family), &sha256(&bytes_out)[..16], extension(&old).unwrap_or("ttf"));
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
        renamed.extend(names.iter().filter(|(old, new)| old != new).map(|(o, n)| (o.clone(), n.clone())));

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

        if to.extension().and_then(|e| e.to_str()) == Some("scaena") && !to.is_dir() {
            write_zip(to, &out)?;
        } else {
            self.write_dir(to, &out, &replaced)?;
        }
        Ok(Saved { renamed, subset: subsets, manifest })
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

/// A zip of `files`, sorted by path, each deflated and dated 1980-01-01, so the same
/// bundle zips to the same bytes.
fn write_zip(to: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<(), StoreError> {
    let zip_error = |source| StoreError::Zip { path: to.to_path_buf(), source };
    let mut zip = zip::ZipWriter::new(std::fs::File::create(to)?);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    for (rel, bytes) in files {
        zip.start_file(rel.as_str(), options).map_err(zip_error)?;
        zip.write_all(bytes)?;
    }
    zip.finish().map_err(zip_error)?;
    Ok(())
}

/// The (file, family name) of each family in a theme, in theme order.
fn theme_families(theme: Option<&Value>) -> Vec<(String, String)> {
    let families = theme.and_then(|t| t.get("type")).and_then(|t| t.get("families")).and_then(Value::as_object);
    families
        .into_iter()
        .flatten()
        .filter_map(|(_, f)| Some((f.get("file")?.as_str()?.to_string(), f.get("family")?.as_str()?.to_string())))
        .collect()
}

/// A theme's family files, renamed by `names`.
fn rename_theme_fonts(theme: Option<&mut Value>, names: &BTreeMap<String, String>) {
    let families =
        theme.and_then(|t| t.get_mut("type")).and_then(|t| t.get_mut("families")).and_then(Value::as_object_mut);
    for family in families.into_iter().flat_map(|f| f.values_mut()) {
        if let Some(Value::String(file)) = family.get_mut("file")
            && let Some(new) = names.get(file.as_str())
        {
            *file = new.clone();
        }
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
