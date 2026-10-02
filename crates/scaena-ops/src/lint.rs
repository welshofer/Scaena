//! Validate and lint (SPEC §7.4–§7.5): the bundle's validation, the document rules, and,
//! once nothing is an error, the layout rules in the deck's own format and each of its
//! `formats`, contrast painted by the CPU painter.
//!
//! Lint reads a deck and a [`View`] of its files, so it can judge a bundle as a change would
//! leave it (a re-theme, a patch, a data file attached, a bundle not yet created) before
//! anything is written.

use crate::{Bundle, Context, OpsError};
use scaena_core::validate::BundleFiles;
use scaena_core::{Deck, Finding, Severity};
use scaena_engine::Engine;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::theme::Theme;
use scaena_paint::Assets;
use scaena_paint::cpu::CpuPainter;
use scaena_store::Files;
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// A bundle's files as a change would leave them: `pending` added or replaced, the rest as
/// the bundle has them. Nothing is written.
pub struct View<'a> {
    pub base: &'a Files,
    pub pending: BTreeMap<String, Vec<u8>>,
}

impl<'a> View<'a> {
    /// The bundle's files as they are.
    pub fn of(b: &'a Bundle) -> View<'a> {
        View { base: &b.files, pending: BTreeMap::new() }
    }

    /// With `path` added or replaced.
    pub fn with(mut self, path: impl Into<String>, bytes: Vec<u8>) -> View<'a> {
        self.pending.insert(path.into(), bytes);
        self
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, OpsError> {
        match self.pending.get(path) {
            Some(bytes) => Ok(bytes.clone()),
            None => Ok(self.base.read(path)?),
        }
    }

    /// The JSON of the theme `deck` names: a file here, or inline.
    pub fn theme(&self, deck: &Deck) -> Result<Option<String>, OpsError> {
        Ok(match &deck.theme {
            Some(serde_json::Value::String(path)) => Some(String::from_utf8_lossy(&self.read(path)?).into_owned()),
            Some(inline @ serde_json::Value::Object(_)) => Some(inline.to_string()),
            _ => None,
        })
    }
}

impl BundleFiles for View<'_> {
    fn exists(&self, path: &str) -> bool {
        self.pending.contains_key(path) || self.base.exists(path)
    }

    fn read_text(&self, path: &str) -> Option<String> {
        String::from_utf8(self.read(path).ok()?).ok()
    }
}

/// What `validate` finds in the bundle at `path`, as it is on disk: its schemas, its
/// references, and each state resolved against its nodes' types (SPEC §7.1).
pub fn validate(path: &Path) -> Result<Vec<Finding>, OpsError> {
    let (deck, files) = scaena_store::open_unparsed(path).with_context(|| format!("opening {}", path.display()))?;
    scaena_core::validate::validate_bundle(&deck, &files).context("deck.json is not JSON")
}

/// What lint found, and whether the layout rules ran: they run once nothing above them is
/// an error.
#[derive(Debug, Clone)]
pub struct Linted {
    pub findings: Vec<Finding>,
    pub laid: bool,
}

/// What `lint` finds in a bundle.
pub fn lint(b: &Bundle) -> Result<Linted, OpsError> {
    lint_in(&b.deck, &View::of(b))
}

/// What `lint` finds in `deck` with the files `files`.
pub fn lint_in(deck: &Deck, files: &View) -> Result<Linted, OpsError> {
    let theme = files.theme(deck)?;
    let mut found = scaena_core::validate::validate_bundle(&deck.to_json()?, files)?;
    let model: Option<scaena_core::model::theme::Theme> = theme.as_deref().and_then(|t| serde_json::from_str(t).ok());
    found.extend(scaena_core::lint::check(deck, model.as_ref()));
    let laid = theme.is_some() && !found.iter().any(|f| f.severity == Severity::Error);
    if let (true, Some(text)) = (laid, &theme) {
        let theme = Theme::from_json(text)?;
        let mut assets = Assets::new();
        let mut engine = engine_in(deck, files, &theme, Some(&mut assets))?;
        let data = data_in(deck, files)?;
        let mut backdrop = scaena_paint::Backdrop { painter: CpuPainter::default(), assets: &assets };
        found.extend(scaena_engine::lint::lint(&mut engine, deck, &theme, &data, Some(&mut backdrop))?);
    }
    scaena_core::lint::sort(&mut found);
    Ok(Linted { findings: found, laid })
}

