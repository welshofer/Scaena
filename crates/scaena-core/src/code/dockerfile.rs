//! A Dockerfile (PLAN 2.135).
//!
//! - **Instructions**, first on their lines in any case (`FROM`, `RUN`, `COPY`), are keywords, and
//!   so are `AS` in `FROM` and the `CMD` and `NONE` of `HEALTHCHECK`.
//! - **Commands**, `RUN`'s, `CMD`'s, `ENTRYPOINT`'s, and a `HEALTHCHECK`'s, read as Shell over the
//!   lines the escape (`\`) joins, the comments and blank lines among them dropped as Docker drops
//!   them; or as PowerShell, where a `SHELL` instruction names it or the escape is a backtick. A
//!   heredoc that is the whole command (`RUN <<EOF`) is a script, read as the command would be. In
//!   the exec form, a JSON array of strings (`CMD ["node", "server.js"]`), the strings are strings.
//! - **Arguments**: a flag (`--from=build`) is an attribute; `ARG`'s and `ENV`'s names are
//!   variables, and `LABEL`'s keys properties, each before its `=`, an operator; `$name` and
//!   `${name:-x}` are variables, in double quotes too; quotes are strings, over the lines the
//!   escape joins; and digits are numbers, but in an image, whose tag is a name (`FROM node:20`,
//!   `COPY --from=alpine:3.20`). A heredoc in another instruction (`COPY <<EOF /etc/x`) is a
//!   string, its body to its terminator.
//! - **Comments**: `#` first on a line; and, at the file's start before anything else, a parser
//!   directive (`# syntax=docker/dockerfile:1`, `` # escape=` ``), which is meta.
//!
//! At most [`MAX_NESTING`] heredocs wait for their bodies after one instruction.

use super::scan::{Spans, blanks_end, byte, is_break, is_space, is_word, is_word_start, line_end, line_next};
use super::{Class, MAX_NESTING, powershell, shell};

/// What reads a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reader {
    Shell,
    PowerShell,
}

/// What an instruction's words are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Words {
    /// Arguments: paths, images, ports.
    Plain,
    /// `FROM`'s: an image, then `AS` and the stage's name.
    From,
    /// Names before their `=` and their values, `ARG`'s and `ENV`'s variables or `LABEL`'s keys.
    Names(Class),
}

/// A heredoc, from its opener: its delimiter, and whether the tabs that start its lines go.
#[derive(Debug, Clone, Copy)]
struct Here {
    from: usize,
    len: usize,
    tabs: bool,
}

/// The instructions, in lowercase and sorted, so one is found by binary search in any case.
const INSTRUCTIONS: &[&str] = &[
    "add",
    "arg",
    "cmd",
    "copy",
    "entrypoint",
    "env",
    "expose",
    "from",
    "healthcheck",
    "label",
    "maintainer",
    "onbuild",
    "run",
    "shell",
    "stopsignal",
    "user",
    "volume",
    "workdir",
];

/// The parser directives, in lowercase.
const DIRECTIVES: &[&str] = &["check", "escape", "syntax"];

/// Writes `text`'s tokens as a Dockerfile.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let mut docker = Docker { text, b: text.as_bytes(), out, escape: b'\\', reader: Reader::Shell, heres: Vec::new() };
    docker.run();
}

struct Docker<'t, 'o> {
    text: &'t str,
    b: &'t [u8],
    out: &'o mut Spans<'t>,
    /// What escapes a character and ends a line that goes on: `\`, or a backtick.
    escape: u8,
    reader: Reader,
    /// The heredocs whose bodies follow the instruction being read, in the order they opened.
    heres: Vec<Here>,
}

impl Docker<'_, '_> {
    fn run(&mut self) {
        let (b, len) = (self.b, self.b.len());
        let mut pos = 0;
        // Whether a parser directive may still come: nothing else has.
        let mut directives = true;
        while pos < len {
            let end = line_end(b, pos, len);
            let first = blanks_end(b, pos, end);
            self.out.space(first);
            if first == end {
                directives = false;
                pos = line_next(b, pos, len);
                self.out.space(pos);
            } else if b[first] == b'#' {
                directives &= self.directive(first, end);
                self.out.push(end, if directives { Class::Meta } else { Class::Comment });
                pos = line_next(b, pos, len);
                self.out.space(pos);
            } else {
                directives = false;
                pos = self.instruction(first, false);
            }
        }
    }

