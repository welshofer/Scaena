//! A Makefile, as GNU make reads one (PLAN 2.135).
//!
//! - **Rules**: a rule's targets, before its `:` or `::`, are functions, and a special target
//!   (`.PHONY`, `.DEFAULT`) a keyword; its prerequisites are plain, and an order-only `|` an
//!   operator.
//! - **Recipes**: a line that starts with a tab after a rule, and what follows a rule's `;`, read
//!   as Shell after their `@`, `-`, and `+`, which are operators. Make expands what they write
//!   first, so `$(CC)` and `$@` are variables there, and `$$` is the shell's `$`.
//! - **Variables**: a name before its assignment (`=`, `:=`, `::=`, `?=`, `+=`, `!=`) is a
//!   variable, the sign an operator, and the value plain; a rule's own too, before any `;`, and
//!   the `export`, `override`, `private`, or `unexport` before it a keyword (`test: private
//!   CFLAGS += -g`); `define NAME` holds a value over the lines to its `endef`.
//! - **Expansions**: `$(NAME)`, `${NAME}`, a substitution (`$(SRC:.c=.o)`), and an automatic
//!   variable (`$@`, `$<`, `$^`) are variables; a function called (`$(patsubst %.c,%.o,$(SRC))`,
//!   `$(shell date)`) has its name as a function, its brackets and commas as punctuation, and its
//!   arguments read as values.
//! - **Directives** (`include`, `ifeq`, `ifdef`, `else`, `endif`, `export`, `override`) are
//!   keywords, and `#` begins a comment, but for an escaped `\#`. A line ending with `\` goes on
//!   to the next.
//!
//! At most [`MAX_NESTING`] expansions stand open inside one another; what would open one more is
//! read as what it stands in.

use super::scan::{Spans, blanks_end, byte, is_break, is_space, line_end, line_next};
use super::tables::{Table, words};
use super::{Class, MAX_NESTING, shell};

/// What a line's words are, read as values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// A variable's value: plain, to a comment.
    Value,
    /// A rule's prerequisites: plain, `|` an operator and `:` punctuation, to a `;` or a comment.
    Prerequisites,
    /// A directive's arguments: `(`, `)`, and `,` punctuation, to a comment.
    Arguments,
    /// A `define`'s body: plain, where `#` begins nothing.
    Body,
}

/// An expansion open: what closes it, whether it calls a function, and the brackets like its own
/// open inside it.
#[derive(Debug, Clone, Copy)]
struct Open {
    close: u8,
    call: bool,
    depth: u32,
}

/// Writes `text`'s tokens as a Makefile.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let mut make = Make { text, b: text.as_bytes(), out, rule: false };
    make.run();
}

struct Make<'t, 'o> {
    text: &'t str,
    b: &'t [u8],
    out: &'o mut Spans<'t>,
    /// Whether a rule came last, so a line that starts with a tab is its recipe.
    rule: bool,
}

impl Make<'_, '_> {
    fn run(&mut self) {
        let (b, len) = (self.b, self.b.len());
        let mut pos = 0;
        while pos < len {
            let end = line_end(b, pos, len);
            let first = blanks_end(b, pos, end);
            self.out.space(first);
            pos = if first == end {
                line_next(b, pos, len)
            } else if b[pos] == b'\t' && self.rule {
                self.recipe(first)
            } else if b[first] == b'#' {
                self.out.push(end, Class::Comment);
                line_next(b, pos, len)
            } else {
                let end = self.line(first);
                line_next(b, end, len)
            };
            self.out.space(pos);
        }
    }

    /// A recipe's line from `at`, past its tab: its prefixes, and its command as Shell. Where it
    /// ends, past its last line.
    fn recipe(&mut self, at: usize) -> usize {
        let b = self.b;
        let mut i = at;
        while matches!(byte(b, i), b'@' | b'-' | b'+') {
            self.out.push(i + 1, Class::Operator);
            i = blanks_end(b, i + 1, b.len());
            self.out.space(i);
        }
        shell::recipe(self.text, self.out, i)
    }

