//! INI, and the files written as it is: `.properties`, `.cfg`, `.conf`, `.editorconfig`, and
//! `.gitconfig` (PLAN 2.135).
//!
//! - **A section**, `[name]`, has its brackets as punctuation and its name as a type, a quoted
//!   part of it a string: `[remote "origin"]`.
//! - **A comment** is a line whose first character is `;` or `#`, or `!` as a `.properties` file
//!   writes one.
//! - **A key**, before the first `=` or `:` on its line, is a property, and the sign an operator.
//!   A line with neither is a key alone.
//! - **A value** is a string, but for a number, and for `true`, `false`, `yes`, `no`, `on`, and
//!   `off` in any case, which are constants; a variable in it, `${name}` or `%(name)s`, is a
//!   variable. A value goes on past a `\` that ends its line, and, as configparser reads one, onto
//!   the lines indented past its key, blank lines and comments among them, an `=` or a `:` in them
//!   its own: `install_requires =` and the requirements under it. Keys indented alike, as
//!   `.gitconfig` writes them, are keys.

use super::Class;
use super::scan::{Spans, blanks_end, byte, line_end, line_next};

/// Writes `text`'s tokens as INI.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let b = text.as_bytes();
    let len = b.len();
    let mut pos = 0;
    // Whether the line before ended its value with a `\`, which goes on into this one.
    let mut more = false;
    // How far the key whose value is open is indented: a line indented past it goes on the value.
    let mut key: Option<usize> = None;
    while pos < len {
        let next = line_next(b, pos, len);
        let end = line_end(b, pos, len);
        let first = blanks_end(b, pos, end);
        out.space(first);
        if first == end {
            more = false;
        } else if more {
            more = values(b, first, end, out);
        } else if matches!(b[first], b';' | b'#' | b'!') {
            out.push(end, Class::Comment);
        } else if key.is_some_and(|indent| first - pos > indent) {
            more = values(b, first, end, out);
        } else if b[first] == b'[' {
            section(b, first, end, out);
            (more, key) = (false, None);
        } else if let Some(sign) = separator(b, first, end) {
            out.push(trimmed(b, first, sign), Class::Property);
            out.space(sign);
            out.push(sign + 1, Class::Operator);
            let from = blanks_end(b, sign + 1, end);
            out.space(from);
            more = values(b, from, end, out);
            key = Some(first - pos);
        } else {
            out.push(trimmed(b, first, end), Class::Property);
            (more, key) = (false, None);
        }
        out.space(next);
        pos = next;
    }
}

/// Where the first `=` or `:` from `from` to `end` stands that no backslash escapes.
fn separator(b: &[u8], from: usize, end: usize) -> Option<usize> {
    let mut i = from;
    while i < end {
        match b[i] {
            b'\\' => i += 2,
            b'=' | b':' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// Where the text from `from` to `end` ends, the spaces and tabs at its end left out.
fn trimmed(b: &[u8], from: usize, end: usize) -> usize {
    let mut k = end;
    while k > from && matches!(b[k - 1], b' ' | b'\t') {
        k -= 1;
    }
    k
}

/// A section's header from its `[` at `at` to the line's `end`.
fn section(b: &[u8], at: usize, end: usize, out: &mut Spans) {
    out.push(at + 1, Class::Punctuation);
    let mut i = at + 1;
    while i < end {
        match b[i] {
            b']' => {
                out.push(i + 1, Class::Punctuation);
                let rest = blanks_end(b, i + 1, end);
                out.space(rest);
                if rest < end {
                    let class = if matches!(b[rest], b';' | b'#') { Class::Comment } else { Class::Plain };
                    out.push(end, class);
                }
                return;
            }
            b' ' | b'\t' => {
                i = blanks_end(b, i, end);
                out.space(i);
            }
            q @ (b'"' | b'\'') => {
                let close = (i + 1..end).find(|&k| b[k] == q).map_or(end, |k| k + 1);
                out.push(close, Class::String);
                i = close;
            }
            _ => {
                let to = (i + 1..end).find(|&k| matches!(b[k], b']' | b'"' | b'\'' | b' ' | b'\t')).unwrap_or(end);
                out.push(to, Class::Type);
                i = to;
            }
        }
    }
}

/// A value's tokens from `from` to the line's `end`, and whether it goes on past a `\` there.
fn values(b: &[u8], from: usize, end: usize, out: &mut Spans) -> bool {
    let stop = trimmed(b, from, end);
    let word = &b[from..stop];
    if number(word) || constant(word) {
        out.push(stop, if number(word) { Class::Number } else { Class::Constant });
        return false;
    }
    // Whether a `}` or a `)` was looked for and not found: none closes a variable after it either,
    // so none is looked for again.
    let (mut braces, mut parens) = (true, true);
    let mut i = from;
    while i < stop {
        // A variable: `${name}`, or Python's `%(name)s`.
        let close = match (b[i], byte(b, i + 1)) {
            (b'$', b'{') if braces => {
                let close = (i + 2..stop).find(|&k| b[k] == b'}').map(|k| k + 1);
                braces = close.is_some();
                close
            }
            (b'%', b'(') if parens => {
                let close = (i + 2..stop).find(|&k| b[k] == b')').map(|k| (k + 2).min(stop));
                parens = close.is_some();
                close
            }
            _ => None,
        };
        if let Some(close) = close {
            out.push(close, Class::Variable);
            i = close;
            continue;
        }
        let to = (i + 1..stop).find(|&k| matches!(b[k], b'$' | b'%')).unwrap_or(stop);
        out.push(to, Class::String);
        i = to;
    }
    stop > from && b[stop - 1] == b'\\'
}

/// Whether `word` is a number: digits, with a sign before them and a point among them.
fn number(word: &[u8]) -> bool {
    let digits = word.strip_prefix(b"-").or_else(|| word.strip_prefix(b"+")).unwrap_or(word);
    let points = digits.iter().filter(|&&c| c == b'.').count();
    digits.first().is_some_and(u8::is_ascii_digit)
        && digits.last().is_some_and(u8::is_ascii_digit)
        && points <= 1
        && digits.iter().all(|&c| c.is_ascii_digit() || c == b'.')
}

/// Whether `word` is one of the constants a value may be, in any case.
fn constant(word: &[u8]) -> bool {
    ["false", "no", "off", "on", "true", "yes"].iter().any(|w| w.as_bytes().eq_ignore_ascii_case(word))
}
