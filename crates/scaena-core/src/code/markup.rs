//! HTML and XML, read by one scanner (PLAN 2.135).
//!
//! - **Tags**: a tag's name is a tag, and its `<`, `</`, `>`, and `/>` punctuation; an attribute's
//!   name is an attribute, its `=` an operator, and its value a string, in quotes or none.
//! - **Entities** (`&amp;`, `&#169;`, `&#x1F600;`) are constants, in text and in values.
//! - **Comments**, `<!-- … -->`, run over lines to their close; a **CDATA** section's
//!   `<![CDATA[` and `]]>` are meta and what it holds a string; a **processing instruction**
//!   (`<?xml version="1.0"?>`) and a **declaration** (`<!DOCTYPE html>`, an internal subset in its
//!   brackets) are meta.
//! - **Text** is plain.
//! - In HTML, what `<script>` and `<style>` hold, and `<textarea>` and `<title>`, is text to their
//!   closing tag: no markup is read in it. Script and style are plain until PLAN 2.145 reads them
//!   in their languages.

use super::Class;
use super::scan::{Spans, byte, is_space, space_end};

/// How far an entity is looked through for its `;`: `&CounterClockwiseContourIntegral;`.
const ENTITY: usize = 40;

/// The elements whose content is text to their closing tag, in HTML: script and style first,
/// which hold no entities, then those that do.
const RAW: &[&[u8]] = &[b"script", b"style", b"textarea", b"title"];

/// Writes `text`'s tokens as HTML, or as XML where `html` is not set.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>, html: bool) {
    let b = text.as_bytes();
    let len = b.len();
    let mut pos = 0;
    while pos < len {
        let c = b[pos];
        if is_space(c) {
            let to = space_end(b, pos, len);
            out.space(to);
            pos = to;
            continue;
        }
        let next = byte(b, pos + 1);
        pos = if c == b'<' {
            if b[pos..].starts_with(b"<!--") {
                let end = find(b, pos + 4, b"-->").map_or(len, |k| k + 3);
                out.push(end, Class::Comment);
                end
            } else if b[pos..].starts_with(b"<![CDATA[") {
                out.push(pos + 9, Class::Meta);
                match find(b, pos + 9, b"]]>") {
                    Some(k) => {
                        out.push(k, Class::String);
                        out.push(k + 3, Class::Meta);
                        k + 3
                    }
                    None => {
                        out.push(len, Class::String);
                        len
                    }
                }
            } else if next == b'?' {
                let end = find(b, pos + 2, b"?>").map_or(len, |k| k + 2);
                out.push(end, Class::Meta);
                end
            } else if next == b'!' {
                let end = declaration(b, pos + 2);
                out.push(end, Class::Meta);
                end
            } else if next == b'/' && name_start(byte(b, pos + 2)) {
                close(b, pos, out)
            } else if name_start(next) {
                let (end, name) = open(b, pos, out);
                let name = &b[name.0..name.1];
                match RAW.iter().position(|r| r.eq_ignore_ascii_case(name)) {
                    // Script and style hold no entities; a title and a text area do.
                    Some(k) if html && b[end - 1] == b'>' => content(b, end, name, k >= 2, out),
                    _ => end,
                }
            } else {
                out.push(pos + 1, Class::Plain);
                pos + 1
            }
        } else if c == b'&' {
            let end = entity(b, pos).unwrap_or(pos + 1);
            out.push(end, if end > pos + 1 { Class::Constant } else { Class::Plain });
            end
        } else {
            let mut e = pos + 1;
            while e < len && !matches!(b[e], b'<' | b'&') && !is_space(b[e]) {
                e += 1;
            }
            out.push(e, Class::Plain);
            e
        };
    }
}

/// Whether a tag's name can start with `c`: a letter, `_`, `:`, or any byte above ASCII.
fn name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || matches!(c, b'_' | b':') || c >= 0x80
}

/// Where the name at `at` ends: `div`, `svg:rect`, `my-element`, `xsl:value-of`.
fn name_end(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while i < b.len() && (name_start(b[i]) || b[i].is_ascii_digit() || matches!(b[i], b'-' | b'.')) {
        i += 1;
    }
    i
}

