//! Python, read by a scanner of its own (PLAN 2.134).
//!
//! - **Strings** open with one quote or three, after a prefix of `r`, `b`, `u`, `f`, or `t`, or
//!   two of them (`rb`, `fr`), in any case. A string in one quote ends at its line's end where it
//!   does not close sooner; a backslash escapes the byte after it, a line's break too.
//! - **f-strings and t-strings** interpolate `{…}`, and `{{` and `}}` are braces. A replacement
//!   field is code to its `}`, in which strings may take the field's own quotes and fields nest
//!   (PEP 701). Its conversion (`!r`) is an operator, and its format spec after a `:` a string
//!   that may hold fields of its own: `{value:>{width}}`.
//! - **Decorators**, an `@` first in a statement and the dotted name after it, are attributes.
//! - **Soft keywords** are keywords only where they act as keywords: `match` and `case` first in
//!   a statement whose line ends with a `:`, `_` in a case's pattern, and `type` first in a
//!   statement that names a type (`type Pair = tuple[int, int]`).
//! - A word is a keyword, a constant, a type, or a variable (`self`, `cls`) where the tables hold
//!   it; after `def` or `class`, the function's or the class's name; after a `from` that begins
//!   a statement, the module's, plain (`from __future__ import`); after a `.`, a property, or a
//!   function where a `(` follows; else, by its case and what follows it: a constant in capitals
//!   (`MAX_SIZE`), a type that starts with one (`ValueError`), a variable between double
//!   underscores (`__name__`), a function where a `(` follows, or plain.
//!
//! At most [`MAX_NESTING`] replacement fields stand open inside one another, and what would open
//! one more is read as its string's own characters.

use super::scan::{Spans, blanks_end, is_break, is_space, is_word_start, line_end, number_end, space_end, word_end};
use super::tables::{Table, words};
use super::{Class, MAX_NESTING};

/// How far the line a soft keyword begins is looked through for the `:` that ends it.
const LOOK: usize = 256;

/// How far a named escape is looked through for its `}`: `\N{EM DASH}`.
const NAME: usize = 128;

/// How a string reads once open.
#[derive(Debug, Clone, Copy)]
struct Str {
    quote: u8,
    /// Whether it opened with three quotes, and runs on past its line's end.
    triple: bool,
    /// Whether a backslash leaves what follows it as it is: `r"\d"`.
    raw: bool,
    /// Whether `{…}` interpolates: an f-string or a t-string.
    fields: bool,
}

/// What the scanner returns to.
#[derive(Debug, Clone, Copy)]
enum Frame {
    /// Code: the text's own at the bottom of the stack, else a replacement field's, which `}`
    /// closes where `depth` brackets inside it are open.
    Code { field: bool, depth: u32 },
    /// A string: on top as it is read, below the field that interrupts it.
    Str(Str),
    /// A replacement field's format spec, after its `:`, to the field's `}`.
    Spec,
}

const TOP: Frame = Frame::Code { field: false, depth: 0 };

/// The prefixes a string opens with, in lowercase.
const PREFIXES: &[&[u8]] = &[b"b", b"br", b"f", b"fr", b"r", b"rb", b"rf", b"rt", b"t", b"tr", b"u"];

/// Writes `text`'s tokens as Python.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let mut scanner = Scanner {
        text,
        b: text.as_bytes(),
        pos: 0,
        out,
        stack: vec![TOP],
        member: false,
        pending: None,
        statement: true,
        pattern: false,
    };
    scanner.run();
}

struct Scanner<'t, 'o> {
    text: &'t str,
    b: &'t [u8],
    pos: usize,
    out: &'o mut Spans<'t>,
    stack: Vec<Frame>,
    /// Whether a `.` came last, so a word is a member.
    member: bool,
    /// What the next word names, after `def` or `class`.
    pending: Option<Class>,
    /// Whether a statement starts here: only whitespace since its line began, outside brackets.
    statement: bool,
    /// Whether a case's pattern is being read, where `_` is a keyword.
    pattern: bool,
}

impl Scanner<'_, '_> {
    fn run(&mut self) {
        while self.pos < self.b.len() {
            let (pos, frames) = (self.pos, self.stack.len());
            match self.stack.last().copied().unwrap_or(TOP) {
                Frame::Code { field, depth } => self.code(field, depth),
                Frame::Str(s) => self.string(s),
                Frame::Spec => self.spec(),
            }
            // Each step reads a byte, or closes what a byte read before opened.
            debug_assert!(self.pos > pos || self.stack.len() != frames, "a step read nothing at {pos}");
            if self.pos <= pos && self.stack.len() == frames {
                self.pos = pos + 1;
                self.out.push(self.pos, Class::Plain);
            }
        }
    }

