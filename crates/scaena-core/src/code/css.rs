//! CSS, and SCSS on it, read by one scanner (PLAN 2.135).
//!
//! - **Selectors**: an element's name and `*` are tags; a class (`.btn`), an id (`#main`), a
//!   pseudo-class or a pseudo-element (`:hover`, `::before`), and an attribute selector's name
//!   (`[type="text"]`) are attributes, its value a string, and its flag (` i]`) a keyword; a
//!   combinator (`>`, `+`, `~`) is an operator; and `from` and `to` in a keyframe are keywords.
//! - **Declarations**: a property, before its `:`, is a property, and a custom property (`--gap`)
//!   a variable. In a value, a number with its unit (`12px`, `1.5em`, `100%`) is a number; a color
//!   (`#fff`, `#3366ccaa`) a constant, wherever it stands; a function (`rgb(`, `calc(`, `var(`) a
//!   function, and the address `url(…)` holds a string; a keyword (`flex`, `solid`, `auto`) a
//!   constant; a string a string; and `!important` a keyword.
//! - **At-rules** (`@media`, `@import`, `@keyframes`) are keywords. In their preludes `and`,
//!   `not`, `only`, and `or` are keywords too, and a feature before its `:` (`(min-width: 600px)`)
//!   a property.
//! - **Comments**: `/* … */`, and in SCSS `//` to the line's end.
//! - **SCSS**: `$variables`, and a module's (`math.$pi`); `@mixin`, `@include`, `@function`, `@if`,
//!   `@each`, and their kin, whose mixin's or function's name is a function, and `in`, `from`, and
//!   `through` keywords in them; `@extend` and `@at-root`, which name selectors, and `@extend`'s
//!   `!optional`; `!default` and `!global` keywords; `&` a keyword and a placeholder (`%name`) an
//!   attribute; a property whose block holds its parts (`font: { family: … }`) a property; a map's
//!   keys (`(small: 576px)`) properties; and interpolation, `#{…}`, in selectors, properties, values,
//!   and strings, which holds a value, a name going on after it as it began (`.#{$block}__item`).
//!
//! A statement is a declaration or a rule, nested ones too, by what ends it: a `{` begins a rule,
//! and a `;` or a block's `}` ends a declaration. A statement is looked through once for that
//! end, and read to it. At most [`MAX_NESTING`] interpolations stand open inside one another; what
//! would open one more is read as what it stands in.

use super::scan::{Spans, block_comment_end, byte, is_break, is_space, is_word, is_word_start, line_end, space_end};
use super::tables::{Table, words};
use super::{Class, MAX_NESTING};

/// What is being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Between statements: what comes next begins one.
    Statement,
    Selector,
    /// A declaration's property, to its `:`.
    Property,
    /// A declaration's value, an interpolation's, or an at-rule's prelude: `at` says which rule's.
    Value(At),
}

/// Whose value a value is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum At {
    /// A declaration's, or an interpolation's.
    Declaration,
    /// An at-rule's prelude: `@media screen and (…)`.
    Rule,
    /// A mixin's or a function's prelude, whose first name is a function: `@include button(…)`.
    Named,
    /// `@extend`'s and `@at-root`'s, which name selectors.
    Selector,
}

/// What comes next in a selector's attribute, `[…]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bracket {
    /// Its name: `[type`.
    Name,
    /// Its value, after its operator: `="text"`.
    Value,
    /// Its flag, after its value: ` i]`.
    Flag,
}

/// What the scanner returns to.
#[derive(Debug, Clone, Copy)]
enum Frame {
    /// The text's own code, at the bottom of the stack.
    Code,
    /// An interpolation, `#{…}`, to its `}`: what the code around it goes on with.
    Interp { mode: Mode, parens: u32 },
    /// A string in `quote`.
    Str { quote: u8 },
}

