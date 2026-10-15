//! TOML (PLAN 2.135).
//!
//! - **Tables**, `[name]` and `[[name]]` first on their lines, have their brackets as punctuation
//!   and their names as types, dotted and quoted alike.
//! - **Keys** are properties, bare, quoted, and dotted (`a."b.c".d`), before their `=`, which is an
//!   operator; and so are an inline table's, in its `{ … }`.
//! - **Strings**: `"…"` with escapes and `'…'` without, each ending at its line's end where it
//!   does not close sooner; and `"""…"""` and `'''…'''`, which run over lines to the run of quotes
//!   that closes them, a quote or two before the close their own.
//! - **Numbers**: integers with underscores, in hexadecimal, octal, and binary (`0xDEAD_BEEF`,
//!   `0o755`, `0b1101`); floats with exponents; `inf` and `nan`, signed or not; and dates and
//!   times, offset or local (`1979-05-27T07:32:00-08:00`, `1979-05-27 07:32:00Z`, `07:32:00.5`).
//! - `true` and `false` are constants, `#` begins a comment, and brackets, braces, commas, and
//!   points are punctuation.
//!
//! An array runs over lines. The kinds of the outermost [`MAX_NESTING`] brackets open inside one
//! another, an array's or an inline table's, are kept, and those inside them are taken to be of
//! the innermost kind kept.
//!
//! [`MAX_NESTING`]: super::MAX_NESTING

use super::Class;
use super::scan::{Nest, Spans, blanks_end, byte, is_break, is_space, is_word, line_end, space_end};

/// What a bracket opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bracket {
    /// An array: `[1, 2]`.
    Array,
    /// An inline table: `{ x = 1 }`, whose keys are properties.
    Table,
}

/// Writes `text`'s tokens as TOML.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let b = text.as_bytes();
    let len = b.len();
    let mut pos = 0;
    let mut nest = Nest::new();
    // Whether a key comes next, else a value.
    let mut key = true;
    // Whether only whitespace has come since the line began.
    let mut first = true;
    while pos < len {
        let c = b[pos];
        if is_space(c) {
            let to = space_end(b, pos, len);
            if b[pos..to].iter().any(|&c| is_break(c)) {
                first = true;
                key |= nest.is_empty();
            }
            out.space(to);
            pos = to;
            continue;
        }
        if c == b'[' && std::mem::take(&mut first) && key && nest.is_empty() {
            pos = header(b, pos, out);
            continue;
        }
        first = false;
        let (to, class) = if c == b'#' {
            (line_end(b, pos, len), Class::Comment)
        } else if key {
            match c {
                b'"' | b'\'' => (string_end(b, pos), Class::Property),
                b'=' => {
                    key = false;
                    (pos + 1, Class::Operator)
                }
                b'.' | b',' => (pos + 1, Class::Punctuation),
                b'}' => {
                    nest.pop();
                    key = false;
                    (pos + 1, Class::Punctuation)
                }
                _ if bare(c) => (bare_end(b, pos), Class::Property),
                _ => (pos + 1, Class::Plain),
            }
        } else {
            match c {
                b'"' | b'\'' => (string_end(b, pos), Class::String),
                b'[' | b'{' => {
                    nest.push(if c == b'[' { Bracket::Array } else { Bracket::Table });
                    key = c == b'{';
                    (pos + 1, Class::Punctuation)
                }
                b']' | b'}' => {
                    nest.pop();
                    (pos + 1, Class::Punctuation)
                }
                b',' => {
                    key = nest.top() == Some(Bracket::Table);
                    (pos + 1, Class::Punctuation)
                }
                _ if bare(c) || c == b'+' => {
                    let to = literal_end(b, pos);
                    (to, literal(&b[pos..to]))
                }
                _ if c.is_ascii_punctuation() => (pos + 1, Class::Operator),
                _ => (pos + 1, Class::Plain),
            }
        };
        let to = to.max(pos + 1);
        out.push(to, class);
        pos = to;
    }
}

