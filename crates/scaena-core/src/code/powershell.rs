//! PowerShell, read by a scanner of its own (PLAN 2.134), which reads a console's commands after
//! a `PS>` prompt too.
//!
//! - **Comments**: `#` where a token starts, to its line's end, and `<# … #>`.
//! - **Strings**: `"…"` expands `$name` and `$(…)`, a backtick escaping and a quote doubled;
//!   `'…'` holds what it holds, a quote doubled. Each runs on over lines. A here-string, `@"` or
//!   `@'` at its line's end, runs to a line that starts with `"@` or `'@`.
//! - **Variables**: `$name`, `$env:PATH`, `${any name}`, and `@name` splatted; `$true`, `$false`,
//!   and `$null` are constants.
//! - **Commands**: a word where a command's name stands, first in a statement or a pipeline and
//!   after `=`, is a function (`Get-ChildItem`, `git`, `./build.ps1`), as a cmdlet's `Verb-Noun`
//!   is anywhere; there, a word before ` = ` is a hashtable's key, a property. A command's
//!   arguments are plain words, paths, numbers, and what they expand, a path going on after a
//!   variable (`$PSScriptRoot\lib.ps1`, `${PWD}:/data`).
//! - **Operators and parameters**: a `-` and a word is an operator where PowerShell has one
//!   (`-eq`, `-like`, `-not`, `-ceq`), else a parameter, an attribute (`-Path`), as a program's
//!   `--name` is.
//! - **Types and attributes**: `[string]`, `[System.IO.File]`, and `[int[]]` where an operand may
//!   start or after another's `]` are types; `[Parameter(…)]` and `[CmdletBinding()]` are
//!   attributes.
//! - Keywords and operators read in any case. After `function` or `filter` a name is a function,
//!   after `class` or `enum` a type, and an enum's members are constants; after a `.` or `::` a
//!   member is a property, or a function where a `(` follows.
//!
//! At most [`MAX_NESTING`] subexpressions stand open inside strings inside one another, and what
//! would open one more is read as its string's own characters.

use super::scan::{
    Spans, blanks_end, ends_with, holds, is_break, is_space, is_word, is_word_start, line_end, line_next, number_end,
    space_end, word_end,
};
use super::tables::{Table, words};
use super::{Class, MAX_NESTING};

/// How far a type in brackets is looked through for its `]`.
const TYPE: usize = 64;

/// What the scanner returns to.
#[derive(Debug, Clone, Copy)]
enum Frame {
    /// Code: the text's own at the bottom of the stack, else a subexpression's in a string,
    /// `$(…)`, which `)` closes where `depth` brackets inside it are open.
    Code { close: bool, depth: u32 },
    /// A string in `quote`, which `"` expands; a here-string where `here` is set.
    Str { quote: u8, here: bool },
    /// A block comment, to its `#>`.
    Comment,
}

const TOP: Frame = Frame::Code { close: false, depth: 0 };

/// Writes `text`'s tokens as PowerShell.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let mut scanner = Scanner::new(text, out, 0);
    scanner.run(text.len());
}

/// Writes the tokens of the command that starts at `at`, after a console's prompt, and gives where
/// it ends: at its line's end, or, while what it opened is open, past the lines it goes on over,
/// each of which may start with the continuation prompt `>> `, which is meta.
pub(super) fn command<'t>(text: &'t str, out: &mut Spans<'t>, at: usize) -> usize {
    let b = text.as_bytes();
    let mut scanner = Scanner::new(text, out, at);
    let mut line = line_next(b, at, b.len());
    scanner.run(line);
    while !scanner.rests() && line < b.len() {
        let next = line_next(b, line, b.len());
        let end = line_end(b, line, next);
        let prompt = if b[line..end].starts_with(b">>") { 2 + usize::from(b.get(line + 2) == Some(&b' ')) } else { 0 };
        scanner.resume(line + prompt.min(end - line));
        scanner.run(next);
        line = next;
    }
    line
}

