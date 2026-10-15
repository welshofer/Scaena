//! Ruby, read by a scanner of its own (PLAN 2.134).
//!
//! - **Strings** run on over lines, as Ruby's do. `"…"` and backquotes interpolate `#{…}`, and
//!   `#@name`, `#@@name`, and `#$name`; `'…'` does not. A `%`-literal takes any delimiter, and
//!   its pair where it has one, nested inside it: `%w[…]`, `%i(…)`, `%q{…}`, `%Q<…>`, `%r|…|`,
//!   `%(…)`, `%s`, and `%x`. Those of symbols, `%i`, `%I`, and `%s`, are constants.
//! - **Heredocs**: `<<~ID`, `<<-ID`, and `<<ID`, the identifier maybe in quotes, `<<~'ID'`
//!   interpolating nothing. The body starts on the line after its opener and runs to a line that
//!   is the identifier alone, indented where `~` or `-` lets it be.
//! - **Regular expressions** in `/…/` where an operand may start, or as a command's argument, a
//!   space before them and none after (`split /,\s*/`), with their flags.
//! - **Symbols** (`:name`, `:"…"`, `:<=>`) and a hash's keys (`name:`) are constants, and
//!   `@name`, `@@name`, and `$name` are variables.
//! - A word is a keyword, a constant, or `self` where the tables hold it; the methods Ruby's code
//!   is written with as keywords are (`attr_reader`, `private`, `require`) are keywords. After
//!   `def`, a method's name is a function, an operator's too (`def <=>(other)`); after a `.` or
//!   `&.`, a method, which is a function; after `::`, a type, a constant, or a method. Else, by
//!   its case and what follows it: a constant in capitals (`MAX_SIZE`), a type that starts with
//!   one (`Comparable`), a function where a `(` or an argument follows (`puts "hi"`), or plain.
//! - `=begin` … `=end` is a comment, and the lines after `__END__` are plain.
//!
//! At most [`MAX_NESTING`] interpolations stand open inside one another, and as many heredocs wait
//! for their bodies; what would open one more is read as what it stands in.

use super::scan::{
    Spans, blanks_end, is_break, is_space, is_word, is_word_start, line_end, line_next, number_end, space_end, word_end,
};
use super::{Class, MAX_NESTING};

/// How far a regular expression is looked for before `/` is taken to divide.
const LOOK: usize = 256;

/// How a string, a symbol, or a regular expression reads once open.
#[derive(Debug, Clone, Copy)]
struct Str {
    /// What closes it.
    close: u8,
    /// What opens one more pair inside it, which `close` closes first: `%w[a [b]]`. 0 where its
    /// delimiters are no pair.
    open: u8,
    /// How many pairs are open inside it.
    depth: u32,
    /// Whether `#{…}` interpolates.
    interp: bool,
    /// Its class: a string's, or a constant's for a symbol.
    class: Class,
    /// Whether it is a regular expression, which flags follow.
    regex: bool,
    /// Whether a regular expression's set is open, in which `/` closes nothing: `/[\/]/`.
    set: bool,
}

/// A heredoc, from its opener.
#[derive(Debug, Clone, Copy)]
struct Here {
    /// Where its identifier is written, and its length.
    from: usize,
    len: usize,
    /// Whether its terminator may be indented: `<<~` and `<<-`.
    indent: bool,
    /// Whether it interpolates: an identifier in no quotes, or in double quotes.
    interp: bool,
}

/// What the scanner returns to.
#[derive(Debug, Clone, Copy)]
enum Frame {
    /// Code: the text's own at the bottom of the stack, else an interpolation's, which `}` closes
    /// where `depth` brackets inside it are open.
    Code { close: bool, depth: u32 },
    /// A string: on top as it is read, below the interpolation that interrupts it.
    Str(Str),
    /// A heredoc's body.
    Here(Here),
}

const TOP: Frame = Frame::Code { close: false, depth: 0 };

/// Writes `text`'s tokens as Ruby.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let mut scanner = Scanner {
        text,
        b: text.as_bytes(),
        pos: 0,
        out,
        stack: vec![TOP],
        operand: false,
        member: false,
        scope: false,
        pending: None,
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
    /// Whether what came last ends an operand, so `/` divides and `%` is a remainder.
    operand: bool,
    /// Whether a `.` or `&.` came last, so a word is a method.
    member: bool,
    /// Whether a `::` came last.
    scope: bool,
    /// What the next word names, after `class` or `module`.
    pending: Option<Class>,
    /// The heredocs whose bodies start on the next line, in the order they opened.
    heres: Vec<Here>,
}