    /// A line of Make's own from `at`: its directives, then an assignment, a rule, or values.
    /// Where it ends, before its last line's break. A conditional leaves the rule before it open to
    /// its recipe; any other directive closes it.
    fn line(&mut self, at: usize) -> usize {
        let (b, len) = (self.b, self.b.len());
        let stop = line_end(b, at, len);
        // The line's first assignment or colon: a directive's word holds no `=`, `:`, `$`, or `#`,
        // so the one after the directives that begin the line is the line's first, found once.
        let found = operator(b, at, stop, false);
        let mut at = at;
        loop {
            let mut w = at;
            while w < stop && (b[w].is_ascii_alphabetic() || (w == at && b[w] == b'-')) {
                w += 1;
            }
            let word = &b[at..w];
            if w == at || !(w == len || is_space(b[w])) || !DIRECTIVES.holds(word) {
                break;
            }
            if !matches!(word, b"else" | b"endif" | b"ifdef" | b"ifeq" | b"ifndef" | b"ifneq") {
                self.rule = false;
            }
            self.out.push(w, Class::Keyword);
            let next = blanks_end(b, w, stop);
            self.out.space(next);
            match word {
                b"define" => {
                    let end = self.name(next, stop);
                    let rest = self.values(end, Mode::Value, Class::Operator).max(stop);
                    return self.body(rest);
                }
                b"ifdef" | b"ifndef" | b"undefine" => {
                    let end = self.name(next, stop);
                    return self.values(end, Mode::Arguments, Class::Plain);
                }
                // The variables `export` and `unexport` name, where they assign none.
                b"export" | b"unexport" if next < stop && found.is_none_or(|(.., set)| !set) => {
                    return self.values(next, Mode::Value, Class::Variable);
                }
                // What may follow on the line: `else ifeq …`, `export NAME = …`.
                b"else" | b"export" | b"override" | b"private" | b"unexport" if next < stop => at = next,
                _ => return self.values(next, Mode::Arguments, Class::Plain),
            }
        }
        match found {
            Some((sign, end, true)) => self.assignment(at, sign, end),
            Some((colon, end, false)) => self.rule(at, colon, end),
            None => self.values(at, Mode::Value, Class::Plain),
        }
    }

    /// The variable's name at `at`, before the line's end at `stop`, a variable: where it ends.
    fn name(&mut self, at: usize, stop: usize) -> usize {
        let b = self.b;
        let mut e = at;
        while e < stop && !is_space(b[e]) && !matches!(b[e], b'=' | b':' | b'?' | b'+' | b'!' | b'#') {
            e += 1;
        }
        self.out.push(e, Class::Variable);
        e
    }

    /// A `define`'s body, from the end of its first line at `at`, to the line its `endef` begins
    /// and that `endef`: where it ends.
    fn body(&mut self, at: usize) -> usize {
        let (b, len) = (self.b, self.b.len());
        let mut line = line_next(b, at, len);
        self.out.space(line);
        let mut depth = 0u32;
        while line < len {
            let end = line_end(b, line, len);
            let first = blanks_end(b, line, end);
            let word = &b[first..end.min(first + 6)];
            let alone = |n: usize| first + n == end || is_space(b[first + n]);
            if word.starts_with(b"endef") && alone(5) {
                if depth == 0 {
                    self.out.space(first);
                    self.out.push(first + 5, Class::Keyword);
                    self.rule = false;
                    return self.values(first + 5, Mode::Value, Class::Plain);
                }
                depth -= 1;
            } else if word.starts_with(b"define") && alone(6) {
                depth = depth.saturating_add(1);
            }
            let stop = self.values(line, Mode::Body, Class::Plain);
            line = line_next(b, stop, len);
            self.out.space(line);
        }
        len
    }

    /// An assignment: its name from `at` to its sign at `sign`, a variable, the sign to `end` an
    /// operator, and its value. Where it ends.
    fn assignment(&mut self, at: usize, sign: usize, end: usize) -> usize {
        let b = self.b;
        let mut name = sign;
        while name > at && matches!(b[name - 1], b' ' | b'\t') {
            name -= 1;
        }
        self.values_to(at, name, Class::Variable);
        self.out.space(sign);
        self.out.push(end, Class::Operator);
        self.rule = false;
        self.values(end, Mode::Value, Class::Plain)
    }