/// Writes the tokens of a Dockerfile instruction's command from `at`, where a `SHELL` instruction
/// names PowerShell or the escape is a backtick, and gives where it ends: past its last line. The
/// command goes on past a line that ends with `escape`, spaces and tabs after it, as Docker joins
/// them. Docker drops the comments and the blank lines among the lines it joins, so the command
/// goes on after each as it was before it. What else it leaves open closes at its last line's end,
/// where the instruction ends.
pub(super) fn instruction<'t>(text: &'t str, out: &mut Spans<'t>, at: usize, escape: u8) -> usize {
    let b = text.as_bytes();
    let mut scanner = Scanner::new(text, out, at);
    let mut line = line_next(b, at, b.len());
    scanner.run(line);
    let mut goes_on = ends_with(b, at, line, escape);
    while line < b.len() && goes_on {
        let next = line_next(b, line, b.len());
        let first = blanks_end(b, line, next);
        if first == line_end(b, line, next) || b[first] == b'#' {
            scanner.dropped(line, next);
        } else {
            scanner.line(line);
            scanner.run(next);
            goes_on = ends_with(b, line, next, escape);
        }
        line = next;
    }
    line
}

/// Writes the tokens of `text` from `from` to `to` as a PowerShell script: the heredoc a
/// Dockerfile's `RUN` runs where its shell is PowerShell.
pub(super) fn script<'t>(text: &'t str, out: &mut Spans<'t>, from: usize, to: usize) {
    let mut scanner = Scanner::new(text, out, from);
    scanner.run(to);
}

struct Scanner<'t, 'o> {
    text: &'t str,
    b: &'t [u8],
    pos: usize,
    /// Where this run stops: the text's end, or a console's line's.
    end: usize,
    /// Where the line being read starts, past a console's prompt.
    start: usize,
    out: &'o mut Spans<'t>,
    stack: Vec<Frame>,
    /// Whether a command's name may stand here: first in a statement or a pipeline, or after `=`.
    command: bool,
    /// Whether a command's arguments are being read, where a word is plain.
    args: bool,
    /// Whether what came last ends an operand, so `[` indexes it.
    operand: bool,
    /// Whether a `.` or `::` came last, so a word is a member.
    member: bool,
    /// What the next word names, after `function` or `class`.
    pending: Option<Class>,
    /// Whether the command goes on past this line: after `|`, `&&`, `||`, or a backtick.
    more: bool,
    /// Whether an attribute's name came last, so the `(` after it opens its arguments.
    attribute: bool,
    /// How many brackets stand open inside an attribute's arguments, where a word names one of
    /// them: `[Parameter(Mandatory)]`.
    named: Option<u32>,
    /// Whether a `]` came last, maybe a type's or an attribute's, so a `[` after it may begin
    /// another: `[Parameter()] [string]`.
    typed: bool,
    /// Whether `enum` came last but for its name, so the `{` after it holds its members.
    enumeration: bool,
    /// How many brackets stand open inside an enum's braces, where a word first in its statement
    /// is a member: `enum Color { Red; Green }`.
    members: Option<u32>,
}

impl<'t, 'o> Scanner<'t, 'o> {
    fn new(text: &'t str, out: &'o mut Spans<'t>, at: usize) -> Self {
        Scanner {
            text,
            b: text.as_bytes(),
            pos: at,
            end: text.len(),
            start: at,
            out,
            stack: vec![TOP],
            command: true,
            args: false,
            operand: false,
            member: false,
            pending: None,
            more: false,
            attribute: false,
            named: None,
            typed: false,
            enumeration: false,
            members: None,
        }
    }

    /// Reads on to `end`.
    fn run(&mut self, end: usize) {
        self.end = end.min(self.b.len());
        while self.pos < self.end {
            let (pos, frames) = (self.pos, self.stack.len());
            match self.stack.last().copied().unwrap_or(TOP) {
                Frame::Code { close, depth } => self.code(close, depth),
                Frame::Str { quote, here } => self.string(quote, here),
                Frame::Comment => self.comment(),
            }
            // Each step reads a byte, or closes what a byte read before opened.
            debug_assert!(self.pos > pos || self.stack.len() != frames, "a step read nothing at {pos}");
            if self.pos <= pos && self.stack.len() == frames {
                self.pos = pos + 1;
                self.out.push(self.pos, Class::Plain);
            }
        }
    }

    /// Whether the command is whole: nothing it opened is open, and its last line does not go on.
    fn rests(&self) -> bool {
        self.stack.len() == 1 && !self.more && matches!(self.stack[0], Frame::Code { depth: 0, .. })
    }

