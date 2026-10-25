//! Code on a slide, set as written (SPEC §3.5, PLAN 2.131, ADR-0029). A text with `code` keeps
//! its lines as it breaks them, a CRLF once, and nothing else breaks, hangs, hyphenates, cases, or
//! measures them. Its tabs stand to stops, and it runs left to right whatever its first strong
//! character is, so an Arabic comment on its first line does not turn the block. It grows its box
//! first, then shrinks to fit, worked out from one layout, never under 12 pt at presentation size
//! by itself (lint W205 where it is drawn smaller).
//!
//! It is colored by its language in the theme's colors (PLAN 2.132): `scaena_core::code` reads its
//! words into tokens of 16 kinds as it is laid out, and each token is drawn in its kind's look
//! ([`TokenLook`]), which `Theme::code_looks` gives, every color held to read on the ground it is
//! drawn on. The deck stores no token and no color. A change of color shapes nothing apart; a
//! change of weight or italic shapes each token apart from its neighbors, as any change of face
//! does.

use crate::EngineError;
use scaena_core::code::{Class, Lang, highlight, lang, tables};
use scaena_core::document::Props;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::ops::Range;

/// The columns between a code text's tab stops, as most editors set them.
pub const TAB: usize = 4;

/// What runs a code text left to right: LEFT-TO-RIGHT MARK, a strong character that sets no ink,
/// laid out before its words, so the block takes its direction and not its first word's.
pub const LTR: char = '\u{200E}';

/// The size code is set no smaller than by itself, at presentation size, canvas units: 12 pt on
/// a 13.33 × 7.5 in slide, 24 cu where the canvas's shorter side is 1080 cu, as W312 holds a
/// chart's text.
pub const PRESENTATION_SIZE: f32 = 24.0;

/// How one class of a code text's tokens is drawn (ADR-0029): its color, opaque, its weight, and
/// whether it is set in its family's italic face, which it is only where the family has one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TokenLook {
    pub color: [u8; 4],
    pub weight: f32,
    pub italic: bool,
}

/// How each class of a code text's tokens is drawn, by `Class as usize`: plain words first, then
/// the 16 kinds in `Class::KINDS`' order.
pub type Looks = [TokenLook; 17];

/// The room between a code card's edges and the code, canvas units, where the theme says none.
pub const INSET: f32 = 32.0;

/// How far a code card stands off the surface where the theme names it no fill: `surface-2`,
/// stepped by the least lightness to read so on a projector (ADR-0029).
pub const CARD_STEP: f64 = 1.3;

/// The room between a code text's line numbers and its code, in ems of its size, where the theme
/// says none.
pub const GAP: f32 = 1.0;

/// The card a code text stands on (PLAN 2.136, ADR-0029), as the theme draws it: its fill, opaque,
/// which is the ground its colors are held to, and its corners, inset, outline, and shadow, as a
/// rect shape's are written.
#[derive(Debug, Clone, PartialEq)]
pub struct CardLook {
    pub fill: [u8; 4],
    pub radius: Option<Value>,
    pub inset: f32,
    pub stroke: Option<String>,
    pub shadow: Option<String>,
}

/// A code text's line numbers (PLAN 2.136): the number its first line goes by, their color, held
/// to read on its ground, the room between them and the code, in ems of its size, and the color
/// of those whose lines are out of focus (PLAN 2.140).
#[derive(Debug, Clone, PartialEq)]
pub struct Numbers {
    pub start: u32,
    pub color: [u8; 4],
    pub gap: f32,
    /// The color of a line's number where its line is out of focus (PLAN 2.140): `color` set
    /// back as its line's tokens are. None where no line is.
    pub dimmed: Option<[u8; 4]>,
}

