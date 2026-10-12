//! Font subsetting at save (SPEC §3.1, PLAN 1.4). A bundle font keeps the glyphs its deck
//! can draw and everything shaping needs for them: layout tables, variations, hinting, and
//! names. Each glyph keeps its id, so a saved bundle draws every frame it drew before.

use skera::{Plan, SubsetFlags, subset_font};
use std::collections::BTreeSet;
use write_fonts::read::FontRef;
use write_fonts::read::collections::IntSet;
use write_fonts::read::types::{GlyphId, NameId, Tag};

/// Characters every subset keeps besides the deck's own, so an edit in a Latin script
/// does not need the original font back: ASCII, Latin-1, Latin Extended-A, the letters
/// Romanian writes (ș, ț), and U+2000–27BF, what a talk writes beside its words:
/// punctuation, super- and subscripts (CO₂), currency, letterlike symbols, number forms,
/// arrows, mathematical operators (−, ≈, ≤), technical symbols, enclosed alphanumerics,
/// shapes, symbols, and dingbats (PLAN G.21). The ranges the shipped themes' fonts keep
/// (`scripts/build_bundle_fonts.py`).
pub const BASE_RANGES: [(u32, u32); 4] = [(0x20, 0x7E), (0xA0, 0x17F), (0x218, 0x21B), (0x2000, 0x27BF)];

/// Scripts a subset keeps whole where the deck writes in them, so an edit in them does not
/// need the original font back either: Greek and Cyrillic, which the shipped themes' fonts
/// carry whole (PLAN G.21); and Arabic and Devanagari, which a deck in Arabic or Hindi takes
/// a font for (PLAN G.22).
pub const SCRIPT_RANGES: [(u32, u32); 6] =
    [(0x370, 0x3FF), (0x400, 0x4FF), (0x600, 0x6FF), (0x750, 0x77F), (0x900, 0x97F), (0xA8E0, 0xA8FF)];

/// What a subset keeps of a font for a script the deck writes in, beside the deck's own
/// characters.
#[derive(Debug, Clone, Copy)]
pub struct ScriptSet {
    /// The code points that say the deck writes in the script.
    pub says: &'static [(u32, u32)],
    /// The ranges kept whole.
    pub whole: &'static [(u32, u32)],
    /// The characters kept, those of them the font has.
    pub chars: &'static [&'static str],
}

/// What a subset keeps of a CJK font where the deck writes in its script: the script's
/// punctuation, kana, and full-width forms whole, and the characters its national standard
/// puts first ([`crate::common`]), so a deck in Japanese, Chinese, or Korean can be edited in
/// it without the original font (PLAN G.22, ADR-0027).
pub const SCRIPT_SETS: [ScriptSet; 2] = [
    // Han and kana: Japanese or Chinese, whose first levels a font keeps those of it has.
    ScriptSet {
        says: &[(0x3040, 0x30FF), (0x3400, 0x4DBF), (0x4E00, 0x9FFF)],
        whole: &[(0x3000, 0x30FF), (0x31F0, 0x31FF), (0xFF00, 0xFFEF)],
        chars: &[crate::common::KANJI, crate::common::HANZI],
    },
    // Hangul.
    ScriptSet {
        says: &[(0x1100, 0x11FF), (0x3130, 0x318F), (0xAC00, 0xD7A3)],
        whole: &[(0x3000, 0x303F), (0x3130, 0x318F), (0xFF00, 0xFFEF)],
        chars: &[crate::common::HANGUL],
    },
];

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

/// The code points a subset for a deck that draws `chars` keeps: `chars`, [`BASE_RANGES`],
/// each of [`SCRIPT_RANGES`] that `chars` writes in, and what [`SCRIPT_SETS`] keeps for each
/// script it writes in.
pub fn kept(chars: &BTreeSet<char>) -> IntSet<u32> {
    let mut unicodes = IntSet::<u32>::empty();
    unicodes.extend(chars.iter().map(|&c| u32::from(c)));
    let written = |&(lo, hi): &(u32, u32)| chars.iter().any(|&c| (lo..=hi).contains(&u32::from(c)));
    for (lo, hi) in BASE_RANGES.into_iter().chain(SCRIPT_RANGES.into_iter().filter(written)) {
        unicodes.insert_range(lo..=hi);
    }
    for set in SCRIPT_SETS {
        if set.says.iter().any(written) {
            set.whole.iter().for_each(|&(lo, hi)| unicodes.insert_range(lo..=hi));
            unicodes.extend(set.chars.iter().flat_map(|chars| chars.chars()).map(u32::from));
        }
    }
    unicodes
}