    /// A rule: its targets from `at` to its colon at `colon`, the colon to `end`, and its
    /// prerequisites, or a variable it sets for its targets, then a recipe after a `;`. Where it
    /// ends.
    fn rule(&mut self, at: usize, colon: usize, end: usize) -> usize {
        let (b, len) = (self.b, self.b.len());
        let mut i = at;
        while i < colon {
            if matches!(b[i], b' ' | b'\t') {
                i = blanks_end(b, i, colon);
                self.out.space(i);
                continue;
            }
            let mut e = i;
            while e < colon && !matches!(b[e], b' ' | b'\t') {
                e += 1;
            }
            let special = SPECIAL.holds(&b[i..e]);
            self.values_to(i, e, if special { Class::Keyword } else { Class::Function });
            i = e;
        }
        self.out.push(end, Class::Punctuation);
        self.rule = true;
        let stop = line_end(b, end, len);
        // A variable the rule sets for its targets, before any `;` that begins its recipe, maybe
        // after the words that export it or not, override it, or keep it from the prerequisites:
        // `test: CFLAGS += -g`, `test: private BAR := 2`.
        if let Some((sign, after, true)) = operator(b, end, stop, true) {
            let mut name = blanks_end(b, end, sign);
            self.out.space(name);
            loop {
                let mut w = name;
                while w < sign && b[w].is_ascii_alphabetic() {
                    w += 1;
                }
                let next = blanks_end(b, w, sign);
                if w == name || next == w || next >= sign || !MODIFIERS.holds(&b[name..w]) {
                    break;
                }
                self.out.push(w, Class::Keyword);
                self.out.space(next);
                name = next;
            }
            return self.assignment(name, sign, after);
        }
        let k = self.values(end, Mode::Prerequisites, Class::Plain);
        if byte(b, k) == b';' {
            self.out.push(k + 1, Class::Punctuation);
            let command = blanks_end(b, k + 1, len);
            self.out.space(command);
            // Its prefixes, as a line's after its tab: `print-%: ; @echo $*=$($*)`.
            let next = self.recipe(command);
            // Its last line's break is read: where the rule ends, before it.
            let mut last = next;
            while last > command && is_break(b[last - 1]) {
                last -= 1;
            }
            return last.max(command);
        }
        k
    }

    /// The text from `at` to `to`, in `class` but for the expansions in it, each a variable whole.
    fn values_to(&mut self, at: usize, to: usize, class: Class) {
        let (b, mut i) = (self.b, at);
        while i < to {
            if b[i] == b'$' {
                let end = match byte(b, i + 1) {
                    b'$' => {
                        self.out.push((i + 2).min(to), class);
                        i = (i + 2).min(to);
                        continue;
                    }
                    open @ (b'(' | b'{') => {
                        let close = if open == b'(' { b')' } else { b'}' };
                        let mut depth = 0u32;
                        let mut k = i + 1;
                        loop {
                            if k >= to {
                                break to;
                            }
                            if b[k] == open {
                                depth += 1;
                            } else if b[k] == close {
                                depth -= 1;
                                if depth == 0 {
                                    break k + 1;
                                }
                            }
                            k += 1;
                        }
                    }
                    _ => (i + 2).min(to),
                };
                self.out.push(end, Class::Variable);
                i = end;
                continue;
            }
            let e = (i + 1..to).find(|&k| b[k] == b'$').unwrap_or(to);
            self.out.push(e, class);
            i = e;
        }
    }

