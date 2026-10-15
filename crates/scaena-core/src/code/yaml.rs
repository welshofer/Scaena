//! YAML (PLAN 2.135).
//!
//! - **Keys** are properties: a plain or a quoted scalar before a `:` that a space or the line's
//!   end follows, or, in a flow collection, a comma or a bracket; and one after a `?`, but for
//!   what a flow collection after it holds.
//! - **Scalars** are strings, but for plain ones that YAML's core schema types by their look:
//!   numbers (`42`, `-1.5e3`, `0x1F`, `0o17`, `.inf`, `.nan`), and `true`, `false`, `null`, and
//!   `~`, in their three cases, constants; after `!!str`, a plain scalar is a string whatever its
//!   look. A plain scalar runs to its line's end, to a ` #`, or to a key's `: `; a quoted one,
//!   `"…"` with escapes or `'…'` with `''`, runs over lines to its close.
//! - **Block scalars**: the header, `|` or `>` and its indicators (`|-`, `>+`, `|2`), is an
//!   operator, and the body a string. The body runs over the lines after its header indented past
//!   the node that holds it, as far as its first line is indented, or as its indicator says: to
//!   its dedent.
//! - **Anchors** (`&base`) and **aliases** (`*base`) are variables, and **tags** (`!!str`, `!Ref`)
//!   types.
//! - **Documents**: `---` and `...` at a line's start, and directives (`%YAML 1.2`), are meta.
//! - `#` after a space or at a line's start begins a comment; `-`, `?`, and `:` before a space,
//!   and a flow collection's brackets and commas, are punctuation, and so is a `:` in a flow
//!   collection after a quoted scalar or a closing bracket with no space after it, as JSON writes
//!   one (`{"a":1}`).

use super::Class;
use super::scan::{Spans, blanks_end, byte, is_break, is_space, line_end, line_next, space_end};

/// A block scalar's header, read: its body starts on the next line.
#[derive(Debug, Clone, Copy)]
struct Block {
    /// The column of the node that holds it, which its body is indented past: -1 at a document's
    /// top.
    parent: isize,
    /// Its indentation indicator, 0 where its body's first line sets its indentation.
    indicator: isize,
}