    /// Whether the comment from `at` to `end` is a parser directive Docker knows, `# name=value`;
    /// an escape it sets is read.
    fn directive(&mut self, at: usize, end: usize) -> bool {
        let b = self.b;
        let name = blanks_end(b, at + 1, end);
        let mut e = name;
        while e < end && b[e].is_ascii_alphabetic() {
            e += 1;
        }
        let sign = blanks_end(b, e, end);
        let known = DIRECTIVES.iter().any(|d| d.as_bytes().eq_ignore_ascii_case(&b[name..e]));
        if !known || sign >= end || b[sign] != b'=' {
            return false;
        }
        let value = blanks_end(b, sign + 1, end);
        if b[name..e].eq_ignore_ascii_case(b"escape") && value < end && matches!(b[value], b'\\' | b'`') {
            self.escape = b[value];
            if self.escape == b'`' {
                self.reader = Reader::PowerShell;
            }
        }
        true
    }

    /// The instruction at `at`, with its arguments and the bodies of its heredocs: where it ends,
    /// past its last line. `inner` is set for the one `ONBUILD` holds.
    fn instruction(&mut self, at: usize, inner: bool) -> usize {
        let (b, len) = (self.b, self.b.len());
        let mut w = at;
        while w < len && b[w].is_ascii_alphabetic() {
            w += 1;
        }
        // No instruction is longer than `HEALTHCHECK`.
        let word =
            if w - at <= 11 { self.text.get(at..w).unwrap_or_default().to_ascii_lowercase() } else { String::new() };
        let known = w > at && INSTRUCTIONS.binary_search(&word.as_str()).is_ok() && (w == len || is_space(b[w]));
        if !known {
            let end = self.words(at, Words::Plain);
            return self.finish(end);
        }
        self.out.push(w, Class::Keyword);
        let end = match word.as_str() {
            "run" | "cmd" | "entrypoint" => self.command(w),
            "healthcheck" => {
                let k = self.flags(w);
                let e = word_end(b, k);
                match &b[k..e] {
                    then if then.eq_ignore_ascii_case(b"cmd") => {
                        self.out.push(e, Class::Keyword);
                        self.command(e)
                    }
                    then if then.eq_ignore_ascii_case(b"none") => {
                        self.out.push(e, Class::Keyword);
                        self.words(e, Words::Plain)
                    }
                    _ => self.words(k, Words::Plain),
                }
            }
            "onbuild" if !inner => {
                let k = self.gap(w);
                if k >= len || is_break(b[k]) {
                    return self.finish(k);
                }
                return self.instruction(k, true);
            }
            "shell" => {
                let k = self.gap(w);
                match self.exec(k) {
                    Some(end) => {
                        self.reader = self.shell(k, end);
                        end
                    }
                    None => self.words(k, Words::Plain),
                }
            }
            // `ARG` names each variable, its default after an `=` (`ARG TARGETOS TARGETARCH`).
            "arg" => self.names(w, Words::Names(Class::Variable), false),
            "env" => self.words(w, Words::Names(Class::Variable)),
            "label" => self.words(w, Words::Names(Class::Property)),
            "from" => self.words(w, Words::From),
            _ => {
                let k = self.flags(w);
                match self.exec(k) {
                    Some(end) => end,
                    None => self.words(k, Words::Plain),
                }
            }
        };
        self.finish(end)
    }

    /// An instruction's end, at `end`, where its line's break follows: past the break, and past the
    /// bodies of the heredocs it opened.
    fn finish(&mut self, end: usize) -> usize {
        let (b, len) = (self.b, self.b.len());
        let mut pos = line_next(b, end, len);
        self.out.space(pos);
        for here in std::mem::take(&mut self.heres) {
            let term = terminator(b, pos, here);
            let next = line_next(b, term, len);
            self.out.push(line_end(b, term, len), Class::String);
            self.out.space(next);
            pos = next;
        }
        pos
    }