    /// The values from `at` to the end of the line, or of the lines a `\` joins, read as `mode`
    /// says, plain text in `class`: where they end, before the last line's break, or at the `;`
    /// that ends a rule's prerequisites.
    fn values(&mut self, at: usize, mode: Mode, class: Class) -> usize {
        let (b, len) = (self.b, self.b.len());
        let mut opens: Vec<Open> = Vec::new();
        let mut i = at;
        while i < len {
            let c = b[i];
            if is_break(c) {
                return i;
            }
            // A line that goes on.
            if c == b'\\' && is_break(byte(b, i + 1)) {
                self.out.push(i + 1, Class::Punctuation);
                i = line_next(b, i + 1, len);
                self.out.space(i);
                continue;
            }
            if matches!(c, b' ' | b'\t') {
                let to = blanks_end(b, i, len);
                self.out.space(to);
                i = to;
                continue;
            }
            if c == b'$' {
                // What the `$` stands in: a variable's name, a call's arguments, or the values.
                let within = match opens.last() {
                    Some(top) if !top.call => Class::Variable,
                    _ => class,
                };
                i = self.dollar(i, len, &mut opens, within);
                continue;
            }
            if let Some(top) = opens.last_mut() {
                let open = if top.close == b')' { b'(' } else { b'{' };
                if c == top.close && top.depth == 0 {
                    let close = if top.call { Class::Punctuation } else { Class::Variable };
                    opens.pop();
                    self.out.push(i + 1, close);
                    i += 1;
                    continue;
                }
                if c == top.close {
                    top.depth -= 1;
                } else if c == open {
                    top.depth += 1;
                }
                let inside = if top.call { class } else { Class::Variable };
                let (to, class) = match c {
                    b',' if top.call => (i + 1, Class::Punctuation),
                    _ => (plain_end(b, i, true), inside),
                };
                self.out.push(to, class);
                i = to;
                continue;
            }
            let (to, class) = match (c, mode) {
                // An escaped `#`, which begins no comment: `HASH := \#`.
                (b'\\', _) if byte(b, i + 1) == b'#' => (i + 2, class),
                (b'#', Mode::Value | Mode::Prerequisites | Mode::Arguments) => (line_end(b, i, len), Class::Comment),
                (b';', Mode::Prerequisites) => return i,
                (b'|', Mode::Prerequisites) => (i + 1, Class::Operator),
                (b':', Mode::Prerequisites) => (i + 1, Class::Punctuation),
                (b'(' | b')' | b',', Mode::Arguments) => (i + 1, Class::Punctuation),
                _ => (plain_end(b, i, false), class),
            };
            self.out.push(to.max(i + 1), class);
            i = to.max(i + 1);
        }
        len
    }

    /// An expansion at `i`, which a `$` begins, before `stop`: `$$`, a variable of one character,
    /// or `$(` or `${` and what opens, pushed on `opens`. Where it ends.
    fn dollar(&mut self, i: usize, stop: usize, opens: &mut Vec<Open>, class: Class) -> usize {
        let b = self.b;
        let next = if i + 1 < stop { b[i + 1] } else { 0 };
        if next == b'$' {
            self.out.push(i + 2, class);
            return i + 2;
        }
        if matches!(next, b'(' | b'{') && opens.len() < MAX_NESTING {
            let close = if next == b'(' { b')' } else { b'}' };
            let mut e = i + 2;
            while e < stop && !is_space(b[e]) && !matches!(b[e], b')' | b'}' | b':' | b',' | b'$' | b'(' | b'{') {
                e += 1;
            }
            let call = e < stop && matches!(b[e], b' ' | b'\t') && FUNCTIONS.holds(&b[i + 2..e]);
            if call {
                self.out.push(i + 2, Class::Punctuation);
                self.out.push(e, Class::Function);
            } else {
                self.out.push(e, Class::Variable);
            }
            opens.push(Open { close, call, depth: 0 });
            return e;
        }
        if next != 0 && !is_space(next) && !matches!(next, b'(' | b'{') {
            self.out.push(i + 2, Class::Variable);
            return i + 2;
        }
        self.out.push(i + 1, class);
        i + 1
    }
}

/// Where the plain text at `i` ends: at whitespace, a break, a `$`, a `\`, and what ends a rule's
/// prerequisites or a directive's arguments; inside an expansion, at its brackets and commas too.
fn plain_end(b: &[u8], i: usize, inside: bool) -> usize {
    let mut e = i + 1;
    while e < b.len()
        && !is_space(b[e])
        && !matches!(b[e], b'$' | b'\\' | b'#' | b';' | b'|' | b':' | b'(' | b')' | b',')
        && !(inside && matches!(b[e], b'{' | b'}'))
    {
        e += 1;
    }
    e
}