    /// The command goes on at `start`, past a continuation prompt, which is meta.
    fn resume(&mut self, start: usize) {
        self.out.push(start, Class::Meta);
        self.line(start);
    }

    /// The command goes on at `start`, where a line begins.
    fn line(&mut self, start: usize) {
        self.pos = start;
        self.start = start;
    }

    /// The comment or the blank line from `line` to `next`, which Docker drops from the lines it
    /// joins: a comment, and the command goes on after it as it was before it.
    fn dropped(&mut self, line: usize, next: usize) {
        let end = line_end(self.b, line, next);
        self.out.space(blanks_end(self.b, line, end));
        self.out.push(end, Class::Comment);
        self.out.space(next);
        self.line(next);
    }

    /// The byte at `i`, or 0 at or past where this run stops.
    fn at(&self, i: usize) -> u8 {
        if i < self.end { self.b[i] } else { 0 }
    }

    /// The text up to `end` in `class`, read.
    fn emit(&mut self, end: usize, class: Class) {
        let end = end.min(self.end);
        self.out.push(end, class);
        self.pos = end.max(self.pos);
    }

    fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    /// Whether one more subexpression may open in a string.
    fn can_nest(&self) -> bool {
        self.stack.iter().filter(|f| matches!(f, Frame::Code { close: true, .. })).count() < MAX_NESTING
    }

    /// One more bracket open in the code on top, or one fewer.
    fn nest(&mut self, open: bool) {
        if let Some(Frame::Code { depth, .. }) = self.stack.last_mut() {
            *depth = if open { depth.saturating_add(1) } else { depth.saturating_sub(1) };
        }
    }

    /// How many brackets stand open in the code on top.
    fn depth(&self) -> Option<u32> {
        match self.stack.last() {
            Some(Frame::Code { depth, .. }) => Some(*depth),
            _ => None,
        }
    }

    /// A statement starts: a command's name may stand next.
    fn statement(&mut self) {
        self.command = true;
        self.args = false;
        self.operand = false;
    }

    /// Whether a token starts at `i`, where `#` begins a comment: not inside a word or a path.
    fn starts_token(&self, i: usize) -> bool {
        i == self.start
            || !(is_word(self.b[i - 1]) || matches!(self.b[i - 1], b'-' | b'.' | b'\\' | b'/' | b':' | b'$' | b'`'))
    }

    /// One step of code.
    fn code(&mut self, close: bool, depth: u32) {
        let (b, i, end) = (self.b, self.pos, self.end);
        let c = b[i];
        let next = self.at(i + 1);
        if is_space(c) {
            let to = space_end(b, i, end);
            if b[i..to].iter().any(|&c| is_break(c)) {
                self.statement();
            }
            self.out.space(to);
            self.pos = to;
            return;
        }
        if c == b'<' && next == b'#' {
            self.emit(i + 2, Class::Comment);
            return self.stack.push(Frame::Comment);
        }
        if c == b'#' && self.starts_token(i) {
            return self.emit(line_end(b, i, end), Class::Comment);
        }
        self.more = false;
        let member = std::mem::take(&mut self.member);
        let pending = self.pending.take();
        let typed = std::mem::take(&mut self.typed);
        let spaced = i == self.start || is_space(b[i - 1]);
        // A subexpression's own close, back in its string.
        if close && depth == 0 && c == b')' {
            self.emit(i + 1, Class::Punctuation);
            return self.pop();
        }
        if c == b'"' || c == b'\'' {
            return self.open(1, c, false);
        }
        if c == b'@' {
            if matches!(next, b'"' | b'\'') && self.here_opens(i + 2) {
                return self.open(2, next, true);
            }
            // An array's or a hashtable's: `@(…)`, `@{…}`.
            if matches!(next, b'(' | b'{') {
                self.emit(i + 2, Class::Punctuation);
                self.nest(true);
                return self.statement();
            }
            // Splatting: `@params`.
            if is_word_start(next) {
                self.emit(word_end(b, i + 1, end), Class::Variable);
                self.operand = true;
                return;
            }
        }
        if c == b'$' {
            return self.variable();
        }
        // A type or an attribute where an operand may start, or after another's `]`:
        // `[Parameter()][string]$Name`.
        if c == b'[' && (!self.operand || typed) && self.bracket() {
            return;
        }
        // A line that goes on.
        if c == b'`' && is_break(next) {
            self.emit(i + 1, Class::Punctuation);
            let after = line_next(b, i + 1, end);
            self.out.space(after);
            self.pos = after;
            self.more = true;
            return;
        }
        if c == b'-' && (is_word_start(next) || (next == b'-' && is_word_start(self.at(i + 2)))) {
            return self.dash();
        }
        // A path where a command's name or an argument stands: `./build.ps1`, `..\src`, `*.txt`,
        // and `.` alone as an argument; `. ` where a command's name stands dot-sources. A point
        // before a digit begins a number: `.5`.
        if spaced
            && matches!(c, b'.' | b'\\' | b'/' | b'~' | b'*' | b'?')
            && !(c == b'.' && next.is_ascii_digit())
            && (self.args || (self.command && next != 0 && !is_space(next)))
        {
            let class = if self.command { Class::Function } else { Class::Plain };
            if self.command {
                self.command = false;
                self.args = true;
            }
            self.operand = true;
            return self.emit(self.bare_end(i + 1), class);
        }
        // A path that goes on after what began it: a backslash is no operator
        // (`$PSScriptRoot\lib.ps1`), and among a command's arguments neither is a `/` or a `:`
        // that follows a variable (`${PWD}:/data`).
        if c == b'\\' || (self.args && self.operand && !spaced && (c == b'/' || (c == b':' && next != b':'))) {
            self.operand = true;
            return self.emit(self.bare_end(i + 1), Class::Plain);
        }
        if c.is_ascii_digit() || (c == b'.' && next.is_ascii_digit() && (!self.operand || spaced)) {
            self.emit(number_end(b, i, end, false), Class::Number);
            self.operand = true;
            self.command = false;
            return;
        }
        if is_word_start(c) {
            return self.word(member, pending);
        }
        self.symbol(c, next);
    }