/// Writes `text`'s tokens as CSS, or as SCSS where `scss` is set.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>, scss: bool) {
    let mut scanner = Scanner {
        b: text.as_bytes(),
        pos: 0,
        out,
        scss,
        stack: vec![Frame::Code],
        mode: Mode::Statement,
        parens: 0,
        depth: 0,
        until: 0,
        bracket: None,
        named: false,
        part: Class::Tag,
        resumed: false,
    };
    scanner.run();
}

struct Scanner<'t, 'o> {
    b: &'t [u8],
    pos: usize,
    out: &'o mut Spans<'t>,
    scss: bool,
    stack: Vec<Frame>,
    mode: Mode,
    /// The parentheses and brackets open in the statement, or in the interpolation on top.
    parens: u32,
    /// The blocks open.
    depth: u32,
    /// Where the statement being read ends: its `{`, `;`, or `}`, or the text's end.
    until: usize,
    /// In a selector's `[…]`: what comes next.
    bracket: Option<Bracket>,
    /// Whether a mixin's or a function's name has been read in its at-rule's prelude.
    named: bool,
    /// The class of the selector's last name, which a name goes on in after an interpolation:
    /// `.#{$block}__item`.
    part: Class,
    /// Whether an interpolation closed just before, so a name here goes on the one before it.
    resumed: bool,
}

