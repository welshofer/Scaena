//! Shell (bash, sh, and zsh), read by a scanner of its own (PLAN 2.134), which reads a console's
//! commands too.
//!
//! - **Comments**: `#` where a word starts, so `a#b` is a word and `${#x}` a length.
//! - **Quotes**: `'…'` holds what it holds, `$'…'` escapes with a backslash, and `"…"` expands
//!   what it holds: variables, `${…}`, `$(…)`, `$((…))`, and backquotes. Each runs on over lines,
//!   as the shell's do.
//! - **Expansions**: `$name`, `$1`, `$@`, and `${…}` are variables, the last whole but for the
//!   quotes and substitutions inside it. `$(…)` and backquotes hold commands; `$((…))` and
//!   `((…))` hold arithmetic, whose names are variables (`$((count + 1))`).
//! - **Heredocs**: `<<EOF`, `<<-EOF`, and `<<'EOF'`, which expands nothing. The body runs from the
//!   line after its opener to a line that is the delimiter alone, after the tabs `<<-` strips.
//! - **Commands**: a command's first word is a function (`grep`, `./configure`), a keyword (`if`,
//!   `export`, `return`), or a constant (`true`); an assignment before it (`NAME=value`) names a
//!   variable. A command starts a line, and after `;`, `|`, `&&`, `(`, `{`, the keywords that go
//!   on to one (`then`, `do`), and a command that runs another (`sudo`, `env`, `xargs`). Its other
//!   words are plain, or numbers in digits; in `[ … ]` and `[[ … ]]` a test's `-f` and `-eq` are
//!   operators. `for NAME in`, `case WORD in` and its patterns to their `)`, `function NAME`, and
//!   the names `export` and `local` declare read as what they are.
//! - **Redirections** are operators, `2>&1` whole, and the file after one is plain.
//!
//! At most [`MAX_NESTING`] substitutions and expansions stand open inside one another, and as many
//! heredocs wait for their bodies; what would open one more is read as what it stands in.
//!
//! A console reads a command line by line ([`command`]): the scanner stops at each line's end and
//! goes on where the next line's continuation prompt ends, while what the command opened is open.

use super::scan::{
    Spans, blanks_end, is_break, is_space, is_word, is_word_start, line_end, line_next, number_end, space_end, word_end,
};
use super::{Class, MAX_NESTING};

/// What the next word is in its command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// A command's name: first on a line, and after `;`, `|`, `&&`, `(`, `then`, and the like.
    Command,
    /// A command's name after a command that runs another (`sudo`), unless an option comes first.
    Wrapped,
    /// An argument.
    Argument,
    /// A name that `export`, `local`, `declare`, `readonly`, `typeset`, or `unset` declares.
    Name,
    /// A loop's variable, after `for` or `select`.
    Loop,
    /// The word a case matches.
    Subject,
    /// `in`, after a loop's variable, or after a case's word (true).
    In(bool),
    /// A function's name, after `function`.
    Function,
}

/// A heredoc, from its opener.
#[derive(Debug, Clone, Copy)]
struct Here {
    /// Where its delimiter is written, and its length.
    from: usize,
    len: usize,
    /// Whether the tabs that start its lines go: `<<-`.
    tabs: bool,
    /// Whether it expands what it holds: a delimiter in no quotes.
    expand: bool,
}

/// What the scanner returns to.
#[derive(Debug, Clone, Copy)]
enum Frame {
    /// Code: the text's own at the bottom of the stack (`close` 0), else a substitution's, which
    /// `)` or a backquote closes where no parenthesis inside it is open. `role` is what the
    /// command around it goes on with.
    Code { close: u8, depth: u32, role: Role },
    /// Arithmetic, `$((…))` or `((…))`, to its `))`.
    Arith { depth: u32, role: Role },
    /// A string in double quotes, which expands what it holds.
    Quoted,
    /// A string in single quotes; with `escapes`, `$'…'`.
    Single { escapes: bool },
    /// A parameter's expansion, `${…}`, to its `}`.
    Param,
    /// A heredoc's body.
    Here(Here),
}

const TOP: Frame = Frame::Code { close: 0, depth: 0, role: Role::Command };

/// Writes `text`'s tokens as Shell.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let mut scanner = Scanner::new(text, out, 0);
    scanner.run(text.len());
}