    /// A string opened here, `len` bytes long: its opener read, the string on top.
    fn open(&mut self, len: usize, quote: u8, here: bool) {
        self.emit(self.pos + len, Class::String);
        self.stack.push(Frame::Str { quote, here });
        self.operand = true;
        self.command = false;
    }

    /// Whether a here-string opens at `@"` or `@'`: nothing but spaces after it on its line.
    fn here_opens(&self, k: usize) -> bool {
        let e = blanks_end(self.b, k, self.end);
        e >= self.end || is_break(self.b[e])
    }

    /// `$` in code: a subexpression, `$(…)`, or a variable.
    fn variable(&mut self) {
        let i = self.pos;
        if self.at(i + 1) == b'(' {
            self.emit(i + 2, Class::Punctuation);
            self.nest(true);
            return self.statement();
        }
        let (end, class) = self.name(i).unwrap_or((i + 1, Class::Operator));
        self.emit(end, class);
        self.operand = true;
        self.command = false;
    }

    /// Where the variable that `$` at `i` begins ends, and its class: `$name`, `$env:PATH`,
    /// `${any name}`, `$_`, `$$`, `$?`, `$^`. `$true`, `$false`, and `$null` are constants.
    fn name(&self, i: usize) -> Option<(usize, Class)> {
        let (b, end) = (self.b, self.end);
        let next = self.at(i + 1);
        if next == b'{' {
            // To its `}`, or to its line's end where none closes it, looked for no further.
            let k = b[i + 2..end].iter().position(|&c| c == b'}' || is_break(c)).map_or(end, |n| i + 2 + n);
            return Some((k + usize::from(self.at(k) == b'}'), Class::Variable));
        }
        if is_word_start(next) {
            let mut e = word_end(b, i + 1, end);
            // A scope's or a drive's name before it: `$env:PATH`, `$script:count`.
            if self.at(e) == b':' && is_word_start(self.at(e + 1)) {
                e = word_end(b, e + 1, end);
            }
            let name = self.text.get(i + 1..e).unwrap_or_default();
            let constant = ["true", "false", "null"].iter().any(|k| k.eq_ignore_ascii_case(name));
            return Some((e, if constant { Class::Constant } else { Class::Variable }));
        }
        matches!(next, b'$' | b'?' | b'^').then_some((i + 2, Class::Variable))
    }