impl Scanner<'_, '_> {
    fn run(&mut self) {
        while self.pos < self.b.len() {
            let (pos, frames) = (self.pos, self.stack.len());
            match self.stack.last().copied().unwrap_or(Frame::Code) {
                Frame::Str { quote } => self.string(quote),
                Frame::Code | Frame::Interp { .. } => self.code(),
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

    /// Whether one more interpolation may open.
    fn can_nest(&self) -> bool {
        self.stack.iter().filter(|f| matches!(f, Frame::Interp { .. })).count() < MAX_NESTING
    }

    /// Whether a comment begins at `i`: `/*`, or in SCSS `//`.
    fn comment(&self, i: usize) -> Option<usize> {
        let (b, len) = (self.b, self.b.len());
        match (b[i], self.at(i + 1)) {
            (b'/', b'*') => Some(block_comment_end(b, i, len, false)),
            (b'/', b'/') if self.scss => Some(line_end(b, i, len)),
            _ => None,
        }
    }

    /// One step of code: the text's own, or an interpolation's.
    fn code(&mut self) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let c = b[i];
        let resumed = std::mem::take(&mut self.resumed);
        if is_space(c) {
            let to = space_end(b, i, len);
            self.out.space(to);
            self.pos = to;
            return;
        }
        if let Some(end) = self.comment(i) {
            return self.emit(end, Class::Comment);
        }
        let top = self.stack.len() == 1;
        if !top {
            // An interpolation's own close.
            if c == b'}' && self.parens == 0 {
                self.emit(i + 1, Class::Punctuation);
                if let Some(Frame::Interp { mode, parens }) = self.stack.pop() {
                    (self.mode, self.parens) = (mode, parens);
                }
                self.resumed = true;
                return;
            }
        } else if self.mode != Mode::Statement && i >= self.until {
            // The statement ends at the end it was looked through for.
            self.mode = Mode::Statement;
            if i == self.until {
                return self.end(c);
            }
        }
        if self.mode == Mode::Statement {
            return self.statement();
        }
        if matches!(c, b'"' | b'\'') {
            if self.bracket == Some(Bracket::Value) {
                self.bracket = Some(Bracket::Flag);
            }
            self.emit(i + 1, Class::String);
            return self.stack.push(Frame::Str { quote: c });
        }
        if self.scss && c == b'#' && self.at(i + 1) == b'{' && self.can_nest() {
            return self.interpolate();
        }
        if self.scss && c == b'$' && is_word_start(self.at(i + 1)) {
            return self.emit(ident_end(b, i + 1), Class::Variable);
        }
        match self.mode {
            Mode::Selector | Mode::Value(At::Selector) => self.selector(resumed),
            Mode::Property => self.property(),
            Mode::Value(at) => self.value(at),
            Mode::Statement => {}
        }
    }

    /// `#{` here: an interpolation opens, which holds a value.
    fn interpolate(&mut self) {
        self.emit(self.pos + 2, Class::Punctuation);
        self.stack.push(Frame::Interp { mode: self.mode, parens: self.parens });
        (self.mode, self.parens) = (Mode::Value(At::Declaration), 0);
    }

    /// The end of a statement, `c` at `until`: a block opens, closes, or a `;` ends it.
    fn end(&mut self, c: u8) {
        match c {
            b'{' => self.depth = self.depth.saturating_add(1),
            b'}' => self.depth = self.depth.saturating_sub(1),
            _ => {}
        }
        self.emit(self.pos + 1, Class::Punctuation);
    }

    /// What begins here: an at-rule, a block's close, or a statement, looked through for its end,
    /// which says whether it is a rule or a declaration.
    fn statement(&mut self) {
        let (b, i) = (self.b, self.pos);
        let c = b[i];
        if matches!(c, b'}' | b';' | b'{') {
            return self.end(c);
        }
        let (until, close) = self.ahead(i);
        self.until = until;
        (self.parens, self.bracket, self.named) = (0, None, false);
        if c == b'@' && is_word_start(self.at(i + 1)) {
            let e = ident_end(b, i + 1);
            let at = match &b[i + 1..e] {
                b"mixin" | b"include" | b"function" => At::Named,
                b"extend" | b"at-root" => At::Selector,
                _ => At::Rule,
            };
            self.mode = Mode::Value(at);
            return self.emit(e, Class::Keyword);
        }
        self.mode = match close {
            b'{' => Mode::Selector,
            0 if self.depth == 0 => Mode::Selector,
            _ => Mode::Property,
        };
        self.code();
    }

    /// Where the statement from `at` ends, and how: at the first `{`, `;`, or `}` outside strings,
    /// comments, escapes, `url(…)`, parentheses, brackets, and interpolations; or at the text's
    /// end, as 0.
    fn ahead(&self, at: usize) -> (usize, u8) {
        let (b, len) = (self.b, self.b.len());
        let (mut parens, mut interps) = (0u32, 0u32);
        let mut i = at;
        while i < len {
            let c = b[i];
            if let Some(end) = self.comment(i) {
                i = end;
                continue;
            }
            match c {
                b'"' | b'\'' => i = string_end(b, i),
                // An escape, which a name holds: `.md\:flex`, `.a\{`.
                b'\\' => i += 2,
                b'#' if self.scss && self.at(i + 1) == b'{' => {
                    interps += 1;
                    i += 2;
                }
                b'}' if interps > 0 => {
                    interps -= 1;
                    i += 1;
                }
                b'(' if url(b, i) => i = url_end(b, i + 1),
                b'(' | b'[' => {
                    parens += 1;
                    i += 1;
                }
                b')' | b']' => {
                    parens = parens.saturating_sub(1);
                    i += 1;
                }
                b'{' | b';' | b'}' if parens == 0 && interps == 0 => return (i, c),
                _ => i += 1,
            }
        }
        (len, 0)
    }

    /// One step of a selector; `resumed` where an interpolation closed just before it.
    fn selector(&mut self, resumed: bool) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let c = b[i];
        let next = self.at(i + 1);
        if let Some(bracket) = self.bracket {
            // An attribute selector: its name, its operator, its value, and its flag.
            let (to, class) = match c {
                b']' => {
                    self.bracket = None;
                    (i + 1, Class::Punctuation)
                }
                b'=' | b'~' | b'|' | b'^' | b'$' | b'*' => {
                    self.bracket = Some(Bracket::Value);
                    (i + 1 + usize::from(c != b'=' && next == b'='), Class::Operator)
                }
                _ if ident_start(b, i) => {
                    let class = match bracket {
                        Bracket::Name => Class::Attribute,
                        Bracket::Value => Class::String,
                        Bracket::Flag => Class::Keyword,
                    };
                    if bracket == Bracket::Value {
                        self.bracket = Some(Bracket::Flag);
                    }
                    (ident_end(b, i), class)
                }
                _ => (i + 1, Class::Plain),
            };
            return self.emit(to, class);
        }
        let (to, class) = match c {
            // A name that goes on after an interpolation, as the one before it: `.#{$block}__item`.
            _ if resumed && (is_word(c) || c == b'-') => (ident_end(b, i), self.part),
            b'.' | b'#' | b'%' if ident_start(b, i + 1) => (ident_end(b, i + 1), Class::Attribute),
            // A class or a placeholder whose name an interpolation begins: `.#{$block}`.
            b'.' | b'%' if self.scss && next == b'#' && self.at(i + 2) == b'{' => (i + 1, Class::Attribute),
            b':' => {
                let k = i + 1 + usize::from(next == b':');
                if ident_start(b, k) { (ident_end(b, k), Class::Attribute) } else { (k, Class::Punctuation) }
            }
            b'&' => (i + 1, Class::Keyword),
            // `@extend`'s flag: `!optional`.
            b'!' if is_word_start(next) => (ident_end(b, i + 1), Class::Keyword),
            // A suffix to the parent's name: `&-title`, `&__item`.
            b'-' | b'_' if i > 0 && b[i - 1] == b'&' => (ident_end(b, i), Class::Attribute),
            b'[' => {
                self.bracket = Some(Bracket::Name);
                (i + 1, Class::Punctuation)
            }
            b'(' | b')' => {
                self.parens = if c == b'(' { self.parens.saturating_add(1) } else { self.parens.saturating_sub(1) };
                (i + 1, Class::Punctuation)
            }
            b'*' => (i + 1, Class::Tag),
            b'>' | b'+' | b'~' | b'|' => (i + 1, Class::Operator),
            b',' | b';' | b'{' | b'}' => (i + 1, Class::Punctuation),
            _ if c.is_ascii_digit() || (c == b'.' && next.is_ascii_digit()) => (number_end(b, i), Class::Number),
            _ if ident_start(b, i) => {
                let e = ident_end(b, i);
                let class = if matches!(&b[i..e], b"from" | b"to") {
                    Class::Keyword
                } else if self.at(e) == b':' && matches!(self.at(e + 1), b' ' | b'\t' | b'\n' | b'\r' | b'{') {
                    // A property whose block holds its parts: `font: { family: … }`.
                    Class::Property
                } else {
                    Class::Tag
                };
                (e, class)
            }
            _ => (i + 1, Class::Plain),
        };
        if matches!(class, Class::Attribute | Class::Tag) {
            self.part = class;
        }
        self.emit(to.min(len).max(i + 1), class);
    }

    /// One step of a declaration's property, to its `:`.
    fn property(&mut self) {
        let (b, i) = (self.b, self.pos);
        let c = b[i];
        let (to, class) = match c {
            b':' => {
                self.mode = Mode::Value(At::Declaration);
                (i + 1, Class::Punctuation)
            }
            b'-' if self.at(i + 1) == b'-' => (ident_end(b, i), Class::Variable),
            _ if ident_start(b, i) => (ident_end(b, i), Class::Property),
            _ => (i + 1, Class::Plain),
        };
        self.emit(to, class);
    }

    /// One step of a value: a declaration's, an interpolation's, or an at-rule's prelude.
    fn value(&mut self, at: At) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let c = b[i];
        let next = self.at(i + 1);
        // Whether what came just before ends an operand, so a sign after it is an operator.
        let operand = i > 0 && (is_word(b[i - 1]) || matches!(b[i - 1], b')' | b'%' | b']'));
        let (to, class) = match c {
            _ if c.is_ascii_digit() || (c == b'.' && next.is_ascii_digit()) => (number_end(b, i), Class::Number),
            b'+' | b'-' if !operand && (next.is_ascii_digit() || (next == b'.' && self.at(i + 2).is_ascii_digit())) => {
                (number_end(b, i + 1), Class::Number)
            }
            b'#' => {
                let e = ident_end(b, i + 1);
                let hex = matches!(e - i - 1, 3 | 4 | 6 | 8) && b[i + 1..e].iter().all(u8::is_ascii_hexdigit);
                (e.max(i + 1), if hex { Class::Constant } else { Class::Plain })
            }
            b'!' if is_word_start(next) => (ident_end(b, i + 1), Class::Keyword),
            b'&' => (i + 1, Class::Keyword),
            b'(' | b'[' => {
                self.parens = self.parens.saturating_add(1);
                (i + 1, Class::Punctuation)
            }
            b')' | b']' => {
                self.parens = self.parens.saturating_sub(1);
                (i + 1, Class::Punctuation)
            }
            b',' | b':' | b';' | b'{' | b'}' => (i + 1, Class::Punctuation),
            _ if ident_start(b, i) => return self.name(at),
            _ if c.is_ascii_punctuation() => {
                let pair = matches!((c, next), (b'=' | b'!' | b'<' | b'>', b'='));
                (i + 1 + usize::from(pair), Class::Operator)
            }
            _ => (i + 1, Class::Plain),
        };
        self.emit(to.min(len).max(i + 1), class);
    }

