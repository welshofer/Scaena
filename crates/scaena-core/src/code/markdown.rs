//! Markdown (PLAN 2.145), as CommonMark and GitHub write it, read a line at a time and shallow.
//!
//! - **Headings** are keywords: an ATX heading's line, its `#`s and all, and a setext heading's
//!   text above its underline of `=` or `-`, which is punctuation.
//! - **Emphasis**, `*…*` and `_…_`, is a variable, and **strong** emphasis, `**…**` and `__…__`,
//!   a constant, each with its delimiters, closed by a run of the same on its line within [`LOOK`]
//!   bytes. A `_` inside a word is a letter (`snake_case`), and a delimiter before white space
//!   opens nothing (`2 * 3`).
//! - A **code span**, `` `…` `` by runs of one length, is a string, closed on its line within
//!   [`LOOK`] bytes.
//! - A **fenced block**, ```` ``` ```` or `~~~` and its info string, is read in the language its
//!   info string names first, as that language colors it alone (`code::embed`), or is a string
//!   where it names none, or plain text; its fences are punctuation and its info string meta. It
//!   closes at a fence of its character at least as long, alone on its line, or runs to the end.
//! - **Front matter** at the text's start, between `---` lines or `+++` lines, is YAML or TOML.
//! - A **link**'s or an image's brackets and parentheses are punctuation, and what its
//!   parentheses hold, its destination and its title, a string; a reference's label, in a link
//!   (`[text][label]`) or a definition (`[label]: https://…`), a property; an autolink,
//!   `<https://…>`, a string.
//! - The markers of **block quotes** (`>`), **lists** (`-`, `*`, `+`, `1.`, `1)`), a task's box
//!   (`[ ]`, `[x]`), and **tables** (`|`, and the row of dashes under a table's head) are
//!   punctuation, and so is a **thematic break** (`***`, `---`, `___`).
//! - An HTML comment is a comment, over lines where it starts its line; other HTML is text.
//! - A backslash escapes the punctuation after it.
//!
//! Markdown is never detected by itself (`code::detect`): prose would read as Markdown.

use super::detect::info_word;
use super::scan::{Spans, blanks_end, byte, is_space, line_end, line_next, space_end};
use super::{Class, Lang, embed, lang};

/// How far a delimiter, a bracket, or an autolink is looked through for its close, on its line.
const LOOK: usize = 256;

/// Writes `text`'s tokens as Markdown, `depth` parts in another language deep.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>, depth: usize) {
    let b = text.as_bytes();
    let len = b.len();
    let mut line = front_matter(text, out, depth);
    while line < len {
        let next = line_next(b, line, len);
        let end = line_end(b, line, len);
        let first = blanks_end(b, line, end);
        out.space(first);
        if first == end {
            out.space(next);
            line = next;
            continue;
        }
        if let Some(after) = fence(text, out, first, depth) {
            line = after;
            continue;
        }
        if b[first..end].starts_with(b"<!--") {
            // A comment that starts its line runs over lines to its close; the rest of the line it
            // closes on is text.
            let close = find(b, first + 4, b"-->").map_or(len, |k| k + 3);
            out.push(close, Class::Comment);
            let rest = line_end(b, close, len);
            inline(b, out, close, rest, false, true);
            line = line_next(b, close, len);
            out.space(line);
            continue;
        }
        if heading(b, first, end) || only(b, first, end, b'=') {
            out.push(end, if heading(b, first, end) { Class::Keyword } else { Class::Punctuation });
        } else if thematic(b, first, end) || delimiter_row(b, first, end) {
            out.push(end, Class::Punctuation);
        } else if !definition(b, out, first, end) {
            let mut i = first;
            let mut marked = false;
            loop {
                if byte(b, i) == b'>' {
                    out.push(i + 1, Class::Punctuation);
                } else if let Some(m) = list_marker(b, i, end) {
                    out.push(m, Class::Punctuation);
                    let k = blanks_end(b, m, end);
                    out.space(k);
                    if let Some(m) = task_box(b, k, end) {
                        out.push(m, Class::Punctuation);
                    }
                } else {
                    break;
                }
                marked = true;
                i = out.end().max(i + 1);
                i = blanks_end(b, i, end);
                out.space(i);
            }
            let underline = next < len && setext(b, next);
            if i < end && heading(b, i, end) {
                out.push(end, Class::Keyword);
            } else if !marked && underline {
                out.push(end, Class::Keyword);
            } else {
                let pipes = byte(b, i) == b'|' || (next < len && delimiter_row(b, next, line_end(b, next, len)));
                inline(b, out, i, end, pipes, true);
            }
        }
        out.space(next);
        line = next;
    }
}

