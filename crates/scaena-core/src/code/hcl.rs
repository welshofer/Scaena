//! HCL, as Terraform, OpenTofu, Packer, and Nomad write it (PLAN 2.135).
//!
//! - **Blocks**: the word that begins one (`resource`, `variable`, `lifecycle`) is a keyword, and
//!   its labels strings: `resource "aws_instance" "web" {`.
//! - **Attributes**: a name before its `=`, in a block or an object, is a property.
//! - **Expressions**: `true`, `false`, and `null` are constants; a type's name (`string`,
//!   `list(string)`) a type; a function called (`lookup(…)`) a function; the roots of references
//!   (`var`, `local`, `module`, `data`, `each`, `count`, `self`, `path`, `terraform`) variables,
//!   and what a `.` reaches properties; and `for`, `in`, and `if` keywords.
//! - **Strings** are templates: `"…"`, with escapes, to its close or its line's end; and heredocs,
//!   `<<EOT` and `<<-EOT`, from the line after the opener to the line that is the delimiter alone,
//!   spaces and tabs before it. An interpolation, `${…}`, and a directive, `%{ if … }`, hold an
//!   expression; `$${` and `%%{` are the string's own.
//! - **Comments**: `#` and `//` to the line's end, and `/* … */`.
//!
//! At most [`MAX_NESTING`] interpolations and directives stand open inside one another, and as many
//! heredocs wait for their bodies; what would open one more is read as what it stands in. The
//! kinds of the outermost brackets are kept as TOML's are, so a block's body is told from an
//! object.

use super::scan::{
    Nest, Spans, blanks_end, block_comment_end, byte, is_break, is_space, is_word, is_word_start, line_end, line_next,
    number_end, space_end,
};
use super::{Class, MAX_NESTING};

/// What a bracket opens, in the text's own code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bracket {
    /// A block's body: `resource "x" "y" { … }`, whose lines begin with attributes and blocks.
    Body,
    /// An object, a tuple, or a call's arguments, which hold an expression.
    Expression,
}

/// A heredoc, from its opener: where its delimiter is written, and its length.
#[derive(Debug, Clone, Copy)]
struct Here {
    from: usize,
    len: usize,
}

/// What the scanner returns to.
#[derive(Debug, Clone, Copy)]
enum Frame {
    /// Code: the text's own at the bottom of the stack, else an interpolation's or a directive's,
    /// which `}` closes where `depth` brackets inside it are open.
    Code { close: bool, depth: u32 },
    /// A string in quotes.
    Str,
    /// A heredoc's body.
    Here(Here),
}

const TOP: Frame = Frame::Code { close: false, depth: 0 };

/// Writes `text`'s tokens as HCL.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let mut scanner = Scanner {
        text,
        b: text.as_bytes(),
        pos: 0,
        out,
        stack: vec![TOP],
        nest: Nest::new(),
        statement: true,
        header: false,
        member: false,
        heres: Vec::new(),
    };
    scanner.run();
}

struct Scanner<'t, 'o> {
    text: &'t str,
    b: &'t [u8],
    pos: usize,
    out: &'o mut Spans<'t>,
    stack: Vec<Frame>,
    /// The brackets open in the text's own code.
    nest: Nest<Bracket>,
    /// Whether an attribute or a block may begin here: first on a line in a body.
    statement: bool,
    /// Whether a block's word came last, so its labels and its body follow.
    header: bool,
    /// Whether a `.` came last, so a word is an attribute it reaches.
    member: bool,
    /// The heredocs whose bodies start on the next line, in the order they opened.
    heres: Vec<Here>,
}