    /// A name in a value: a function called, `url(…)` and the address it holds, a variable, a key
    /// before its `:` in parentheses, a keyword of a prelude, or a constant.
    fn name(&mut self, at: At) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let mut e = ident_end(b, i);
        // A module's member in SCSS: `math.div(`.
        while self.scss && self.at(e) == b'.' && is_word_start(self.at(e + 1)) {
            e = ident_end(b, e + 1);
        }
        // A module's variable in SCSS: `math.$pi`.
        if self.scss && self.at(e) == b'.' && self.at(e + 1) == b'$' && is_word_start(self.at(e + 2)) {
            return self.emit(ident_end(b, e + 2), Class::Variable);
        }
        let word = &b[i..e];
        let first = at == At::Named && !std::mem::replace(&mut self.named, true);
        if self.at(e) == b'(' {
            self.emit(e, Class::Function);
            // An address with no quotes: a string to its `)`.
            if url(b, e) {
                self.emit(e + 1, Class::Punctuation);
                let start = space_end(b, e + 1, len);
                self.out.space(start);
                self.pos = start;
                let close = url_end(b, start);
                let address = if self.at(close - 1) == b')' { close - 1 } else { close };
                self.emit(address, Class::String);
                self.emit(close, Class::Punctuation);
            }
            return;
        }
        let keyed = self.parens > 0 && self.at(blanks(b, e)) == b':' && self.at(blanks(b, e) + 1) != b':';
        // A prelude's keywords; in a value, SCSS's `and`, `or`, and `not`.
        let keyword = match at {
            At::Declaration => self.scss && matches!(word, b"and" | b"not" | b"or"),
            _ => KEYWORDS.holds(word),
        };
        let class = if first {
            Class::Function
        } else if word.starts_with(b"--") {
            Class::Variable
        } else if keyed {
            Class::Property
        } else if keyword {
            Class::Keyword
        } else {
            Class::Constant
        };
        self.emit(e, class);
    }

    /// One step of a string: to its close, to its line's end, where it ends unless a backslash
    /// escapes the break, or in SCSS to an interpolation, which interrupts it.
    fn string(&mut self, quote: u8) {
        let (b, len) = (self.b, self.b.len());
        self.resumed = false;
        let mut i = self.pos;
        while i < len {
            let c = b[i];
            if c == quote {
                self.emit(i + 1, Class::String);
                self.stack.pop();
                return;
            }
            if is_break(c) {
                self.emit(i, Class::String);
                self.stack.pop();
                return;
            }
            if c == b'\\' {
                i += 2;
                continue;
            }
            if self.scss && c == b'#' && self.at(i + 1) == b'{' && self.can_nest() {
                self.out.push(i, Class::String);
                self.pos = i;
                return self.interpolate();
            }
            i += 1;
        }
        self.emit(len, Class::String);
    }
}