/// Front matter at the text's start: YAML between `---` lines, or TOML between `+++` lines, read
/// in its language. Where the text after it starts, or 0 where there is none.
fn front_matter<'t>(text: &'t str, out: &mut Spans<'t>, depth: usize) -> usize {
    let b = text.as_bytes();
    let len = b.len();
    let (fence, language) = if b.starts_with(b"---") {
        (b"---", "yaml")
    } else if b.starts_with(b"+++") {
        (b"+++", "toml")
    } else {
        return 0;
    };
    if blanks_end(b, 3, len) != line_end(b, 0, len) {
        return 0;
    }
    let body = line_next(b, 0, len);
    let mut line = body;
    while line < len {
        let end = line_end(b, line, len);
        let mut trimmed = end;
        while trimmed > line && matches!(b[trimmed - 1], b' ' | b'\t') {
            trimmed -= 1;
        }
        let closes = b[line..trimmed] == *fence || (language == "yaml" && b[line..trimmed] == *b"...");
        if closes {
            out.push(3, Class::Punctuation);
            out.space(body);
            embed(text, body, line, lang(language), depth, out);
            out.push(end, Class::Punctuation);
            let next = line_next(b, line, len);
            out.space(next);
            return next;
        }
        line = line_next(b, line, len);
    }
    0
}

/// A fenced block whose fence starts at `at`, the first byte of its line that is no space: read
/// to its closing fence's line. Where the text after it starts; none where no fence opens here.
fn fence<'t>(text: &'t str, out: &mut Spans<'t>, at: usize, depth: usize) -> Option<usize> {
    let b = text.as_bytes();
    let len = b.len();
    let c = b[at];
    if c != b'`' && c != b'~' {
        return None;
    }
    let run = run_end(b, at, len, c);
    let end = line_end(b, at, len);
    if run - at < 3 || (c == b'`' && b[run..end].contains(&b'`')) {
        return None;
    }
    out.push(run, Class::Punctuation);
    let info = blanks_end(b, run, end);
    out.space(info);
    out.push(end, Class::Meta);
    let body = line_next(b, at, len);
    out.space(body);
    // Its closing fence: its character, at least as many, alone on a line.
    let mut close = body;
    let mut closing = None;
    while close < len {
        let first = blanks_end(b, close, len);
        let stop = line_end(b, close, len);
        let r = run_end(b, first, stop, c);
        if r - first >= run - at && blanks_end(b, r, stop) == stop {
            closing = Some((first, r));
            break;
        }
        close = line_next(b, close, len);
    }
    let named = info_word(text.get(info..end).unwrap_or_default());
    match lang(named) {
        _ if named.is_empty() => out.push(close, Class::String),
        Lang::Plain => out.push(close, Class::String),
        language => embed(text, body, close, language, depth, out),
    }
    let Some((first, r)) = closing else {
        return Some(len);
    };
    out.space(first);
    out.push(r, Class::Punctuation);
    let next = line_next(b, close, len);
    out.space(next);
    Some(next)
}

/// Whether the line from `at` to `end` is an ATX heading: one to six `#`s, then white space or its
/// end.
fn heading(b: &[u8], at: usize, end: usize) -> bool {
    let hashes = run_end(b, at, end, b'#') - at;
    (1..=6).contains(&hashes) && (at + hashes == end || matches!(b[at + hashes], b' ' | b'\t'))
}

/// Whether the line from `at` to `end` is a thematic break: three or more of one of `*`, `-`, and
/// `_`, and spaces and tabs among them, and nothing else.
fn thematic(b: &[u8], at: usize, end: usize) -> bool {
    let c = b[at];
    if !matches!(c, b'*' | b'-' | b'_') {
        return false;
    }
    let mut count = 0;
    for &k in &b[at..end] {
        if k == c {
            count += 1;
        } else if !matches!(k, b' ' | b'\t') {
            return false;
        }
    }
    count >= 3
}

/// Whether the line from `at` to `end` is `c` alone, at least once, then spaces and tabs.
fn only(b: &[u8], at: usize, end: usize, c: u8) -> bool {
    let r = run_end(b, at, end, c);
    r > at && blanks_end(b, r, end) == end
}

/// Whether the line at `line` underlines a setext heading: `=`s or `-`s alone, after at most three
/// spaces.
fn setext(b: &[u8], line: usize) -> bool {
    let end = line_end(b, line, b.len());
    let first = blanks_end(b, line, end);
    first - line <= 3 && first < end && (only(b, first, end, b'=') || only(b, first, end, b'-'))
}

/// Whether the line from `at` to `end` is the row under a table's head: `|`, `-`, `:`, spaces,
/// and tabs, a `|` and a `-` among them.
fn delimiter_row(b: &[u8], at: usize, end: usize) -> bool {
    let row = &b[at..end];
    row.iter().all(|&c| matches!(c, b'|' | b'-' | b':' | b' ' | b'\t')) && row.contains(&b'|') && row.contains(&b'-')
}