    /// A command, `RUN`'s, `CMD`'s, or `ENTRYPOINT`'s, from just after its instruction at `at`:
    /// its flags, then its exec form, its script, or its words as the shell reads them. Where it
    /// ends, before its last line's break.
    fn command(&mut self, at: usize) -> usize {
        let k = self.flags(at);
        if let Some(end) = self.exec(k) {
            return end;
        }
        if let Some(end) = self.script(k) {
            return end;
        }
        let end = match self.reader {
            Reader::Shell => shell::instruction(self.text, self.out, k, self.escape),
            Reader::PowerShell => powershell::instruction(self.text, self.out, k, self.escape),
        };
        // Its last line's break is read; where the instruction ends, before it.
        let mut last = end;
        if last > k && byte(self.b, last - 1) == b'\n' {
            last -= 1;
        }
        if last > k && byte(self.b, last - 1) == b'\r' {
            last -= 1;
        }
        last
    }

    /// Where the line goes on after the escape at `i` that ends it, the spaces and tabs after the
    /// escape passed over: where the next line starts. `None` where `i` holds no escape, or one
    /// that ends no line.
    fn joins(&self, i: usize) -> Option<usize> {
        let (b, len) = (self.b, self.b.len());
        if i >= len || b[i] != self.escape {
            return None;
        }
        let k = blanks_end(b, i + 1, len);
        (k < len && is_break(b[k])).then(|| line_next(b, k, len))
    }

    /// The comments and the blank lines from `at`, which Docker passes over among the lines the
    /// escape joins, comments: where the line after them starts, or the text's end.
    fn dropped(&mut self, at: usize) -> usize {
        let (b, len) = (self.b, self.b.len());
        let mut line = at;
        loop {
            let first = blanks_end(b, line, len);
            if first >= len {
                return line;
            }
            if b[first] == b'#' {
                self.out.space(first);
                self.out.push(line_end(b, first, len), Class::Comment);
            } else if !is_break(b[first]) {
                return line;
            }
            line = line_next(b, first, len);
            self.out.space(line);
        }
    }

    /// The spaces and tabs at `at`, and a line the escape joins to the next: the escape, the
    /// break, and the comments and blank lines after it, which Docker passes over. Where what
    /// follows starts, or the line's break where the line does not go on.
    fn gap(&mut self, at: usize) -> usize {
        let (b, len) = (self.b, self.b.len());
        let mut i = blanks_end(b, at, len);
        self.out.space(i);
        while let Some(line) = self.joins(i) {
            self.out.push(i + 1, Class::Punctuation);
            self.out.space(line);
            let line = self.dropped(line);
            i = blanks_end(b, line, len);
            self.out.space(i);
        }
        i
    }

    /// The flags at `at`, before an instruction's other arguments (`--mount=type=cache,target=/x`),
    /// over the lines the escape joins: where what follows them starts.
    fn flags(&mut self, at: usize) -> usize {
        let b = self.b;
        let mut i = self.gap(at);
        while byte(b, i) == b'-' && byte(b, i + 1) == b'-' && is_word_start(byte(b, i + 2)) {
            i = self.word(i, Words::Plain);
            i = self.gap(i);
        }
        i
    }

    /// The words from `at` to the instruction's end, over the lines the escape joins, each read as
    /// `words` says: where they end, before the last line's break.
    fn words(&mut self, at: usize, words: Words) -> usize {
        self.names(at, words, true)
    }

    /// The words from `at` to the instruction's end, as [`Docker::words`] reads them, where `legacy`
    /// says whether a name with no `=` is followed by its value as the rest of the line, as `ENV`
    /// and `LABEL` once wrote one (`ENV PATH /bin`); `ARG` never does.
    fn names(&mut self, at: usize, words: Words, legacy: bool) -> usize {
        let b = self.b;
        let mut i = at;
        // Whether a name was written with no `=`: the rest is its value (`ENV PATH /bin`).
        let mut rest = false;
        loop {
            i = self.gap(i);
            if i >= b.len() || is_break(b[i]) {
                return i;
            }
            let role = if rest { Words::Plain } else { words };
            let start = i;
            i = self.word(i, role);
            if let Words::Names(_) = role {
                rest = legacy && !b[start..i].contains(&b'=');
            }
        }
    }