/// Whether a name begins at `i`: a letter, `_`, any byte above ASCII, an escape, or a `-` before
/// one of them or another `-` (`-webkit-box`, `--gap`).
fn ident_start(b: &[u8], i: usize) -> bool {
    let c = byte(b, i);
    is_word_start(c)
        || (c == b'\\' && !is_break(byte(b, i + 1)) && i + 1 < b.len())
        || (c == b'-' && (is_word_start(byte(b, i + 1)) || byte(b, i + 1) == b'-' || byte(b, i + 1) == b'\\'))
}

/// Where the name at `at` ends: its letters, digits, `-`, `_`, and escapes (`.md\:flex`).
fn ident_end(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while i < b.len() {
        if is_word(b[i]) || b[i] == b'-' {
            i += 1;
        } else if b[i] == b'\\' && i + 1 < b.len() && !is_break(b[i + 1]) {
            i += 2;
        } else {
            break;
        }
    }
    i.min(b.len())
}

/// Where the spaces and tabs at `at` end.
fn blanks(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while matches!(byte(b, i), b' ' | b'\t') {
        i += 1;
    }
    i
}

/// Where the number at `at` ends, with its fraction, its exponent, and its unit or `%`.
fn number_end(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while byte(b, i).is_ascii_digit() {
        i += 1;
    }
    if byte(b, i) == b'.' && byte(b, i + 1).is_ascii_digit() {
        i += 1;
        while byte(b, i).is_ascii_digit() {
            i += 1;
        }
    }
    let sign = usize::from(matches!(byte(b, i + 1), b'+' | b'-'));
    if matches!(byte(b, i), b'e' | b'E') && byte(b, i + 1 + sign).is_ascii_digit() {
        i += 1 + sign;
        while byte(b, i).is_ascii_digit() {
            i += 1;
        }
    }
    if byte(b, i) == b'%' {
        return i + 1;
    }
    while byte(b, i).is_ascii_alphabetic() {
        i += 1;
    }
    i
}