    /// The byte at `i`, or 0 past the text's end.
    fn at(&self, i: usize) -> u8 {
        self.b.get(i).copied().unwrap_or(0)
    }

    /// The text up to `end` in `class`, read.
    fn emit(&mut self, end: usize, class: Class) {
        self.out.push(end, class);
        self.pos = end.max(self.pos);
    }

    fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    /// Whether one more replacement field may open.
    fn can_nest(&self) -> bool {
        self.stack.iter().filter(|f| matches!(f, Frame::Code { field: true, .. })).count() < MAX_NESTING
    }

    /// One more bracket open in the code on top, or one fewer.
    fn nest(&mut self, open: bool) {
        if let Some(Frame::Code { depth, .. }) = self.stack.last_mut() {
            *depth = if open { depth.saturating_add(1) } else { depth.saturating_sub(1) };
        }
    }

    /// One step of code.
    fn code(&mut self, field: bool, depth: u32) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let c = b[i];
        let next = self.at(i + 1);
        if is_space(c) {
            let end = space_end(b, i, len);
            if b[i..end].iter().any(|&c| is_break(c)) {
                self.statement = !field && depth == 0;
                self.pattern = false;
            }
            self.out.space(end);
            self.pos = end;
            return;
        }
        if c == b'#' {
            return self.emit(line_end(b, i, len), Class::Comment);
        }
        let statement = std::mem::take(&mut self.statement);
        let member = std::mem::take(&mut self.member);
        let pending = self.pending.take();
        // A replacement field's own close, format spec, and conversion.
        if field && depth == 0 {
            if c == b'}' {
                self.emit(i + 1, Class::Punctuation);
                return self.pop();
            }
            if c == b':' {
                self.emit(i + 1, Class::Punctuation);
                return self.stack.push(Frame::Spec);
            }
            if c == b'!' && matches!(next, b'r' | b's' | b'a') && matches!(self.at(i + 2), b':' | b'}') {
                return self.emit(i + 2, Class::Operator);
            }
        }
        if let Some((open, s)) = self.opener() {
            self.emit(i + open, Class::String);
            return self.stack.push(Frame::Str(s));
        }
        if c == b'@' && statement && is_word_start(next) {
            return self.decorator();
        }
        if c.is_ascii_digit() || (c == b'.' && next.is_ascii_digit()) {
            return self.emit(self.number(i), Class::Number);
        }
        if is_word_start(c) {
            return self.word(member, pending, statement);
        }
        self.symbol(c, next);
    }

    /// The string that opens here, after its prefix: how many bytes open it, and how it reads.
    fn opener(&self) -> Option<(usize, Str)> {
        let (b, i) = (self.b, self.pos);
        let mut j = i;
        while j < i + 2 && self.at(j).is_ascii_alphabetic() {
            j += 1;
        }
        let quote = self.at(j);
        if !matches!(quote, b'"' | b'\'') {
            return None;
        }
        let mut lower = [0u8; 2];
        for (k, c) in b[i..j].iter().enumerate() {
            lower[k] = c.to_ascii_lowercase();
        }
        let prefix = &lower[..j - i];
        if j > i && !PREFIXES.contains(&prefix) {
            return None;
        }
        let triple = self.at(j + 1) == quote && self.at(j + 2) == quote;
        let s = Str {
            quote,
            triple,
            raw: prefix.contains(&b'r'),
            fields: prefix.iter().any(|&c| matches!(c, b'f' | b't')),
        };
        Some((j - i + if triple { 3 } else { 1 }, s))
    }

    /// A decorator: `@` and the dotted name after it.
    fn decorator(&mut self) {
        let (b, len) = (self.b, self.b.len());
        let mut j = word_end(b, self.pos + 1, len);
        while self.at(j) == b'.' && is_word_start(self.at(j + 1)) {
            j = word_end(b, j + 1, len);
        }
        self.emit(j, Class::Attribute);
    }

    /// Where the number at `i` ends, with the point a float may end with (`5.`), an exponent
    /// after it (`5.e3`), and an imaginary's `j`.
    fn number(&self, i: usize) -> usize {
        let b = self.b;
        let mut j = number_end(b, i, b.len(), false);
        let digits = b[i..j].iter().all(|c| c.is_ascii_digit() || *c == b'_');
        if digits && self.at(j) == b'.' && self.at(j + 1) != b'.' {
            j += 1;
            let sign = usize::from(matches!(self.at(j + 1), b'+' | b'-'));
            if matches!(self.at(j), b'e' | b'E') && self.at(j + 1 + sign).is_ascii_digit() {
                j += 1 + sign;
                while self.at(j).is_ascii_digit() || self.at(j) == b'_' {
                    j += 1;
                }
            }
            if matches!(self.at(j), b'j' | b'J') {
                j += 1;
            }
        }
        j
    }

    /// The first byte at or after `i` that is no space or tab.
    fn blanks(&self, i: usize) -> usize {
        blanks_end(self.b, i, self.b.len())
    }

    /// A word: a name, a keyword, a type, or what else its tables and its case make it.
    fn word(&mut self, member: bool, pending: Option<Class>, statement: bool) {
        let (b, i) = (self.b, self.pos);
        let j = word_end(b, i, b.len());
        let word = self.text.get(i..j).unwrap_or_default();
        let call = self.at(self.blanks(j)) == b'(';
        let name = word.trim_start_matches('_').as_bytes();
        let capital = name.first().is_some_and(u8::is_ascii_uppercase);
        let capitals = capital && name.len() > 1 && !name.iter().any(u8::is_ascii_lowercase);
        let class = if member {
            if capitals && !call {
                Class::Constant
            } else if capital {
                Class::Type
            } else if call {
                Class::Function
            } else {
                Class::Property
            }
        } else if KEYWORDS.binary_search(&word).is_ok() {
            self.pending = match word {
                "def" => Some(Class::Function),
                "class" => Some(Class::Type),
                // The module a statement imports from, a name as any module's is: `__future__`.
                "from" if statement => Some(Class::Plain),
                _ => None,
            };
            Class::Keyword
        } else if (statement && self.soft(word, j)) || (self.pattern && word == "_") {
            Class::Keyword
        } else if let Some(class) = pending {
            class
        } else if CONSTANTS.binary_search(&word).is_ok() {
            Class::Constant
        } else if TYPES.binary_search(&word).is_ok() {
            Class::Type
        } else if BUILTINS.binary_search(&word).is_ok()
            || (word.len() > 4 && word.starts_with("__") && word.ends_with("__") && !call)
        {
            Class::Variable
        } else if capitals {
            if call { Class::Function } else { Class::Constant }
        } else if capital {
            Class::Type
        } else if call {
            Class::Function
        } else {
            Class::Plain
        };
        self.emit(j, class);
    }

    /// Whether the soft keyword `word`, first in its statement and ending at `end`, acts as a
    /// keyword here: `match` and `case` before their subject or pattern on a line that ends with
    /// a `:`, and `type` before the name of the type it declares and its `=` or `[`.
    fn soft(&mut self, word: &str, end: usize) -> bool {
        match word {
            "match" | "case" => {
                let k = self.blanks(end);
                let c = self.at(k);
                // What follows a name, not a subject: `match = …`, `match.group(1)`, `case: int`.
                let name = c == 0
                    || is_break(c)
                    || b"=.,)]}:;|&^@<>%/#".contains(&c)
                    || (k == end && !matches!(c, b'(' | b'[' | b'{'));
                if name || !self.header(k) {
                    return false;
                }
                if word == "case" {
                    self.pattern = true;
                }
                true
            }
            "type" => {
                let k = self.blanks(end);
                if k == end || !is_word_start(self.at(k)) {
                    return false;
                }
                let after = self.blanks(word_end(self.b, k, self.b.len()));
                let alias = self.at(after) == b'[' || (self.at(after) == b'=' && self.at(after + 1) != b'=');
                if alias {
                    self.pending = Some(Class::Type);
                }
                alias
            }
            _ => false,
        }
    }

    /// Whether the line from `from` ends with a `:` before any comment, as a compound statement's
    /// header does. It looks no further than [`LOOK`] bytes.
    fn header(&self, from: usize) -> bool {
        let limit = self.b.len().min(from + LOOK);
        let (mut last, mut quote) = (0u8, 0u8);
        let mut i = from;
        while i < limit {
            let c = self.b[i];
            if is_break(c) || (quote == 0 && c == b'#') {
                return last == b':';
            }
            if quote != 0 && c == b'\\' {
                i += 2;
                continue;
            }
            if quote != 0 && c == quote {
                quote = 0;
            } else if quote == 0 && matches!(c, b'"' | b'\'') {
                quote = c;
            }
            if !is_space(c) {
                last = c;
            }
            i += 1;
        }
        limit == self.b.len() && last == b':'
    }

    /// An operator or punctuation.
    fn symbol(&mut self, c: u8, next: u8) {
        let i = self.pos;
        let (end, class) = match c {
            b'(' | b'[' | b'{' => {
                self.nest(true);
                (i + 1, Class::Punctuation)
            }
            b')' | b']' | b'}' => {
                self.nest(false);
                (i + 1, Class::Punctuation)
            }
            b':' if next == b'=' => (i + 2, Class::Operator),
            b'-' if next == b'>' => (i + 2, Class::Operator),
            b'.' if next == b'.' && self.at(i + 2) == b'.' => (i + 3, Class::Constant),
            b',' | b';' | b':' | b'.' | b'\\' => (i + 1, Class::Punctuation),
            _ if c.is_ascii_punctuation() => (i + 1, Class::Operator),
            _ => (i + 1, Class::Plain),
        };
        self.member = c == b'.' && end == i + 1;
        self.emit(end, class);
    }

    /// One step of a string: to its close, to its line's end where it runs no further, or to a
    /// replacement field, which interrupts it.
    fn string(&mut self, s: Str) {
        let (b, len) = (self.b, self.b.len());
        let mut i = self.pos;
        while i < len {
            let c = b[i];
            if c == b'\\' {
                let next = self.at(i + 1);
                // A field opens after a backslash all the same, and a named escape's braces are
                // its own: `\N{BULLET}`.
                if s.fields && next == b'{' {
                    i += 1;
                    continue;
                }
                if s.fields
                    && !s.raw
                    && next == b'N'
                    && self.at(i + 2) == b'{'
                    && let Some(k) = self.named(i + 3)
                {
                    i = k;
                    continue;
                }
                i += if next == b'\r' && self.at(i + 2) == b'\n' { 3 } else { 2 };
                continue;
            }
            if c == s.quote && (!s.triple || (self.at(i + 1) == c && self.at(i + 2) == c)) {
                self.emit(i + if s.triple { 3 } else { 1 }, Class::String);
                return self.pop();
            }
            if !s.triple && is_break(c) {
                self.emit(i, Class::String);
                return self.pop();
            }
            if s.fields && c == b'{' {
                if self.at(i + 1) == b'{' {
                    i += 2;
                    continue;
                }
                if self.can_nest() {
                    self.out.push(i, Class::String);
                    self.emit(i + 1, Class::Punctuation);
                    return self.stack.push(Frame::Code { field: true, depth: 0 });
                }
            }
            if s.fields && c == b'}' && self.at(i + 1) == b'}' {
                i += 2;
                continue;
            }
            i += 1;
        }
        // Never closed: it runs to the end.
        self.emit(len, Class::String);
    }

    /// Where a named escape's name, from `from`, ends past its `}`, if one closes it within
    /// [`NAME`] bytes.
    fn named(&self, from: usize) -> Option<usize> {
        let limit = self.b.len().min(from + NAME);
        let mut k = from;
        while k < limit && (self.b[k].is_ascii_alphanumeric() || matches!(self.b[k], b' ' | b'-')) {
            k += 1;
        }
        (self.at(k) == b'}').then_some(k + 1)
    }

    /// One step of a format spec: to the field's `}`, which closes the field too, or to a field
    /// of its own. A format spec ends at its line's end, and its field with it.
    fn spec(&mut self) {
        let (b, len) = (self.b, self.b.len());
        let mut i = self.pos;
        while i < len {
            match b[i] {
                b'}' => {
                    self.out.push(i, Class::String);
                    self.emit(i + 1, Class::Punctuation);
                    self.pop();
                    return self.pop();
                }
                b'{' if self.can_nest() => {
                    self.out.push(i, Class::String);
                    self.emit(i + 1, Class::Punctuation);
                    return self.stack.push(Frame::Code { field: true, depth: 0 });
                }
                c if is_break(c) => {
                    self.emit(i, Class::String);
                    self.pop();
                    return self.pop();
                }
                _ => i += 1,
            }
        }
        self.emit(len, Class::String);
    }
}