    /// Whether the word ends at `i`: at a space, a tab, a break, or the text's end.
    fn stops(&self, i: usize) -> bool {
        i >= self.b.len() || matches!(self.b[i], b' ' | b'\t' | b'\n' | b'\r')
    }

    /// The word at `at`, to the space or the line's end after it, read as `words` says: where it
    /// ends.
    fn word(&mut self, at: usize, words: Words) -> usize {
        let (b, len) = (self.b, self.b.len());
        let mut i = at;
        // A flag's name, or a name before its `=`: `--from=build`, `NODE_ENV=production`, and a
        // label's key in quotes, `"com.example.vendor"="ACME"`.
        let flag = byte(b, i) == b'-' && byte(b, i + 1) == b'-' && is_word_start(byte(b, i + 2));
        // Whether the word names an image, whose tag is a name, not a number: `FROM node:20`, and
        // `COPY --from=ghcr.io/astral-sh/uv:0.4`.
        let mut image = words == Words::From;
        if flag || matches!(words, Words::Names(_)) {
            let class = match words {
                Words::Names(class) if !flag => class,
                _ => Class::Attribute,
            };
            let mut e = i;
            if !flag && matches!(byte(b, i), b'"' | b'\'') {
                let close = self.quote_end(i);
                if byte(b, close) == b'=' {
                    self.out.push(close, class);
                    e = close;
                }
            } else {
                while !self.stops(e) && !matches!(b[e], b'=' | b'"' | b'\'' | b'$') && b[e] != self.escape {
                    e += 1;
                }
                self.out.push(e, class);
                image |= flag && b[i..e].eq_ignore_ascii_case(b"--from");
            }
            i = e;
            if byte(b, i) == b'=' {
                self.out.push(i + 1, Class::Operator);
                i += 1;
            }
        } else if words == Words::From && is_word_start(byte(b, i)) {
            let e = word_end(b, i);
            if b[i..e].eq_ignore_ascii_case(b"as") && self.stops(e) {
                self.out.push(e, Class::Keyword);
                return e;
            }
        } else if byte(b, i) == b'<'
            && byte(b, i + 1) == b'<'
            && let Some(end) = self.heredoc(i)
        {
            return end;
        }
        while !self.stops(i) {
            let c = b[i];
            if c == self.escape {
                if self.joins(i).is_some() {
                    break;
                }
                let to = (i + 2).min(len);
                self.out.push(to, Class::Plain);
                i = to;
                continue;
            }
            i = match c {
                b'"' => self.quoted(i),
                b'\'' => {
                    let close = self.quote_end(i);
                    self.out.push(close, Class::String);
                    close
                }
                b'$' => self.variable(i, Class::Plain),
                b'/' | b':' | b'=' | b',' => {
                    self.out.push(i + 1, Class::Plain);
                    i + 1
                }
                _ => {
                    let mut e = i + 1;
                    while !self.stops(e)
                        && !matches!(b[e], b'"' | b'\'' | b'$' | b'/' | b':' | b'=' | b',')
                        && b[e] != self.escape
                    {
                        e += 1;
                    }
                    let numeric = !image && number(&b[i..e]);
                    self.out.push(e, if numeric { Class::Number } else { Class::Plain });
                    e
                }
            };
        }
        i
    }

    /// Where the string that opens with the quote at `at` ends: past the same quote, or at the end
    /// of the last line the escape joins to its line. It reads ahead and writes nothing.
    fn quote_end(&self, at: usize) -> usize {
        let (b, len) = (self.b, self.b.len());
        let quote = b[at];
        let mut i = at + 1;
        while i < len {
            let c = b[i];
            if c == quote {
                return i + 1;
            }
            if is_break(c) {
                return i;
            }
            i = match self.joins(i) {
                Some(line) => line,
                None if c == self.escape && quote == b'"' => i + 2,
                None => i + 1,
            };
        }
        len
    }

    /// A string in double quotes from `at`, to its close, over the lines the escape joins, the
    /// variables in it variables: where it ends.
    fn quoted(&mut self, at: usize) -> usize {
        let (b, len) = (self.b, self.b.len());
        self.out.push(at + 1, Class::String);
        let mut i = at + 1;
        while i < len {
            let c = b[i];
            if c == b'"' {
                self.out.push(i + 1, Class::String);
                return i + 1;
            }
            if is_break(c) {
                break;
            }
            if let Some(line) = self.joins(i) {
                i = line;
            } else if c == self.escape {
                i += 2;
            } else if c == b'$' {
                self.out.push(i, Class::String);
                i = self.variable(i, Class::String);
            } else {
                i += 1;
            }
        }
        let end = i.min(len);
        self.out.push(end, Class::String);
        end
    }