/// A list's marker at `at`: `-`, `*`, or `+`, or one to nine digits and `.` or `)`, then white
/// space or the line's end. Where it ends.
fn list_marker(b: &[u8], at: usize, end: usize) -> Option<usize> {
    let c = byte(b, at);
    let m = if matches!(c, b'-' | b'*' | b'+') {
        at + 1
    } else {
        let mut k = at;
        while k < end && k - at < 9 && b[k].is_ascii_digit() {
            k += 1;
        }
        if k == at || !matches!(byte(b, k), b'.' | b')') {
            return None;
        }
        k + 1
    };
    (m <= end && (m == end || matches!(b[m], b' ' | b'\t'))).then_some(m)
}

/// A task's box at `at`, after a list's marker: `[ ]`, `[x]`, or `[X]`, then white space or the
/// line's end. Where it ends.
fn task_box(b: &[u8], at: usize, end: usize) -> Option<usize> {
    let boxed = byte(b, at) == b'[' && matches!(byte(b, at + 1), b' ' | b'x' | b'X') && byte(b, at + 2) == b']';
    (boxed && (at + 3 == end || matches!(byte(b, at + 3), b' ' | b'\t'))).then_some(at + 3)
}

/// A reference's definition from `at` to `end`, `[label]: destination "title"`, read: its
/// brackets and colon punctuation, its label a property, and the rest a string. False where the
/// line is none.
fn definition(b: &[u8], out: &mut Spans, at: usize, end: usize) -> bool {
    if byte(b, at) != b'[' {
        return false;
    }
    let Some(close) = bracket_end(b, at, end) else {
        return false;
    };
    if byte(b, close + 1) != b':' || close == at + 1 {
        return false;
    }
    out.push(at + 1, Class::Punctuation);
    out.push(close, Class::Property);
    out.push(close + 2, Class::Punctuation);
    let rest = blanks_end(b, close + 2, end);
    out.space(rest);
    out.push(end, Class::String);
    true
}

/// The text from `at` to `to`, a line's or a link's, read for what stands in a paragraph: code
/// spans, emphasis, links, autolinks, escapes, and a table's `|` where `pipes` is set; links
/// where `links` is set, as no link stands in a link's text.
fn inline(b: &[u8], out: &mut Spans, at: usize, to: usize, pipes: bool, links: bool) {
    let mut i = at;
    while i < to {
        let c = b[i];
        let next = byte(b, i + 1);
        i = match c {
            _ if is_space(c) => {
                let e = space_end(b, i, to);
                out.space(e);
                e
            }
            b'\\' if i + 1 < to && next.is_ascii_punctuation() => {
                out.push(i + 2, Class::Plain);
                i + 2
            }
            b'`' => {
                let run = run_end(b, i, to, b'`');
                let end = closing_run(b, run, to, b'`', run - i);
                out.push(end.unwrap_or(run), if end.is_some() { Class::String } else { Class::Plain });
                end.unwrap_or(run)
            }
            b'*' | b'_' => emphasis(b, out, i, to),
            // An image: its `![` one mark.
            b'!' if next == b'[' && links => link(b, out, i + 1, to, pipes).unwrap_or_else(|| {
                out.push(i + 1, Class::Plain);
                i + 1
            }),
            b'[' if links => link(b, out, i, to, pipes).unwrap_or_else(|| {
                out.push(i + 1, Class::Plain);
                i + 1
            }),
            b'<' => angle(b, out, i, to),
            b'|' if pipes => {
                out.push(i + 1, Class::Punctuation);
                i + 1
            }
            _ => {
                let mut e = i + 1;
                while e < to
                    && !is_space(b[e])
                    && !matches!(b[e], b'\\' | b'`' | b'*' | b'_' | b'!' | b'[' | b'<')
                    && !(pipes && b[e] == b'|')
                {
                    e += 1;
                }
                out.push(e, Class::Plain);
                e
            }
        };
    }
}

/// A run of `*` or `_` at `at`: emphasis to the run of the same length that closes it, on its
/// line within [`LOOK`] bytes, a variable, or strong emphasis, two or more long, a constant; else
/// text. Where it ends.
fn emphasis(b: &[u8], out: &mut Spans, at: usize, to: usize) -> usize {
    let c = b[at];
    let run = run_end(b, at, to, c);
    let n = run - at;
    let opens = run < to && !is_space(b[run]) && !(c == b'_' && at > 0 && b[at - 1].is_ascii_alphanumeric());
    let mut close = None;
    if opens {
        let limit = to.min(run + LOOK);
        let mut k = run;
        while k < limit {
            if b[k] == c {
                let r = run_end(b, k, to, c);
                let flanked = !is_space(b[k - 1]) && !(c == b'_' && byte(b, r).is_ascii_alphanumeric());
                if r - k == n && flanked {
                    close = Some(r);
                    break;
                }
                k = r;
            } else if b[k] == b'`' {
                // A code span inside it is passed over whole, so its `*` closes nothing.
                let r = run_end(b, k, to, b'`');
                k = closing_run(b, r, limit, b'`', r - k).unwrap_or(r);
            } else {
                k += 1;
            }
        }
    }
    match close {
        Some(end) => {
            out.push(end, if n == 1 { Class::Variable } else { Class::Constant });
            end
        }
        None => {
            out.push(run, Class::Plain);
            run
        }
    }
}