    /// `[` where an operand may start: a type (`[string]`, `[System.IO.File]`, `[int[]]`) or an
    /// attribute (`[Parameter(Mandatory)]`), read; false where neither closes within [`TYPE`]
    /// bytes.
    fn bracket(&mut self) -> bool {
        let (b, i) = (self.b, self.pos);
        if !is_word_start(self.at(i + 1)) {
            return false;
        }
        let limit = self.end.min(i + TYPE);
        let (mut k, mut depth) = (i + 1, 1u32);
        let close = loop {
            if k >= limit {
                return false;
            }
            match b[k] {
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        break k;
                    }
                }
                // An attribute: its name, then its arguments as code, to its `]`.
                b'(' if depth == 1 => {
                    self.emit(i + 1, Class::Punctuation);
                    self.nest(true);
                    let mut e = word_end(b, i + 1, self.end);
                    while self.at(e) == b'.' && is_word_start(self.at(e + 1)) {
                        e = word_end(b, e + 1, self.end);
                    }
                    self.emit(e, Class::Attribute);
                    self.operand = false;
                    self.attribute = true;
                    return true;
                }
                c if is_word(c) || matches!(c, b'.' | b',' | b' ') => {}
                _ => return false,
            }
            k += 1;
        };
        // A type: its names, and the points, brackets, and commas between them.
        while self.pos <= close {
            let c = b[self.pos];
            if is_word(c) {
                self.emit(word_end(b, self.pos, close), Class::Type);
            } else if c == b' ' {
                let to = blanks_end(b, self.pos, close);
                self.out.space(to);
                self.pos = to;
            } else {
                self.emit(self.pos + 1, Class::Punctuation);
            }
        }
        self.operand = true;
        self.typed = true;
        true
    }

    /// A `-` and a word: an operator where PowerShell has one, else a parameter; and `--` and a
    /// word, a program's option, as a parameter (`git commit --amend`).
    fn dash(&mut self) {
        let i = self.pos;
        let double = self.at(i + 1) == b'-';
        let j = word_end(self.b, i + 1 + usize::from(double), self.end);
        let word = self.text.get(i + 1..j).unwrap_or_default();
        let class = if !double && operator(word) { Class::Operator } else { Class::Attribute };
        self.operand = false;
        self.command = false;
        self.emit(j, class);
    }

    /// Where a bare word that starts before `from` ends: at whitespace, or at what begins
    /// something else (`(`, `;`, `|`, a quote, `$`).
    fn bare_end(&self, from: usize) -> usize {
        let mut j = from;
        while j < self.end {
            let c = self.b[j];
            if is_space(c) || b"(){}[];,|&<>\"'$@=`".contains(&c) {
                break;
            }
            j += 1;
        }
        j
    }

    /// The first byte at or after `i` that is no space or tab.
    fn blanks(&self, i: usize) -> usize {
        blanks_end(self.b, i, self.end)
    }

    /// A word: a keyword, a command, a member, a key, or plain.
    fn word(&mut self, member: bool, pending: Option<Class>) {
        let (b, i, end) = (self.b, self.pos, self.end);
        let mut j = word_end(b, i, end);
        // A cmdlet's name: `Get-ChildItem`, a `-` between words.
        while self.at(j) == b'-' && is_word_start(self.at(j + 1)) {
            j = word_end(b, j + 1, end);
        }
        // A command's name or argument runs on through a path: `build.ps1`, `C:\tools`.
        if (self.command || self.args) && !member {
            j = self.bare_end(j);
        }
        let word = self.text.get(i..j).unwrap_or_default();
        let after = self.blanks(j);
        let command = std::mem::take(&mut self.command);
        let class = if member {
            if self.at(after) == b'(' { Class::Function } else { Class::Property }
        } else if self.named.is_some() && self.named == self.depth() {
            // An attribute's argument: `Mandatory`, `Position = 0`.
            Class::Property
        } else if command && self.members.is_some() && self.members == self.depth() {
            // An enum's member: `Red`, `Green = 2`.
            Class::Constant
        } else if !self.args && holds(&KEYWORDS, word) {
            self.keyword(word);
            Class::Keyword
        } else if let Some(class) = pending {
            class
        } else if command && self.at(after) == b'=' && self.at(after + 1) != b'=' {
            // A hashtable's key: `@{ Name = … }`.
            Class::Property
        } else if command || cmdlet(word) {
            self.args = true;
            Class::Function
        } else if self.at(after) == b'(' && !self.args {
            Class::Function
        } else {
            Class::Plain
        };
        self.operand = class != Class::Keyword;
        self.emit(j, class);
    }

    /// What a keyword names next, and whether a command may follow it.
    fn keyword(&mut self, word: &str) {
        let is = |k: &str| k.eq_ignore_ascii_case(word);
        self.pending = if is("function") || is("filter") {
            Some(Class::Function)
        } else if is("class") || is("enum") {
            Some(Class::Type)
        } else {
            None
        };
        self.enumeration = is("enum");
        self.command = is("return") || is("throw") || is("in");
        self.args = false;
    }

    /// An operator or punctuation.
    fn symbol(&mut self, c: u8, next: u8) {
        let i = self.pos;
        let mut command = false;
        let (end, class) = match c {
            b'(' | b'{' => {
                self.nest(true);
                if std::mem::take(&mut self.attribute) && c == b'(' {
                    self.named = self.depth();
                }
                if c == b'{' && std::mem::take(&mut self.enumeration) {
                    self.members = self.depth();
                }
                command = true;
                (i + 1, Class::Punctuation)
            }
            b'[' => {
                self.nest(true);
                (i + 1, Class::Punctuation)
            }
            b')' | b'}' | b']' => {
                self.nest(false);
                let depth = self.depth();
                if self.named.is_some_and(|named| depth.is_none_or(|depth| depth < named)) {
                    self.named = None;
                }
                if self.members.is_some_and(|members| depth.is_none_or(|depth| depth < members)) {
                    self.members = None;
                }
                (i + 1, Class::Punctuation)
            }
            b';' => {
                command = true;
                (i + 1, Class::Punctuation)
            }
            b'|' | b'&' => {
                command = true;
                let n = if next == c { 2 } else { 1 };
                self.more = c == b'|' || n == 2;
                (i + n, Class::Operator)
            }
            b':' if next == b':' => (i + 2, Class::Punctuation),
            b'.' if next == b'.' => (i + 2, Class::Operator),
            // Dot-sourcing: `. ./setup.ps1`.
            b'.' if self.command && is_space(next) => {
                command = true;
                (i + 1, Class::Operator)
            }
            b',' | b'.' | b':' => (i + 1, Class::Punctuation),
            // An assignment, after which a command may stand; among a command's arguments, a
            // part of one: `--name=web`.
            b'=' if next != b'=' => {
                command = !self.args;
                (i + 1, Class::Operator)
            }
            // An escaped character.
            b'`' => (i + 2, Class::Plain),
            _ if c.is_ascii_punctuation() => (i + 1, Class::Operator),
            _ => (i + 1, Class::Plain),
        };
        self.member = (c == b'.' && end == i + 1 && !command) || (c == b':' && next == b':');
        self.operand = matches!(c, b')' | b']' | b'}');
        self.typed = c == b']';
        if command || matches!(c, b')' | b'}') {
            self.args = false;
        }
        self.command = command;
        self.emit(end, class);
    }

    /// One step of a string: to its close, or to what it expands.
    fn string(&mut self, quote: u8, here: bool) {
        let (b, end) = (self.b, self.end);
        let mut i = self.pos;
        while i < end {
            let c = b[i];
            if here {
                // A here-string closes first on a line.
                if i == self.start || is_break(b[i - 1]) {
                    let k = blanks_end(b, i, end);
                    if self.at(k) == quote && self.at(k + 1) == b'@' {
                        self.emit(k + 2, Class::String);
                        return self.pop();
                    }
                }
            } else if c == quote {
                if self.at(i + 1) == quote {
                    i += 2;
                    continue;
                }
                self.emit(i + 1, Class::String);
                return self.pop();
            }
            if quote == b'"' {
                if c == b'`' {
                    i += 2;
                    continue;
                }
                if c == b'$' && self.expands(i) {
                    self.out.push(i, Class::String);
                    self.pos = i;
                    return self.interpolate();
                }
            }
            i += 1;
        }
        // Never closed: it runs to the end.
        self.emit(end, Class::String);
    }

    /// Whether a `$` at `i` in a string expands what follows it.
    fn expands(&self, i: usize) -> bool {
        let next = self.at(i + 1);
        is_word_start(next) || matches!(next, b'(' | b'{' | b'$' | b'?' | b'^')
    }

    /// `$` in a string: a subexpression, which interrupts it, or a variable.
    fn interpolate(&mut self) {
        let i = self.pos;
        if self.at(i + 1) == b'(' {
            if !self.can_nest() {
                return self.emit(i + 1, Class::String);
            }
            self.emit(i + 2, Class::Punctuation);
            self.stack.push(Frame::Code { close: true, depth: 0 });
            return self.statement();
        }
        let (end, class) = self.name(i).unwrap_or((i + 1, Class::String));
        self.emit(end, class);
    }

    /// One step of a block comment, to its `#>`.
    fn comment(&mut self) {
        let (b, end) = (self.b, self.end);
        let mut i = self.pos;
        while i + 1 < end {
            if b[i] == b'#' && b[i + 1] == b'>' {
                self.emit(i + 2, Class::Comment);
                return self.pop();
            }
            i += 1;
        }
        self.emit(end, Class::Comment);
    }
}

