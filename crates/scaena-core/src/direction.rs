//! Which way a deck reads (ADR-0031, call 8; PLAN T.5a): a deck whose language runs right to
//! left is laid out mirrored, in every theme, as it is laid out. Its slots, cells, and boxes
//! stand from the right edge; what is drawn inside a box (a photograph, a logo, a chart's
//! figures, a shape's outline) is not mirrored, as Apple's guidelines keep photographs, logos,
//! and digits.

use crate::Deck;

/// The languages written right to left, by their primary subtag (ISO 639): Arabic, Aramaic,
/// Sorani Kurdish, Divehi, Persian, Hebrew (and its old code), Kashmiri, Pashto, Sindhi, Syriac,
/// Uyghur, Urdu, and Yiddish (and its old code).
const RIGHT_TO_LEFT: [&str; 15] =
    ["ar", "arc", "ckb", "dv", "fa", "he", "iw", "ks", "ps", "sd", "syr", "ug", "ur", "yi", "ji"];

/// The scripts written right to left (ISO 15924), which a tag's script subtag names: Arabic,
/// Hebrew, Thaana, Syriac, N'Ko, Adlam, and Rohingya's Hanifi.
const SCRIPTS: [&str; 7] = ["arab", "hebr", "thaa", "syrc", "nkoo", "adlm", "rohg"];

/// Whether a language, by its BCP 47 tag, runs right to left: by its script subtag where it
/// names one (`az-Arab` does, `ku-Latn` does not), else by its language.
pub fn right_to_left(tag: &str) -> bool {
    let mut parts = tag.split(['-', '_']).filter(|p| !p.is_empty());
    let Some(language) = parts.next() else { return false };
    // A script subtag is four letters, after the language (and any extended language subtags).
    if let Some(script) = parts.find(|p| p.len() == 4 && p.chars().all(|c| c.is_ascii_alphabetic())) {
        return SCRIPTS.contains(&script.to_ascii_lowercase().as_str());
    }
    RIGHT_TO_LEFT.contains(&language.to_ascii_lowercase().as_str())
}

/// Whether `deck` is laid out mirrored: its language (`meta.lang`) runs right to left.
pub fn mirrored(deck: &Deck) -> bool {
    deck.meta.as_ref().and_then(|m| m.lang.as_deref()).is_some_and(right_to_left)
}

/// A shape's points, as fractions of its box, mirrored across it: what a slide started in a deck
/// that reads right to left writes for a theme's shape, so a chevron or an arrow points on, the
/// way the deck reads, and is edited as it is drawn.
pub fn flipped(points: &[crate::model::values::Point]) -> Vec<crate::model::values::Point> {
    points.iter().map(|&[x, y]| [1.0 - x, y]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arabic_hebrew_and_persian_run_right_to_left_and_latin_does_not() {
        for tag in ["ar", "ar-EG", "he", "he-IL", "fa-IR", "ur", "yi", "az-Arab", "pa-Arab-PK", "AR"] {
            assert!(right_to_left(tag), "{tag}");
        }
        for tag in ["en", "en-US", "de", "ja", "hi", "ku-Latn", "uz-Cyrl", "", "-"] {
            assert!(!right_to_left(tag), "{tag}");
        }
    }
}