    /// `$name` or `${name:-default}` at `at`, to its line's end at most, a variable; a `$` that
    /// names none is in `rest`. Where it ends.
    fn variable(&mut self, at: usize, rest: Class) -> usize {
        let (b, len) = (self.b, self.b.len());
        let end = if byte(b, at + 1) == b'{' {
            let mut k = at + 2;
            while k < len && b[k] != b'}' && !is_break(b[k]) {
                k += 1;
            }
            if k < len && b[k] == b'}' { k + 1 } else { k }
        } else if is_word_start(byte(b, at + 1)) {
            word_end(b, at + 1)
        } else {
            self.out.push(at + 1, rest);
            return at + 1;
        };
        self.out.push(end, Class::Variable);
        end
    }

    /// A heredoc's opener at `at`, `<<EOF`, `<<-EOF`, or `<<"EOF"`, its body waiting for the
    /// instruction's end: where the opener ends. `None` where `<<` opens none, or as many wait as
    /// may.
    fn heredoc(&mut self, at: usize) -> Option<usize> {
        if self.heres.len() >= MAX_NESTING {
            return None;
        }
        let (here, end) = opener(self.b, at)?;
        self.heres.push(here);
        self.out.push(at + 2 + usize::from(here.tabs), Class::Operator);
        self.out.push(end, Class::String);
        Some(end)
    }

    /// A command that is a heredoc alone, `RUN <<EOF`, at `at`: its opener, its body read as the
    /// script it is, and its terminator. Where it ends, before the terminator's break.
    fn script(&mut self, at: usize) -> Option<usize> {
        let (b, len) = (self.b, self.b.len());
        let (here, end) = opener(b, at)?;
        let rest = blanks_end(b, end, len);
        if rest < len && !is_break(b[rest]) {
            return None;
        }
        self.out.push(at + 2 + usize::from(here.tabs), Class::Operator);
        self.out.push(end, Class::String);
        let body = line_next(b, end, len);
        self.out.space(body);
        let term = terminator(b, body, here);
        match self.reader {
            Reader::Shell => shell::script(self.text, self.out, body, term),
            Reader::PowerShell => powershell::script(self.text, self.out, body, term),
        }
        let last = line_end(b, term, len);
        self.out.push(last, Class::String);
        Some(last)
    }

    /// The exec form at `at`, a JSON array of strings (`["node", "server.js"]`) with nothing after
    /// it on its line, over the lines the escape joins: where it ends, its tokens read; `None`, and
    /// nothing read, for any other words.
    fn exec(&mut self, at: usize) -> Option<usize> {
        let end = self.array(at)?;
        let b = self.b;
        let mut i = at;
        while i < end {
            i = self.gap(i);
            if i >= end {
                break;
            }
            let to = match b[i] {
                b'"' => json_end(b, i).unwrap_or(end),
                _ => i + 1,
            };
            self.out.push(to, if b[i] == b'"' { Class::String } else { Class::Punctuation });
            i = to;
        }
        Some(end)
    }

    /// Where the JSON array of strings at `at` ends, past its `]`, if one is there with nothing but
    /// spaces after it on its line, the escape joining lines. It reads ahead and writes nothing.
    fn array(&self, at: usize) -> Option<usize> {
        let (b, len) = (self.b, self.b.len());
        if byte(b, at) != b'[' {
            return None;
        }
        // Past spaces and the lines the escape joins, as `gap` goes, writing nothing.
        let skip = |mut i: usize| {
            loop {
                i = blanks_end(b, i, len);
                let Some(mut line) = self.joins(i) else { return i };
                // The comments and blank lines after it, as `dropped` passes over them.
                loop {
                    let first = blanks_end(b, line, len);
                    if first >= len || !(b[first] == b'#' || is_break(b[first])) {
                        break;
                    }
                    line = line_next(b, first, len);
                }
                i = line;
            }
        };
        let mut i = skip(at + 1);
        if byte(b, i) != b']' {
            loop {
                if byte(b, i) != b'"' {
                    return None;
                }
                i = skip(json_end(b, i)?);
                match byte(b, i) {
                    b',' => i = skip(i + 1),
                    b']' => break,
                    _ => return None,
                }
            }
        }
        let close = i + 1;
        let rest = blanks_end(b, close, len);
        (rest >= len || is_break(b[rest])).then_some(close)
    }