/// The first assignment's sign or rule's colon from `at` to `stop`, outside expansions and
/// before a comment: where it starts and ends, and whether it assigns. After a rule's colon
/// (`recipe`), the search ends at a `;`, which begins the recipe: what follows it is the shell's.
fn operator(b: &[u8], at: usize, stop: usize, recipe: bool) -> Option<(usize, usize, bool)> {
    let mut depth = 0u32;
    let mut i = at;
    while i < stop {
        let c = b[i];
        if c == b'$' && matches!(byte(b, i + 1), b'(' | b'{') {
            depth += 1;
            i += 2;
            continue;
        }
        // A `$` and what it expands, and an escaped `#`, which begins no comment.
        if c == b'$' || (c == b'\\' && byte(b, i + 1) == b'#') {
            i += 2;
            continue;
        }
        if depth > 0 {
            match c {
                b'(' | b'{' => depth += 1,
                b')' | b'}' => depth -= 1,
                _ => {}
            }
            i += 1;
            continue;
        }
        match c {
            b'#' => return None,
            b';' if recipe => return None,
            b'=' => return Some((i, i + 1, true)),
            b'?' | b'+' | b'!' if byte(b, i + 1) == b'=' => return Some((i, i + 2, true)),
            b':' => {
                let mut k = i;
                while byte(b, k) == b':' && k - i < 3 {
                    k += 1;
                }
                if byte(b, k) == b'=' {
                    return Some((i, k + 1, true));
                }
                return Some((i, (i + 2).min(k), false));
            }
            _ => i += 1,
        }
    }
    None
}

// The tables, each sorted, so a word is found by binary search (`every_table_is_sorted`).

/// The directives a line begins with.
const DIRECTIVES: Table = words!(
    "makefile/directives",
    [
        "-include", "define", "else", "endef", "endif", "export", "ifdef", "ifeq", "ifndef", "ifneq", "include",
        "override", "private", "sinclude", "undefine", "unexport", "vpath"
    ]
);

/// The functions an expansion calls.
const FUNCTIONS: Table = words!(
    "makefile/functions",
    [
        "abspath",
        "addprefix",
        "addsuffix",
        "and",
        "basename",
        "call",
        "dir",
        "error",
        "eval",
        "file",
        "filter",
        "filter-out",
        "findstring",
        "firstword",
        "flavor",
        "foreach",
        "guile",
        "if",
        "info",
        "intcmp",
        "join",
        "lastword",
        "let",
        "notdir",
        "or",
        "origin",
        "patsubst",
        "realpath",
        "shell",
        "sort",
        "strip",
        "subst",
        "suffix",
        "value",
        "warning",
        "wildcard",
        "word",
        "wordlist",
        "words"
    ]
);

/// The words that may come before a variable a rule sets for its targets.
const MODIFIERS: Table = words!("makefile/modifiers", ["export", "override", "private", "unexport"]);

/// The special targets.
const SPECIAL: Table = words!(
    "makefile/special",
    [
        ".DEFAULT",
        ".DELETE_ON_ERROR",
        ".EXPORT_ALL_VARIABLES",
        ".IGNORE",
        ".INTERMEDIATE",
        ".LOW_RESOLUTION_TIME",
        ".NOTINTERMEDIATE",
        ".NOTPARALLEL",
        ".ONESHELL",
        ".PHONY",
        ".POSIX",
        ".PRECIOUS",
        ".SECONDARY",
        ".SECONDEXPANSION",
        ".SILENT",
        ".SUFFIXES",
        ".WAIT"
    ]
);

/// The tables above, in the file they are handed over in (`code::tables`).
pub(super) const WORDS: &[Table] = &[DIRECTIVES, FUNCTIONS, MODIFIERS, SPECIAL];

#[cfg(test)]
mod tests {
    use super::*;

    /// Each table is sorted, with no word twice, so binary search finds what it holds.
    #[test]
    fn every_table_is_sorted() {
        for table in [DIRECTIVES, FUNCTIONS, MODIFIERS, SPECIAL] {
            assert!(table.windows(2).all(|w| w[0] < w[1]), "not sorted: {table:?}");
        }
    }
}
