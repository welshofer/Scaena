//! Bundle-only font access (SPEC §3.1, §13.3).
//!
//! The render path sees exactly the fonts the bundle carries, so the same
//! bundle shapes to the same glyphs on every machine. System font discovery is
//! closed twice: `parley` and `fontique` are built without their `system`
//! feature (no platform backend is compiled in), and the collection here is
//! created with `system_fonts: false`, so cargo feature unification elsewhere
//! in a build cannot reopen it.
//!
//! The engine never reads files: callers hand [`BundleFonts`] the bytes of each
//! font in the bundle, keyed by its bundle id (its path inside the bundle).

use crate::EngineError;
use crate::theme::Theme;
use fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{FontContext, FontData};
use scaena_core::displaylist::FontRef;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A font context that can see only fonts registered from bundle bytes.
///
/// Never use `FontContext::new()` or `FontContext::default()` in the engine:
/// both discover system fonts when the platform backend is present.
pub fn bundle_font_context() -> FontContext {
    FontContext {
        collection: Collection::new(CollectionOptions { shared: false, system_fonts: false }),
        source_cache: SourceCache::default(),
    }
}

/// The bundle's fonts, registered into a bundle-only context, plus the map from
/// what parley hands back (a font blob and face index) to display-list font ids.
pub struct BundleFonts {
    pub(crate) cx: FontContext,
    /// Blob id → bundle font id. Blob ids are process-unique lookup keys and never
    /// reach a display list.
    by_blob: BTreeMap<u64, String>,
    /// Bundle font id → family names that file provides.
    families: BTreeMap<String, Vec<String>>,
}

impl Default for BundleFonts {
    fn default() -> Self {
        Self { cx: bundle_font_context(), by_blob: BTreeMap::new(), families: BTreeMap::new() }
    }
}

impl BundleFonts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one font file under its bundle id (for example `fonts/RobotoSerif-VF.ttf`)
    /// and return the family names it provides. Registering the same id twice is an error.
    pub fn register(&mut self, id: &str, bytes: Vec<u8>) -> Result<Vec<String>, EngineError> {
        if self.families.contains_key(id) {
            return Err(EngineError::Font(format!("{id}: registered twice")));
        }
        let blob = Blob::new(Arc::new(bytes));
        let blob_id = blob.id();
        let mut names: Vec<String> = Vec::new();
        for (family, _) in self.cx.collection.register_fonts(blob, None) {
            let name = self.cx.collection.family_name(family).unwrap_or_default().to_string();
            if !names.contains(&name) {
                names.push(name);
            }
        }
        if names.is_empty() {
            return Err(EngineError::Font(format!("{id}: not a font file")));
        }
        self.by_blob.insert(blob_id, id.to_string());
        self.families.insert(id.to_string(), names.clone());
        Ok(names)
    }

    /// Check that every family in `theme` has its file registered and that the file
    /// provides the family name the theme declares; a mismatch would silently fall
    /// through to the next font in the stack.
    pub fn check_theme(&self, theme: &Theme) -> Result<(), EngineError> {
        for (key, def) in theme.families()? {
            let provided = self
                .families
                .get(&def.file)
                .ok_or_else(|| EngineError::Font(format!("family `{key}`: {} is not in the bundle", def.file)))?;
            if !provided.contains(&def.family) {
                return Err(EngineError::Font(format!(
                    "family `{key}`: {} provides {provided:?}, not `{}`",
                    def.file, def.family
                )));
            }
        }
        Ok(())
    }

    /// The display-list reference for a font parley selected.
    pub fn font_ref(&self, font: &FontData) -> Result<FontRef, EngineError> {
        let id = self.by_blob.get(&font.data.id()).ok_or_else(|| {
            EngineError::Font("parley selected a font that is not in the bundle (fallback leaked)".into())
        })?;
        Ok(FontRef { id: id.clone(), index: font.index })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fontique::GenericFamily;

    #[test]
    fn bundle_context_sees_no_system_fonts() {
        let mut cx = bundle_font_context();
        let names: Vec<&str> = cx.collection.family_names().collect();
        assert!(names.is_empty(), "system fonts leaked into the render path: {names:?}");
        for generic in [
            GenericFamily::Serif,
            GenericFamily::SansSerif,
            GenericFamily::Monospace,
            GenericFamily::Cursive,
            GenericFamily::Fantasy,
            GenericFamily::SystemUi,
            GenericFamily::UiSerif,
            GenericFamily::UiSansSerif,
            GenericFamily::UiMonospace,
            GenericFamily::UiRounded,
            GenericFamily::Emoji,
            GenericFamily::Math,
            GenericFamily::FangSong,
        ] {
            assert_eq!(cx.collection.generic_families(generic).count(), 0, "{generic:?} resolves to a system font");
        }
    }

    #[test]
    fn rejects_non_fonts_and_double_registration() {
        let mut fonts = BundleFonts::new();
        assert!(fonts.register("fonts/x.ttf", b"not a font".to_vec()).is_err());
        let bytes = std::fs::read("../../tests/fixtures/torture.scaena/fonts/NotoSansHebrew-VF.ttf").unwrap();
        assert_eq!(fonts.register("fonts/he.ttf", bytes.clone()).unwrap(), ["Noto Sans Hebrew"]);
        assert!(fonts.register("fonts/he.ttf", bytes).is_err());
    }
}