/// Writes `text`'s tokens as YAML.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let b = text.as_bytes();
    let len = b.len();
    let mut pos = 0;
    // Where the line being read starts.
    let mut line = 0;
    // Whether the next token is the first on its line.
    let mut first = true;
    // The column of the node that holds what the line goes on with: the last key, `-`, or `?` on
    // it, else one less than the line's indentation.
    let mut parent: isize = -1;
    // How many flow collections stand open inside one another.
    let mut flow = 0u32;
    let mut block: Option<Block> = None;
    // Whether a `?` came last on its line, so the scalar after it is a key.
    let mut asked = false;
    // Whether `!!str` came last, so the plain scalar after it is a string whatever its look.
    let mut stringly = false;
    // Whether a quoted scalar or a flow collection's close came last, after which a `:` in a flow
    // collection is a value's indicator with no space after it, as JSON writes one: `{"a":1}`.
    let mut adjacent = false;
    while pos < len {
        let c = b[pos];
        if is_space(c) {
            let to = space_end(b, pos, len);
            if let Some(k) = b[pos..to].iter().position(|&c| is_break(c)) {
                first = true;
                asked = false;
                if let Some(header) = block.take() {
                    let start = line_next(b, pos + k, len);
                    out.space(start);
                    let end = body(b, start, header);
                    out.push(end, Class::String);
                    (pos, line) = (end, end);
                    continue;
                }
                line = pos + b[pos..to].iter().rposition(|&c| is_break(c)).unwrap_or(k) + 1;
            }
            out.space(to);
            pos = to;
            continue;
        }
        let col = (pos - line) as isize;
        let starts = std::mem::take(&mut first);
        let tagged = std::mem::take(&mut stringly);
        let json = std::mem::take(&mut adjacent);
        if starts {
            parent = col - 1;
        }
        let next = byte(b, pos + 1);
        // What may follow an indicator: whitespace, or the text's end.
        let spaced = |i: usize| i >= len || is_space(b[i]);
        let lone = spaced(pos + 1) || (flow > 0 && c == b':' && (json || matches!(next, b',' | b']' | b'}')));
        let (to, class) = match c {
            b'-' | b'.'
                if pos == line && (b[pos..].starts_with(b"---") || b[pos..].starts_with(b"...")) && spaced(pos + 3) =>
            {
                if c == b'-' {
                    parent = -1;
                }
                flow = 0;
                (pos + 3, Class::Meta)
            }
            b'%' if pos == line => (line_end(b, pos, len), Class::Meta),
            b'#' if pos == 0 || is_space(b[pos - 1]) => (line_end(b, pos, len), Class::Comment),
            b'-' | b'?' | b':' if lone => {
                if c != b':' || starts {
                    parent = col;
                }
                asked = c == b'?';
                (pos + 1, Class::Punctuation)
            }
            b'[' | b'{' => {
                // A collection after a `?` is the key whole: no scalar in it is one.
                asked = false;
                flow = flow.saturating_add(1);
                (pos + 1, Class::Punctuation)
            }
            b']' | b'}' => {
                adjacent = true;
                flow = flow.saturating_sub(1);
                (pos + 1, Class::Punctuation)
            }
            b',' if flow > 0 => (pos + 1, Class::Punctuation),
            b'&' | b'*' => (name_end(b, pos + 1, flow > 0), Class::Variable),
            b'!' => {
                let to = name_end(b, pos + 1, flow > 0);
                stringly = &b[pos..to] == b"!!str";
                (to, Class::Type)
            }
            b'|' | b'>' if flow == 0 => {
                let mut j = pos + 1;
                let mut indicator = 0;
                while j < pos + 3 {
                    match byte(b, j) {
                        digit @ b'1'..=b'9' => indicator = isize::from(digit - b'0'),
                        b'+' | b'-' => {}
                        _ => break,
                    }
                    j += 1;
                }
                block = Some(Block { parent, indicator });
                (j, Class::Operator)
            }
            b'"' | b'\'' => {
                adjacent = true;
                let to = if c == b'"' { double_end(b, pos) } else { single_end(b, pos) };
                // A quoted scalar may run over lines: the line it ends on is the one read on.
                if let Some(k) = b[pos..to].iter().rposition(|&c| is_break(c)) {
                    line = pos + k + 1;
                }
                if std::mem::take(&mut asked) || byte(b, blanks_end(b, to, len)) == b':' {
                    parent = col;
                    (to, Class::Property)
                } else {
                    (to, Class::String)
                }
            }
            _ => {
                let (to, key) = plain(b, pos, flow > 0);
                if std::mem::take(&mut asked) || key {
                    parent = col;
                    (to, Class::Property)
                } else if tagged {
                    (to, Class::String)
                } else {
                    (to, typed(&b[pos..to]))
                }
            }
        };
        let to = to.max(pos + 1);
        out.push(to, class);
        pos = to;
    }
}

/// Where an anchor's, an alias's, or a tag's name from `at` ends: at whitespace, or in a flow
/// collection at a comma or a bracket.
fn name_end(b: &[u8], at: usize, flow: bool) -> usize {
    let mut i = at;
    while i < b.len() && !is_space(b[i]) && !(flow && matches!(b[i], b',' | b'[' | b']' | b'{' | b'}')) {
        i += 1;
    }
    i
}

/// Where the plain scalar at `at` ends, the spaces and tabs at its end left out, and whether it is
/// a key: it ends at its line's end, before a ` #`, before a `:` that whitespace follows, and in a
/// flow collection before a comma, a bracket, or a `:` that one follows.
fn plain(b: &[u8], at: usize, flow: bool) -> (usize, bool) {
    let len = b.len();
    let indicator = |c: u8| matches!(c, b',' | b'[' | b']' | b'{' | b'}');
    let mut end = at + 1;
    let mut j = at;
    while j < len {
        let c = b[j];
        if is_break(c) || (c == b'#' && j > at && is_space(b[j - 1])) || (flow && indicator(c) && j > at) {
            break;
        }
        if c == b':' && j > at {
            let next = byte(b, j + 1);
            if j + 1 >= len || is_space(next) || (flow && indicator(next)) {
                return (end, true);
            }
        }
        if !matches!(c, b' ' | b'\t') {
            end = j + 1;
        }
        j += 1;
    }
    (end, false)
}

/// The class of a plain scalar that is no key, by its look, as YAML's core schema types it.
fn typed(s: &[u8]) -> Class {
    match s {
        b"null" | b"Null" | b"NULL" | b"~" | b"true" | b"True" | b"TRUE" | b"false" | b"False" | b"FALSE" => {
            Class::Constant
        }
        _ if number(s) => Class::Number,
        _ => Class::String,
    }
}