impl Scanner<'_, '_> {
    fn run(&mut self) {
        while self.pos < self.b.len() {
            let (pos, frames) = (self.pos, self.stack.len());
            match self.stack.last().copied().unwrap_or(TOP) {
                Frame::Code { close, depth } => self.code(close, depth),
                Frame::Str => self.string(),
                Frame::Here(h) => self.here(h),
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
        byte(self.b, i)
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

    /// Whether one more interpolation or directive may open.
    fn can_nest(&self) -> bool {
        self.stack.iter().filter(|f| matches!(f, Frame::Code { close: true, .. })).count() < MAX_NESTING
    }

    /// One more bracket open in an interpolation, or one fewer.
    fn nest(&mut self, open: bool) {
        if let Some(Frame::Code { depth, .. }) = self.stack.last_mut() {
            *depth = if open { depth.saturating_add(1) } else { depth.saturating_sub(1) };
        }
    }

    /// Whether the text's own code is in a body, where a line begins with an attribute or a block.
    fn in_body(&self) -> bool {
        self.stack.len() == 1 && self.nest.top().is_none_or(|b| b == Bracket::Body)
    }

    /// One step of code.
    fn code(&mut self, close: bool, depth: u32) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let c = b[i];
        let next = self.at(i + 1);
        if is_space(c) {
            let end = space_end(b, i, len);
            if let Some(k) = b[i..end].iter().position(|&c| is_break(c)) {
                self.statement = self.in_body();
                self.header = false;
                // A heredoc's body starts on the line after its opener.
                if !self.heres.is_empty() {
                    let body = line_next(b, i + k, len);
                    self.out.space(body);
                    self.pos = body;
                    let here = self.heres.remove(0);
                    return self.stack.push(Frame::Here(here));
                }
            }
            self.out.space(end);
            self.pos = end;
            return;
        }
        if c == b'#' || (c == b'/' && next == b'/') {
            return self.emit(line_end(b, i, len), Class::Comment);
        }
        if c == b'/' && next == b'*' {
            return self.emit(block_comment_end(b, i, len, false), Class::Comment);
        }
        let statement = std::mem::take(&mut self.statement);
        let member = std::mem::take(&mut self.member);
        // An interpolation's or a directive's own close.
        if close && depth == 0 && c == b'}' {
            self.emit(i + 1, Class::Punctuation);
            return self.pop();
        }
        match c {
            b'"' => {
                self.emit(i + 1, Class::String);
                self.stack.push(Frame::Str);
            }
            b'<' if next == b'<' && self.heredoc() => {}
            _ if c.is_ascii_digit() => self.emit(number_end(b, i, len, false), Class::Number),
            _ if is_word_start(c) => self.word(statement, member),
            b'{' | b'(' | b'[' => {
                if close {
                    self.nest(true);
                } else if c == b'{' && std::mem::take(&mut self.header) {
                    self.nest.push(Bracket::Body);
                    self.statement = true;
                } else {
                    self.nest.push(Bracket::Expression);
                }
                self.emit(i + 1, Class::Punctuation);
            }
            b'}' | b')' | b']' => {
                if close {
                    self.nest(false);
                } else {
                    self.nest.pop();
                }
                self.emit(i + 1, Class::Punctuation);
            }
            b'.' if next == b'.' && self.at(i + 2) == b'.' => self.emit(i + 3, Class::Operator),
            b'.' => {
                self.member = is_word_start(next);
                self.emit(i + 1, Class::Punctuation);
            }
            b',' => self.emit(i + 1, Class::Punctuation),
            _ if c.is_ascii_punctuation() => self.emit(i + 1, Class::Operator),
            _ => self.emit(i + 1, Class::Plain),
        }
    }

    /// A word: a block's, an attribute's name, a label, or what else its tables and what follows
    /// it make it.
    fn word(&mut self, statement: bool, member: bool) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let mut j = i;
        while j < len && (is_word(b[j]) || b[j] == b'-') {
            j += 1;
        }
        let word = self.text.get(i..j).unwrap_or_default();
        let after = blanks_end(b, j, len);
        let (c, next) = (self.at(after), self.at(after + 1));
        let assigned = c == b'=' && !matches!(next, b'=' | b'>');
        let class = if member || assigned {
            Class::Property
        } else if statement && self.in_body() && (c == b'{' || c == b'"' || is_word_start(c)) {
            self.header = true;
            Class::Keyword
        } else if self.header {
            Class::Plain
        } else if KEYWORDS.binary_search(&word).is_ok() {
            Class::Keyword
        } else if CONSTANTS.binary_search(&word).is_ok() {
            Class::Constant
        } else if TYPES.binary_search(&word).is_ok() {
            Class::Type
        } else if c == b'(' {
            Class::Function
        } else if VARIABLES.binary_search(&word).is_ok() && self.at(j) == b'.' {
            Class::Variable
        } else {
            Class::Plain
        };
        self.emit(j, class);
    }

    /// A heredoc's opener here, `<<EOT` or `<<-EOT` and its line's end, read, its body waiting for
    /// the next line; false where `<<` opens none, or as many wait as may.
    fn heredoc(&mut self) -> bool {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let dash = self.at(i + 2) == b'-';
        let from = i + 2 + usize::from(dash);
        if !is_word_start(self.at(from)) || self.heres.len() >= MAX_NESTING {
            return false;
        }
        let mut e = from;
        while e < len && (is_word(b[e]) || b[e] == b'-') {
            e += 1;
        }
        // Nothing but spaces and tabs may follow it on its line.
        let rest = blanks_end(b, e, len);
        if rest < len && !is_break(b[rest]) {
            return false;
        }
        self.heres.push(Here { from, len: e - from });
        self.emit(from, Class::Operator);
        self.emit(e, Class::String);
        true
    }

    /// One step of a string in quotes: to its close, to its line's end, where it ends, or to an
    /// interpolation or a directive, which interrupts it.
    fn string(&mut self) {
        let (b, len) = (self.b, self.b.len());
        let mut i = self.pos;
        while i < len {
            match b[i] {
                b'"' => {
                    self.emit(i + 1, Class::String);
                    return self.pop();
                }
                c if is_break(c) => {
                    self.emit(i, Class::String);
                    return self.pop();
                }
                b'\\' if !is_break(self.at(i + 1)) => i += 2,
                _ => match self.template(i) {
                    Some(skip) => i = skip,
                    None => return,
                },
            }
        }
        self.emit(len, Class::String);
    }

    /// At `i` in a template: past `$${` or `%%{`, which are its own, or past a byte; `None` where
    /// an interpolation or a directive opens, read.
    fn template(&mut self, i: usize) -> Option<usize> {
        let (c, next) = (self.b[i], self.at(i + 1));
        if matches!(c, b'$' | b'%') && next == c && self.at(i + 2) == b'{' {
            return Some(i + 3);
        }
        if matches!(c, b'$' | b'%') && next == b'{' && self.can_nest() {
            self.out.push(i, Class::String);
            self.emit(i + 2, Class::Punctuation);
            self.stack.push(Frame::Code { close: true, depth: 0 });
            return None;
        }
        Some(i + 1)
    }

    /// One step of a heredoc's body: its terminator, or a line of it, to its end or to an
    /// interpolation or a directive.
    fn here(&mut self, h: Here) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        if i == 0 || is_break(b[i - 1]) {
            let k = blanks_end(b, i, len);
            let close = k + h.len;
            if close <= len && (close == len || is_break(b[close])) && b[k..close] == b[h.from..h.from + h.len] {
                self.emit(close, Class::String);
                return self.pop();
            }
        }
        let mut j = i;
        while j < len {
            if is_break(b[j]) {
                return self.emit(line_next(b, j, len), Class::String);
            }
            match self.template(j) {
                Some(skip) => j = skip,
                None => return,
            }
        }
        self.emit(len, Class::String);
    }
}

// The tables, each sorted, so a word is found by binary search (`every_table_is_sorted`).

/// The keywords of expressions and of a template's directives.
const KEYWORDS: &[&str] = &["else", "endfor", "endif", "for", "if", "in"];
const CONSTANTS: &[&str] = &["false", "null", "true"];
const TYPES: &[&str] = &["any", "bool", "list", "map", "number", "object", "set", "string", "tuple"];
/// The names references begin with.
const VARIABLES: &[&str] = &["count", "data", "each", "local", "module", "path", "self", "terraform", "var"];

#[cfg(test)]
mod tests {
    use super::*;

    /// Each table is sorted, with no word twice, so binary search finds what it holds.
    #[test]
    fn every_table_is_sorted() {
        for table in [KEYWORDS, CONSTANTS, TYPES, VARIABLES] {
            assert!(table.windows(2).all(|w| w[0] < w[1]), "not sorted: {table:?}");
        }
    }
}