/// A link or an image's text in brackets at `at`, its `[`, then what it goes to: `(destination
/// "title")`, or `[label]`. Its brackets and parentheses punctuation, its text read as a
/// paragraph's, what the parentheses hold a string, and a label a property. Where it ends, or none
/// where no link stands here.
fn link(b: &[u8], out: &mut Spans, at: usize, to: usize, pipes: bool) -> Option<usize> {
    let close = bracket_end(b, at, to)?;
    let after = byte(b, close + 1);
    let target = match after {
        b'(' => paren_end(b, close + 1, to)?,
        b'[' => bracket_end(b, close + 1, to)?,
        _ => return None,
    };
    out.push(at + 1, Class::Punctuation);
    inline(b, out, at + 1, close, pipes, false);
    out.push(close + 2, Class::Punctuation);
    out.push(target, if after == b'(' { Class::String } else { Class::Property });
    out.push(target + 1, Class::Punctuation);
    Some(target + 1)
}

/// An `<` at `at`: an autolink to its `>`, `<https://…>` or `<name@host>`, a string; an HTML
/// comment on its line, a comment; each closed within [`LOOK`] bytes; else text. Where it ends.
fn angle(b: &[u8], out: &mut Spans, at: usize, to: usize) -> usize {
    let limit = to.min(at + LOOK);
    if b[at..to].starts_with(b"<!--")
        && let Some(k) = find(&b[..limit], at + 4, b"-->")
    {
        out.push(k + 3, Class::Comment);
        return k + 3;
    }
    let mut k = at + 1;
    while k < limit && !is_space(b[k]) && !matches!(b[k], b'<' | b'>') {
        k += 1;
    }
    let inside = &b[at + 1..k];
    let scheme = inside.iter().position(|&c| c == b':').is_some_and(|colon| {
        colon >= 2
            && inside[0].is_ascii_alphabetic()
            && inside[..colon].iter().all(|c| c.is_ascii_alphanumeric() || b"+.-".contains(c))
    });
    if byte(b, k) == b'>' && k < to && (scheme || inside.contains(&b'@')) {
        out.push(k + 1, Class::String);
        return k + 1;
    }
    out.push(at + 1, Class::Plain);
    at + 1
}

/// Where the brackets that open at `at` close, on the line before `to` and within [`LOOK`] bytes,
/// past the brackets inside them and escaped ones: at the `]`.
fn bracket_end(b: &[u8], at: usize, to: usize) -> Option<usize> {
    close_of(b, at, to, b'[', b']')
}

/// Where the parentheses that open at `at` close, as [`bracket_end`] finds a `]`.
fn paren_end(b: &[u8], at: usize, to: usize) -> Option<usize> {
    close_of(b, at, to, b'(', b')')
}

fn close_of(b: &[u8], at: usize, to: usize, open: u8, close: u8) -> Option<usize> {
    let limit = to.min(at + LOOK);
    let (mut depth, mut k) = (0u32, at + 1);
    while k < limit {
        match b[k] {
            b'\\' => k += 1,
            c if c == open => depth += 1,
            c if c == close && depth == 0 => return Some(k),
            c if c == close => depth -= 1,
            _ => {}
        }
        k += 1;
    }
    None
}

/// Where a run of `c` exactly `n` long closes what a run of `n` opened, from `at` to `to`: past
/// it. None where none does.
fn closing_run(b: &[u8], at: usize, to: usize, c: u8, n: usize) -> Option<usize> {
    let limit = to.min(at + LOOK);
    let mut k = at;
    while k < limit {
        if b[k] == c {
            let r = run_end(b, k, to, c);
            if r - k == n {
                return Some(r);
            }
            k = r;
        } else {
            k += 1;
        }
    }
    None
}

/// Where the run of `c` at `at` ends, before `to`.
fn run_end(b: &[u8], at: usize, to: usize, c: u8) -> usize {
    let mut k = at;
    while k < to && b[k] == c {
        k += 1;
    }
    k
}

/// Where `what` is first found in `b` from `at`, if it is.
fn find(b: &[u8], at: usize, what: &[u8]) -> Option<usize> {
    let mut i = at;
    while i + what.len() <= b.len() {
        if b[i..].starts_with(what) {
            return Some(i);
        }
        i += 1;
    }
    None
}