impl Scanner<'_, '_> {
    fn run(&mut self) {
        while self.pos < self.b.len() {
            let (pos, frames) = (self.pos, self.stack.len());
            match self.stack.last().copied().unwrap_or(TOP) {
                Frame::Code { close, depth } => self.code(close, depth),
                Frame::Str(s) => self.string(s),
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

    /// Whether one more interpolation may open.
    fn can_nest(&self) -> bool {
        self.stack.iter().filter(|f| matches!(f, Frame::Code { close: true, .. })).count() < MAX_NESTING
    }

    /// One more bracket open in the code on top, or one fewer.
    fn nest(&mut self, open: bool) {
        if let Some(Frame::Code { depth, .. }) = self.stack.last_mut() {
            *depth = if open { depth.saturating_add(1) } else { depth.saturating_sub(1) };
        }
    }

    /// One step of code.
    fn code(&mut self, close: bool, depth: u32) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let c = b[i];
        let next = self.at(i + 1);
        if is_space(c) {
            let end = space_end(b, i, len);
            if let Some(k) = b[i..end].iter().position(|&c| is_break(c)) {
                self.operand = false;
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
        let column0 = i == 0 || is_break(b[i - 1]);
        if c == b'#' {
            return self.emit(line_end(b, i, len), Class::Comment);
        }
        if column0 && b[i..].starts_with(b"=begin") && !is_word(self.at(i + 6)) {
            return self.emit(self.embedded(i), Class::Comment);
        }
        if column0 && b[i..].starts_with(b"__END__") && (i + 7 == len || is_break(b[i + 7])) {
            self.emit(i + 7, Class::Keyword);
            return self.emit(len, Class::Plain);
        }
        let member = std::mem::take(&mut self.member);
        let scope = std::mem::take(&mut self.scope);
        let pending = self.pending.take();
        // Where an operand may start: after an operator, or as a command's argument, with a space
        // before it and none after (`puts /x/`).
        let spaced = i > 0 && is_space(b[i - 1]);
        let argument = !self.operand || (spaced && !is_space(next) && next != b'=');
        if close && depth == 0 && c == b'}' {
            self.emit(i + 1, Class::Punctuation);
            self.pop();
            self.operand = true;
            return;
        }
        if c == b'"' || c == b'`' {
            return self.open(1, quoted(c, true, Class::String));
        }
        if c == b'\'' {
            return self.open(1, quoted(c, false, Class::String));
        }
        if c == b':' {
            if next == b':' {
                self.emit(i + 2, Class::Punctuation);
                self.scope = true;
                self.operand = false;
                return;
            }
            if matches!(next, b'"' | b'\'') {
                return self.open(2, quoted(next, next == b'"', Class::Constant));
            }
            if is_word_start(next) && (!self.operand || spaced) {
                return self.symbol();
            }
            if !self.operand
                && let Some(n) = operator(&b[i + 1..])
            {
                self.emit(i + 1 + n, Class::Constant);
                self.operand = true;
                return;
            }
        }
        if let Some(end) = self.sigil(i) {
            self.emit(end, Class::Variable);
            self.operand = true;
            return;
        }
        if c == b'?'
            && !self.operand
            && let Some(end) = self.character()
        {
            self.emit(end, Class::String);
            self.operand = true;
            return;
        }
        if c == b'%'
            && argument
            && let Some((open, s)) = self.percent()
        {
            return self.open(open, s);
        }
        if c == b'/' && argument && self.regex_closes(i) {
            return self.open(1, Str { regex: true, ..quoted(b'/', true, Class::String) });
        }
        if c == b'<' && next == b'<' && self.heredoc(argument) {
            return;
        }
        if c.is_ascii_digit() {
            self.emit(number_end(b, i, len, false), Class::Number);
            self.operand = true;
            return;
        }
        if is_word_start(c) {
            return self.word(member, scope, pending);
        }
        self.punct(c, next);
    }

    /// A string opened here, `len` bytes long: its opener read, the string on top.
    fn open(&mut self, len: usize, s: Str) {
        self.emit(self.pos + len, s.class);
        self.stack.push(Frame::Str(s));
        self.operand = true;
    }

    /// A symbol by its name: `:name`, `:empty?`, `:name=`.
    fn symbol(&mut self) {
        let mut j = word_end(self.b, self.pos + 1, self.b.len());
        if matches!(self.at(j), b'?' | b'!' | b'=') && !matches!(self.at(j + 1), b'=' | b'~' | b'>') {
            j += 1;
        }
        self.emit(j, Class::Constant);
        self.operand = true;
    }

    /// Where the variable a sigil at `i` begins ends: `@name`, `@@name`, `$name`, `$1`, `$!`.
    fn sigil(&self, i: usize) -> Option<usize> {
        let (b, len) = (self.b, self.b.len());
        let next = self.at(i + 1);
        match b[i] {
            b'@' => {
                let k = i + 1 + usize::from(next == b'@');
                is_word_start(self.at(k)).then(|| word_end(b, k, len))
            }
            b'$' if is_word_start(next) => Some(word_end(b, i + 1, len)),
            b'$' if next.is_ascii_digit() => {
                let mut k = i + 1;
                while self.at(k).is_ascii_digit() {
                    k += 1;
                }
                Some(k)
            }
            b'$' if next == b'-' && self.at(i + 2).is_ascii_alphanumeric() => Some(i + 3),
            b'$' if b"!@&`'+~=/\\,;.<>*$?:\"".contains(&next) => Some(i + 2),
            _ => None,
        }
    }

    /// Where the character a `?` at an operand's start writes ends: `?a`, `?\n`.
    fn character(&self) -> Option<usize> {
        let i = self.pos;
        let next = self.at(i + 1);
        if next == 0 || is_space(next) {
            return None;
        }
        let end = if next == b'\\' {
            if is_space(self.at(i + 2)) || i + 2 >= self.b.len() {
                return None;
            }
            i + 3
        } else {
            i + 1 + self.text.get(i + 1..)?.chars().next()?.len_utf8()
        };
        (!is_word(self.at(end))).then_some(end)
    }

    /// The `%`-literal that opens here: how many bytes open it, and how it reads.
    fn percent(&self) -> Option<(usize, Str)> {
        let i = self.pos;
        let (kind, at) = match self.at(i + 1) {
            k @ (b'q' | b'Q' | b'w' | b'W' | b'i' | b'I' | b'r' | b's' | b'x') => (k, i + 2),
            _ => (b'Q', i + 1),
        };
        let d = self.at(at);
        if !d.is_ascii_punctuation() || (at == i + 1 && d == b'=') {
            return None;
        }
        let (close, open) = match d {
            b'(' => (b')', d),
            b'[' => (b']', d),
            b'{' => (b'}', d),
            b'<' => (b'>', d),
            _ => (d, 0),
        };
        let class = if matches!(kind, b'i' | b'I' | b's') { Class::Constant } else { Class::String };
        let s = Str {
            close,
            open,
            depth: 0,
            interp: matches!(kind, b'Q' | b'W' | b'I' | b'r' | b'x'),
            class,
            regex: kind == b'r',
            set: false,
        };
        Some((at + 1 - i, s))
    }

    /// Whether a regular expression that opens at `at` closes on its line, within [`LOOK`] bytes.
    fn regex_closes(&self, at: usize) -> bool {
        let (b, len) = (self.b, self.b.len());
        let limit = len.min(at + LOOK);
        let mut set = false;
        let mut i = at + 1;
        while i < limit {
            match b[i] {
                c if is_break(c) => return false,
                b'\\' => i += 2,
                b'[' => {
                    set = true;
                    i += 1;
                }
                b']' => {
                    set = false;
                    i += 1;
                }
                b'/' if !set => return true,
                _ => i += 1,
            }
        }
        false
    }

    /// A heredoc's opener here, read, its body waiting for the next line; false where `<<` is
    /// an operator. `<<~` and `<<-` open one where an operand may start; a bare `<<` only after
    /// an operator, as `x << y` appends. As a command's argument, or after a bare `<<`, the
    /// identifier starts with a capital, as heredocs' do: `puts <<~EOS`.
    fn heredoc(&mut self, argument: bool) -> bool {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let mut k = i + 2;
        let indent = matches!(self.at(k), b'~' | b'-');
        if indent {
            k += 1;
        }
        if !(if indent { argument } else { !self.operand }) || self.heres.len() >= MAX_NESTING {
            return false;
        }
        let q = self.at(k);
        let (from, name, interp, end) = if matches!(q, b'"' | b'\'' | b'`') {
            let line = line_end(b, k + 1, len);
            let Some(n) = b[k + 1..line].iter().position(|&c| c == q) else {
                return false;
            };
            (k + 1, n, q != b'\'', k + 2 + n)
        } else if q.is_ascii_uppercase() || q == b'_' || (q.is_ascii_lowercase() && indent && !self.operand) {
            let e = word_end(b, k, len);
            (k, e - k, true, e)
        } else {
            return false;
        };
        if name == 0 {
            return false;
        }
        self.heres.push(Here { from, len: name, indent, interp });
        self.emit(end, Class::String);
        self.operand = true;
        true
    }

    /// The first byte at or after `i` that is no space or tab.
    fn blanks(&self, i: usize) -> usize {
        blanks_end(self.b, i, self.b.len())
    }

    /// A word: a name, a keyword, a method, or what else its tables and its case make it.
    fn word(&mut self, member: bool, scope: bool, pending: Option<Class>) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let bare = word_end(b, i, len);
        // A method's name may end in `?` or `!`: `empty?`, `save!`; but not before `=` or `~`,
        // as `a != b` and `a !~ b` compare.
        let j = bare + usize::from(matches!(self.at(bare), b'?' | b'!') && !matches!(self.at(bare + 1), b'=' | b'~'));
        let word = self.text.get(i..j).unwrap_or_default();
        // A hash's key or a keyword argument, `name: value`, is a symbol.
        if !member && !scope && j == bare && self.at(j) == b':' && self.at(j + 1) != b':' {
            self.emit(j + 1, Class::Constant);
            self.operand = false;
            return;
        }
        if word == "def" && !member && !scope {
            self.emit(j, Class::Keyword);
            return self.def(j);
        }
        let name = word.trim_start_matches('_').as_bytes();
        let capital = name.first().is_some_and(u8::is_ascii_uppercase);
        let capitals = capital && name.len() > 1 && !name.iter().any(u8::is_ascii_lowercase);
        let mut operand = true;
        let class = if member {
            Class::Function
        } else if scope {
            if capitals {
                Class::Constant
            } else if capital {
                Class::Type
            } else {
                Class::Function
            }
        } else if KEYWORDS.binary_search(&word).is_ok() {
            operand = word == "end";
            self.pending = matches!(word, "class" | "module").then_some(Class::Type);
            Class::Keyword
        } else if CONSTANTS.binary_search(&word).is_ok() {
            Class::Constant
        } else if word == "self" {
            Class::Variable
        } else if let Some(class) = pending {
            class
        } else if capitals {
            Class::Constant
        } else if capital {
            if self.at(j) == b'(' { Class::Function } else { Class::Type }
        } else if self.at(self.blanks(j)) == b'(' || self.argument(j) {
            Class::Function
        } else {
            Class::Plain
        };
        self.operand = operand;
        self.emit(j, class);
    }

    /// Whether the word that ends at `j` is a method given an argument with no parentheses: a
    /// space or more, then what can only begin an argument, a string, a symbol, a variable, a
    /// number, a heredoc, a `%`-literal, a regular expression, or a name that is no keyword
    /// (`puts "hi"`, `include Comparable`, `puts <<~EOS`, `p total`).
    fn argument(&self, j: usize) -> bool {
        let k = self.blanks(j);
        if k == j {
            return false;
        }
        let (c, next) = (self.at(k), self.at(k + 1));
        match c {
            b'"' | b'\'' | b'@' => true,
            b':' => is_word_start(next) || matches!(next, b'"' | b'\''),
            b'$' => is_word_start(next),
            b'<' => {
                let id = self.at(k + 3);
                next == b'<'
                    && matches!(self.at(k + 2), b'~' | b'-')
                    && (id.is_ascii_uppercase() || b"_\"'".contains(&id))
            }
            b'%' => {
                let typed = matches!(next, b'q' | b'Q' | b'w' | b'W' | b'i' | b'I' | b'r' | b's' | b'x');
                if typed {
                    self.at(k + 2).is_ascii_punctuation()
                } else {
                    matches!(next, b'(' | b'[' | b'{' | b'<' | b'|')
                }
            }
            b'/' => !is_space(next) && next != b'=' && self.regex_closes(k),
            _ if c.is_ascii_digit() => true,
            _ if is_word_start(c) => {
                let e = word_end(self.b, k, self.b.len());
                KEYWORDS.binary_search(&self.text.get(k..e).unwrap_or_default()).is_err()
            }
            _ => false,
        }
    }

    /// After `def`: its receiver, where it names one (`self.`, `Config.`), and its name, an
    /// operator's too (`==`, `[]=`, `<=>`), as a function.
    fn def(&mut self, from: usize) {
        let (b, len) = (self.b, self.b.len());
        self.operand = false;
        let mut k = self.blanks(from);
        if k == from {
            return;
        }
        let w = word_end(b, k, len);
        if w > k && self.at(w) == b'.' {
            let receiver = self.text.get(k..w).unwrap_or_default();
            self.out.space(k);
            self.emit(w, if receiver == "self" { Class::Variable } else { Class::Type });
            self.emit(w + 1, Class::Punctuation);
            k = w + 1;
        }
        let end = if is_word_start(self.at(k)) {
            let e = word_end(b, k, len);
            let suffix = matches!(self.at(e), b'?' | b'!')
                || (self.at(e) == b'=' && !matches!(self.at(e + 1), b'=' | b'~' | b'>'));
            e + usize::from(suffix)
        } else {
            operator(&b[k..]).map_or(k, |n| k + n)
        };
        if end > k {
            self.out.space(k);
            self.emit(end, Class::Function);
        }
    }

    /// An operator or punctuation.
    fn punct(&mut self, c: u8, next: u8) {
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
            b'&' if next == b'.' => (i + 2, Class::Punctuation),
            b'.' if next == b'.' => {
                let mut j = i;
                while self.at(j) == b'.' {
                    j += 1;
                }
                (j, Class::Punctuation)
            }
            b'-' if next == b'>' => (i + 2, Class::Operator),
            b',' | b';' | b'.' | b':' => (i + 1, Class::Punctuation),
            _ if c.is_ascii_punctuation() => (i + 1, Class::Operator),
            _ => (i + 1, Class::Plain),
        };
        self.member = (c == b'.' && end == i + 1) || (c == b'&' && next == b'.');
        self.operand = matches!(c, b')' | b']' | b'}');
        self.emit(end, class);
    }

    /// One step of a string: to its close, or to an interpolation, which interrupts it.
    fn string(&mut self, mut s: Str) {
        let (b, len) = (self.b, self.b.len());
        let mut i = self.pos;
        while i < len {
            let c = b[i];
            if c == b'\\' {
                i += 2;
                continue;
            }
            // A regular expression's set holds a `/` that closes nothing: `/[\/]/`.
            if s.regex && s.close == b'/' && (s.set || c == b'[') {
                s.set = c != b']';
                i += 1;
                continue;
            }
            if s.open != 0 && c == s.open {
                s.depth += 1;
                i += 1;
                continue;
            }
            if c == s.close {
                if s.depth > 0 {
                    s.depth -= 1;
                    i += 1;
                    continue;
                }
                let mut end = i + 1;
                while s.regex && matches!(self.at(end), b'i' | b'm' | b'x' | b'o' | b'u' | b'e' | b's' | b'n') {
                    end += 1;
                }
                self.emit(end, s.class);
                self.pop();
                self.operand = true;
                return;
            }
            if s.interp && c == b'#' {
                if self.at(i + 1) == b'{' && self.can_nest() {
                    if let Some(Frame::Str(top)) = self.stack.last_mut() {
                        *top = s;
                    }
                    return self.interpolate(i, s.class);
                }
                if let Some(end) = self.shorthand(i + 1) {
                    self.out.push(i + 1, s.class);
                    self.out.push(end, Class::Variable);
                    i = end;
                    continue;
                }
            }
            i += 1;
        }
        // Never closed: it runs to the end.
        self.emit(len, s.class);
    }

    /// The string in `class` interrupted at `at` by `#{`.
    fn interpolate(&mut self, at: usize, class: Class) {
        self.out.push(at, class);
        self.emit(at + 2, Class::Punctuation);
        self.stack.push(Frame::Code { close: true, depth: 0 });
        self.operand = false;
    }

    /// Where a variable a string interpolates after its `#`, at `k`, ends: `#@name`, `#@@name`,
    /// `#$name`.
    fn shorthand(&self, k: usize) -> Option<usize> {
        let name = k + match (self.at(k), self.at(k + 1)) {
            (b'@', b'@') => 2,
            (b'@' | b'$', _) => 1,
            _ => return None,
        };
        is_word_start(self.at(name)).then(|| word_end(self.b, name, self.b.len()))
    }

    /// One step of a heredoc's body: its terminator, or a line of it, to its end or to an
    /// interpolation.
    fn here(&mut self, h: Here) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        if i == 0 || is_break(b[i - 1]) {
            let k = if h.indent { self.blanks(i) } else { i };
            let close = k + h.len;
            if close <= len && b[k..close] == b[h.from..h.from + h.len] && (close == len || is_break(b[close])) {
                self.emit(close, Class::String);
                self.pop();
                self.operand = true;
                return;
            }
        }
        let mut j = i;
        while j < len {
            let c = b[j];
            if is_break(c) {
                return self.emit(line_next(b, j, len), Class::String);
            }
            if h.interp && c == b'\\' && !is_break(self.at(j + 1)) {
                j += 2;
                continue;
            }
            if h.interp && c == b'#' {
                if self.at(j + 1) == b'{' && self.can_nest() {
                    return self.interpolate(j, Class::String);
                }
                if let Some(end) = self.shorthand(j + 1) {
                    self.out.push(j + 1, Class::String);
                    self.out.push(end, Class::Variable);
                    j = end;
                    continue;
                }
            }
            j += 1;
        }
        self.emit(len, Class::String);
    }

    /// Where the embedded document that `=begin` opens at `i` ends: at the end of the line that
    /// `=end` begins, or at the text's end.
    fn embedded(&self, i: usize) -> usize {
        let (b, len) = (self.b, self.b.len());
        let mut k = line_next(b, i, len);
        while k < len {
            if b[k..].starts_with(b"=end") && !is_word(self.at(k + 4)) {
                return line_end(b, k, len);
            }
            k = line_next(b, k, len);
        }
        len
    }
}

const fn quoted(close: u8, interp: bool, class: Class) -> Str {
    Str { close, open: 0, depth: 0, interp, class, regex: false, set: false }
}

/// How many bytes of `rest` name an operator method, as `def` and a symbol write it.
fn operator(rest: &[u8]) -> Option<usize> {
    OPERATORS.iter().find(|o| rest.starts_with(o.as_bytes())).map(|o| o.len())
}

/// The operators a method may be named, each before the operators it begins.
const OPERATORS: &[&str] = &[
    "[]=", "[]", "<=>", "<=", "<<", "<", "===", "==", "=~", "!=", "!~", "!", "**", "*", "+@", "+", "-@", "-", ">=",
    ">>", ">", "/", "%", "~", "&", "|", "^",
];

// The tables, each sorted, so a word is found by binary search (`every_table_is_sorted`).

const KEYWORDS: &[&str] = &[
    "BEGIN",
    "END",
    "alias",
    "and",
    "attr_accessor",
    "attr_reader",
    "attr_writer",
    "begin",
    "block_given?",
    "break",
    "case",
    "catch",
    "class",
    "def",
    "defined?",
    "do",
    "else",
    "elsif",
    "end",
    "ensure",
    "extend",
    "fail",
    "for",
    "if",
    "in",
    "include",
    "lambda",
    "loop",
    "module",
    "module_function",
    "next",
    "not",
    "or",
    "prepend",
    "private",
    "private_constant",
    "proc",
    "protected",
    "public",
    "raise",
    "redo",
    "refine",
    "require",
    "require_relative",
    "rescue",
    "retry",
    "return",
    "super",
    "then",
    "throw",
    "undef",
    "unless",
    "until",
    "using",
    "when",
    "while",
    "yield",
];
const CONSTANTS: &[&str] = &["__ENCODING__", "__FILE__", "__LINE__", "false", "nil", "true"];

#[cfg(test)]
mod tests {
    use super::*;

    /// Each table is sorted, with no word twice, so binary search finds what it holds.
    #[test]
    fn every_table_is_sorted() {
        for table in [KEYWORDS, CONSTANTS] {
            assert!(table.windows(2).all(|w| w[0] < w[1]), "not sorted: {table:?}");
        }
    }

    /// An operator comes before every operator it begins, so the longer one is found: `<=>`
    /// before `<=`.
    #[test]
    fn an_operator_comes_before_the_operators_it_begins() {
        for (k, first) in OPERATORS.iter().enumerate() {
            for later in &OPERATORS[k + 1..] {
                assert!(!later.starts_with(first), "{first:?} hides {later:?}");
            }
        }
    }
}