/// Whether `word` is a cmdlet's `Verb-Noun`, the verb starting with a capital: `Get-Item`.
fn cmdlet(word: &str) -> bool {
    let Some((verb, noun)) = word.split_once('-') else {
        return false;
    };
    verb.len() > 1
        && verb.as_bytes()[0].is_ascii_uppercase()
        && verb.bytes().all(|c| c.is_ascii_alphabetic())
        && noun.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && noun.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

/// Whether `-` and `word` are an operator, in any case: `-eq`, and `-ceq` and `-ieq`, which
/// compare with case or without.
fn operator(word: &str) -> bool {
    holds(&OPERATORS, word)
        || (word.len() > 2
            && matches!(word.as_bytes()[0], b'c' | b'i' | b'C' | b'I')
            && holds(&CASED, word.get(1..).unwrap_or_default()))
}

// The tables, each sorted and in lowercase, so a word is found by binary search in any case
// (`every_table_is_sorted`).

const KEYWORDS: Table = words!(
    "powershell/keywords",
    [
        "begin",
        "break",
        "catch",
        "class",
        "clean",
        "continue",
        "data",
        "default",
        "do",
        "dynamicparam",
        "else",
        "elseif",
        "end",
        "enum",
        "exit",
        "filter",
        "finally",
        "for",
        "foreach",
        "function",
        "hidden",
        "if",
        "in",
        "param",
        "process",
        "return",
        "static",
        "switch",
        "throw",
        "trap",
        "try",
        "until",
        "using",
        "while",
    ]
);
/// The operators after a `-`.
const OPERATORS: Table = words!(
    "powershell/operators",
    [
        "and",
        "as",
        "band",
        "bnot",
        "bor",
        "bxor",
        "contains",
        "eq",
        "f",
        "ge",
        "gt",
        "in",
        "is",
        "isnot",
        "join",
        "le",
        "like",
        "lt",
        "match",
        "ne",
        "not",
        "notcontains",
        "notin",
        "notlike",
        "notmatch",
        "or",
        "replace",
        "shl",
        "shr",
        "split",
        "xor",
    ]
);
/// The operators that compare with case or without after a `c` or an `i`: `-ceq`, `-ilike`.
const CASED: Table = words!(
    "powershell/cased",
    [
        "contains",
        "eq",
        "ge",
        "gt",
        "in",
        "le",
        "like",
        "lt",
        "match",
        "ne",
        "notcontains",
        "notin",
        "notlike",
        "notmatch",
        "replace",
        "split",
    ]
);

/// Every table this scanner reads, in the order its file lists them (`tables::written`).
pub(super) const WORDS: &[Table] = &[KEYWORDS, OPERATORS, CASED];

#[cfg(test)]
mod tests {
    use super::*;

    /// Each table is sorted and in lowercase, with no word twice, so binary search finds what it
    /// holds in any case.
    #[test]
    fn every_table_is_sorted() {
        for table in [KEYWORDS, OPERATORS, CASED] {
            assert!(table.windows(2).all(|w| w[0] < w[1]), "not sorted: {table:?}");
            assert!(table.iter().all(|w| w.bytes().all(|c| c.is_ascii_lowercase())), "{table:?}");
        }
        assert!(holds(&KEYWORDS, "ForEach") && holds(&OPERATORS, "EQ") && operator("CLike") && !operator("Path"));
    }
}