/// Whether `s` is a number as YAML's core schema reads one: an integer, in hexadecimal or octal
/// too; a float with its point and its exponent; or infinity or not-a-number. Underscores may
/// stand between digits, as YAML 1.1 writes them.
fn number(s: &[u8]) -> bool {
    let unsigned = s.strip_prefix(b"-").or_else(|| s.strip_prefix(b"+")).unwrap_or(s);
    if matches!(unsigned, b".inf" | b".Inf" | b".INF") || matches!(s, b".nan" | b".NaN" | b".NAN") {
        return true;
    }
    if let Some(hex) = s.strip_prefix(b"0x") {
        return !hex.is_empty() && hex.iter().all(u8::is_ascii_hexdigit);
    }
    if let Some(octal) = s.strip_prefix(b"0o") {
        return !octal.is_empty() && octal.iter().all(|c| (b'0'..=b'7').contains(c));
    }
    let digits = |s: &[u8]| s.iter().take_while(|&&c| c.is_ascii_digit() || c == b'_').count();
    if unsigned.first().is_some_and(|&c| c == b'_') {
        return false;
    }
    let whole = digits(unsigned);
    let mut rest = &unsigned[whole..];
    let mut fraction = 0;
    if let Some(after) = rest.strip_prefix(b".") {
        fraction = digits(after);
        rest = &after[fraction..];
    }
    if whole == 0 && fraction == 0 {
        return false;
    }
    match rest.strip_prefix(b"e").or_else(|| rest.strip_prefix(b"E")) {
        Some(exponent) => {
            let exponent = exponent.strip_prefix(b"-").or_else(|| exponent.strip_prefix(b"+")).unwrap_or(exponent);
            !exponent.is_empty() && exponent.iter().all(u8::is_ascii_digit)
        }
        None => rest.is_empty(),
    }
}

/// Where the double-quoted scalar at `at` ends: past its close, a backslash escaping the byte
/// after it, or at the text's end.
fn double_end(b: &[u8], at: usize) -> usize {
    let mut i = at + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

/// Where the single-quoted scalar at `at` ends: past its close, `''` a quote in it, or at the
/// text's end.
fn single_end(b: &[u8], at: usize) -> usize {
    let mut i = at + 1;
    while i < b.len() {
        if b[i] == b'\'' {
            if byte(b, i + 1) != b'\'' {
                return i + 1;
            }
            i += 1;
        }
        i += 1;
    }
    b.len()
}

/// Where a block scalar's body, from the line at `from`, ends: past the last line indented past
/// the node that holds it and at least as far as its first line (or its indicator) sets, before
/// the line that is not, a document's marker, or the text's end. Blank lines among its lines are
/// its own.
fn body(b: &[u8], from: usize, header: Block) -> usize {
    let len = b.len();
    let mut least = (header.indicator > 0).then(|| (header.parent + header.indicator).max(0) as usize);
    let (mut line, mut end) = (from, from);
    while line < len {
        let next = line_next(b, line, len);
        let stop = line_end(b, line, len);
        let indent = b[line..stop].iter().take_while(|&&c| c == b' ').count();
        if b[line..stop].iter().any(|&c| !matches!(c, b' ' | b'\t')) {
            let marker = (b[line..].starts_with(b"---") || b[line..].starts_with(b"..."))
                && (line + 3 >= len || is_space(b[line + 3]));
            let least = *least.get_or_insert(indent);
            if (indent == 0 && marker) || indent as isize <= header.parent || indent < least {
                break;
            }
            end = next;
        }
        line = next;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_scalars_are_typed_by_their_look() {
        for s in
            ["0", "-12", "+3", "1_000", "3.14", "1.", ".5", "6.02e23", "1E-9", "0x1F", "0o17", ".inf", "-.Inf", ".nan"]
        {
            assert_eq!(typed(s.as_bytes()), Class::Number, "{s}");
        }
        for s in ["true", "False", "NULL", "~", "null"] {
            assert_eq!(typed(s.as_bytes()), Class::Constant, "{s}");
        }
        for s in ["yes", "1.2.3", "0x", "1e", "_1", "-", ".", "-.nan", "web", "12px", "2024-01-15"] {
            assert_eq!(typed(s.as_bytes()), Class::String, "{s}");
        }
    }
}