/// Where `what` is first found in `b` from `at`, if it is.
fn find(b: &[u8], at: usize, what: &[u8]) -> Option<usize> {
    let first = what[0];
    let mut i = at;
    while i + what.len() <= b.len() {
        if b[i] == first && b[i..].starts_with(what) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Where the entity at `at` ends, past its `;`: a name, or a number in decimal or hexadecimal.
fn entity(b: &[u8], at: usize) -> Option<usize> {
    let limit = b.len().min(at + ENTITY);
    let mut i = at + 1;
    if byte(b, i) == b'#' {
        i += 1;
        let hex = matches!(byte(b, i), b'x' | b'X');
        i += usize::from(hex);
        let digits = i;
        while i < limit && (if hex { b[i].is_ascii_hexdigit() } else { b[i].is_ascii_digit() }) {
            i += 1;
        }
        if i == digits {
            return None;
        }
    } else {
        let name = i;
        while i < limit && b[i].is_ascii_alphanumeric() {
            i += 1;
        }
        if i == name || !b[name].is_ascii_alphabetic() {
            return None;
        }
    }
    (byte(b, i) == b';').then_some(i + 1)
}

/// Where a declaration from `at`, past its `<!`, ends: past the `>` that closes it, its quotes and
/// its internal subset's brackets, and the declarations and comments in them, passed over.
fn declaration(b: &[u8], at: usize) -> usize {
    let len = b.len();
    let (mut depth, mut i) = (0u32, at);
    while i < len {
        match b[i] {
            q @ (b'"' | b'\'') => i = (i + 1..len).find(|&k| b[k] == q).map_or(len, |k| k + 1),
            b'<' if b[i..].starts_with(b"<!--") => i = find(b, i + 4, b"-->").map_or(len, |k| k + 3),
            b'[' | b'<' => {
                depth += 1;
                i += 1;
            }
            b']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'>' if depth == 0 => return i + 1,
            b'>' => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    len
}

/// A closing tag at `at`, `</name>`: where it ends.
fn close(b: &[u8], at: usize, out: &mut Spans) -> usize {
    let len = b.len();
    out.push(at + 2, Class::Punctuation);
    let e = name_end(b, at + 2);
    out.push(e, Class::Tag);
    let mut i = e;
    while i < len && b[i] != b'>' && b[i] != b'<' {
        if is_space(b[i]) {
            i = space_end(b, i, len);
            out.space(i);
        } else {
            i += 1;
            out.push(i, Class::Plain);
        }
    }
    if byte(b, i) == b'>' {
        out.push(i + 1, Class::Punctuation);
        i += 1;
    }
    i
}

/// An opening tag at `at`, its name and its attributes, to its `>` or `/>`: where it ends, and
/// where its name starts and ends.
fn open(b: &[u8], at: usize, out: &mut Spans) -> (usize, (usize, usize)) {
    let len = b.len();
    out.push(at + 1, Class::Punctuation);
    let e = name_end(b, at + 1);
    out.push(e, Class::Tag);
    let name = (at + 1, e);
    let mut i = e;
    while i < len {
        let c = b[i];
        if is_space(c) {
            i = space_end(b, i, len);
            out.space(i);
            continue;
        }
        let (to, class) = match c {
            b'>' => {
                out.push(i + 1, Class::Punctuation);
                return (i + 1, name);
            }
            b'/' if byte(b, i + 1) == b'>' => {
                out.push(i + 2, Class::Punctuation);
                return (i + 2, name);
            }
            b'=' => {
                out.push(i + 1, Class::Operator);
                let v = space_end(b, i + 1, len);
                out.space(v);
                i = value(b, v, out);
                continue;
            }
            b'/' => (i + 1, Class::Punctuation),
            _ => {
                let mut k = i + 1;
                while k < len && !is_space(b[k]) && !matches!(b[k], b'"' | b'\'' | b'>' | b'/' | b'=') {
                    k += 1;
                }
                (k, Class::Attribute)
            }
        };
        out.push(to, class);
        i = to;
    }
    (len, name)
}

/// An attribute's value at `at`: in quotes, over lines to the quote that closes it, or none, to
/// whitespace or the tag's `>`; a string, its entities constants. Where it ends.
fn value(b: &[u8], at: usize, out: &mut Spans) -> usize {
    let len = b.len();
    let quote = byte(b, at);
    let quoted = matches!(quote, b'"' | b'\'');
    let mut i = at + usize::from(quoted);
    out.push(i, Class::String);
    while i < len {
        let c = b[i];
        if quoted && c == quote {
            out.push(i + 1, Class::String);
            return i + 1;
        }
        if !quoted && (is_space(c) || c == b'>') {
            break;
        }
        if c == b'&'
            && let Some(end) = entity(b, i)
        {
            out.push(i, Class::String);
            out.push(end, Class::Constant);
            i = end;
            continue;
        }
        i += 1;
    }
    out.push(i, Class::String);
    i
}

/// The text a raw element holds, from `at` to its closing tag (`</script>`, in any case), read,
/// its entities constants where `entities` is set: where it ends, past the closing tag, or at the
/// text's end.
fn content(b: &[u8], at: usize, name: &[u8], entities: bool, out: &mut Spans) -> usize {
    let len = b.len();
    let mut i = at;
    while i < len {
        if b[i] == b'<'
            && byte(b, i + 1) == b'/'
            && b.len() >= i + 2 + name.len()
            && b[i + 2..i + 2 + name.len()].eq_ignore_ascii_case(name)
            && !name_start(byte(b, i + 2 + name.len()))
            && !byte(b, i + 2 + name.len()).is_ascii_digit()
        {
            break;
        }
        i += 1;
    }
    // Its text, whitespace as whitespace is anywhere.
    let mut k = at;
    while k < i {
        if is_space(b[k]) {
            k = space_end(b, k, i);
            out.space(k);
        } else if entities
            && b[k] == b'&'
            && let Some(end) = entity(b, k).filter(|&end| end <= i)
        {
            out.push(end, Class::Constant);
            k = end;
        } else {
            let mut e = k + 1;
            while e < i && !is_space(b[e]) && !(entities && b[e] == b'&') {
                e += 1;
            }
            out.push(e, Class::Plain);
            k = e;
        }
    }
    if i < len { close(b, i, out) } else { len }
}
