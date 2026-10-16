//! The C family, read by one scanner that a table drives for each language: C, C++, C#, Go,
//! Java, JavaScript with JSX, TypeScript and TSX, Kotlin, Swift, Rust, Dart, Scala, Zig,
//! Objective-C, and Protocol Buffers.
//!
//! A language's [`Spec`] says what its words are, which strings open how, and what its
//! comments, `#`, `@`, `'`, `/`, and `<` begin. A word is:
//! - a **keyword**, a **constant**, a **type**, or a **variable** the language defines (`this`),
//!   where its sorted tables hold it;
//! - after a `.`, a **property**, or a **function** where a `(` follows it, but for a word C,
//!   C++, and Objective-C keep for themselves, which no member is named: C++'s `-> int`;
//! - after a keyword that declares one (`fn`, `class`), a **function**'s or a **type**'s name;
//! - else, by its case and what follows it: a **constant** in capitals (`MAX_LEN`), a **type**
//!   that starts with one (`HashMap`), a **function** where a `(` follows it, or plain. In C#,
//!   whose methods and properties are named in capitals, a name just after a type is the one
//!   it declares, and plain: `string Name`.
//!
//! The scanner keeps a stack of what it returns to: the code it reads, a string that an
//! interpolation interrupts, a JSX element's tag or its children. Strings take no place in it:
//! at most [`MAX_NESTING`] interpolations, JSX elements, and JSX expressions stand open inside one
//! another, and what would open one more is read as what it stands in.

use super::scan::{
    Spans, block_comment_end, is_break, is_space, is_word, is_word_start, line_end, number_end, space_end, word_end,
};
use super::tables::{Table, words};
use super::{Class, MAX_NESTING};

/// Sorted tables of words, each searched by binary search.
type Words = &'static [Table];

/// How far a regular expression is looked for before `/` is taken to divide: a step at each `/`
/// where an operand may start, so the scan stays linear on a line of them.
const LOOK: usize = 256;

/// The most `#`s a raw string opens with, as Rust allows, a run of them counted once.
const HASHES: usize = 255;

/// How far type arguments are looked through for the `(` of a generic call.
const TYPE_ARGUMENTS: usize = 64;

/// How far C#'s format after an interpolation's `:` is looked through for its `}`.
const FORMAT: usize = 64;

/// How a language of the C family reads.
#[derive(Debug)]
pub(crate) struct Spec {
    /// Its keywords.
    keywords: Words,
    /// The types it names itself: `int`, `u8`.
    types: Words,
    /// Its constants: `true`, `null`.
    constants: Words,
    /// The variables it defines: `this`, `self`.
    builtins: Words,
    /// The keywords whose next word names a function: `fn`, `func`.
    functions: Table,
    /// The keywords whose next word names a type: `class`, `struct`.
    declares: Table,
    /// The keywords that stay keywords after a `.`: Rust's `.await`.
    dotted: Table,
    /// The strings it opens, an opener before any that begins it.
    strings: &'static [Opener],
    /// Its raw strings, which close as they open.
    raw: Raw,
    /// Whether a word just before a `"` makes the string an interpolated one: Scala's `s"…"`.
    interpolators: bool,
    /// Its block comments.
    block: Block,
    /// What a `#` begins.
    hash: Hash,
    /// What an `@` makes the word after it.
    at: At,
    /// Whether a word that starts with a capital names a type: `HashMap`.
    capital_types: bool,
    /// Whether a word in capitals names a constant: `MAX_LEN`.
    capital_constants: bool,
    /// Whether a word that starts with a capital is a function where it is called, as C#'s
    /// methods are named (`ToString()`), but for a type `new` makes (`new Item(…)`); and plain
    /// just after a type, which declares it, as C#'s properties are named (`string Name`).
    capital_methods: bool,
    /// Whether a word just before `{` is a function given a lambda: Kotlin's `apply { … }`.
    closures: bool,
    /// The keywords that begin a condition, which runs to its `{` and gives no word in it a
    /// lambda: Swift's `if items.isEmpty {`.
    conditions: Table,
    /// Whether `[` first on its line or where no operand comes before it, then a name in
    /// capitals, opens an attribute: C#'s `[HttpGet("{id}")]`.
    bracket_attributes: bool,
    /// Whether `/` opens a regular expression where an operand may start.
    regex: bool,
    /// Whether `<` opens a JSX element where an operand may start.
    jsx: bool,
    /// Whether `'` before a word is a lifetime where no `'` closes it as a character: Rust's `'a`.
    lifetimes: bool,
    /// Whether `'` separates digits: C++'s `1'000`.
    digit_quotes: bool,
    /// Whether `$` is a word's character.
    dollar: bool,
    /// Whether a word just before `!` is a macro, which reads as a function: Rust's `println!`.
    macros: bool,
    /// Whether `->` reaches a member, as `.` does.
    arrow: bool,
}

/// What opens a string, as written, and how the string reads once open.
#[derive(Debug, Clone, Copy)]
struct Opener {
    open: &'static str,
    str: Str,
}

/// How a string reads once open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Str {
    /// What closes it.
    close: Close,
    /// What escapes a character in it.
    escape: Escape,
    /// What interrupts it with code.
    interp: Interp,
    /// Whether it runs on past its line's end.
    lines: bool,
    /// The class it is colored in: a string's, or plain for a name in backticks.
    class: Class,
}