/// Writes the tokens of the command that starts at `at`, after a console's prompt, and gives where
/// it ends: at its line's end, or, while what it opened is open, past the lines it goes on over.
/// Each of them may start with a continuation prompt, which is meta: `> `, or zsh's `for> `.
pub(super) fn command<'t>(text: &'t str, out: &mut Spans<'t>, at: usize) -> usize {
    let b = text.as_bytes();
    let mut scanner = Scanner::new(text, out, at);
    let mut line = line_next(b, at, b.len());
    scanner.run(line);
    while !scanner.rests() && line < b.len() {
        let next = line_next(b, line, b.len());
        scanner.resume(line + continuation(b, line, next));
        scanner.run(next);
        line = next;
    }
    line
}

/// How long the continuation prompt is that starts the line at `at`: `> `, or a word and `> `.
fn continuation(b: &[u8], at: usize, end: usize) -> usize {
    let line = line_end(b, at, end);
    let mut k = at;
    while k < line && k - at < 16 && b[k].is_ascii_lowercase() {
        k += 1;
    }
    if k < line && b[k] == b'>' && (k + 1 == line || b[k + 1] == b' ') {
        k + 1 + usize::from(k + 1 < line) - at
    } else {
        0
    }
}

struct Scanner<'t, 'o> {
    text: &'t str,
    b: &'t [u8],
    pos: usize,
    /// Where this run stops: the text's end, or a console's line's.
    end: usize,
    /// Where the line being read starts, past a console's prompt: a word starts there, and a
    /// heredoc's terminator may stand there.
    start: usize,
    out: &'o mut Spans<'t>,
    stack: Vec<Frame>,
    /// What the next word is in its command.
    role: Role,
    /// Whether the next word is the file a redirection names.
    target: bool,
    /// Whether a word is being read: what follows it with no space between is a part of it, as
    /// `/bin` is of `"$HOME"/bin`.
    word: bool,
    /// Whether `[ … ]` or `[[ … ]]` is open.
    test: bool,
    /// Whether a case's pattern is being read, to its `)`.
    pattern: bool,
    /// The role the command goes on with after an array's `)`: `items=(a b)`.
    array: Option<Role>,
    /// The compound commands open: `if`, `case`, and a loop's `do`.
    blocks: u32,
    /// The cases open.
    cases: u32,
    /// The heredocs whose bodies start on the next line, in the order they opened.
    heres: Vec<Here>,
    /// Whether the command goes on past this line: after a backslash, `|`, `&&`, or `||`.
    more: bool,
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
            role: Role::Command,
            target: false,
            word: false,
            test: false,
            pattern: false,
            array: None,
            blocks: 0,
            cases: 0,
            heres: Vec::new(),
            more: false,
        }
    }

    /// Reads on to `end`.
    fn run(&mut self, end: usize) {
        self.end = end.min(self.b.len());
        while self.pos < self.end {
            let (pos, frames) = (self.pos, self.stack.len());
            match self.stack.last().copied().unwrap_or(TOP) {
                Frame::Code { close, depth, role } => self.code(close, depth, role),
                Frame::Arith { depth, role } => self.arith(depth, role),
                Frame::Quoted => self.quoted(),
                Frame::Single { escapes } => self.single(escapes),
                Frame::Param => self.param(),
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

    /// Whether the command is whole: nothing it opened is open, no heredoc waits for its body,
    /// and its last line does not go on.
    fn rests(&self) -> bool {
        self.stack.len() == 1
            && self.heres.is_empty()
            && !self.more
            && self.blocks == 0
            && matches!(self.stack[0], Frame::Code { depth: 0, .. })
    }

    /// The command goes on at `start`, past a continuation prompt, which is meta.
    fn resume(&mut self, start: usize) {
        self.out.push(start, Class::Meta);
        self.pos = start;
        self.start = start;
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

    /// Whether one more substitution or expansion may open.
    fn can_nest(&self) -> bool {
        let open = |f: &&Frame| matches!(f, Frame::Code { close: 1.., .. } | Frame::Arith { .. } | Frame::Param);
        self.stack.iter().filter(open).count() < MAX_NESTING
    }

    /// One more parenthesis open in the code on top, or one fewer.
    fn nest(&mut self, open: bool) {
        if let Some(Frame::Code { depth, .. } | Frame::Arith { depth, .. }) = self.stack.last_mut() {
            *depth = if open { depth.saturating_add(1) } else { depth.saturating_sub(1) };
        }
    }

    /// Whether a word starts at `i`: where the line starts, or after whitespace or what ends a
    /// word (`;`, `&`, `|`, `(`, `)`, `<`, `>`).
    fn starts_word(&self, i: usize) -> bool {
        i == self.start
            || (i > 0
                && (is_space(self.b[i - 1]) || matches!(self.b[i - 1], b';' | b'&' | b'|' | b'(' | b')' | b'<' | b'>')))
    }

    /// Whether a word ends at `j`: at whitespace, at what ends a word, or where the run stops.
    fn ends_word(&self, j: usize) -> bool {
        j >= self.end
            || is_space(self.b[j])
            || matches!(self.b[j], b';' | b'&' | b'|' | b'<' | b'>' | b'(' | b')' | b'`')
    }

    /// A line has ended: a command starts on the next.
    fn ended(&mut self) {
        self.role = Role::Command;
        self.test = false;
        self.target = false;
        self.word = false;
    }

    /// A word's part is read: the first moves its command on past the word.
    fn part(&mut self) {
        if std::mem::replace(&mut self.word, true) || std::mem::take(&mut self.target) {
            return;
        }
        self.role = match self.role {
            Role::Command | Role::Wrapped | Role::Function => Role::Argument,
            Role::Loop => Role::In(false),
            Role::Subject => Role::In(true),
            role => role,
        };
    }

    /// One step of code.
    fn code(&mut self, close: u8, depth: u32, role: Role) {
        let (b, i, end) = (self.b, self.pos, self.end);
        let c = b[i];
        let next = self.at(i + 1);
        if is_space(c) {
            let to = space_end(b, i, end);
            self.word = false;
            if let Some(k) = b[i..to].iter().position(|&c| is_break(c)) {
                self.ended();
                // A heredoc's body starts on the line after its opener.
                if !self.heres.is_empty() {
                    let body = line_next(b, i + k, end);
                    self.out.space(body);
                    self.pos = body;
                    let here = self.heres.remove(0);
                    return self.stack.push(Frame::Here(here));
                }
            }
            self.out.space(to);
            self.pos = to;
            return;
        }
        if c == b'#' && self.starts_word(i) {
            return self.emit(line_end(b, i, end), Class::Comment);
        }
        self.more = false;
        // A substitution's own close.
        if (close == b')' && c == b')' && depth == 0 && !self.pattern) || (close == b'`' && c == b'`') {
            self.emit(i + 1, Class::Punctuation);
            self.pop();
            self.role = role;
            self.word = true;
            return;
        }
        match c {
            b'\'' => {
                self.part();
                self.emit(i + 1, Class::String);
                return self.stack.push(Frame::Single { escapes: false });
            }
            b'"' => {
                self.part();
                self.emit(i + 1, Class::String);
                return self.stack.push(Frame::Quoted);
            }
            b'$' => {
                self.part();
                return self.dollar(false, Class::Plain);
            }
            b'`' => {
                self.part();
                return self.tick(Class::Plain);
            }
            // A line that goes on.
            b'\\' if is_break(next) => {
                self.emit(i + 1, Class::Punctuation);
                let after = line_next(b, i + 1, end);
                self.out.space(after);
                self.pos = after;
                self.more = true;
                return;
            }
            b';' => return self.semicolon(next),
            b'&' | b'|' => return self.control(c, next),
            b'<' | b'>' => return self.redirect(c, next),
            b'(' => return self.open(next),
            b')' => return self.close(),
            _ => {}
        }
        if c.is_ascii_digit()
            && let Some((to, file)) = self.descriptor(i)
        {
            self.word = false;
            self.target = file;
            return self.emit(to, Class::Operator);
        }
        self.bare();
    }

    /// `;`, or `;;`, `;&`, and `;;&`, which end a case's item.
    fn semicolon(&mut self, next: u8) {
        let i = self.pos;
        let n = match (next, self.at(i + 2)) {
            (b';', b'&') => 3,
            (b';' | b'&', _) => 2,
            _ => 1,
        };
        if n > 1 && self.cases > 0 {
            self.pattern = true;
        }
        self.word = false;
        self.role = Role::Command;
        self.test = false;
        self.emit(i + n, Class::Punctuation);
    }

    /// `&`, `&&`, `|`, `||`, and `|&`, after which a command starts, and `&>`, a redirection.
    fn control(&mut self, c: u8, next: u8) {
        let i = self.pos;
        self.word = false;
        if c == b'&' && next == b'>' {
            self.target = true;
            return self.emit(i + if self.at(i + 2) == b'>' { 3 } else { 2 }, Class::Operator);
        }
        // A pattern's alternatives: `start|stop)`.
        if c == b'|' && self.pattern {
            return self.emit(i + 1, Class::Operator);
        }
        let n = if next == c || (c == b'|' && next == b'&') { 2 } else { 1 };
        // `&&` and `||` inside `[[ … ]]` join its tests.
        if !self.test || n == 1 {
            self.more = n == 2 || c == b'|';
            self.role = Role::Command;
            self.test = false;
        }
        self.emit(i + n, Class::Operator);
    }

    /// A redirection, a heredoc's opener, or a process substitution.
    fn redirect(&mut self, c: u8, next: u8) {
        let i = self.pos;
        if next == b'(' && self.can_nest() {
            // `<(…)` and `>(…)`.
            self.part();
            self.emit(i + 2, Class::Punctuation);
            self.stack.push(Frame::Code { close: b')', depth: 0, role: self.role });
            self.role = Role::Command;
            self.word = false;
            return;
        }
        self.word = false;
        // Inside `[[ … ]]`, `<` and `>` compare.
        if self.test {
            return self.emit(i + 1, Class::Operator);
        }
        if c == b'<' && next == b'<' {
            if self.at(i + 2) == b'<' {
                return self.emit(i + 3, Class::Operator);
            }
            if self.heredoc() {
                return;
            }
            return self.emit(i + 2, Class::Operator);
        }
        let mut n = 1 + usize::from(matches!((c, next), (b'>', b'>' | b'|' | b'&') | (b'<', b'&' | b'>')));
        // A descriptor it copies: `>&2`, `<&-`.
        if self.at(i + n - 1) == b'&' && (self.at(i + n).is_ascii_digit() || self.at(i + n) == b'-') {
            while self.at(i + n).is_ascii_digit() || self.at(i + n) == b'-' {
                n += 1;
            }
        } else {
            self.target = true;
        }
        self.emit(i + n, Class::Operator);
    }

    /// A descriptor's number and its redirection, where they start a word: `2>`, `2>>`, `2>&1`.
    /// Where it ends, and whether a file follows it.
    fn descriptor(&self, i: usize) -> Option<(usize, bool)> {
        if !self.starts_word(i) {
            return None;
        }
        let mut k = i;
        while self.at(k).is_ascii_digit() {
            k += 1;
        }
        let c = self.at(k);
        if !matches!(c, b'<' | b'>') || self.at(k + 1) == b'(' || (c == b'<' && self.at(k + 1) == b'<') {
            return None;
        }
        let mut e = k + 1 + usize::from(matches!(self.at(k + 1), b'>' | b'&' | b'|'));
        if self.at(e - 1) == b'&' {
            let digits = e;
            while self.at(e).is_ascii_digit() || self.at(e) == b'-' {
                e += 1;
            }
            return Some((e, e == digits));
        }
        Some((e, true))
    }

    /// A heredoc's opener here, read, its body waiting for the next line; false where `<<` has
    /// no delimiter after it.
    fn heredoc(&mut self) -> bool {
        let (b, i, end) = (self.b, self.pos, self.end);
        let tabs = self.at(i + 2) == b'-';
        let op = i + 2 + usize::from(tabs);
        let k = blanks_end(b, op, end);
        let q = self.at(k);
        let (from, len, expand, to) = if q == b'\'' || q == b'"' {
            let Some(n) = b[k + 1..line_end(b, k + 1, end)].iter().position(|&c| c == q) else {
                return false;
            };
            (k + 1, n, false, k + 2 + n)
        } else {
            let from = k + usize::from(q == b'\\');
            let mut e = from;
            while e < end
                && !is_space(b[e])
                && !matches!(b[e], b';' | b'&' | b'|' | b'<' | b'>' | b'(' | b')' | b'\'' | b'"' | b'`')
            {
                e += 1;
            }
            (from, e - from, q != b'\\', e)
        };
        if len == 0 || self.heres.len() >= MAX_NESTING {
            return false;
        }
        self.heres.push(Here { from, len, tabs, expand });
        self.emit(op, Class::Operator);
        self.out.space(k);
        self.emit(to, Class::String);
        true
    }

    /// `(`: arithmetic where a command or a loop's variable starts (`((i++))`, `for ((…))`), an
    /// array's items after `=`, a pattern's opener in a case, or a subshell.
    fn open(&mut self, next: u8) {
        let (b, i) = (self.b, self.pos);
        let array = std::mem::take(&mut self.word) && i > 0 && b[i - 1] == b'=';
        if next == b'(' && matches!(self.role, Role::Command | Role::Loop) && !array && self.can_nest() {
            self.emit(i + 2, Class::Punctuation);
            return self.stack.push(Frame::Arith { depth: 0, role: Role::Argument });
        }
        self.emit(i + 1, Class::Punctuation);
        if self.pattern {
            return;
        }
        self.nest(true);
        if array {
            self.array = Some(self.role);
            self.role = Role::Argument;
        } else {
            self.role = Role::Command;
        }
    }

    /// `)`: a pattern's close, an array's, or a subshell's.
    fn close(&mut self) {
        self.word = false;
        self.emit(self.pos + 1, Class::Punctuation);
        if self.pattern {
            self.pattern = false;
            self.role = Role::Command;
            return;
        }
        self.nest(false);
        self.role = self.array.take().unwrap_or(Role::Argument);
    }

    /// A word's unquoted part, to what ends a word or begins another part: a command, a keyword,
    /// an assignment, an argument, or what else its role makes it.
    fn bare(&mut self) {
        let (b, i, end) = (self.b, self.pos, self.end);
        let mut j = i;
        while j < end {
            let c = b[j];
            if is_space(c) || matches!(c, b';' | b'&' | b'|' | b'<' | b'>' | b'(' | b')' | b'\'' | b'"' | b'$' | b'`') {
                break;
            }
            if c == b'\\' {
                if is_break(self.at(j + 1)) {
                    break;
                }
                j += 2;
                continue;
            }
            j += 1;
        }
        let j = j.clamp(i + 1, end);
        let word = self.text.get(i..j).unwrap_or_default();
        let first = !std::mem::replace(&mut self.word, true);
        let whole = first && self.ends_word(j);
        if !first || std::mem::take(&mut self.target) {
            return self.emit(j, if number(word) { Class::Number } else { Class::Plain });
        }
        if self.pattern {
            if whole && word == "esac" {
                self.esac();
                return self.emit(j, Class::Keyword);
            }
            return self.emit(j, Class::Plain);
        }
        if whole && matches!(word, "{" | "}") {
            self.role = if word == "{" { Role::Command } else { Role::Argument };
            self.word = false;
            return self.emit(j, Class::Punctuation);
        }
        if whole && self.role == Role::Command && matches!(word, "[[" | "[") {
            self.test = true;
            self.role = Role::Argument;
            return self.emit(j, Class::Punctuation);
        }
        if whole && self.test && matches!(word, "]]" | "]") {
            self.test = false;
            return self.emit(j, Class::Punctuation);
        }
        if whole && self.role == Role::Command && word == "!" {
            self.word = false;
            return self.emit(j, Class::Operator);
        }
        let class = match self.role {
            Role::Command | Role::Wrapped => return self.command(i, j, word, whole),
            Role::Name => return self.name(i, j, word),
            Role::Loop => {
                self.role = Role::In(false);
                Class::Variable
            }
            Role::Subject => {
                self.role = Role::In(true);
                Class::Plain
            }
            Role::In(case) if whole && word == "in" => {
                if case {
                    self.pattern = true;
                    self.role = Role::Command;
                } else {
                    self.role = Role::Argument;
                }
                Class::Keyword
            }
            Role::In(_) => {
                self.role = Role::Argument;
                self.argument(word)
            }
            Role::Function => {
                self.role = Role::Argument;
                Class::Function
            }
            Role::Argument => self.argument(word),
        };
        self.emit(j, class);
    }

    /// A word where a command's name stands: an assignment before it, a keyword, a constant, or
    /// the command, a function.
    fn command(&mut self, i: usize, j: usize, word: &str, whole: bool) {
        if let Some((name, value)) = assignment(self.b, i, j) {
            return self.assign(name, value, j);
        }
        let wrapped = self.role == Role::Wrapped;
        self.role = Role::Argument;
        let class = if !whole {
            Class::Function
        } else if KEYWORDS.binary_search(&word).is_ok() {
            self.keyword(word);
            Class::Keyword
        } else if matches!(word, "true" | "false") {
            Class::Constant
        } else if wrapped && word.starts_with('-') {
            Class::Plain
        } else {
            if WRAPPERS.binary_search(&word).is_ok() {
                self.role = Role::Wrapped;
            }
            self.test = word == "test";
            Class::Function
        };
        self.emit(j, class);
    }

    /// What a keyword opens, closes, and goes on to.
    fn keyword(&mut self, word: &str) {
        match word {
            "if" | "do" => self.blocks += 1,
            "case" => {
                self.blocks += 1;
                self.cases += 1;
            }
            "fi" | "done" => self.blocks = self.blocks.saturating_sub(1),
            "esac" => self.esac(),
            _ => {}
        }
        self.role = match word {
            "for" | "select" => Role::Loop,
            "case" => Role::Subject,
            "function" => Role::Function,
            "declare" | "export" | "local" | "readonly" | "typeset" | "unset" => Role::Name,
            "break" | "continue" | "done" | "esac" | "exit" | "fi" | "return" => Role::Argument,
            _ => Role::Command,
        };
    }

    /// `esac` closes a case.
    fn esac(&mut self) {
        self.blocks = self.blocks.saturating_sub(1);
        self.cases = self.cases.saturating_sub(1);
        self.pattern = false;
        self.role = Role::Argument;
    }

    /// An assignment: its name a variable, its `=` an operator, and the rest of its part the value.
    fn assign(&mut self, name: usize, value: usize, j: usize) {
        self.emit(name, Class::Variable);
        self.emit(value, Class::Operator);
        let rest = self.text.get(value..j).unwrap_or_default();
        self.emit(j, if number(rest) { Class::Number } else { Class::Plain });
    }

    /// A word after `export`, `local`, and the like: an assignment, a name it declares, or an
    /// option.
    fn name(&mut self, i: usize, j: usize, word: &str) {
        if let Some((name, value)) = assignment(self.b, i, j) {
            return self.assign(name, value, j);
        }
        let named = word.bytes().next().is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
            && word.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_');
        self.emit(j, if named { Class::Variable } else { Class::Plain });
    }

    /// An argument: an operator in a test (`-f`, `-eq`, `==`), a number, or plain.
    fn argument(&self, word: &str) -> Class {
        let flag = word.len() > 1 && word.starts_with('-') && word[1..].bytes().all(|c| c.is_ascii_lowercase());
        if self.test && (flag || matches!(word, "==" | "!=" | "=~" | "=" | "!")) {
            Class::Operator
        } else if number(word) {
            Class::Number
        } else {
            Class::Plain
        }
    }

    /// `$` and what it expands: a substitution, an expansion, a quote, or a variable. A `$`
    /// that expands nothing is in `rest`, the class of what it stands in.
    fn dollar(&mut self, quoted: bool, rest: Class) {
        let i = self.pos;
        let next = self.at(i + 1);
        if next == b'(' && self.can_nest() {
            if self.at(i + 2) == b'(' {
                self.emit(i + 3, Class::Punctuation);
                return self.stack.push(Frame::Arith { depth: 0, role: self.role });
            }
            self.emit(i + 2, Class::Punctuation);
            self.stack.push(Frame::Code { close: b')', depth: 0, role: self.role });
            self.role = Role::Command;
            self.word = false;
            return;
        }
        if next == b'{' && self.can_nest() {
            self.emit(i + 2, Class::Variable);
            return self.stack.push(Frame::Param);
        }
        if !quoted && next == b'\'' {
            self.emit(i + 2, Class::String);
            return self.stack.push(Frame::Single { escapes: true });
        }
        if !quoted && next == b'"' {
            self.emit(i + 2, Class::String);
            return self.stack.push(Frame::Quoted);
        }
        if next.is_ascii_alphabetic() || next == b'_' {
            let mut e = i + 2;
            while self.at(e).is_ascii_alphanumeric() || self.at(e) == b'_' {
                e += 1;
            }
            return self.emit(e, Class::Variable);
        }
        if next.is_ascii_digit() || b"@*#?-$!".contains(&next) {
            return self.emit(i + 2, Class::Variable);
        }
        self.emit(i + 1, rest);
    }

    /// Whether a `$` at `i` expands what follows it, in a string or a heredoc.
    fn expands(&self, i: usize) -> bool {
        let next = self.at(i + 1);
        next.is_ascii_alphanumeric() || b"_({@*#?-$!".contains(&next)
    }

    /// A backquote: a substitution opens, or, where none may, it is in `rest`.
    fn tick(&mut self, rest: Class) {
        let i = self.pos;
        if !self.can_nest() {
            return self.emit(i + 1, rest);
        }
        self.emit(i + 1, Class::Punctuation);
        self.stack.push(Frame::Code { close: b'`', depth: 0, role: self.role });
        self.role = Role::Command;
        self.word = false;
    }

    /// One step of a string in double quotes: to its close, or to what it expands.
    fn quoted(&mut self) {
        let (b, end) = (self.b, self.end);
        let mut i = self.pos;
        while i < end {
            match b[i] {
                b'\\' => i += 2,
                b'"' => {
                    self.emit(i + 1, Class::String);
                    return self.pop();
                }
                b'$' if self.expands(i) => {
                    self.out.push(i, Class::String);
                    self.pos = i;
                    return self.dollar(true, Class::String);
                }
                b'`' if self.can_nest() => {
                    self.out.push(i, Class::String);
                    self.pos = i;
                    return self.tick(Class::String);
                }
                _ => i += 1,
            }
        }
        self.emit(end, Class::String);
    }

    /// One step of a string in single quotes, to its close.
    fn single(&mut self, escapes: bool) {
        let (b, end) = (self.b, self.end);
        let mut i = self.pos;
        while i < end {
            match b[i] {
                b'\\' if escapes => i += 2,
                b'\'' => {
                    self.emit(i + 1, Class::String);
                    return self.pop();
                }
                _ => i += 1,
            }
        }
        self.emit(end, Class::String);
    }

    /// One step of a parameter's expansion: to its `}`, or to a quote or a substitution in it.
    fn param(&mut self) {
        let (b, end) = (self.b, self.end);
        let mut i = self.pos;
        while i < end {
            match b[i] {
                b'}' => {
                    self.emit(i + 1, Class::Variable);
                    return self.pop();
                }
                b'\\' => i += 2,
                c @ (b'$' | b'`' | b'"' | b'\'') => {
                    self.out.push(i, Class::Variable);
                    self.pos = i;
                    return match c {
                        b'$' => self.dollar(false, Class::Variable),
                        b'`' => self.tick(Class::Variable),
                        _ => {
                            self.emit(i + 1, Class::String);
                            let frame = if c == b'"' { Frame::Quoted } else { Frame::Single { escapes: false } };
                            self.stack.push(frame);
                        }
                    };
                }
                _ => i += 1,
            }
        }
        self.emit(end, Class::Variable);
    }

    /// One step of arithmetic: its names are variables, to its `))`.
    fn arith(&mut self, depth: u32, role: Role) {
        let (b, i, end) = (self.b, self.pos, self.end);
        let c = b[i];
        let next = self.at(i + 1);
        if is_space(c) {
            let to = space_end(b, i, end);
            self.out.space(to);
            self.pos = to;
            return;
        }
        if c == b')' && next == b')' && depth == 0 {
            self.emit(i + 2, Class::Punctuation);
            self.pop();
            self.role = role;
            self.word = true;
            return;
        }
        match c {
            b'(' | b')' => {
                self.nest(c == b'(');
                self.emit(i + 1, Class::Punctuation);
            }
            b'$' => self.dollar(false, Class::Plain),
            b'`' => self.tick(Class::Plain),
            b'"' | b'\'' => {
                self.emit(i + 1, Class::String);
                let frame = if c == b'"' { Frame::Quoted } else { Frame::Single { escapes: false } };
                self.stack.push(frame);
            }
            b',' | b';' => self.emit(i + 1, Class::Punctuation),
            _ if c.is_ascii_digit() => {
                // A number, in its base too: `16#ff`.
                let mut e = number_end(b, i, end, false);
                if self.at(e) == b'#' && is_word(self.at(e + 1)) {
                    e = word_end(b, e + 1, end);
                }
                self.emit(e, Class::Number);
            }
            _ if is_word_start(c) => self.emit(word_end(b, i, end), Class::Variable),
            _ if c.is_ascii_punctuation() => self.emit(i + 1, Class::Operator),
            _ => self.emit(i + 1, Class::Plain),
        }
    }

    /// One step of a heredoc's body: its terminator, or a line of it, to its end or to what it
    /// expands.
    fn here(&mut self, h: Here) {
        let (b, i, end) = (self.b, self.pos, self.end);
        if i == self.start || is_break(b[i - 1]) {
            let mut k = i;
            while h.tabs && self.at(k) == b'\t' {
                k += 1;
            }
            let close = k + h.len;
            if close <= end && b[k..close] == b[h.from..h.from + h.len] && (close == end || is_break(b[close])) {
                self.emit(close, Class::String);
                return self.pop();
            }
        }
        let mut j = i;
        while j < end {
            let c = b[j];
            if is_break(c) {
                return self.emit(line_next(b, j, end), Class::String);
            }
            if h.expand {
                if c == b'\\' && !is_break(self.at(j + 1)) {
                    j += 2;
                    continue;
                }
                if c == b'$' && self.expands(j) {
                    self.out.push(j, Class::String);
                    self.pos = j;
                    return self.dollar(true, Class::String);
                }
                if c == b'`' && self.can_nest() {
                    self.out.push(j, Class::String);
                    self.pos = j;
                    return self.tick(Class::String);
                }
            }
            j += 1;
        }
        self.emit(end, Class::String);
    }
}

/// Where an assignment's name ends in the word from `i` to `j`, and its `=`: `NAME=`, `NAME+=`,
/// `items[2]=`.
fn assignment(b: &[u8], i: usize, j: usize) -> Option<(usize, usize)> {
    if !(b[i].is_ascii_alphabetic() || b[i] == b'_') {
        return None;
    }
    let mut k = i + 1;
    while k < j && (b[k].is_ascii_alphanumeric() || b[k] == b'_') {
        k += 1;
    }
    if k < j && b[k] == b'[' {
        k += b[k..j].iter().position(|&c| c == b']')? + 1;
    }
    let name = k;
    if k < j && b[k] == b'+' {
        k += 1;
    }
    (k < j && b[k] == b'=').then_some((name, k + 1))
}

/// Whether `word` is a number: digits, with a point between them at most once.
fn number(word: &str) -> bool {
    let b = word.as_bytes();
    b.first().is_some_and(u8::is_ascii_digit)
        && b.last().is_some_and(u8::is_ascii_digit)
        && b.iter().all(|c| c.is_ascii_digit() || *c == b'.')
        && b.iter().filter(|&&c| c == b'.').count() <= 1
}

// The tables, each sorted, so a word is found by binary search (`every_table_is_sorted`).

/// The shell's reserved words, the builtins that declare names, and those that leave a loop or
/// a function.
const KEYWORDS: &[&str] = &[
    "break", "case", "continue", "coproc", "declare", "do", "done", "elif", "else", "esac", "exit", "export", "fi",
    "for", "function", "if", "local", "readonly", "return", "select", "then", "time", "typeset", "unset", "until",
    "while",
];
/// The commands that run the command after them.
const WRAPPERS: &[&str] = &["builtin", "command", "env", "exec", "nice", "nohup", "sudo", "xargs"];

#[cfg(test)]
mod tests {
    use super::*;

    /// Each table is sorted, with no word twice, so binary search finds what it holds.
    #[test]
    fn every_table_is_sorted() {
        for table in [KEYWORDS, WRAPPERS] {
            assert!(table.windows(2).all(|w| w[0] < w[1]), "not sorted: {table:?}");
        }
    }
}