/// `font` with the glyphs for what [`kept`] keeps for `chars`, at their old glyph ids.
pub fn subset(font: &[u8], chars: &BTreeSet<char>) -> Result<Vec<u8>, SubsetError> {
    let font = FontRef::new(font).map_err(|e| SubsetError::Read(e.to_string()))?;
    let unicodes = kept(chars);
    let flags = SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
        | SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE
        | SubsetFlags::SUBSET_FLAGS_GLYPH_NAMES
        | SubsetFlags::SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES;
    let mut drop_tables = IntSet::<Tag>::empty();
    drop_tables.extend(
        skera::DEFAULT_DROP_TABLES.iter().copied().filter(|t| !SHAPING_TABLES.iter().any(|s| Tag::new(s) == *t)),
    );
    // skera unwraps much of what it reads, so a damaged font can panic it. Off the web a
    // panic unwinds, and the save says it could not subset the font; in the browser the
    // subsetter is a module of its own, whose page says the same (`web/src/worker.ts`).
    let subset = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
        subset_font(&font, &plan)
    }));
    match subset {
        Ok(subset) => subset.map_err(|e| SubsetError::Subset(e.to_string())),
        Err(panic) => {
            let why = (panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            Err(SubsetError::Subset(format!("the subsetter stopped on it ({why}): the font file may be damaged")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::read::TableProvider;

    const FONTS: &str = "../../tests/fixtures/torture.scaena/fonts";

    /// skera unwraps much of what it reads, so a font damaged where it reads stops it: the
    /// subset is an error that says so, not a panic (PLAN 2.25).
    #[test]
    fn a_damaged_font_does_not_subset() {
        let mut font = std::fs::read(format!("{FONTS}/EBGaramond-VF.ttf")).unwrap();
        // Its `maxp` table one byte long, by its record.
        let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
        let record = (0..tables).map(|i| 12 + 16 * i).find(|&r| &font[r..r + 4] == b"maxp").unwrap();
        font[record + 12..record + 16].copy_from_slice(&1u32.to_be_bytes());
        let chars: BTreeSet<char> = "Office".chars().collect();
        let error = subset(&font, &chars).expect_err("no subset of a damaged font").to_string();
        assert!(error.contains("the subsetter stopped on it") && error.contains("damaged"), "{error}");
    }

    /// A save keeps what a talk writes beside its words, so a deck saved can still be given
    /// a CO₂, a minus, or an arrow; and Greek or Cyrillic whole where the deck writes in it,
    /// so a deck in Ukrainian can be edited in Ukrainian (PLAN G.21).
    #[test]
    fn a_subset_keeps_a_talks_symbols_and_the_scripts_its_deck_writes_in() {
        let inter = std::fs::read("../../docs/examples/fonts/Inter-VF.ttf").unwrap();
        let has = |font: &[u8], c: char| FontRef::new(font).unwrap().cmap().unwrap().map_codepoint(c).is_some();
        let english = subset(&inter, &"Climate".chars().collect()).unwrap();
        for c in ['\u{2082}', '\u{2212}', '\u{2192}', '\u{2248}', '\u{2264}', '\u{2153}', '\u{2122}', '\u{20b9}', 'ș']
        {
            assert!(has(&inter, c) && has(&english, c), "U+{:04X}", c as u32);
        }
        assert!(!has(&english, 'ї') && !has(&english, 'λ'), "a deck in English keeps no Cyrillic or Greek");
        let ukrainian = subset(&inter, &"Клімат".chars().collect()).unwrap();
        assert!(has(&ukrainian, 'ї') && has(&ukrainian, 'Ґ') && has(&ukrainian, 'є'), "Cyrillic kept whole");
        assert!(!has(&ukrainian, 'λ'), "and not Greek");
        let greek = subset(&inter, &"Δ".chars().collect()).unwrap();
        assert!(has(&greek, 'λ') && has(&greek, 'Ω') && !has(&greek, 'ї'));
    }

    /// A deck in Japanese, Chinese, or Korean keeps its script's punctuation, kana, and the
    /// characters its standard puts first, and one in Arabic or Hindi its script whole; a deck
    /// in English keeps none of them (PLAN G.22).
    #[test]
    fn a_subset_keeps_the_characters_a_deck_in_its_script_writes_most() {
        let keeps = |text: &str, c: char| kept(&text.chars().collect()).contains(u32::from(c));
        let japanese = "気候変動の現状";
        for c in ['の', 'ア', '、', '「', '一', '働', 'Ａ'] {
            assert!(keeps(japanese, c), "{c} for a deck in Japanese");
        }
        assert!(keeps("气候变化", '们') && keeps("气候变化", '这'), "hanzi for a deck in Chinese");
        assert!(keeps("기후 변화", '한') && keeps("기후 변화", '국') && keeps("기후 변화", 'ㄱ'));
        assert!(!keeps("기후 변화", '一'), "a deck in Korean keeps no kanji");
        assert!(keeps("تغير المناخ", 'ي') && keeps("तापमान", 'ह'), "Arabic and Devanagari whole");
        for c in ['の', '一', '한', 'ي', 'ह', 'λ', 'ї'] {
            assert!(!keeps("Climate", c), "{c} for a deck in English");
        }
        assert_eq!(crate::common::KANJI.chars().count(), 2965);
        assert_eq!(crate::common::HANZI.chars().count(), 3755);
        assert_eq!(crate::common::HANGUL.chars().count(), 2350);
    }

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