impl Str {
    /// The `#`s that close it after its quotes, and that a backslash needs after it to escape:
    /// Rust's `r#"…"#` and Swift's `#"…\#n…"#`.
    fn hashes(self) -> usize {
        match self.close {
            Close::Quotes { hashes, .. } => usize::from(hashes),
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Close {
    /// `count` of `quote`, then `hashes` of `#`.
    Quotes { quote: u8, count: u8, hashes: u8 },
    /// `)`, the `len` bytes at `from`, and `"`: C++'s `R"x(…)x"`.
    Delimiter { from: usize, len: u8 },
    /// The line's end: Zig's `\\` lines.
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escape {
    /// Nothing: a raw string.
    None,
    /// A backslash escapes the character after it.
    Backslash,
    /// A quote doubled is a quote: C#'s `@"say ""hi"""`.
    Doubled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Interp {
    None,
    /// `${…}`: JavaScript's templates.
    DollarBrace,
    /// `$name` and `${…}`: Kotlin, Dart, and Scala.
    Dollar,
    /// `\(…)`: Swift.
    Paren,
    /// `{…}`, with `{{` a brace: C#.
    Brace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Raw {
    None,
    /// `r"…"`, `r#"…"#`, `br"…"`, and `cr"…"`.
    Rust,
    /// `#"…"#` and `#"""…"""#`, interpolated by `\#(…)`.
    Swift,
    /// `R"x(…)x"`, after `u8`, `u`, `U`, or `L`.
    Cpp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    /// No block comments: Zig's.
    None,
    /// `/* */`, the first `*/` closing it.
    Flat,
    /// `/* */` inside one another.
    Nested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hash {
    /// An operator.
    None,
    /// A directive to the preprocessor, first on its line: `#include <stdio.h>`.
    Directive,
    /// An attribute: Rust's `#[derive(Debug)]`.
    Attribute,
    /// A directive or a macro: Swift's `#if`, `#selector`.
    Meta,
    /// A private member: JavaScript's `#count`.
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum At {
    /// An operator.
    None,
    /// An annotation, an attribute, or a decorator: `@Override`, `@objc`.
    Attribute,
    /// A builtin function: Zig's `@import`.
    Function,
    /// A keyword: Objective-C's `@interface`.
    Keyword,
    /// A name written as a keyword is: C#'s `@class`.
    Name,
}

/// What the scanner returns to.
#[derive(Debug, Clone, Copy)]
enum Frame {
    /// Code: the text's own at the bottom of the stack (`close` 0), else an interpolation's or a
    /// JSX expression's, which `close` ends where no `open` inside it is still open.
    Code { close: u8, open: u8, depth: u32 },
    /// A string: on top as it is read, below the code of an interpolation that interrupts it.
    Str(Str),
    /// A JSX tag's attributes, to its `>` or `/>`.
    Tag,
    /// A JSX element's children, to its closing tag.
    Children,
}

const TOP: Frame = Frame::Code { close: 0, open: 0, depth: 0 };

/// Writes `text`'s tokens in the language `spec` says.
pub(super) fn scan<'t>(text: &'t str, spec: &'static Spec, out: &mut Spans<'t>) {
    let mut scanner = Scanner {
        spec,
        text,
        b: text.as_bytes(),
        pos: 0,
        out,
        stack: vec![TOP],
        operand: false,
        member: false,
        pending: None,
        line_start: true,
        condition: false,
        typed: None,
    };
    scanner.run();
}

struct Scanner<'t, 'o> {
    spec: &'static Spec,
    text: &'t str,
    b: &'t [u8],
    pos: usize,
    out: &'o mut Spans<'t>,
    stack: Vec<Frame>,
    /// Whether what came last ends an operand, so `/` divides and `<` compares.
    operand: bool,
    /// Whether a `.` came last, so a word is a member.
    member: bool,
    /// What the next word names, after a keyword that declares a function or a type.
    pending: Option<Class>,
    /// Whether only whitespace has come since the line began.
    line_start: bool,
    /// Whether a condition is open, from the keyword that begins it to its `{`.
    condition: bool,
    /// Where the type read last ends, with the `?`, `[]`, and type arguments' `>` just after
    /// it: a name in capitals after it, past spaces, is the name it declares.
    typed: Option<usize>,
}

impl Scanner<'_, '_> {
    fn run(&mut self) {
        while self.pos < self.b.len() {
            let (pos, frames) = (self.pos, self.stack.len());
            match self.stack.last().copied().unwrap_or(TOP) {
                Frame::Code { close, open, depth } => self.code(close, open, depth),
                Frame::Str(s) => self.string(s),
                Frame::Tag => self.tag(),
                Frame::Children => self.children(),
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

    /// Whether one more interpolation, JSX element, or JSX expression may open.
    fn can_nest(&self) -> bool {
        self.stack.iter().filter(|f| !matches!(f, Frame::Str(_))).count() <= MAX_NESTING
    }

    /// One step of code.
    fn code(&mut self, close: u8, open: u8, depth: u32) {
        let (b, i) = (self.b[self.pos], self.pos);
        let next = self.at(i + 1);
        if is_space(b) {
            let end = space_end(self.b, i, self.b.len());
            if self.b[i..end].iter().any(|&c| is_break(c)) {
                self.line_start = true;
            }
            self.out.space(end);
            self.pos = end;
            return;
        }
        // A line comment, or a script's first line, which names what runs it.
        if (b == b'/' && next == b'/') || (i == 0 && b == b'#' && next == b'!' && self.at(2) != b'[') {
            return self.emit(line_end(self.b, i, self.b.len()), Class::Comment);
        }
        if b == b'/' && next == b'*' && self.spec.block != Block::None {
            let end = block_comment_end(self.b, i, self.b.len(), self.spec.block == Block::Nested);
            return self.emit(end, Class::Comment);
        }
        let line_start = std::mem::take(&mut self.line_start);
        let member = std::mem::take(&mut self.member);
        let pending = self.pending.take();
        // C#'s format after an interpolation's `:`, which runs to its `}`: `{price:C2}`.
        if b == b':'
            && close == b'}'
            && depth == 0
            && next != b':'
            && let Some(end) = self.format_end()
        {
            self.emit(i + 1, Class::Punctuation);
            return self.emit(end, Class::String);
        }
        // An interpolation's or a JSX expression's own brackets.
        if close != 0 && (b == close || b == open) {
            self.emit(i + 1, Class::Punctuation);
            if b == close && depth == 0 {
                self.pop();
                self.operand = true;
            } else {
                if let Some(Frame::Code { depth, .. }) = self.stack.last_mut() {
                    *depth = if b == open { depth.saturating_add(1) } else { *depth - 1 };
                }
                self.operand = b == b')';
            }
            return;
        }
        if let Some((len, s)) = self.raw().or_else(|| self.opener()) {
            return self.open(len, s);
        }
        if b == b'[' && self.spec.bracket_attributes && (!self.operand || line_start) && next.is_ascii_uppercase() {
            return self.bracket_attribute();
        }
        if (b == b'#' && self.hash(line_start)) || (b == b'@' && self.at_word()) {
            return;
        }
        if b == b'\'' && self.spec.lifetimes {
            return self.tick();
        }
        if b == b'/' && self.spec.regex && !self.operand && self.regex() {
            return;
        }
        if b == b'<'
            && self.spec.jsx
            && !self.operand
            && (is_word_start(next) || next == b'>')
            && self.can_nest()
            && !self.type_parameters()
        {
            return self.element();
        }
        if b == b'-' && next == b'>' && self.spec.arrow {
            self.emit(i + 2, Class::Operator);
            self.operand = false;
            self.member = true;
            return;
        }
        // A tuple's field by its index, after a `.`: `pair.0.1` is two of them, not a number.
        if b.is_ascii_digit() && member {
            let mut end = i + 1;
            while self.at(end).is_ascii_digit() {
                end += 1;
            }
            self.emit(end, Class::Number);
            self.operand = true;
            return;
        }
        if b.is_ascii_digit() || (b == b'.' && next.is_ascii_digit() && !self.operand) {
            self.emit(number_end(self.b, i, self.b.len(), self.spec.digit_quotes), Class::Number);
            self.operand = true;
            return;
        }
        if is_word_start(b) || (b == b'$' && self.spec.dollar) {
            return self.word(member, pending);
        }
        self.symbol(b, next);
    }

    /// A word: a name, a keyword, a type, or what else its tables and its case make it.
    fn word(&mut self, member: bool, pending: Option<Class>) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let mut j = i + 1;
        while j < len && (is_word(b[j]) || (b[j] == b'$' && self.spec.dollar)) {
            j += 1;
        }
        // Rust's raw names: `r#type`.
        if self.spec.raw == Raw::Rust && b[i..j] == *b"r" && self.at(j) == b'#' && is_word_start(self.at(j + 1)) {
            self.emit(word_end(b, j + 1, len), Class::Plain);
            self.operand = true;
            return;
        }
        // Scala's interpolated strings: `s"…"`, `f"""…"""`.
        if self.spec.interpolators && self.at(j) == b'"' {
            let triple = b[j..].starts_with(b"\"\"\"");
            let (count, escape, lines) = if triple { (3, Escape::None, true) } else { (1, Escape::Backslash, false) };
            let close = Close::Quotes { quote: b'"', count, hashes: 0 };
            let s = Str { close, escape, interp: Interp::Dollar, lines, class: Class::String };
            return self.open(j + usize::from(count) - i, s);
        }
        let word = self.text.get(i..j).unwrap_or_default();
        // Rust's macros: `println!`, `vec!`.
        if self.spec.macros && self.at(j) == b'!' && self.at(j + 1) != b'=' && !found(self.spec.keywords, word) {
            self.emit(j + 1, Class::Function);
            self.operand = true;
            return;
        }
        let class = self.classify(word, j, member, pending);
        if class == Class::Type {
            self.typed = Some(j);
        }
        self.emit(j, class);
    }

    /// What `word`, which ends at `end`, names.
    fn classify(&mut self, word: &str, end: usize, member: bool, pending: Option<Class>) -> Class {
        let spec = self.spec;
        let next = self.blanks(end);
        let after = self.at(next);
        // Called: `parse(`, `useState<string>(`, Rust's `parse::<u8>(`, and Kotlin's `apply {`,
        // but not a word a condition ends with: Swift's `if items.isEmpty {`.
        let call = after == b'('
            || (matches!(after, b'<' | b':') && self.generic_call(next))
            || (spec.closures && after == b'{' && !self.condition);
        let name = word.trim_start_matches('_').as_bytes();
        let capital = name.first().is_some_and(u8::is_ascii_uppercase);
        let capitals = capital && name.len() > 1 && !name.iter().any(u8::is_ascii_lowercase);
        self.operand = true;
        // Where `->` reaches a member too, a word the language keeps for itself, or a namespace
        // a `::` follows, is no member: C++'s `-> int`, `-> std::string`, and `.template`.
        let member = member
            && !(spec.arrow
                && (found(spec.keywords, word)
                    || found(spec.types, word)
                    || (after == b':' && self.at(next + 1) == b':')));
        if member {
            return if found(spec.builtins, word) {
                Class::Variable
            } else if spec.dotted.binary_search(&word).is_ok() {
                self.operand = false;
                Class::Keyword
            } else if spec.capital_constants && capitals && !call {
                Class::Constant
            } else if spec.capital_types && capital && !spec.capital_methods {
                // A member that starts with a capital is a type a module or a type holds:
                // `java.util.List`, `std.ArrayList(T)`, Kotlin's `Result.Ok(…)`.
                Class::Type
            } else if call {
                Class::Function
            } else {
                Class::Property
            };
        }
        if found(spec.keywords, word) {
            self.operand = false;
            if spec.conditions.binary_search(&word).is_ok() {
                self.condition = true;
            }
            self.pending = if spec.functions.binary_search(&word).is_ok() {
                Some(Class::Function)
            } else if spec.declares.binary_search(&word).is_ok() {
                Some(Class::Type)
            } else {
                pending
            };
            return Class::Keyword;
        }
        if found(spec.constants, word) {
            return Class::Constant;
        }
        if found(spec.types, word) {
            return Class::Type;
        }
        if found(spec.builtins, word) {
            return Class::Variable;
        }
        if let Some(class) = pending {
            // A name a `.` follows is what the declared one is reached through: Kotlin's
            // `fun String.shout()`.
            if after != b'.' {
                return class;
            }
            self.pending = Some(class);
        }
        if spec.capital_constants && capitals {
            // In capitals: a constant, a macro called (`MAX(a, b)`), or a namespace reached
            // through (`JSX.Element`).
            return if call {
                Class::Function
            } else if after == b'.' && spec.capital_types {
                Class::Type
            } else {
                Class::Constant
            };
        }
        if spec.capital_types && capital {
            return if spec.capital_methods && call {
                Class::Function
            } else if spec.capital_methods && self.declared(end - word.len()) {
                Class::Plain
            } else {
                Class::Type
            };
        }
        if call { Class::Function } else { Class::Plain }
    }

    /// Whether the word at `start` stands just after a type, past spaces, as the name the type
    /// declares: `string Name`, `int? Count`, `List<Item> Items`, `int[] Scores`.
    fn declared(&self, start: usize) -> bool {
        let mut k = start;
        while k > 0 && matches!(self.b[k - 1], b' ' | b'\t') {
            k -= 1;
        }
        k < start && self.typed == Some(k)
    }

    /// The first byte at or after `i` that is no space or tab.
    fn blanks(&self, i: usize) -> usize {
        let mut k = i;
        while matches!(self.at(k), b' ' | b'\t') {
            k += 1;
        }
        k
    }

    /// Whether type arguments at `i` (`<…>`, or Rust's `::<…>`) close before a `(`, so the word
    /// before them is called. It looks no further than [`TYPE_ARGUMENTS`] bytes, and takes only
    /// what type arguments are written with, so `a < b && c > (d)` is no call: names, lifetimes
    /// and literal types (`'a`, `'idle'`), and a union's single `|` (`useState<User | null>(`).
    /// After `::`, where nothing compares, it takes references, pointers, and tuples too:
    /// Rust's `collect::<Vec<&str>>()`.
    fn generic_call(&self, i: usize) -> bool {
        let mut k = i;
        let turbofish = self.at(k) == b':' && self.at(k + 1) == b':';
        if turbofish {
            k += 2;
        }
        if self.at(k) != b'<' {
            return false;
        }
        let limit = self.b.len().min(k + TYPE_ARGUMENTS);
        let mut depth = 0u32;
        while k < limit {
            match self.b[k] {
                b'<' => depth += 1,
                b'>' => {
                    depth -= 1;
                    if depth == 0 {
                        return self.at(self.blanks(k + 1)) == b'(';
                    }
                }
                b'|' if self.b[k - 1] != b'|' && self.at(k + 1) != b'|' => {}
                b'&' | b'*' | b'(' | b')' | b';' if turbofish => {}
                c if is_word(c) || matches!(c, b' ' | b',' | b'.' | b':' | b'?' | b'[' | b']' | b'\'') => {}
                _ => return false,
            }
            k += 1;
        }
        false
    }

    /// Whether the `<` here opens type parameters, not a JSX element: TSX's generic arrow,
    /// `<T,>(item: T) => …` or `<T extends Item>(…`, which no element writes.
    fn type_parameters(&self) -> bool {
        let j = self.blanks(word_end(self.b, self.pos + 1, self.b.len()));
        self.at(j) == b','
            || (self.b[j..].starts_with(b"extends") && !is_word(self.at(j + 7)) && self.at(self.blanks(j + 7)) != b'=')
    }

    /// C#'s attribute in brackets: the `[` and the attribute's name, by its namespace where it
    /// gives one (`[HttpGet`, `[System.Serializable`).
    fn bracket_attribute(&mut self) {
        let (b, len) = (self.b, self.b.len());
        self.emit(self.pos + 1, Class::Punctuation);
        let mut j = word_end(b, self.pos, len);
        while self.at(j) == b'.' && is_word_start(self.at(j + 1)) {
            j = word_end(b, j + 1, len);
        }
        self.emit(j, Class::Attribute);
        self.operand = true;
    }

    /// Where C#'s format after an interpolation's `:` ends, at the `}` that closes the
    /// interpolation, no more than [`FORMAT`] bytes on: `{price:C2}`, `{when:HH:mm}`. None where
    /// the `:` stands in an interpolation of another language's, or before a quote, a bracket, or
    /// the line's end, as a conditional's does inside its parentheses (`{(a ? "x" : "y")}`).
    fn format_end(&self) -> Option<usize> {
        let below = self.stack.len().checked_sub(2).and_then(|k| self.stack.get(k));
        if !matches!(below, Some(Frame::Str(Str { interp: Interp::Brace, .. }))) {
            return None;
        }
        let limit = self.b.len().min(self.pos + FORMAT);
        let mut j = self.pos + 1;
        while j < limit {
            match self.b[j] {
                b'}' => return Some(j),
                b'"' | b'\'' | b'(' | b')' | b'{' | b'\n' | b'\r' => return None,
                _ => j += 1,
            }
        }
        None
    }

    /// An operator or punctuation.
    fn symbol(&mut self, b: u8, next: u8) {
        let i = self.pos;
        // `++` and `--` leave what came before as it was: after an operand, `a++ / 2` divides.
        if matches!(b, b'+' | b'-') && next == b {
            return self.emit(i + 2, Class::Operator);
        }
        // A type's `?`, `[]`, and its type arguments' `>` belong to it: `int? Count`.
        if matches!(b, b'?' | b'[' | b']' | b'>') && self.typed == Some(i) {
            self.typed = Some(i + 1);
        }
        if b == b'{' {
            self.condition = false;
        }
        let (end, class) = match b {
            b'.' if next == b'.' => {
                let mut j = i;
                while self.at(j) == b'.' {
                    j += 1;
                }
                (j, Class::Punctuation)
            }
            b':' if next == b'=' => (i + 2, Class::Operator),
            b'(' | b')' | b'[' | b']' | b'{' | b'}' | b',' | b';' | b'.' | b':' => (i + 1, Class::Punctuation),
            _ if b.is_ascii_punctuation() => (i + 1, Class::Operator),
            _ => (i + 1, Class::Plain),
        };
        self.member = b == b'.' && end == i + 1;
        self.operand = matches!(b, b')' | b']');
        self.emit(end, class);
    }

    /// The string opener here, by the language's table.
    fn opener(&self) -> Option<(usize, Str)> {
        let rest = &self.b[self.pos..];
        self.spec.strings.iter().find(|o| rest.starts_with(o.open.as_bytes())).map(|o| (o.open.len(), o.str))
    }

    /// The raw string opener here, which says how the string closes.
    fn raw(&self) -> Option<(usize, Str)> {
        let (b, i) = (self.b, self.pos);
        let string = |close, escape, interp, lines| Str { close, escape, interp, lines, class: Class::String };
        match self.spec.raw {
            Raw::None => None,
            Raw::Rust => {
                let j = i + usize::from(matches!(b[i], b'b' | b'c'));
                if self.at(j) != b'r' {
                    return None;
                }
                let hashes = b[j + 1..].iter().take(HASHES).take_while(|&&c| c == b'#').count();
                if self.at(j + 1 + hashes) != b'"' {
                    return None;
                }
                let close = Close::Quotes { quote: b'"', count: 1, hashes: u8::try_from(hashes).unwrap_or(u8::MAX) };
                Some((j + 2 + hashes - i, string(close, Escape::None, Interp::None, true)))
            }
            Raw::Swift => {
                // A run of `#`s opens a string at its first or nowhere, so it is counted once.
                if i > 0 && b[i - 1] == b'#' {
                    return None;
                }
                let hashes = b[i..].iter().take(HASHES).take_while(|&&c| c == b'#').count();
                let j = i + hashes;
                let count = if b[j..].starts_with(b"\"\"\"") { 3 } else { 1 };
                if hashes == 0 || self.at(j) != b'"' {
                    return None;
                }
                let close = Close::Quotes { quote: b'"', count, hashes: u8::try_from(hashes).unwrap_or(u8::MAX) };
                Some((hashes + usize::from(count), string(close, Escape::Backslash, Interp::Paren, count == 3)))
            }
            Raw::Cpp => {
                let prefix =
                    ["u8R\"", "uR\"", "UR\"", "LR\"", "R\""].into_iter().find(|p| b[i..].starts_with(p.as_bytes()))?;
                let from = i + prefix.len();
                let mut j = from;
                while j < b.len() && j - from < 16 && !matches!(b[j], b'(' | b')' | b'\\' | b'"') && !is_space(b[j]) {
                    j += 1;
                }
                if self.at(j) != b'(' {
                    return None;
                }
                let close = Close::Delimiter { from, len: u8::try_from(j - from).unwrap_or(u8::MAX) };
                Some((j + 1 - i, string(close, Escape::None, Interp::None, true)))
            }
        }
    }

    /// A string opened here: its opener read, the string on top.
    fn open(&mut self, len: usize, s: Str) {
        self.emit(self.pos + len, s.class);
        self.stack.push(Frame::Str(s));
        self.operand = true;
    }

    /// One step of a string: to its close, to its line's end where it runs no further, or to
    /// an interpolation, which interrupts it.
    fn string(&mut self, s: Str) {
        let b = self.b;
        let end = b.len();
        let mut i = self.pos;
        while i < end {
            let c = b[i];
            if s.escape == Escape::Doubled && Some(c) == s.quote() && self.at(i + 1) == c {
                i += 2;
                continue;
            }
            if let Some(len) = self.closes(s, i) {
                self.emit(i + len, s.class);
                self.pop();
                return;
            }
            if !s.lines && is_break(c) {
                self.emit(i, s.class);
                self.pop();
                return;
            }
            if c == b'\\' && s.escape == Escape::Backslash {
                // In a raw string by `#`s, a backslash escapes only with as many after it.
                let j = i + 1 + s.hashes();
                if b.get(i + 1..j).is_none_or(|h| h.iter().any(|&h| h != b'#')) {
                    i += 1;
                    continue;
                }
                if s.interp == Interp::Paren && self.at(j) == b'(' && self.can_nest() {
                    return self.interpolate(i, j + 1, b')', b'(', s);
                }
                i = j + 1;
                continue;
            }
            let interp = s.interp;
            if c == b'$'
                && matches!(interp, Interp::DollarBrace | Interp::Dollar)
                && self.at(i + 1) == b'{'
                && self.can_nest()
            {
                return self.interpolate(i, i + 2, b'}', b'{', s);
            }
            if c == b'$' && interp == Interp::Dollar && (self.at(i + 1).is_ascii_alphabetic() || self.at(i + 1) == b'_')
            {
                let mut to = i + 2;
                while self.at(to).is_ascii_alphanumeric() || self.at(to) == b'_' {
                    to += 1;
                }
                self.out.push(i, s.class);
                self.out.push(to, Class::Variable);
                i = to;
                continue;
            }
            if c == b'{' && interp == Interp::Brace {
                if self.at(i + 1) == b'{' {
                    i += 2;
                    continue;
                }
                if self.can_nest() {
                    return self.interpolate(i, i + 1, b'}', b'{', s);
                }
            }
            i += 1;
        }
        // Never closed: it runs to the end.
        self.emit(end, s.class);
    }

    /// The string `s` interrupted at `at` by an interpolation that opens up to `code`, and
    /// closes at `close`.
    fn interpolate(&mut self, at: usize, code: usize, close: u8, open: u8, s: Str) {
        self.out.push(at, s.class);
        self.emit(code, Class::Punctuation);
        self.stack.push(Frame::Code { close, open, depth: 0 });
        self.operand = false;
    }

    /// Where the string `s` closes at `i`: how many bytes close it, if it does.
    fn closes(&self, s: Str, i: usize) -> Option<usize> {
        let b = self.b;
        match s.close {
            Close::Quotes { quote, count, hashes } => {
                let (count, hashes) = (usize::from(count), usize::from(hashes));
                let end = i + count + hashes;
                let closes = end <= b.len()
                    && b[i..i + count].iter().all(|&c| c == quote)
                    && b[i + count..end].iter().all(|&c| c == b'#');
                closes.then_some(count + hashes)
            }
            Close::Delimiter { from, len } => {
                let len = usize::from(len);
                let end = i + len + 2;
                let closes = b[i] == b')'
                    && end <= b.len()
                    && b[i + 1..i + 1 + len] == b[from..from + len]
                    && b[end - 1] == b'"';
                closes.then_some(len + 2)
            }
            Close::Line => None,
        }
    }

    /// What a `#` begins here, read; false where it is an operator.
    fn hash(&mut self, line_start: bool) -> bool {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let next = self.at(i + 1);
        match self.spec.hash {
            Hash::Directive if line_start => {
                let mut j = i + 1;
                while matches!(self.at(j), b' ' | b'\t') {
                    j += 1;
                }
                let name = j;
                let j = word_end(b, j, len);
                self.emit(j, Class::Meta);
                // A file included by its place among the system's: `<stdio.h>`.
                if matches!(&b[name..j], b"include" | b"import" | b"include_next") {
                    let mut k = j;
                    while matches!(self.at(k), b' ' | b'\t') {
                        k += 1;
                    }
                    if self.at(k) == b'<'
                        && let Some(n) = b[k..line_end(b, k, len)].iter().position(|&c| c == b'>')
                    {
                        self.out.space(k);
                        self.emit(k + n + 1, Class::String);
                    }
                }
                true
            }
            Hash::Attribute if next == b'[' || (next == b'!' && self.at(i + 2) == b'[') => {
                // `#[`, and the attribute's path: `#[derive`, `#![allow`, `#[serde::rename`.
                let mut j = i + if next == b'[' { 2 } else { 3 };
                loop {
                    j = word_end(b, j, len);
                    if self.at(j) == b':' && self.at(j + 1) == b':' && is_word_start(self.at(j + 2)) {
                        j += 2;
                    } else {
                        break;
                    }
                }
                self.emit(j, Class::Attribute);
                true
            }
            Hash::Meta | Hash::Private if is_word_start(next) => {
                let class = if self.spec.hash == Hash::Meta { Class::Meta } else { Class::Property };
                self.emit(word_end(b, i + 1, len), class);
                self.operand = class == Class::Property;
                true
            }
            _ => false,
        }
    }

    /// What an `@` makes the word after it, read; false where it is an operator.
    fn at_word(&mut self) -> bool {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        if !is_word_start(self.at(i + 1)) {
            return false;
        }
        let class = match self.spec.at {
            At::None => return false,
            At::Attribute => Class::Attribute,
            At::Function => Class::Function,
            At::Keyword => Class::Keyword,
            At::Name => Class::Plain,
        };
        let mut j = word_end(b, i + 1, len);
        // An annotation by its package: `@java.lang.Deprecated`.
        while class == Class::Attribute && self.at(j) == b'.' && is_word_start(self.at(j + 1)) {
            j = word_end(b, j + 1, len);
        }
        if class == Class::Keyword
            && self.spec.declares.binary_search(&self.text.get(i + 1..j).unwrap_or_default()).is_ok()
        {
            self.pending = Some(Class::Type);
        }
        self.emit(j, class);
        self.operand = class != Class::Keyword;
        true
    }

    /// Rust's `'`: a character (`'x'`, `'\n'`), or else a lifetime or a label (`'a`).
    fn tick(&mut self) {
        let i = self.pos;
        let next = self.at(i + 1);
        if next == b'\\' {
            return self.open(1, SQ);
        }
        if let Some(c) = self.text.get(i + 1..).and_then(|rest| rest.chars().next()) {
            let after = i + 1 + c.len_utf8();
            if c != '\'' && !is_break(next) && self.at(after) == b'\'' {
                self.emit(after + 1, Class::String);
                self.operand = true;
                return;
            }
        }
        if is_word_start(next) {
            self.emit(word_end(self.b, i + 1, self.b.len()), Class::Type);
            self.operand = true;
            return;
        }
        self.emit(i + 1, Class::Punctuation);
    }

    /// A regular expression here, read; false where none closes on this line, so `/` divides.
    fn regex(&mut self) -> bool {
        let (b, len) = (self.b, self.b.len());
        let limit = len.min(self.pos + LOOK);
        let mut class = false;
        let mut i = self.pos + 1;
        while i < limit {
            match b[i] {
                c if is_break(c) => return false,
                b'\\' if is_break(self.at(i + 1)) => return false,
                b'\\' => i += 2,
                b'[' => {
                    class = true;
                    i += 1;
                }
                b']' => {
                    class = false;
                    i += 1;
                }
                b'/' if !class => {
                    // Its flags: `/x/gi`.
                    self.emit(word_end(b, i + 1, len), Class::String);
                    self.operand = true;
                    return true;
                }
                _ => i += 1,
            }
        }
        false
    }

    /// A JSX element's `<` and its tag's name, or a fragment's `<>`.
    fn element(&mut self) {
        self.emit(self.pos + 1, Class::Punctuation);
        if self.at(self.pos) == b'>' {
            self.emit(self.pos + 1, Class::Punctuation);
            self.stack.push(Frame::Children);
        } else {
            self.emit(self.tag_name(self.pos), Class::Tag);
            if self.at(self.pos) == b'<' {
                self.type_arguments();
            }
            self.stack.push(Frame::Tag);
        }
        self.operand = false;
    }

    /// A generic component's type arguments, just after its name: TSX's `<List<Item> …>`. Where
    /// they close within [`TYPE_ARGUMENTS`] bytes, their names are types; else the tag reads on.
    fn type_arguments(&mut self) {
        let limit = self.b.len().min(self.pos + TYPE_ARGUMENTS);
        let (mut k, mut depth) = (self.pos, 0u32);
        let close = loop {
            if k >= limit {
                return;
            }
            match self.b[k] {
                b'<' => depth += 1,
                b'>' => {
                    depth -= 1;
                    if depth == 0 {
                        break k;
                    }
                }
                c if is_word(c) || matches!(c, b' ' | b',' | b'.' | b'?' | b'[' | b']' | b'|') => {}
                _ => return,
            }
            k += 1;
        };
        while self.pos <= close {
            let c = self.b[self.pos];
            if is_word_start(c) {
                self.emit(word_end(self.b, self.pos, close), Class::Type);
            } else if c == b' ' {
                self.pos = self.blanks(self.pos);
                self.out.space(self.pos);
            } else {
                self.emit(self.pos + 1, if matches!(c, b'<' | b'>') { Class::Operator } else { Class::Punctuation });
            }
        }
    }

    /// Where the tag's or the attribute's name at `at` ends: `div`, `Menu.Item`, `aria-label`.
    fn tag_name(&self, at: usize) -> usize {
        let mut j = at;
        while j < self.b.len() && (is_word(self.b[j]) || matches!(self.b[j], b'-' | b'.' | b':' | b'$')) {
            j += 1;
        }
        j
    }

    /// One step of a JSX tag's attributes.
    fn tag(&mut self) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let (c, next) = (b[i], self.at(i + 1));
        if is_space(c) {
            let end = space_end(b, i, len);
            self.out.space(end);
            self.pos = end;
        } else if c == b'/' && next == b'>' {
            self.emit(i + 2, Class::Punctuation);
            self.pop();
            self.closed();
        } else if c == b'>' {
            self.emit(i + 1, Class::Punctuation);
            if let Some(top) = self.stack.last_mut() {
                *top = Frame::Children;
            }
        } else if c == b'{' && self.can_nest() {
            self.emit(i + 1, Class::Punctuation);
            self.stack.push(Frame::Code { close: b'}', open: b'{', depth: 0 });
            self.operand = false;
        } else if c == b'"' || c == b'\'' {
            // A value runs to its quote, over lines, with no escapes.
            let end = b[i + 1..].iter().position(|&q| q == c).map_or(len, |n| i + n + 2);
            self.emit(end, Class::String);
        } else if c == b'/' && next == b'/' {
            self.emit(line_end(b, i, len), Class::Comment);
        } else if c == b'/' && next == b'*' {
            self.emit(block_comment_end(b, i, len, false), Class::Comment);
        } else if is_word_start(c) {
            self.emit(self.tag_name(i), Class::Attribute);
        } else {
            self.emit(i + 1, if c == b'=' { Class::Operator } else { Class::Punctuation });
        }
    }

    /// One step of a JSX element's children: an expression, a child, its closing tag, or text.
    fn children(&mut self) {
        let (b, i, len) = (self.b, self.pos, self.b.len());
        let (c, next) = (b[i], self.at(i + 1));
        if c == b'{' && self.can_nest() {
            self.emit(i + 1, Class::Punctuation);
            self.stack.push(Frame::Code { close: b'}', open: b'{', depth: 0 });
            self.operand = false;
        } else if c == b'<' && next == b'/' {
            // `</`, the name, and `>`.
            self.emit(i + 2, Class::Punctuation);
            let name = space_end(b, self.pos, len);
            self.out.space(name);
            self.pos = name;
            self.emit(self.tag_name(name), Class::Tag);
            let close = space_end(b, self.pos, len);
            self.out.space(close);
            self.pos = close;
            if self.at(close) == b'>' {
                self.emit(close + 1, Class::Punctuation);
            }
            self.pop();
            self.closed();
        } else if c == b'<' && (is_word_start(next) || next == b'>') && self.can_nest() {
            self.element();
        } else {
            let mut j = i + 1;
            while j < len && !matches!(b[j], b'{' | b'<') {
                j += 1;
            }
            self.emit(j, Class::Plain);
        }
    }

    /// After a JSX element closes: back in code, it is an operand.
    fn closed(&mut self) {
        if matches!(self.stack.last(), Some(Frame::Code { .. })) {
            self.operand = true;
        }
    }
}

impl Str {
    /// The quote it closes with, where one closes it.
    fn quote(self) -> Option<u8> {
        match self.close {
            Close::Quotes { quote, .. } => Some(quote),
            _ => None,
        }
    }
}

/// Whether one of `tables` holds `word`.
fn found(tables: Words, word: &str) -> bool {
    tables.iter().any(|table| table.binary_search(&word).is_ok())
}

const fn quoted(quote: u8, count: u8, escape: Escape, interp: Interp, lines: bool) -> Str {
    Str { close: Close::Quotes { quote, count, hashes: 0 }, escape, interp, lines, class: Class::String }
}

const fn opens(open: &'static str, str: Str) -> Opener {
    Opener { open, str }
}

/// A string in double quotes, or a character in single ones, on one line, a backslash escaping.
const DQ: Str = quoted(b'"', 1, Escape::Backslash, Interp::None, false);
const SQ: Str = quoted(b'\'', 1, Escape::Backslash, Interp::None, false);

/// A name in backticks: Kotlin's, Scala's, and Swift's `` `default` ``.
const NAME: Str = Str {
    close: Close::Quotes { quote: b'`', count: 1, hashes: 0 },
    escape: Escape::None,
    interp: Interp::None,
    lines: false,
    class: Class::Plain,
};

const C_STRINGS: &[Opener] = &[
    opens("u8\"", DQ),
    opens("u8'", SQ),
    opens("u\"", DQ),
    opens("u'", SQ),
    opens("U\"", DQ),
    opens("U'", SQ),
    opens("L\"", DQ),
    opens("L'", SQ),
    opens("\"", DQ),
    opens("'", SQ),
];
const OBJC_STRINGS: &[Opener] = &[
    opens("@\"", DQ),
    opens("u8\"", DQ),
    opens("u8'", SQ),
    opens("u\"", DQ),
    opens("u'", SQ),
    opens("U\"", DQ),
    opens("U'", SQ),
    opens("L\"", DQ),
    opens("L'", SQ),
    opens("\"", DQ),
    opens("'", SQ),
];
const CSHARP_STRINGS: &[Opener] = &[
    opens("$\"\"\"", quoted(b'"', 3, Escape::None, Interp::Brace, true)),
    opens("\"\"\"", quoted(b'"', 3, Escape::None, Interp::None, true)),
    opens("$@\"", quoted(b'"', 1, Escape::Doubled, Interp::Brace, true)),
    opens("@$\"", quoted(b'"', 1, Escape::Doubled, Interp::Brace, true)),
    opens("@\"", quoted(b'"', 1, Escape::Doubled, Interp::None, true)),
    opens("$\"", quoted(b'"', 1, Escape::Backslash, Interp::Brace, false)),
    opens("\"", DQ),
    opens("'", SQ),
];
const JAVA_STRINGS: &[Opener] =
    &[opens("\"\"\"", quoted(b'"', 3, Escape::Backslash, Interp::None, true)), opens("\"", DQ), opens("'", SQ)];
const KOTLIN_STRINGS: &[Opener] = &[
    opens("\"\"\"", quoted(b'"', 3, Escape::None, Interp::Dollar, true)),
    opens("\"", quoted(b'"', 1, Escape::Backslash, Interp::Dollar, false)),
    opens("'", SQ),
    opens("`", NAME),
];
const SWIFT_STRINGS: &[Opener] = &[
    opens("\"\"\"", quoted(b'"', 3, Escape::Backslash, Interp::Paren, true)),
    opens("\"", quoted(b'"', 1, Escape::Backslash, Interp::Paren, false)),
    opens("`", NAME),
];
const GO_STRINGS: &[Opener] =
    &[opens("`", quoted(b'`', 1, Escape::None, Interp::None, true)), opens("\"", DQ), opens("'", SQ)];
const JS_STRINGS: &[Opener] =
    &[opens("`", quoted(b'`', 1, Escape::Backslash, Interp::DollarBrace, true)), opens("\"", DQ), opens("'", SQ)];
const DART_STRINGS: &[Opener] = &[
    opens("r\"\"\"", quoted(b'"', 3, Escape::None, Interp::None, true)),
    opens("r'''", quoted(b'\'', 3, Escape::None, Interp::None, true)),
    opens("\"\"\"", quoted(b'"', 3, Escape::Backslash, Interp::Dollar, true)),
    opens("'''", quoted(b'\'', 3, Escape::Backslash, Interp::Dollar, true)),
    opens("r\"", quoted(b'"', 1, Escape::None, Interp::None, false)),
    opens("r'", quoted(b'\'', 1, Escape::None, Interp::None, false)),
    opens("\"", quoted(b'"', 1, Escape::Backslash, Interp::Dollar, false)),
    opens("'", quoted(b'\'', 1, Escape::Backslash, Interp::Dollar, false)),
];
const SCALA_STRINGS: &[Opener] = &[
    opens("\"\"\"", quoted(b'"', 3, Escape::None, Interp::None, true)),
    opens("\"", DQ),
    opens("'", SQ),
    opens("`", NAME),
];
const ZIG_STRINGS: &[Opener] = &[
    opens(
        "\\\\",
        Str { close: Close::Line, escape: Escape::None, interp: Interp::None, lines: false, class: Class::String },
    ),
    opens("\"", DQ),
    opens("'", SQ),
];
const RUST_STRINGS: &[Opener] = &[
    opens("b\"", quoted(b'"', 1, Escape::Backslash, Interp::None, true)),
    opens("c\"", quoted(b'"', 1, Escape::Backslash, Interp::None, true)),
    opens("b'", SQ),
    opens("\"", quoted(b'"', 1, Escape::Backslash, Interp::None, true)),
];
const PROTO_STRINGS: &[Opener] = &[opens("\"", DQ), opens("'", SQ)];

/// What a language is before its own table says otherwise.
const BASE: Spec = Spec {
    keywords: &[],
    types: &[],
    constants: &[],
    builtins: &[],
    functions: Table::EMPTY,
    declares: Table::EMPTY,
    dotted: Table::EMPTY,
    strings: &[],
    raw: Raw::None,
    interpolators: false,
    block: Block::Flat,
    hash: Hash::None,
    at: At::None,
    capital_types: true,
    capital_constants: true,
    capital_methods: false,
    closures: false,
    conditions: Table::EMPTY,
    bracket_attributes: false,
    regex: false,
    jsx: false,
    lifetimes: false,
    digit_quotes: false,
    dollar: false,
    macros: false,
    arrow: false,
};

pub(crate) static C: Spec = Spec {
    keywords: &[C_KEYWORDS],
    types: &[C_TYPES],
    constants: &[C_CONSTANTS],
    declares: C_DECLARES,
    strings: C_STRINGS,
    hash: Hash::Directive,
    capital_types: false,
    arrow: true,
    ..BASE
};

pub(crate) static CPP: Spec = Spec {
    keywords: &[C_KEYWORDS, CPP_KEYWORDS],
    types: &[C_TYPES],
    constants: &[C_CONSTANTS],
    builtins: &[THIS],
    declares: CPP_DECLARES,
    strings: C_STRINGS,
    raw: Raw::Cpp,
    hash: Hash::Directive,
    digit_quotes: true,
    arrow: true,
    ..BASE
};

pub(crate) static OBJC: Spec = Spec {
    keywords: &[C_KEYWORDS, OBJC_KEYWORDS],
    types: &[C_TYPES, OBJC_TYPES],
    constants: &[C_CONSTANTS, OBJC_CONSTANTS],
    builtins: &[OBJC_BUILTINS],
    declares: OBJC_DECLARES,
    strings: OBJC_STRINGS,
    hash: Hash::Directive,
    at: At::Keyword,
    arrow: true,
    capital_methods: true,
    ..BASE
};

pub(crate) static CSHARP: Spec = Spec {
    keywords: &[CSHARP_KEYWORDS],
    types: &[CSHARP_TYPES],
    constants: &[CSHARP_CONSTANTS],
    builtins: &[CSHARP_BUILTINS],
    declares: CSHARP_DECLARES,
    strings: CSHARP_STRINGS,
    hash: Hash::Directive,
    at: At::Name,
    capital_methods: true,
    bracket_attributes: true,
    ..BASE
};

pub(crate) static JAVA: Spec = Spec {
    keywords: &[JAVA_KEYWORDS],
    types: &[JAVA_TYPES],
    constants: &[JAVA_CONSTANTS],
    builtins: &[JAVA_BUILTINS],
    declares: JAVA_DECLARES,
    strings: JAVA_STRINGS,
    at: At::Attribute,
    dollar: true,
    ..BASE
};

pub(crate) static KOTLIN: Spec = Spec {
    keywords: &[KOTLIN_KEYWORDS],
    constants: &[KOTLIN_CONSTANTS],
    builtins: &[KOTLIN_BUILTINS],
    functions: KOTLIN_FUNCTIONS,
    declares: KOTLIN_DECLARES,
    strings: KOTLIN_STRINGS,
    block: Block::Nested,
    at: At::Attribute,
    closures: true,
    ..BASE
};

pub(crate) static SWIFT: Spec = Spec {
    keywords: &[SWIFT_KEYWORDS],
    constants: &[SWIFT_CONSTANTS],
    builtins: &[SWIFT_BUILTINS],
    functions: SWIFT_FUNCTIONS,
    declares: SWIFT_DECLARES,
    strings: SWIFT_STRINGS,
    raw: Raw::Swift,
    block: Block::Nested,
    hash: Hash::Meta,
    at: At::Attribute,
    dollar: true,
    closures: true,
    conditions: SWIFT_CONDITIONS,
    ..BASE
};

pub(crate) static GO: Spec = Spec {
    keywords: &[GO_KEYWORDS],
    types: &[GO_TYPES],
    constants: &[GO_CONSTANTS],
    functions: GO_FUNCTIONS,
    declares: GO_DECLARES,
    strings: GO_STRINGS,
    capital_types: false,
    capital_constants: false,
    ..BASE
};

pub(crate) static JAVASCRIPT: Spec = Spec {
    keywords: &[JS_KEYWORDS],
    constants: &[JS_CONSTANTS],
    builtins: &[JS_BUILTINS],
    functions: JS_FUNCTIONS,
    declares: JS_DECLARES,
    strings: JS_STRINGS,
    hash: Hash::Private,
    at: At::Attribute,
    regex: true,
    jsx: true,
    dollar: true,
    ..BASE
};

const TS: Spec = Spec {
    keywords: &[JS_KEYWORDS, TS_KEYWORDS],
    types: &[TS_TYPES],
    constants: &[JS_CONSTANTS],
    builtins: &[JS_BUILTINS],
    functions: JS_FUNCTIONS,
    declares: TS_DECLARES,
    strings: JS_STRINGS,
    hash: Hash::Private,
    at: At::Attribute,
    regex: true,
    dollar: true,
    ..BASE
};

pub(crate) static TYPESCRIPT: Spec = TS;

pub(crate) static TSX: Spec = Spec { jsx: true, ..TS };

pub(crate) static DART: Spec = Spec {
    keywords: &[DART_KEYWORDS],
    types: &[DART_TYPES],
    constants: &[DART_CONSTANTS],
    builtins: &[DART_BUILTINS],
    declares: DART_DECLARES,
    strings: DART_STRINGS,
    block: Block::Nested,
    at: At::Attribute,
    dollar: true,
    ..BASE
};

pub(crate) static SCALA: Spec = Spec {
    keywords: &[SCALA_KEYWORDS],
    constants: &[SCALA_CONSTANTS],
    builtins: &[SCALA_BUILTINS],
    functions: SCALA_FUNCTIONS,
    declares: SCALA_DECLARES,
    strings: SCALA_STRINGS,
    interpolators: true,
    block: Block::Nested,
    at: At::Attribute,
    dollar: true,
    closures: true,
    ..BASE
};

pub(crate) static ZIG: Spec = Spec {
    keywords: &[ZIG_KEYWORDS],
    types: &[ZIG_TYPES],
    constants: &[ZIG_CONSTANTS],
    functions: ZIG_FUNCTIONS,
    strings: ZIG_STRINGS,
    block: Block::None,
    at: At::Function,
    ..BASE
};

pub(crate) static RUST: Spec = Spec {
    keywords: &[RUST_KEYWORDS],
    types: &[RUST_TYPES],
    constants: &[RUST_CONSTANTS],
    builtins: &[RUST_BUILTINS],
    functions: RUST_FUNCTIONS,
    declares: RUST_DECLARES,
    dotted: RUST_DOTTED,
    strings: RUST_STRINGS,
    raw: Raw::Rust,
    block: Block::Nested,
    hash: Hash::Attribute,
    lifetimes: true,
    macros: true,
    ..BASE
};

pub(crate) static PROTOBUF: Spec = Spec {
    keywords: &[PROTO_KEYWORDS],
    types: &[PROTO_TYPES],
    constants: &[PROTO_CONSTANTS],
    functions: PROTO_FUNCTIONS,
    declares: PROTO_DECLARES,
    strings: PROTO_STRINGS,
    ..BASE
};

// The tables, each sorted, so a word is found by binary search (`every_table_is_sorted`).

const C_KEYWORDS: Table = words!(
    "clike/c-keywords",
    [
        "_Alignas",
        "_Alignof",
        "_Atomic",
        "_Generic",
        "_Noreturn",
        "_Static_assert",
        "_Thread_local",
        "alignas",
        "alignof",
        "auto",
        "break",
        "case",
        "const",
        "constexpr",
        "continue",
        "default",
        "do",
        "else",
        "enum",
        "extern",
        "for",
        "goto",
        "if",
        "inline",
        "register",
        "restrict",
        "return",
        "sizeof",
        "static",
        "static_assert",
        "struct",
        "switch",
        "thread_local",
        "typedef",
        "typeof",
        "typeof_unqual",
        "union",
        "volatile",
        "while",
    ]
);
const C_TYPES: Table = words!(
    "clike/c-types",
    [
        "_BitInt",
        "_Bool",
        "_Complex",
        "_Decimal128",
        "_Decimal32",
        "_Decimal64",
        "_Imaginary",
        "bool",
        "char",
        "char16_t",
        "char32_t",
        "char8_t",
        "double",
        "float",
        "int",
        "int16_t",
        "int32_t",
        "int64_t",
        "int8_t",
        "intmax_t",
        "intptr_t",
        "long",
        "ptrdiff_t",
        "short",
        "signed",
        "size_t",
        "ssize_t",
        "uint16_t",
        "uint32_t",
        "uint64_t",
        "uint8_t",
        "uintmax_t",
        "uintptr_t",
        "unsigned",
        "va_list",
        "void",
        "wchar_t",
    ]
);
const C_CONSTANTS: Table = words!("clike/c-constants", ["false", "nullptr", "true"]);
const C_DECLARES: Table = words!("clike/c-declares", ["enum", "struct", "union"]);
const CPP_KEYWORDS: Table = words!(
    "clike/cpp-keywords",
    [
        "and",
        "and_eq",
        "asm",
        "bitand",
        "bitor",
        "catch",
        "class",
        "co_await",
        "co_return",
        "co_yield",
        "compl",
        "concept",
        "const_cast",
        "consteval",
        "constinit",
        "decltype",
        "delete",
        "dynamic_cast",
        "explicit",
        "export",
        "final",
        "friend",
        "import",
        "module",
        "mutable",
        "namespace",
        "new",
        "noexcept",
        "not",
        "not_eq",
        "operator",
        "or",
        "or_eq",
        "override",
        "private",
        "protected",
        "public",
        "reinterpret_cast",
        "requires",
        "static_cast",
        "template",
        "throw",
        "try",
        "typeid",
        "typename",
        "using",
        "virtual",
        "xor",
        "xor_eq",
    ]
);
const CPP_DECLARES: Table = words!("clike/cpp-declares", ["class", "concept", "enum", "namespace", "struct", "union"]);
const THIS: Table = words!("clike/this", ["this"]);
const OBJC_KEYWORDS: Table = words!(
    "clike/objc-keywords",
    [
        "_Nonnull",
        "_Nullable",
        "__block",
        "__bridge",
        "__bridge_retained",
        "__bridge_transfer",
        "__kindof",
        "__strong",
        "__unsafe_unretained",
        "__weak",
        "bycopy",
        "byref",
        "in",
        "inout",
        "oneway",
        "out",
    ]
);
const OBJC_TYPES: Table = words!("clike/objc-types", ["BOOL", "Class", "IMP", "Protocol", "SEL", "id", "instancetype"]);
const OBJC_CONSTANTS: Table = words!("clike/objc-constants", ["NO", "Nil", "YES", "nil"]);
const OBJC_BUILTINS: Table = words!("clike/objc-builtins", ["self", "super"]);
const OBJC_DECLARES: Table =
    words!("clike/objc-declares", ["class", "enum", "implementation", "interface", "protocol", "struct", "union"]);
const CSHARP_KEYWORDS: Table = words!(
    "clike/csharp-keywords",
    [
        "abstract",
        "and",
        "as",
        "async",
        "await",
        "break",
        "case",
        "catch",
        "checked",
        "class",
        "const",
        "continue",
        "default",
        "delegate",
        "do",
        "else",
        "enum",
        "event",
        "explicit",
        "extern",
        "finally",
        "fixed",
        "for",
        "foreach",
        "get",
        "goto",
        "if",
        "implicit",
        "in",
        "init",
        "interface",
        "internal",
        "is",
        "lock",
        "nameof",
        "namespace",
        "new",
        "not",
        "operator",
        "or",
        "out",
        "override",
        "params",
        "partial",
        "private",
        "protected",
        "public",
        "readonly",
        "record",
        "ref",
        "required",
        "return",
        "sealed",
        "set",
        "sizeof",
        "stackalloc",
        "static",
        "struct",
        "switch",
        "throw",
        "try",
        "typeof",
        "unchecked",
        "unsafe",
        "using",
        "var",
        "virtual",
        "volatile",
        "when",
        "where",
        "while",
        "with",
        "yield",
    ]
);
const CSHARP_TYPES: Table = words!(
    "clike/csharp-types",
    [
        "bool", "byte", "char", "decimal", "double", "dynamic", "float", "int", "long", "nint", "nuint", "object",
        "sbyte", "short", "string", "uint", "ulong", "ushort", "void",
    ]
);
const CSHARP_CONSTANTS: Table = words!("clike/csharp-constants", ["false", "null", "true"]);
const CSHARP_BUILTINS: Table = words!("clike/csharp-builtins", ["base", "this"]);
const CSHARP_DECLARES: Table =
    words!("clike/csharp-declares", ["class", "enum", "interface", "new", "record", "struct"]);
const JAVA_KEYWORDS: Table = words!(
    "clike/java-keywords",
    [
        "abstract",
        "assert",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "default",
        "do",
        "else",
        "enum",
        "extends",
        "final",
        "finally",
        "for",
        "goto",
        "if",
        "implements",
        "import",
        "instanceof",
        "interface",
        "native",
        "new",
        "package",
        "permits",
        "private",
        "protected",
        "public",
        "record",
        "return",
        "sealed",
        "static",
        "strictfp",
        "switch",
        "synchronized",
        "throw",
        "throws",
        "transient",
        "try",
        "var",
        "volatile",
        "when",
        "while",
        "yield",
    ]
);
const JAVA_TYPES: Table =
    words!("clike/java-types", ["boolean", "byte", "char", "double", "float", "int", "long", "short", "void"]);
const JAVA_CONSTANTS: Table = words!("clike/java-constants", ["false", "null", "true"]);
const JAVA_BUILTINS: Table = words!("clike/java-builtins", ["super", "this"]);
const JAVA_DECLARES: Table = words!("clike/java-declares", ["class", "enum", "interface", "record"]);
const KOTLIN_KEYWORDS: Table = words!(
    "clike/kotlin-keywords",
    [
        "abstract",
        "actual",
        "annotation",
        "as",
        "break",
        "by",
        "catch",
        "class",
        "companion",
        "const",
        "constructor",
        "continue",
        "crossinline",
        "data",
        "do",
        "else",
        "enum",
        "expect",
        "external",
        "final",
        "finally",
        "for",
        "fun",
        "if",
        "import",
        "in",
        "infix",
        "init",
        "inline",
        "inner",
        "interface",
        "internal",
        "is",
        "lateinit",
        "noinline",
        "object",
        "open",
        "operator",
        "out",
        "override",
        "package",
        "private",
        "protected",
        "public",
        "reified",
        "return",
        "sealed",
        "suspend",
        "tailrec",
        "throw",
        "try",
        "typealias",
        "typeof",
        "val",
        "var",
        "vararg",
        "when",
        "where",
        "while",
    ]
);
const KOTLIN_CONSTANTS: Table = words!("clike/kotlin-constants", ["false", "null", "true"]);
const KOTLIN_BUILTINS: Table = words!("clike/kotlin-builtins", ["it", "super", "this"]);
const KOTLIN_FUNCTIONS: Table = words!("clike/kotlin-functions", ["fun"]);
const KOTLIN_DECLARES: Table = words!("clike/kotlin-declares", ["class", "interface", "object", "typealias"]);
const SWIFT_KEYWORDS: Table = words!(
    "clike/swift-keywords",
    [
        "actor",
        "any",
        "as",
        "associatedtype",
        "async",
        "await",
        "borrowing",
        "break",
        "case",
        "catch",
        "class",
        "consuming",
        "continue",
        "convenience",
        "default",
        "defer",
        "deinit",
        "didSet",
        "do",
        "dynamic",
        "else",
        "enum",
        "extension",
        "fallthrough",
        "fileprivate",
        "final",
        "for",
        "func",
        "get",
        "guard",
        "if",
        "import",
        "in",
        "indirect",
        "infix",
        "init",
        "inout",
        "internal",
        "is",
        "isolated",
        "lazy",
        "let",
        "macro",
        "mutating",
        "nonisolated",
        "nonmutating",
        "open",
        "operator",
        "optional",
        "override",
        "package",
        "postfix",
        "precedencegroup",
        "prefix",
        "private",
        "protocol",
        "public",
        "repeat",
        "required",
        "rethrows",
        "return",
        "set",
        "some",
        "static",
        "struct",
        "subscript",
        "switch",
        "throw",
        "throws",
        "try",
        "typealias",
        "unowned",
        "var",
        "weak",
        "where",
        "while",
        "willSet",
    ]
);
const SWIFT_CONSTANTS: Table = words!("clike/swift-constants", ["false", "nil", "true"]);
const SWIFT_BUILTINS: Table = words!("clike/swift-builtins", ["self", "super"]);
const SWIFT_FUNCTIONS: Table = words!("clike/swift-functions", ["func"]);
const SWIFT_DECLARES: Table = words!(
    "clike/swift-declares",
    ["actor", "associatedtype", "class", "enum", "extension", "protocol", "struct", "typealias"]
);
const SWIFT_CONDITIONS: Table = words!("clike/swift-conditions", ["catch", "for", "guard", "if", "switch", "while"]);
const GO_KEYWORDS: Table = words!(
    "clike/go-keywords",
    [
        "break",
        "case",
        "chan",
        "const",
        "continue",
        "default",
        "defer",
        "else",
        "fallthrough",
        "for",
        "func",
        "go",
        "goto",
        "if",
        "import",
        "interface",
        "map",
        "package",
        "range",
        "return",
        "select",
        "struct",
        "switch",
        "type",
        "var",
    ]
);
const GO_TYPES: Table = words!(
    "clike/go-types",
    [
        "any",
        "bool",
        "byte",
        "comparable",
        "complex128",
        "complex64",
        "error",
        "float32",
        "float64",
        "int",
        "int16",
        "int32",
        "int64",
        "int8",
        "rune",
        "string",
        "uint",
        "uint16",
        "uint32",
        "uint64",
        "uint8",
        "uintptr",
    ]
);
const GO_CONSTANTS: Table = words!("clike/go-constants", ["false", "iota", "nil", "true"]);
const GO_FUNCTIONS: Table = words!("clike/go-functions", ["func"]);
const GO_DECLARES: Table = words!("clike/go-declares", ["type"]);
const JS_KEYWORDS: Table = words!(
    "clike/js-keywords",
    [
        "as",
        "async",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "export",
        "extends",
        "finally",
        "for",
        "from",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "let",
        "new",
        "of",
        "return",
        "static",
        "switch",
        "throw",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "yield",
    ]
);
const JS_CONSTANTS: Table = words!("clike/js-constants", ["Infinity", "NaN", "false", "null", "true", "undefined"]);
const JS_BUILTINS: Table = words!("clike/js-builtins", ["arguments", "super", "this"]);
const JS_FUNCTIONS: Table = words!("clike/js-functions", ["function"]);
const JS_DECLARES: Table = words!("clike/js-declares", ["class"]);
const TS_KEYWORDS: Table = words!(
    "clike/ts-keywords",
    [
        "abstract",
        "accessor",
        "asserts",
        "declare",
        "enum",
        "implements",
        "infer",
        "interface",
        "is",
        "keyof",
        "module",
        "namespace",
        "override",
        "private",
        "protected",
        "public",
        "readonly",
        "satisfies",
        "type",
        "unique",
    ]
);
const TS_TYPES: Table =
    words!("clike/ts-types", ["any", "bigint", "boolean", "never", "number", "object", "string", "symbol", "unknown"]);
const TS_DECLARES: Table = words!("clike/ts-declares", ["class", "enum", "interface", "type"]);
const DART_KEYWORDS: Table = words!(
    "clike/dart-keywords",
    [
        "abstract",
        "as",
        "assert",
        "async",
        "await",
        "base",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "covariant",
        "default",
        "deferred",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "extension",
        "external",
        "factory",
        "final",
        "finally",
        "for",
        "get",
        "hide",
        "if",
        "implements",
        "import",
        "in",
        "interface",
        "is",
        "late",
        "library",
        "mixin",
        "new",
        "on",
        "operator",
        "part",
        "required",
        "rethrow",
        "return",
        "sealed",
        "set",
        "show",
        "static",
        "switch",
        "sync",
        "throw",
        "try",
        "typedef",
        "var",
        "when",
        "while",
        "with",
        "yield",
    ]
);
const DART_TYPES: Table = words!("clike/dart-types", ["bool", "double", "dynamic", "int", "num", "void"]);
const DART_CONSTANTS: Table = words!("clike/dart-constants", ["false", "null", "true"]);
const DART_BUILTINS: Table = words!("clike/dart-builtins", ["super", "this"]);
const DART_DECLARES: Table = words!("clike/dart-declares", ["class", "enum", "extension", "mixin", "typedef"]);
const SCALA_KEYWORDS: Table = words!(
    "clike/scala-keywords",
    [
        "abstract",
        "case",
        "catch",
        "class",
        "def",
        "derives",
        "do",
        "else",
        "end",
        "enum",
        "export",
        "extends",
        "extension",
        "final",
        "finally",
        "for",
        "forSome",
        "given",
        "if",
        "implicit",
        "import",
        "inline",
        "lazy",
        "match",
        "new",
        "object",
        "opaque",
        "open",
        "override",
        "package",
        "private",
        "protected",
        "return",
        "sealed",
        "then",
        "throw",
        "trait",
        "transparent",
        "try",
        "type",
        "using",
        "val",
        "var",
        "while",
        "with",
        "yield",
    ]
);
const SCALA_CONSTANTS: Table = words!("clike/scala-constants", ["false", "null", "true"]);
const SCALA_BUILTINS: Table = words!("clike/scala-builtins", ["super", "this"]);
const SCALA_FUNCTIONS: Table = words!("clike/scala-functions", ["def"]);
const SCALA_DECLARES: Table = words!("clike/scala-declares", ["class", "enum", "object", "trait", "type"]);
const ZIG_KEYWORDS: Table = words!(
    "clike/zig-keywords",
    [
        "addrspace",
        "align",
        "allowzero",
        "and",
        "anyframe",
        "asm",
        "break",
        "callconv",
        "catch",
        "comptime",
        "const",
        "continue",
        "defer",
        "else",
        "enum",
        "errdefer",
        "error",
        "export",
        "extern",
        "fn",
        "for",
        "if",
        "inline",
        "linksection",
        "noalias",
        "noinline",
        "nosuspend",
        "opaque",
        "or",
        "orelse",
        "packed",
        "pub",
        "resume",
        "return",
        "struct",
        "suspend",
        "switch",
        "test",
        "threadlocal",
        "try",
        "union",
        "unreachable",
        "usingnamespace",
        "var",
        "volatile",
        "while",
    ]
);
const ZIG_TYPES: Table = words!(
    "clike/zig-types",
    [
        "anyerror",
        "anyopaque",
        "anytype",
        "bool",
        "c_char",
        "c_int",
        "c_long",
        "c_longdouble",
        "c_longlong",
        "c_short",
        "c_uint",
        "c_ulong",
        "c_ulonglong",
        "c_ushort",
        "comptime_float",
        "comptime_int",
        "f128",
        "f16",
        "f32",
        "f64",
        "f80",
        "i128",
        "i16",
        "i32",
        "i64",
        "i8",
        "isize",
        "noreturn",
        "type",
        "u128",
        "u16",
        "u32",
        "u64",
        "u8",
        "usize",
        "void",
    ]
);
const ZIG_CONSTANTS: Table = words!("clike/zig-constants", ["false", "null", "true", "undefined"]);
const ZIG_FUNCTIONS: Table = words!("clike/zig-functions", ["fn"]);
const RUST_KEYWORDS: Table = words!(
    "clike/rust-keywords",
    [
        "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate", "do", "dyn",
        "else", "enum", "extern", "final", "fn", "for", "if", "impl", "in", "let", "loop", "macro", "match", "mod",
        "move", "mut", "override", "priv", "pub", "ref", "return", "static", "struct", "super", "trait", "try", "type",
        "typeof", "union", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
    ]
);
const RUST_TYPES: Table = words!(
    "clike/rust-types",
    [
        "bool", "char", "f32", "f64", "i128", "i16", "i32", "i64", "i8", "isize", "str", "u128", "u16", "u32", "u64",
        "u8", "usize",
    ]
);
const RUST_CONSTANTS: Table = words!("clike/rust-constants", ["false", "true"]);
const RUST_BUILTINS: Table = words!("clike/rust-builtins", ["self"]);
const RUST_FUNCTIONS: Table = words!("clike/rust-functions", ["fn"]);
const RUST_DECLARES: Table = words!("clike/rust-declares", ["enum", "struct", "trait", "type", "union"]);
const RUST_DOTTED: Table = words!("clike/rust-dotted", ["await"]);
const PROTO_KEYWORDS: Table = words!(
    "clike/proto-keywords",
    [
        "edition",
        "enum",
        "extend",
        "extensions",
        "group",
        "import",
        "max",
        "message",
        "oneof",
        "option",
        "optional",
        "package",
        "public",
        "repeated",
        "required",
        "reserved",
        "returns",
        "rpc",
        "service",
        "stream",
        "syntax",
        "to",
        "weak",
    ]
);
const PROTO_TYPES: Table = words!(
    "clike/proto-types",
    [
        "bool", "bytes", "double", "fixed32", "fixed64", "float", "int32", "int64", "map", "sfixed32", "sfixed64",
        "sint32", "sint64", "string", "uint32", "uint64",
    ]
);
const PROTO_CONSTANTS: Table = words!("clike/proto-constants", ["false", "inf", "nan", "true"]);
const PROTO_FUNCTIONS: Table = words!("clike/proto-functions", ["rpc"]);
const PROTO_DECLARES: Table = words!("clike/proto-declares", ["enum", "message", "service"]);

/// Every table this scanner reads, in the order its file lists them (`tables::written`).
pub(super) const WORDS: &[Table] = &[
    C_KEYWORDS,
    C_TYPES,
    C_CONSTANTS,
    C_DECLARES,
    CPP_KEYWORDS,
    CPP_DECLARES,
    THIS,
    OBJC_KEYWORDS,
    OBJC_TYPES,
    OBJC_CONSTANTS,
    OBJC_BUILTINS,
    OBJC_DECLARES,
    CSHARP_KEYWORDS,
    CSHARP_TYPES,
    CSHARP_CONSTANTS,
    CSHARP_BUILTINS,
    CSHARP_DECLARES,
    JAVA_KEYWORDS,
    JAVA_TYPES,
    JAVA_CONSTANTS,
    JAVA_BUILTINS,
    JAVA_DECLARES,
    KOTLIN_KEYWORDS,
    KOTLIN_CONSTANTS,
    KOTLIN_BUILTINS,
    KOTLIN_FUNCTIONS,
    KOTLIN_DECLARES,
    SWIFT_KEYWORDS,
    SWIFT_CONSTANTS,
    SWIFT_BUILTINS,
    SWIFT_FUNCTIONS,
    SWIFT_DECLARES,
    SWIFT_CONDITIONS,
    GO_KEYWORDS,
    GO_TYPES,
    GO_CONSTANTS,
    GO_FUNCTIONS,
    GO_DECLARES,
    JS_KEYWORDS,
    JS_CONSTANTS,
    JS_BUILTINS,
    JS_FUNCTIONS,
    JS_DECLARES,
    TS_KEYWORDS,
    TS_TYPES,
    TS_DECLARES,
    DART_KEYWORDS,
    DART_TYPES,
    DART_CONSTANTS,
    DART_BUILTINS,
    DART_DECLARES,
    SCALA_KEYWORDS,
    SCALA_CONSTANTS,
    SCALA_BUILTINS,
    SCALA_FUNCTIONS,
    SCALA_DECLARES,
    ZIG_KEYWORDS,
    ZIG_TYPES,
    ZIG_CONSTANTS,
    ZIG_FUNCTIONS,
    RUST_KEYWORDS,
    RUST_TYPES,
    RUST_CONSTANTS,
    RUST_BUILTINS,
    RUST_FUNCTIONS,
    RUST_DECLARES,
    RUST_DOTTED,
    PROTO_KEYWORDS,
    PROTO_TYPES,
    PROTO_CONSTANTS,
    PROTO_FUNCTIONS,
    PROTO_DECLARES,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn specs() -> [&'static Spec; 16] {
        [
            &C,
            &CPP,
            &OBJC,
            &CSHARP,
            &JAVA,
            &KOTLIN,
            &SWIFT,
            &GO,
            &JAVASCRIPT,
            &TYPESCRIPT,
            &TSX,
            &DART,
            &SCALA,
            &ZIG,
            &RUST,
            &PROTOBUF,
        ]
    }

    /// Each table is sorted, with no word twice, so binary search finds what it holds.
    #[test]
    fn every_table_is_sorted() {
        for spec in specs() {
            let tables = [spec.keywords, spec.types, spec.constants, spec.builtins].into_iter().flatten().copied();
            for table in tables.chain([spec.functions, spec.declares, spec.dotted, spec.conditions]) {
                assert!(table.windows(2).all(|w| w[0] < w[1]), "not sorted: {table:?}");
                assert!(table.iter().all(|w| w.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')), "{table:?}");
            }
        }
    }

    /// A word that declares a function or a type, stays a keyword after a `.`, or begins a
    /// condition is a keyword, or an `@` makes it one.
    #[test]
    fn what_declares_is_a_keyword() {
        for spec in specs() {
            for word in spec.functions.iter().chain(spec.declares).chain(spec.dotted).chain(spec.conditions) {
                assert!(found(spec.keywords, word) || spec.at == At::Keyword, "{word} is no keyword");
            }
        }
    }

    /// An opener comes before every opener it begins, so the longer one is found: `"""` before `"`.
    #[test]
    fn an_opener_comes_before_the_openers_it_begins() {
        for spec in specs() {
            for (k, first) in spec.strings.iter().enumerate() {
                for later in &spec.strings[k + 1..] {
                    assert!(!later.open.starts_with(first.open), "{:?} hides {:?}", first.open, later.open);
                }
            }
        }
    }
}
