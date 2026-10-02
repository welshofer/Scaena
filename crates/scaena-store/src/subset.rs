//! Font subsetting at save (SPEC §3.1, PLAN 1.4). A bundle font keeps the glyphs its deck
//! can draw and everything shaping needs for them: layout tables, variations, hinting, and
//! names. Each glyph keeps its id, so a saved bundle draws every frame it drew before.

use skera::{Plan, SubsetFlags, subset_font};
use std::collections::BTreeSet;
use write_fonts::read::FontRef;
use write_fonts::read::collections::IntSet;
use write_fonts::read::types::{GlyphId, NameId, Tag};

/// Characters every subset keeps besides the deck's own, so an edit in a Latin script
/// does not need the original font back: ASCII, Latin-1, Latin Extended-A, general
/// punctuation, and the euro sign. The same ranges the repository's font scripts keep.
pub const BASE_RANGES: [(u32, u32); 4] = [(0x20, 0x7E), (0xA0, 0x17F), (0x2010, 0x203A), (0x20AC, 0x20AC)];

/// Tables hb-subset drops by default that shaping can still read: legacy kerning and AAT
/// layout. A font that has them keeps them.
const SHAPING_TABLES: [&[u8; 4]; 4] = [b"kern", b"kerx", b"morx", b"mort"];

#[derive(Debug, thiserror::Error)]
pub enum SubsetError {
    #[error("not a font: {0}")]
    Read(String),
    #[error("subsetting failed: {0}")]
    Subset(String),
}

/// `font` with the glyphs for `chars` (and [`BASE_RANGES`]), at their old glyph ids.
pub fn subset(font: &[u8], chars: &BTreeSet<char>) -> Result<Vec<u8>, SubsetError> {
    let font = FontRef::new(font).map_err(|e| SubsetError::Read(e.to_string()))?;
    let mut unicodes = IntSet::<u32>::empty();
    unicodes.extend(chars.iter().map(|&c| u32::from(c)));
    for (lo, hi) in BASE_RANGES {
        unicodes.insert_range(lo..=hi);
    }
    let flags = SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
        | SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE
        | SubsetFlags::SUBSET_FLAGS_GLYPH_NAMES
        | SubsetFlags::SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES;
    let mut drop_tables = IntSet::<Tag>::empty();
    drop_tables.extend(
        skera::DEFAULT_DROP_TABLES.iter().copied().filter(|t| !SHAPING_TABLES.iter().any(|s| Tag::new(s) == *t)),
    );
    let plan = Plan::new(
        &IntSet::<GlyphId>::empty(),
        &unicodes,
        &font,
        flags,
        &drop_tables,
        &IntSet::<Tag>::all(),
        &IntSet::<Tag>::all(),
        &IntSet::<NameId>::all(),
        &IntSet::<u16>::all(),
    );
    subset_font(&font, &plan).map_err(|e| SubsetError::Subset(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::read::TableProvider;

    const FONTS: &str = "../../tests/fixtures/torture.scaena/fonts";

    #[test]
    fn every_torture_font_subsets_keeping_layout_variations_and_glyph_ids() {
        let chars: BTreeSet<char> = "Office ﬁnancial — “quoted” العربية עברית Ǆ 🇺🇸".chars().collect();
        for entry in std::fs::read_dir(FONTS).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("ttf") {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let out = subset(&bytes, &chars).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let before = FontRef::new(&bytes).unwrap();
            let after = FontRef::new(&out).unwrap();
            for tag in [b"GSUB", b"GPOS", b"fvar", b"gvar", b"COLR", b"CPAL"] {
                let tag = Tag::new(tag);
                assert_eq!(
                    before.table_data(tag).is_some(),
                    after.table_data(tag).is_some(),
                    "{}: {tag}",
                    path.display()
                );
            }
            assert_eq!(
                before.maxp().unwrap().num_glyphs(),
                after.maxp().unwrap().num_glyphs(),
                "{}: glyph ids",
                path.display()
            );
            assert!(out.len() <= bytes.len(), "{}: {} → {}", path.display(), bytes.len(), out.len());
        }
    }
}