/// Whether a bare key may hold `c`: a letter, a digit, `_`, `-`, or, as TOML 1.1 lets one, any
/// character above ASCII.
fn bare(c: u8) -> bool {
    is_word(c) || c == b'-'
}

/// Where the bare key at `at` ends.
fn bare_end(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while i < b.len() && bare(b[i]) {
        i += 1;
    }
    i
}

/// A table's header, from its `[` at `at` to its `]` or its line's end: where it ends.
fn header(b: &[u8], at: usize, out: &mut Spans) -> usize {
    let end = line_end(b, at, b.len());
    let double = byte(b, at + 1) == b'[';
    out.push(at + 1 + usize::from(double), Class::Punctuation);
    let mut i = at + 1 + usize::from(double);
    while i < end {
        let c = b[i];
        if matches!(c, b' ' | b'\t') {
            i = blanks_end(b, i, end);
            out.space(i);
            continue;
        }
        let (to, class) = match c {
            b']' => {
                let close = i + 1 + usize::from(double && byte(b, i + 1) == b']');
                out.push(close, Class::Punctuation);
                return close;
            }
            b'"' | b'\'' => (string_end(b, i).min(end), Class::Type),
            b'.' => (i + 1, Class::Punctuation),
            _ if bare(c) => (bare_end(b, i), Class::Type),
            _ => (i + 1, Class::Plain),
        };
        out.push(to, class);
        i = to;
    }
    end
}

/// Where the string that opens at `at` ends. One in three quotes runs over lines to a run of
/// three quotes or more, which closes it; one in one quote ends at its line's end where it does
/// not close sooner. A backslash escapes in `"`, and nothing does in `'`.
fn string_end(b: &[u8], at: usize) -> usize {
    let (len, quote) = (b.len(), b[at]);
    let escapes = quote == b'"';
    if byte(b, at + 1) == quote && byte(b, at + 2) == quote {
        let mut i = at + 3;
        while i < len {
            let c = b[i];
            if escapes && c == b'\\' {
                i += 2;
            } else if c == quote {
                let run = b[i..].iter().take_while(|&&q| q == quote).count();
                if run >= 3 {
                    return i + run;
                }
                i += run;
            } else {
                i += 1;
            }
        }
        return len;
    }
    let mut i = at + 1;
    while i < len {
        match b[i] {
            c if is_break(c) => return i,
            b'\\' if escapes && !is_break(byte(b, i + 1)) => i += 2,
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    len
}

/// Where the bare value at `at` ends: a number, a date or a time, a constant, read as far as
/// their characters go, and a date's time after the one space that may stand between them
/// (`1979-05-27 07:32:00Z`).
fn literal_end(b: &[u8], at: usize) -> usize {
    let more = |c: u8| is_word(c) || matches!(c, b'.' | b':' | b'+' | b'-');
    let mut i = at;
    while i < b.len() && more(b[i]) {
        i += 1;
    }
    let date = i - at == 10
        && b[at..i].iter().enumerate().all(|(k, c)| if k == 4 || k == 7 { *c == b'-' } else { c.is_ascii_digit() });
    let time = byte(b, i + 1).is_ascii_digit() && byte(b, i + 2).is_ascii_digit() && byte(b, i + 3) == b':';
    if date && byte(b, i) == b' ' && time {
        i += 1;
        while i < b.len() && more(b[i]) {
            i += 1;
        }
    }
    i
}

/// The class of a bare value: `true` and `false` are constants; a number, a date, or a time,
/// which starts with a digit or a sign, and `inf` and `nan` are numbers.
fn literal(word: &[u8]) -> Class {
    let unsigned = word.strip_prefix(b"+").or_else(|| word.strip_prefix(b"-")).unwrap_or(word);
    match word {
        b"true" | b"false" => Class::Constant,
        _ if unsigned.first().is_some_and(u8::is_ascii_digit) || matches!(unsigned, b"inf" | b"nan") => Class::Number,
        _ => Class::Plain,
    }
}