/// What a code text's `code` says of its card and numbers: whether it stands on a card (on where
/// unset), and the number its first line goes by where its lines are numbered.
pub fn framed(props: &Props) -> (bool, Option<u32>) {
    let code = props.get("code");
    let card = code.and_then(|c| c.get("card")).and_then(Value::as_bool).unwrap_or(true);
    let numbered = code.and_then(|c| c.get("numbers")).and_then(Value::as_bool).unwrap_or(false);
    let start = code.and_then(|c| c.get("start")).and_then(Value::as_u64).map_or(1, |n| n.clamp(1, 999_999) as u32);
    (card, numbered.then_some(start))
}

/// A code text's settings, as the cascade reads them.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeSpec {
    /// The language it is written in, as the deck names it.
    pub language: String,
    /// The columns between its tab stops: the theme's `code.tab`, else [`TAB`].
    pub tab: usize,
    /// The least it shrinks to by itself, a share of its own size: its `minSize`, else its
    /// role's, else 12 pt at presentation size ([`floor`]), and never more than its own.
    pub least: f32,
    /// How each class of its tokens is drawn ([`Looks`]): none draws every token as its plain
    /// words.
    pub looks: Option<Box<Looks>>,
    /// The card it stands on, where it stands on one.
    pub card: Option<CardLook>,
    /// Its line numbers, where its lines are numbered.
    pub numbers: Option<Numbers>,
    /// The lines in focus (PLAN 2.140), counted from 0, each the first and the last of a run, in
    /// the order written; none where every line is.
    pub focus: Option<Vec<(usize, usize)>>,
    /// How each class of its tokens is drawn on the lines out of focus: [`CodeSpec::looks`] on
    /// its ground, each color set back toward it, never under 3:1. None where none is set back.
    pub dimmed: Option<Box<Looks>>,
    /// The color of the band behind its lines in focus, opaque, where the theme draws one and a
    /// focus names lines.
    pub band: Option<[u8; 4]>,
}

impl CodeSpec {
    /// The settings `props` make a code text with, if they do, set at `size` in a role whose
    /// own `minSize` is `min`, on a canvas of `canvas`: a `code` object, its language `plain`
    /// where it names none (E106). Its tokens are drawn as its plain words until it is given
    /// [`CodeSpec::looks`].
    pub fn of(props: &Props, size: f32, min: Option<f32>, canvas: [f32; 2]) -> Option<CodeSpec> {
        let code = props.get("code")?.as_object()?;
        let language = code.get("language").and_then(Value::as_str).unwrap_or("plain");
        let own = props.get("minSize").and_then(Value::as_f64).map(|m| m as f32);
        let least = own.or(min).unwrap_or(floor(canvas)) / size;
        Some(CodeSpec {
            language: language.to_string(),
            tab: TAB,
            least: least.min(1.0),
            looks: None,
            card: None,
            numbers: None,
            focus: focus(code.get("focus")),
            dimmed: None,
            band: None,
        })
    }

    /// Whether its line `line`, counted from 0, is in focus: every line is where it names none.
    pub fn in_focus(&self, line: usize) -> bool {
        self.focus.as_ref().is_none_or(|spans| in_spans(spans, line))
    }