    /// The shell a `SHELL` instruction's array from `at` to `end` names: PowerShell where its
    /// program is `powershell` or `pwsh`, else Shell.
    fn shell(&self, at: usize, end: usize) -> Reader {
        let b = self.b;
        let Some(open) = (at..end).find(|&k| b[k] == b'"') else { return self.reader };
        let Some(close) = json_end(b, open) else { return self.reader };
        let program = &b[open + 1..close - 1];
        let name = program.rsplit(|&c| c == b'/' || c == b'\\').next().unwrap_or(program);
        let name = name.strip_suffix(b".exe").unwrap_or(name);
        if name.eq_ignore_ascii_case(b"powershell") || name.eq_ignore_ascii_case(b"pwsh") {
            Reader::PowerShell
        } else {
            Reader::Shell
        }
    }
}

/// Whether `word` is a number: digits, with a point between them at most once (`8080`, `1.0`).
fn number(word: &[u8]) -> bool {
    word.first().is_some_and(u8::is_ascii_digit)
        && word.last().is_some_and(u8::is_ascii_digit)
        && word.iter().all(|c| c.is_ascii_digit() || *c == b'.')
        && word.iter().filter(|&&c| c == b'.').count() <= 1
}

/// Where the word at `at` ends.
fn word_end(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while i < b.len() && is_word(b[i]) {
        i += 1;
    }
    i
}

/// Where the JSON string at `at` ends, past its close, a backslash escaping the byte after it;
/// `None` where its line ends first.
fn json_end(b: &[u8], at: usize) -> Option<usize> {
    let mut i = at + 1;
    while i < b.len() {
        match b[i] {
            b'\\' if !is_break(byte(b, i + 1)) => i += 2,
            b'"' => return Some(i + 1),
            c if is_break(c) => return None,
            _ => i += 1,
        }
    }
    None
}

/// A heredoc's opener at `at`: `<<`, maybe `-`, and a delimiter maybe in quotes. The heredoc, and
/// where its opener ends.
fn opener(b: &[u8], at: usize) -> Option<(Here, usize)> {
    if byte(b, at) != b'<' || byte(b, at + 1) != b'<' {
        return None;
    }
    let tabs = byte(b, at + 2) == b'-';
    let mut k = at + 2 + usize::from(tabs);
    let quote = byte(b, k);
    let quoted = matches!(quote, b'"' | b'\'');
    k += usize::from(quoted);
    let from = k;
    while k < b.len() && is_word(b[k]) {
        k += 1;
    }
    if k == from || (quoted && byte(b, k) != quote) {
        return None;
    }
    Some((Here { from, len: k - from, tabs }, k + usize::from(quoted)))
}

/// Where the terminator's line of the heredoc whose body starts at `from` begins: the first line
/// that is its delimiter alone, after the tabs `<<-` strips; or the text's end.
fn terminator(b: &[u8], from: usize, here: Here) -> usize {
    let len = b.len();
    let delimiter = &b[here.from..here.from + here.len];
    let mut line = from;
    while line < len {
        let end = line_end(b, line, len);
        let mut k = line;
        while here.tabs && k < end && b[k] == b'\t' {
            k += 1;
        }
        if &b[k..end] == delimiter {
            return line;
        }
        line = line_next(b, line, len);
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The instructions are sorted and in lowercase, so binary search finds each in any case.
    #[test]
    fn the_instructions_are_sorted() {
        for table in [INSTRUCTIONS, DIRECTIVES] {
            assert!(table.windows(2).all(|w| w[0] < w[1]), "not sorted: {table:?}");
            assert!(table.iter().all(|w| w.bytes().all(|c| c.is_ascii_lowercase())));
        }
    }
}
