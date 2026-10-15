//! Code on a slide, set as written (SPEC §3.5, PLAN 2.131, ADR-0029). A text with `code` keeps
//! its lines as it breaks them, and nothing else breaks, hangs, hyphenates, cases, or measures
//! them. Its tabs stand to stops, and it runs left to right whatever its first strong character
//! is, so an Arabic comment on its first line does not turn the block. It grows its box first,
//! then shrinks to fit, worked out from one layout, never under 12 pt at presentation size by
//! itself (lint W205 where a `minSize` lets it). It draws in its own color: coloring it by its
//! language is PLAN 2.132's.

use scaena_core::document::Props;
use serde_json::Value;

/// The columns between a code text's tab stops, as most editors set them.
pub const TAB: usize = 4;

/// What runs a code text left to right: LEFT-TO-RIGHT MARK, a strong character that sets no ink,
/// laid out before its words, so the block takes its direction and not its first word's.
pub const LTR: char = '\u{200E}';

/// The size code is set no smaller than by itself, at presentation size, canvas units: 12 pt on
/// a 13.33 × 7.5 in slide, 24 cu where the canvas's shorter side is 1080 cu, as W312 holds a
/// chart's text.
pub const PRESENTATION_SIZE: f32 = 24.0;

/// A code text's settings, as the cascade reads them.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeSpec {
    /// The language it is written in, as the deck names it.
    pub language: String,
    /// The columns between its tab stops.
    pub tab: usize,
}

impl CodeSpec {
    /// The settings `props` make a code text with, if they do: a `code` object, its language
    /// `plain` where it names none (E106).
    pub fn of(props: &Props) -> Option<CodeSpec> {
        let code = props.get("code")?.as_object()?;
        let language = code.get("language").and_then(Value::as_str).unwrap_or("plain");
        Some(CodeSpec { language: language.to_string(), tab: TAB })
    }
}

/// Whether `props` make a text code.
pub fn is_code(props: &Props) -> bool {
    props.get("code").is_some_and(Value::is_object)
}

/// The size code is set no smaller than by itself on a canvas of `canvas`, canvas units:
/// [`PRESENTATION_SIZE`] in proportion to its shorter side.
pub fn floor(canvas: [f32; 2]) -> f32 {
    PRESENTATION_SIZE * canvas[0].min(canvas[1]) / 1080.0
}

/// `texts`, a code text's spans as written, as it lays them out: [`LTR`] before them, and each
/// tab stood to the next stop in spaces, its column counted in characters from its line's start,
/// across the spans. Each span's part, and where each character as written starts in what is
/// laid out, then both ends ([`crate::text::TextLayout::offsets`]), so a caret steps over a tab
/// whole. A text with no words lays out none: no mark either.
pub(crate) fn laid(texts: &[&str], tab: usize) -> (Vec<String>, Vec<(usize, usize)>) {
    let tab = tab.max(1);
    let empty = texts.iter().all(|t| t.is_empty());
    let mut out = Vec::with_capacity(texts.len());
    let mut offsets = Vec::with_capacity(texts.iter().map(|t| t.len()).sum::<usize>() + 1);
    let (mut written, mut at, mut column) = (0, 0, 0);
    for (k, text) in texts.iter().enumerate() {
        let mut part = String::with_capacity(text.len() + 4);
        if k == 0 && !empty {
            part.push(LTR);
            at += LTR.len_utf8();
        }
        for c in text.chars() {
            offsets.push((written, at));
            written += c.len_utf8();
            if c == '\t' {
                let stop = tab - column % tab;
                part.extend(std::iter::repeat_n(' ', stop));
                (at, column) = (at + stop, column + stop);
            } else {
                part.push(c);
                at += c.len_utf8();
                column = if crate::carets::breaks(c) { 0 } else { column + 1 };
            }
        }
        out.push(part);
    }
    offsets.push((written, at));
    (out, offsets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_stands_to_the_next_stop_and_maps_back_whole() {
        let (set, offsets) = laid(&["\tx = 1;\nab\tc"], 4);
        assert_eq!(set, ["\u{200E}    x = 1;\nab  c"]);
        // The first line's tab after the mark, the `x` past its stop, the second line's start,
        // its tab two columns in, and the `c` past it.
        assert_eq!(offsets[0], (0, 3));
        assert_eq!(offsets[1], (1, 7));
        assert_eq!(offsets[8], (8, 14));
        assert_eq!(offsets[10], (10, 16));
        assert_eq!(offsets[11], (11, 18));
        assert_eq!(offsets.last(), Some(&(12, set[0].len())));
    }

    #[test]
    fn spans_share_their_columns_and_an_empty_text_lays_out_nothing() {
        let (set, _) = laid(&["ab", "\tc"], 4);
        assert_eq!(set, ["\u{200E}ab", "  c"]);
        assert_eq!(laid(&[""], 4), (vec![String::new()], vec![(0, 0)]));
    }

    #[test]
    fn the_floor_is_twelve_points_at_presentation_size() {
        assert_eq!(floor([1920.0, 1080.0]), 24.0);
        assert_eq!(floor([1080.0, 1920.0]), 24.0);
        assert_eq!(floor([3840.0, 2160.0]), 48.0);
    }
}