    /// The runs of `written`, its words as written, that its tokens draw in another look than its
    /// plain words (PLAN 2.132), each by its range in the text as laid out, which `offsets` maps
    /// to ([`laid`]), with that look; neighbors in one look are one run. None where it has no
    /// looks, or its language colors nothing. An error where the tables its language is colored
    /// by are neither compiled in nor handed over ([`set_loader`]).
    pub(crate) fn runs(
        &self,
        written: &str,
        offsets: &[(usize, usize)],
    ) -> Result<Vec<(Range<usize>, TokenLook)>, EngineError> {
        let Some(looks) = self.looks.as_deref() else { return Ok(Vec::new()) };
        let lang = lang(&self.language);
        tables_for(&self.language, lang)?;
        let plain = looks[Class::Plain as usize];
        // Where a character as written starts in the text as laid out: a token ends on one.
        let at = |w: usize| offsets.get(offsets.partition_point(|o| o.0 < w)).map_or(usize::MAX, |o| o.1);
        // Where each line starts as written, for the looks of the lines out of focus (PLAN 2.140):
        // a token is cut where a line breaks inside it, a block comment's lines each in its own.
        let starts = match (&self.focus, &self.dimmed) {
            (Some(_), Some(_)) => line_starts(written),
            _ => vec![0],
        };
        let mut out: Vec<(Range<usize>, TokenLook)> = Vec::new();
        let mut start = 0;
        for token in highlight(written, lang) {
            let from = start;
            start = token.end;
            let first = starts.partition_point(|&s| s <= from).saturating_sub(1);
            let mut cut = from;
            for line in first..starts.len() {
                let end = starts.get(line + 1).map_or(token.end, |&next| next.min(token.end));
                if cut >= token.end {
                    break;
                }
                let look = match self.dimmed.as_deref() {
                    Some(dimmed) if !self.in_focus(line) => dimmed[token.class as usize],
                    _ => looks[token.class as usize],
                };
                let range = at(cut)..at(end);
                cut = end;
                if look == plain || range.is_empty() || range.end == usize::MAX {
                    continue;
                }
                match out.last_mut() {
                    Some((last, was)) if last.end == range.start && *was == look => last.end = range.end,
                    _ => out.push((range, look)),
                }
            }
        }
        Ok(out)
    }
}

/// A code text's `code.focus` (PLAN 2.140) as runs of lines counted from 0: each span, one line
/// or the first and the last, in the order written. A span that names no line (0) or runs
/// backward (E106) is passed over; none where it names none, or none of it reads.
fn focus(value: Option<&Value>) -> Option<Vec<(usize, usize)>> {
    let line = |v: &Value| v.as_u64().filter(|&n| n >= 1).map(|n| n as usize - 1);
    let spans: Vec<(usize, usize)> = (value?.as_array()?.iter())
        .filter_map(|span| match span {
            Value::Array(pair) if pair.len() == 2 => Some((line(&pair[0])?, line(&pair[1])?)),
            one => line(one).map(|n| (n, n)),
        })
        .filter(|(a, b)| a <= b)
        .collect();
    (!spans.is_empty()).then_some(spans)
}

/// Whether `line` falls in one of `spans`.
pub(crate) fn in_spans(spans: &[(usize, usize)], line: usize) -> bool {
    spans.iter().any(|&(a, b)| (a..=b).contains(&line))
}

/// Where each of `written`'s lines starts, as written: at 0, and after each character that
/// breaks a line, a CRLF once, as [`laid`] breaks them and the deck counts them.
fn line_starts(written: &str) -> Vec<usize> {
    scaena_core::lists::paragraphs(written).into_iter().map(|r| r.start).collect()
}

/// The SHA-256 of each file of the tables the scanners read (`scaena_core::code::tables`, in
/// `crates/scaena-core/code/`), in hex: what one handed over is held to.
pub const TABLES: [(&str, &str); 11] = [
    ("clike", "8c87ffd411a530756888b9512a8ab397f81afbf22fd2fb1655ec92ddaa652463"),
    ("css", "085c66feb5ee53507e0875db5d35e71bc9d6785ad36bc377e3f696141ff096ca"),
    ("dockerfile", "1a3268dea51c84dfe3f27782000e86b8e74d6bee771226f9cfc2e02b47837dce"),
    ("graphql", "97ce256e18c0e12d03e1a63c372a3c675c7952c9ee27a1b7afbf28883d1a17d2"),
    ("hcl", "4b9e6ce7ec481b512edda75f4755396de6b55d31a4c236390510d8347c5cb786"),
    ("makefile", "006ece179f50923b03aff10dcad14c3ba51b2105cc34ae338c1fd6b3b1063d72"),
    ("powershell", "6085c69b72f0e2389e2ff5bcada40cb1c9e1b1c1b552244bbe995150c970f9b3"),
    ("python", "cd7e95eccc8f20e6367e16430a8cee43a0e17e033126ee0baffdf17e03c020a5"),
    ("ruby", "eb18c5ac0f40a503c7798e9e705f99fd4cedc440bf1b2f675f0fdbc50b307847"),
    ("shell", "587ca758689c91b4d1871a1e329801b92a2e8e41b624b4f270a8b2d303d99abe"),
    ("sql", "a6619ac5929e5a861a2a3c69dd9285edee06d4673de3eb0b113cfd0ac86ebe0d"),
];

