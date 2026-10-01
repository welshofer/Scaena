//! Bundle-only font access (SPEC §3.1, §13.3).
//!
//! The render path sees exactly the fonts the bundle carries, so the same
//! bundle shapes to the same glyphs on every machine. System font discovery is
//! closed twice: `parley` and `fontique` are built without their `system`
//! feature (no platform backend is compiled in), and the collection here is
//! created with `system_fonts: false`, so cargo feature unification elsewhere
//! in a build cannot reopen it.

use fontique::{Collection, CollectionOptions, SourceCache};
use parley::FontContext;

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
}
