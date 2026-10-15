//! The generic scanner, for a language Scaena has no scanner of its own for: it knows what most
//! languages write alike, and colors nothing else.
//!
//! - **Strings** in `"`, `'`, and backticks, a backslash escaping the character after it. Each
//!   ends at its line's end where it does not close sooner. A `'` just after a word is no quote:
//!   `don't`, and Haskell's `x'`.
//! - **Numbers**, from a digit, or a point before one.
//! - **Comments**: `//` and `/* */`, and `#`, `;`, and `--` where a word could start, after
//!   whitespace or at the text's start (`--` with no word or digit just after it, so `--x`
//!   stays code).
//! - **Brackets and punctuation**, and the other ASCII symbols as operators.
//!
//! Any other run of word characters is plain.

use super::Class;
use super::scan::{
    Spans, block_comment_end, is_space, is_word, is_word_start, line_end, number_end, space_end, string_end, word_end,
};

pub(super) fn scan(text: &str, out: &mut Spans) {
    let b = text.as_bytes();
    let end = b.len();
    let mut pos = 0;
    while pos < end {
        let c = b[pos];
        if is_space(c) {
            pos = space_end(b, pos, end);
            out.space(pos);
            continue;
        }
        let next = b.get(pos + 1).copied().unwrap_or(0);
        let starts_word = pos == 0 || is_space(b[pos - 1]);
        let after_word = pos > 0 && is_word(b[pos - 1]);
        let (to, class) = match c {
            b'/' if next == b'/' => (line_end(b, pos, end), Class::Comment),
            b'/' if next == b'*' => (block_comment_end(b, pos, end, false), Class::Comment),
            b'#' | b';' if starts_word => (line_end(b, pos, end), Class::Comment),
            b'-' if next == b'-' && starts_word && !b.get(pos + 2).is_some_and(|&c| is_word(c)) => {
                (line_end(b, pos, end), Class::Comment)
            }
            b'"' | b'`' => (string_end(b, pos, end), Class::String),
            b'\'' if !after_word => (string_end(b, pos, end), Class::String),
            b'0'..=b'9' => (number_end(b, pos, end, false), Class::Number),
            b'.' if next.is_ascii_digit() && !after_word => (number_end(b, pos, end, false), Class::Number),
            _ if is_word_start(c) => (word_end(b, pos, end), Class::Plain),
            b'(' | b')' | b'[' | b']' | b'{' | b'}' | b',' | b';' | b'.' | b':' => (pos + 1, Class::Punctuation),
            _ if c.is_ascii_punctuation() => (pos + 1, Class::Operator),
            _ => (pos + 1, Class::Plain),
        };
        out.push(to, class);
        pos = to;
    }
}