/// What `lint --fix` did: the findings whose fixes it applied, and what lint finds after.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Fixed {
    pub fixed: Vec<Finding>,
    pub findings: Vec<Finding>,
}

/// Apply every fix lint offers, each checked by laying its state out again with it and
/// never a change of content; write the deck canonically if any applied; lint again.
pub fn lint_fix(b: &Bundle) -> Result<Fixed, OpsError> {
    let found = lint(b)?.findings;
    let mut doc = serde_json::to_value(&b.deck)?;
    let mut fixed = Vec::new();
    for f in found.into_iter().filter(|f| f.fix.is_some()) {
        // Two findings can carry one fix (the same text in two states); it applies once.
        let patch = f.fix.as_deref().unwrap_or_default();
        let mut next = doc.clone();
        if scaena_core::patch::apply(&mut next, patch).is_ok() {
            doc = next;
            fixed.push(f);
        }
    }
    let deck = Deck::from_json(&doc.to_string()).context("the fixed deck")?;
    if !fixed.is_empty() {
        write_deck(b, &deck, BTreeMap::new())?;
    }
    let findings = lint_in(&deck, &View::of(b))?.findings;
    Ok(Fixed { fixed, findings })
}

/// Write `deck` canonically into the bundle, with `files` beside it.
pub fn write_deck(b: &Bundle, deck: &Deck, mut files: BTreeMap<String, Vec<u8>>) -> Result<(), OpsError> {
    files.insert(b.deck_file.clone(), (deck.to_json()? + "\n").into_bytes());
    b.write(&files).with_context(|| format!("writing {}", b.root.display()))
}

/// The bundle's data files, as the engine reads them.
pub fn data_files(b: &Bundle) -> Result<DataFiles, OpsError> {
    data_in(&b.deck, &View::of(b))
}

/// The files `deck`'s data sources name, from `files`, as the engine reads them.
pub fn data_in(deck: &Deck, files: &View) -> Result<DataFiles, OpsError> {
    let mut data = DataFiles::new();
    for source in deck.data.values() {
        if let serde_json::Value::String(path) = &source.source {
            data.insert(path.clone(), files.read(path)?);
        }
    }
    Ok(data)
}

/// An engine with the bundle's fonts and images registered, for what only layout knows;
/// and, given a store, the same fonts and images for a painter.
pub fn engine_with(b: &Bundle, theme: &Theme, store: Option<&mut Assets>) -> Result<Engine, OpsError> {
    engine_in(&b.deck, &View::of(b), theme, store)
}

/// An engine with the fonts and images `deck` names, from `files`.
pub fn engine_in(deck: &Deck, files: &View, theme: &Theme, mut store: Option<&mut Assets>) -> Result<Engine, OpsError> {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        let bytes = files.read(&font.file)?;
        if let Some(store) = store.as_deref_mut() {
            store.insert_font(&font.file, bytes.clone());
        }
        fonts.register(&font.file, bytes)?;
    }
    fonts.check_theme(theme)?;
    let mut images = BundleImages::new();
    for path in deck.image_files() {
        let bytes = files.read(&path)?;
        let info = images.register(&path, &bytes)?;
        if let Some(store) = store.as_deref_mut() {
            store.insert_image(&info.id, &bytes)?;
        }
    }
    Ok(Engine::new(fonts).with_images(images))
}

/// The number of findings that are errors.
pub fn errors(findings: &[Finding]) -> usize {
    findings.iter().filter(|f| f.severity == Severity::Error).count()
}
