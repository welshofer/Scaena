//! Code on a slide, set as written (SPEC §3.5, PLAN 2.131, ADR-0029). A text with `code` keeps
//! its lines as it breaks them, a CRLF once, and nothing else breaks, hangs, hyphenates, cases, or
//! measures them. Its tabs stand to stops, and it runs left to right whatever its first strong
//! character is, so an Arabic comment on its first line does not turn the block. It grows its box
//! first, then shrinks to fit, worked out from one layout, never under 12 pt at presentation size
//! by itself (lint W205 where it is drawn smaller). It draws in its own color: coloring it by its
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
    /// The least it shrinks to by itself, a share of its own size: its `minSize`, else its
    /// role's, else 12 pt at presentation size ([`floor`]), and never more than its own.
    pub least: f32,
}

impl CodeSpec {
    /// The settings `props` make a code text with, if they do, set at `size` in a role whose
    /// own `minSize` is `min`, on a canvas of `canvas`: a `code` object, its language `plain`
    /// where it names none (E106).
    pub fn of(props: &Props, size: f32, min: Option<f32>, canvas: [f32; 2]) -> Option<CodeSpec> {
        let code = props.get("code")?.as_object()?;
        let language = code.get("language").and_then(Value::as_str).unwrap_or("plain");
        let own = props.get("minSize").and_then(Value::as_f64).map(|m| m as f32);
        let least = own.or(min).unwrap_or(floor(canvas)) / size;
        Some(CodeSpec { language: language.to_string(), tab: TAB, least: least.min(1.0) })
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

/// `texts`, a code text's spans as written, as it lays them out: [`LTR`] before them, each tab
/// stood to the next stop in spaces, its column counted in characters from its line's start,
/// across the spans, and each CRLF one line break, as a code editor reads it: its CR lays out
/// nothing. Each span's part, and where each character as written starts in what is laid out,
/// then both ends ([`crate::text::TextLayout::offsets`]), so a caret steps over a tab whole. A
/// text with no words lays out none: no mark either.
pub(crate) fn laid(texts: &[&str], tab: usize) -> (Vec<String>, Vec<(usize, usize)>) {
    let tab = tab.max(1);
    let mut out: Vec<String> = texts.iter().map(|t| String::with_capacity(t.len() + 4)).collect();
    let mut offsets = Vec::with_capacity(texts.iter().map(|t| t.len()).sum::<usize>() + 1);
    let (mut written, mut at, mut column) = (0, 0, 0);
    if texts.iter().any(|t| !t.is_empty()) {
        out[0].push(LTR);
        at += LTR.len_utf8();
    }
    let mut chars = texts.iter().enumerate().flat_map(|(k, t)| t.chars().map(move |c| (k, c))).peekable();
    while let Some((k, c)) = chars.next() {
        offsets.push((written, at));
        written += c.len_utf8();
        match c {
            '\t' => {
                let stop = tab - column % tab;
                out[k].extend(std::iter::repeat_n(' ', stop));
                (at, column) = (at + stop, column + stop);
            }
            // The CR of a CRLF lays out nothing: its LF breaks the line, once.
            '\r' if chars.peek().is_some_and(|&(_, next)| next == '\n') => column = 0,
            _ => {
                out[k].push(c);
                at += c.len_utf8();
                column = if crate::carets::breaks(c) { 0 } else { column + 1 };
            }
        }
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
        assert_eq!(laid(&[], 4), (Vec::<String>::new(), vec![(0, 0)]));
    }

    #[test]
    fn a_crlf_is_one_line_break() {
        // Its CR lays out nothing, and maps to where its LF stands; a CR alone breaks as it is.
        let (set, offsets) = laid(&["a\r\n\tb\rc"], 4);
        assert_eq!(set, ["\u{200E}a\n    b\rc"]);
        assert_eq!(&offsets[1..4], [(1, 4), (2, 4), (3, 5)]);
        // Across spans too.
        let (set, offsets) = laid(&["a\r", "\nb"], 4);
        assert_eq!(set, ["\u{200E}a", "\nb"]);
        assert_eq!(&offsets[1..3], [(1, 4), (2, 4)]);
    }

    #[test]
    fn the_floor_is_twelve_points_at_presentation_size() {
        assert_eq!(floor([1920.0, 1080.0]), 24.0);
        assert_eq!(floor([1080.0, 1920.0]), 24.0);
        assert_eq!(floor([3840.0, 2160.0]), 48.0);
    }
}