/// What gives a file of tables a build leaves out: given its name (`clike`), its bytes.
type Loader = Box<dyn Fn(&str) -> Option<Vec<u8>>>;

thread_local! {
    static LOADER: RefCell<Option<Loader>> = RefCell::default();
}

/// Where this thread's engines get a file of the tables the scanners read that is not compiled in
/// (PLAN 2.132, as ADR-0015 hands a language's hyphenation patterns over): `loader`, given its name
/// (`clike`), gives the bytes of `crates/scaena-core/code/clike.txt`, or none. It is asked the first
/// time a code text in a language that reads the file lays out, and what it gives is held to the
/// file's SHA-256. Native builds and the player's module have every table compiled in, and never
/// ask.
pub fn set_loader(loader: impl Fn(&str) -> Option<Vec<u8>> + 'static) {
    LOADER.with(|l| *l.borrow_mut() = Some(Box::new(loader)));
}

/// `bytes` as file `name` of tables, as text: refused unless they are its file, byte for byte.
fn checked(name: &str, bytes: Vec<u8>) -> Result<String, EngineError> {
    let digest: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if !TABLES.iter().any(|(file, d)| *file == name && *d == digest) {
        return Err(EngineError::Code(format!(
            "`{name}`: what was handed over ({} bytes) is not the engine's tables for it",
            bytes.len()
        )));
    }
    String::from_utf8(bytes).map_err(|_| EngineError::Code(format!("`{name}`: its tables are not UTF-8")))
}

/// Every table `lang`, which a code text names `language`, is colored by: here, as compiled in or
/// handed over before, or handed over now. A file that cannot be had is an error that says so, never
/// a text colored by less than its tables.
fn tables_for(language: &str, lang: Lang) -> Result<(), EngineError> {
    for &file in tables::files(lang) {
        if tables::has(file) {
            continue;
        }
        let bytes = LOADER.with(|l| l.borrow().as_ref().and_then(|load| load(file))).ok_or_else(|| {
            EngineError::Code(format!("the tables `{language}` is colored by (`{file}`) were not handed over"))
        })?;
        tables::hand(file, &checked(file, bytes)?).map_err(EngineError::Code)?;
    }
    Ok(())
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

    /// Each file of tables is held to its digest, and the engine knows every file the scanners
    /// read (PLAN 2.132): what the editor's module is handed is what it would have compiled in.
    #[test]
    fn each_file_of_tables_is_held_to_its_digest() {
        let names: Vec<&str> = TABLES.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, tables::FILES);
        for (name, _) in TABLES {
            let bytes = std::fs::read(format!("../scaena-core/code/{name}.txt")).unwrap();
            let text = checked(name, bytes.clone()).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(text, tables::written(name).unwrap());
            let mut wrong = bytes;
            wrong.push(b'\n');
            assert!(checked(name, wrong).unwrap_err().to_string().contains("is not the engine's tables"));
        }
        // A language whose tables are compiled in needs none handed over.
        assert!(tables_for("rust", lang("rust")).is_ok());
    }

    #[test]
    fn the_floor_is_twelve_points_at_presentation_size() {
        assert_eq!(floor([1920.0, 1080.0]), 24.0);
        assert_eq!(floor([1080.0, 1920.0]), 24.0);
        assert_eq!(floor([3840.0, 2160.0]), 48.0);
    }
}