// The tables, each sorted, so a word is found by binary search (`every_table_is_sorted`).

const KEYWORDS: Table = words!(
    "python/keywords",
    [
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except",
        "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass",
        "raise", "return", "try", "while", "with", "yield",
    ]
);
const CONSTANTS: Table =
    words!("python/constants", ["Ellipsis", "False", "None", "NotImplemented", "True", "__debug__"]);
const TYPES: Table = words!(
    "python/types",
    [
        "bool",
        "bytearray",
        "bytes",
        "complex",
        "dict",
        "float",
        "frozenset",
        "int",
        "list",
        "object",
        "set",
        "str",
        "tuple",
        "type",
    ]
);
const BUILTINS: Table = words!("python/builtins", ["cls", "self"]);

/// Every table this scanner reads, in the order its file lists them (`tables::written`).
pub(super) const WORDS: &[Table] = &[KEYWORDS, CONSTANTS, TYPES, BUILTINS];

#[cfg(test)]
mod tests {
    use super::*;

    /// Each table is sorted, with no word twice, so binary search finds what it holds.
    #[test]
    fn every_table_is_sorted() {
        for table in [KEYWORDS, CONSTANTS, TYPES, BUILTINS] {
            assert!(table.windows(2).all(|w| w[0] < w[1]), "not sorted: {table:?}");
        }
        assert!(PREFIXES.windows(2).all(|w| w[0] < w[1]), "not sorted: {PREFIXES:?}");
    }
}