/// Where the string at `at` ends: past its close, a backslash escaping the byte after it, or at its
/// line's end.
fn string_end(b: &[u8], at: usize) -> usize {
    let quote = b[at];
    let mut i = at + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            c if c == quote => return i + 1,
            c if is_break(c) => return i,
            _ => i += 1,
        }
    }
    b.len()
}

/// Whether the `(` at `i` opens an address with no quotes: `url(` before anything but a quote.
fn url(b: &[u8], i: usize) -> bool {
    i >= 3 && b[i - 3..i].eq_ignore_ascii_case(b"url") && !matches!(byte(b, blanks(b, i + 1)), b'"' | b'\'')
}

/// Where an address with no quotes from `at`, past its `(`, ends: past its `)`, or at its line's
/// end.
fn url_end(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while i < b.len() && !is_break(b[i]) {
        if b[i] == b')' {
            return i + 1;
        }
        i += 1;
    }
    i
}

/// The keywords of at-rules' preludes and of SCSS's expressions, sorted.
const KEYWORDS: Table = words!(
    "css/keywords",
    ["and", "as", "from", "hide", "if", "in", "not", "only", "or", "show", "through", "to", "with"]
);

/// The tables above, in the file they are handed over in (`code::tables`).
pub(super) const WORDS: &[Table] = &[KEYWORDS];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keywords_are_sorted() {
        assert!(KEYWORDS.windows(2).all(|w| w[0] < w[1]));
    }
}
