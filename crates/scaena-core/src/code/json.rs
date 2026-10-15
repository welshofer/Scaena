//! JSON, and JSON with comments and JSON5 as leniently: one scanner reads all three.
//!
//! - A string, in either quote, is a **property** where a `:` follows it, else a **string**.
//!   So is a bare word, as JSON5 writes its keys.
//! - `true`, `false`, and `null` are **constants**; `Infinity` and `NaN` are **numbers**, as is
//!   any number JSON5 writes (`0x1F`, `.5`, `5.`, `+1`, `-Infinity`).
//! - `//` and `/* */` are **comments**, and brackets, commas, and colons **punctuation**.

use super::Class;
use super::scan::{
    Spans, block_comment_end, is_space, is_word, is_word_start, line_end, number_end, space_end, string_end,
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
        let (to, class) = match c {
            b'/' if next == b'/' => (line_end(b, pos, end), Class::Comment),
            b'/' if next == b'*' => (block_comment_end(b, pos, end, false), Class::Comment),
            b'"' | b'\'' => {
                let to = string_end(b, pos, end);
                (to, if keyed(b, to) { Class::Property } else { Class::String })
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => (pos + 1, Class::Punctuation),
            b'-' | b'+' if next.is_ascii_digit() || next == b'.' => (number(b, pos + 1, end), Class::Number),
            b'-' | b'+' if matches!(&b[pos + 1..word(b, pos + 1)], b"Infinity" | b"NaN") => {
                (word(b, pos + 1), Class::Number)
            }
            b'0'..=b'9' => (number(b, pos, end), Class::Number),
            b'.' if next.is_ascii_digit() => (number(b, pos, end), Class::Number),
            _ if is_word_start(c) || c == b'$' => {
                let to = word(b, pos);
                let class = match &b[pos..to] {
                    b"true" | b"false" | b"null" => Class::Constant,
                    b"Infinity" | b"NaN" => Class::Number,
                    _ if keyed(b, to) => Class::Property,
                    _ => Class::Plain,
                };
                (to, class)
            }
            _ => (pos + 1, Class::Plain),
        };
        out.push(to, class);
        pos = to;
    }
}

/// Where the number at `at` ends, with the point JSON5 lets one end with: `5.`.
fn number(b: &[u8], at: usize, end: usize) -> usize {
    let to = number_end(b, at, end, false);
    if b.get(to) == Some(&b'.') && b.get(to + 1) != Some(&b'.') && !b[at..to].contains(&b'.') { to + 1 } else { to }
}

/// Where the word at `at` ends: JSON5's keys may hold `$`.
fn word(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while i < b.len() && (is_word(b[i]) || b[i] == b'$') {
        i += 1;
    }
    i
}

/// Whether what ends at `at` is a key: a `:` follows it, past any whitespace.
fn keyed(b: &[u8], at: usize) -> bool {
    b.get(space_end(b, at, b.len())) == Some(&b':')
}
