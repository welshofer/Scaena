//! HTML and XML, read by one scanner (PLAN 2.135), and the languages that write HTML around parts
//! of their own (PLAN 2.145): PHP, Vue's components, and Svelte's.
//!
//! - **Tags**: a tag's name is a tag, and its `<`, `</`, `>`, and `/>` punctuation; an attribute's
//!   name is an attribute, its `=` an operator, and its value a string, in quotes or none.
//! - **Entities** (`&amp;`, `&#169;`, `&#x1F600;`) are constants, in text and in values.
//! - **Comments**, `<!-- … -->`, run over lines to their close; a **CDATA** section's
//!   `<![CDATA[` and `]]>` are meta and what it holds a string; a **processing instruction**
//!   (`<?xml version="1.0"?>`) and a **declaration** (`<!DOCTYPE html>`, an internal subset in its
//!   brackets) are meta.
//! - **Text** is plain.
//! - In HTML, what `<textarea>` and `<title>` hold is text to their closing tag: no markup is read
//!   in it. What `<script>` holds is JavaScript, or what its `lang` names (`ts`), or what its
//!   `type` says: TypeScript, JSON for an import map or data, and a template's text, plain, for a
//!   type no browser runs. What `<style>` holds is CSS, or SCSS where its `lang` says `scss` or
//!   `less`. Each is colored as its language colors it alone (`code::embed`).
//! - **PHP**: its code, from `<?php`, `<?=`, or `<?` and a space to `?>` or the text's end, is
//!   PHP's (the C family's scanner, by PHP's table), its opener and `?>` meta; the HTML around it,
//!   and around it in an attribute's value, is HTML. The code ends at the first `?>` outside its
//!   strings, block comments, and heredocs, as PHP ends it, in a line comment too. A PHP text with
//!   no `<?php` and no `<?=` is code throughout, as a slide shows a snippet.
//! - **Vue**: `{{ … }}` in text is JavaScript, and so is a directive's value: an attribute's
//!   named `v-…`, `:…`, `@…`, or `#…`.
//! - **Svelte**: `{ … }` in text and in a tag, as an attribute's value, in a quoted one, or as an
//!   attribute itself (`{...props}`), is JavaScript; a block's `{#if`, `{:else if`, and `{/if`, and
//!   a tag's `{@html`, are keywords before it.
//! - An expression runs to the `}` that closes it, past the braces, strings, and block comments in
//!   it, or to the text's end.

use super::scan::{Spans, blanks_end, byte, is_break, is_space, is_word, line_next, space_end, word_end};
use super::{Class, Lang, clike, embed, embed_with, lang};

/// How far an entity is looked through for its `;`: `&CounterClockwiseContourIntegral;`.
const ENTITY: usize = 40;

/// The longest identifier a PHP heredoc opens with.
const IDENTIFIER: usize = 64;

/// The elements whose content is text to their closing tag, in HTML: script and style first,
/// which hold no entities and may be read in their languages, then those that hold entities.
const RAW: &[&[u8]] = &[b"script", b"style", b"textarea", b"title"];

/// What a markup text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Markup {
    Xml,
    Html,
    /// HTML around PHP's code.
    Php,
    /// A Vue component.
    Vue,
    /// A Svelte component.
    Svelte,
}

/// Writes `text`'s tokens as `kind` reads it, `depth` parts in another language deep.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>, kind: Markup, depth: usize) {
    if kind == Markup::Php && !php_opens(text.as_bytes()) {
        return clike::scan(text, &clike::PHP, out);
    }
    let mut scanner = Scanner { text, b: text.as_bytes(), out, kind, depth };
    scanner.run();
}

struct Scanner<'t, 'o> {
    text: &'t str,
    b: &'t [u8],
    out: &'o mut Spans<'t>,
    kind: Markup,
    depth: usize,
}

/// An opening tag read: where it ends, where its name starts and ends, and the values of its
/// `type` and `lang`, within their quotes, where it has them.
struct Opened {
    end: usize,
    name: (usize, usize),
    kind: Option<(usize, usize)>,
    lang: Option<(usize, usize)>,
}

impl Scanner<'_, '_> {
    fn run(&mut self) {
        let (b, len) = (self.b, self.b.len());
        let mut pos = 0;
        while pos < len {
            let c = b[pos];
            if is_space(c) {
                let to = space_end(b, pos, len);
                self.out.space(to);
                pos = to;
                continue;
            }
            let next = byte(b, pos + 1);
            pos = if c == b'<' {
                if b[pos..].starts_with(b"<!--") {
                    let end = find(b, pos + 4, b"-->").map_or(len, |k| k + 3);
                    self.out.push(end, Class::Comment);
                    end
                } else if b[pos..].starts_with(b"<![CDATA[") {
                    self.out.push(pos + 9, Class::Meta);
                    match find(b, pos + 9, b"]]>") {
                        Some(k) => {
                            self.out.push(k, Class::String);
                            self.out.push(k + 3, Class::Meta);
                            k + 3
                        }
                        None => {
                            self.out.push(len, Class::String);
                            len
                        }
                    }
                } else if self.kind == Markup::Php && php_open(b, pos).is_some() {
                    self.php(pos)
                } else if next == b'?' {
                    let end = find(b, pos + 2, b"?>").map_or(len, |k| k + 2);
                    self.out.push(end, Class::Meta);
                    end
                } else if next == b'!' {
                    let end = declaration(b, pos + 2);
                    self.out.push(end, Class::Meta);
                    end
                } else if next == b'/' && name_start(byte(b, pos + 2)) {
                    self.close(pos)
                } else if name_start(next) {
                    let opened = self.open(pos);
                    let name = &b[opened.name.0..opened.name.1];
                    match RAW.iter().position(|r| r.eq_ignore_ascii_case(name)) {
                        Some(k) if self.kind != Markup::Xml && b[opened.end - 1] == b'>' => self.content(&opened, k),
                        _ => opened.end,
                    }
                } else {
                    self.out.push(pos + 1, Class::Plain);
                    pos + 1
                }
            } else if c == b'&' {
                let end = entity(b, pos).unwrap_or(pos + 1);
                self.out.push(end, if end > pos + 1 { Class::Constant } else { Class::Plain });
                end
            } else if self.expression_at(pos) {
                self.expression(pos)
            } else {
                let braces = matches!(self.kind, Markup::Vue | Markup::Svelte);
                let mut e = pos + 1;
                while e < len && !matches!(b[e], b'<' | b'&') && !is_space(b[e]) && !(braces && b[e] == b'{') {
                    e += 1;
                }
                self.out.push(e, Class::Plain);
                e
            };
        }
    }

    /// Whether an expression opens at `at`: Vue's `{{` or Svelte's `{`.
    fn expression_at(&self, at: usize) -> bool {
        match self.kind {
            Markup::Vue => self.b[at..].starts_with(b"{{"),
            Markup::Svelte => self.b[at] == b'{',
            _ => false,
        }
    }

    /// An expression at `at`, read as JavaScript: Vue's `{{ … }}`, or Svelte's `{ … }` with a
    /// block's or a tag's keywords first (`{#each`, `{:else if`, `{/each`, `{@html`). Where it
    /// ends, past its close, or at the text's end.
    fn expression(&mut self, at: usize) -> usize {
        let (b, len) = (self.b, self.b.len());
        let open = if self.kind == Markup::Vue { 2 } else { 1 };
        self.out.push(at + open, Class::Punctuation);
        let mut from = at + open;
        if self.kind == Markup::Svelte && matches!(byte(b, from), b'#' | b':' | b'/' | b'@') {
            let mut e = word_end(b, from + 1, len);
            let s = blanks_end(b, e, len);
            if b[from..e] == *b":else" && b[s..].starts_with(b"if") && !is_word(byte(b, s + 2)) {
                e = s + 2;
            }
            self.out.push(e, Class::Keyword);
            from = e;
        }
        let close = brace_end(b, from);
        embed(self.text, from, close, lang("javascript"), self.depth, self.out);
        if close >= len {
            return len;
        }
        let end = if self.kind == Markup::Vue && byte(b, close + 1) == b'}' { close + 2 } else { close + 1 };
        self.out.push(end, Class::Punctuation);
        end
    }

    /// PHP's code at `at`, from its opener to `?>` or the text's end: the opener and the close
    /// meta, the code between read by PHP's table. Where it ends.
    fn php(&mut self, at: usize) -> usize {
        let open = at + php_open(self.b, at).unwrap_or(2);
        self.out.push(open, Class::Meta);
        let close = php_close(self.b, open);
        embed_with(self.text, open, close, self.depth, self.out, |part, out, _| clike::scan(part, &clike::PHP, out));
        if close >= self.b.len() {
            return close;
        }
        self.out.push(close + 2, Class::Meta);
        close + 2
    }

    /// A closing tag at `at`, `</name>`: where it ends.
    fn close(&mut self, at: usize) -> usize {
        let (b, len) = (self.b, self.b.len());
        self.out.push(at + 2, Class::Punctuation);
        let e = name_end(b, at + 2);
        self.out.push(e, Class::Tag);
        let mut i = e;
        while i < len && b[i] != b'>' && b[i] != b'<' {
            if is_space(b[i]) {
                i = space_end(b, i, len);
                self.out.space(i);
            } else {
                i += 1;
                self.out.push(i, Class::Plain);
            }
        }
        if byte(b, i) == b'>' {
            self.out.push(i + 1, Class::Punctuation);
            i += 1;
        }
        i
    }

    /// An opening tag at `at`, its name and its attributes, to its `>` or `/>`.
    fn open(&mut self, at: usize) -> Opened {
        let (b, len) = (self.b, self.b.len());
        self.out.push(at + 1, Class::Punctuation);
        let e = name_end(b, at + 1);
        self.out.push(e, Class::Tag);
        let mut opened = Opened { end: len, name: (at + 1, e), kind: None, lang: None };
        let mut attribute = (e, e);
        let mut i = e;
        while i < len {
            let c = b[i];
            if is_space(c) {
                i = space_end(b, i, len);
                self.out.space(i);
                continue;
            }
            let (to, class) = match c {
                b'>' => {
                    self.out.push(i + 1, Class::Punctuation);
                    opened.end = i + 1;
                    return opened;
                }
                b'/' if byte(b, i + 1) == b'>' => {
                    self.out.push(i + 2, Class::Punctuation);
                    opened.end = i + 2;
                    return opened;
                }
                b'=' => {
                    self.out.push(i + 1, Class::Operator);
                    let v = space_end(b, i + 1, len);
                    self.out.space(v);
                    let named = &b[attribute.0..attribute.1];
                    i = self.value(v, self.kind == Markup::Vue && directive(named));
                    let quoted = matches!(byte(b, v), b'"' | b'\'');
                    let closed = quoted && i > v + 1 && b[i - 1] == b[v];
                    let inner = (v + usize::from(quoted), if closed { i - 1 } else { i });
                    if named.eq_ignore_ascii_case(b"type") {
                        opened.kind = Some(inner);
                    } else if named.eq_ignore_ascii_case(b"lang") {
                        opened.lang = Some(inner);
                    }
                    continue;
                }
                b'{' if self.kind == Markup::Svelte => {
                    i = self.expression(i);
                    continue;
                }
                b'<' if self.kind == Markup::Php && php_open(b, i).is_some() => {
                    i = self.php(i);
                    continue;
                }
                b'/' => (i + 1, Class::Punctuation),
                _ => {
                    let mut k = i + 1;
                    while k < len && !is_space(b[k]) && !matches!(b[k], b'"' | b'\'' | b'>' | b'/' | b'=') {
                        k += 1;
                    }
                    attribute = (i, k);
                    (k, Class::Attribute)
                }
            };
            self.out.push(to, class);
            i = to;
        }
        opened
    }

    /// An attribute's value at `at`: in quotes, over lines to the quote that closes it, or none,
    /// to whitespace or the tag's `>`; a string, its entities constants. A Vue directive's value
    /// in quotes is JavaScript, and so is Svelte's in braces, alone or in quotes; PHP's code in a
    /// value is PHP's. Where it ends.
    fn value(&mut self, at: usize, directive: bool) -> usize {
        let (b, len) = (self.b, self.b.len());
        let quote = byte(b, at);
        if self.kind == Markup::Svelte && quote == b'{' {
            return self.expression(at);
        }
        let quoted = matches!(quote, b'"' | b'\'');
        let mut i = at + usize::from(quoted);
        self.out.push(i, Class::String);
        if directive && quoted {
            let close = (i..len).find(|&k| b[k] == quote).unwrap_or(len);
            embed(self.text, i, close, lang("javascript"), self.depth, self.out);
            if close >= len {
                return len;
            }
            self.out.push(close + 1, Class::String);
            return close + 1;
        }
        while i < len {
            let c = b[i];
            if quoted && c == quote {
                self.out.push(i + 1, Class::String);
                return i + 1;
            }
            if !quoted && (is_space(c) || c == b'>') {
                break;
            }
            if c == b'&'
                && let Some(end) = entity(b, i)
            {
                self.out.push(i, Class::String);
                self.out.push(end, Class::Constant);
                i = end;
                continue;
            }
            if quoted && c == b'{' && self.kind == Markup::Svelte {
                self.out.push(i, Class::String);
                i = self.expression(i);
                continue;
            }
            if c == b'<' && self.kind == Markup::Php && php_open(b, i).is_some() {
                self.out.push(i, Class::String);
                i = self.php(i);
                continue;
            }
            i += 1;
        }
        self.out.push(i, Class::String);
        i
    }

    /// What the raw element `opened`, the `k`th of [`RAW`], holds, to its closing tag (`</script>`,
    /// in any case), read: a script's or a style's in its language, else as text, its entities
    /// constants in a title and a text area. Where it ends, past the closing tag, or at the text's
    /// end.
    fn content(&mut self, opened: &Opened, k: usize) -> usize {
        let (b, len) = (self.b, self.b.len());
        let (at, name) = (opened.end, &b[opened.name.0..opened.name.1]);
        let mut i = at;
        while i < len {
            if b[i] == b'<'
                && byte(b, i + 1) == b'/'
                && b.len() >= i + 2 + name.len()
                && b[i + 2..i + 2 + name.len()].eq_ignore_ascii_case(name)
                && !name_start(byte(b, i + 2 + name.len()))
                && !byte(b, i + 2 + name.len()).is_ascii_digit()
            {
                break;
            }
            i += 1;
        }
        let text = self.text;
        let value = |range: Option<(usize, usize)>| range.and_then(|(from, to)| text.get(from..to));
        let part = match k {
            0 => script(value(opened.kind), value(opened.lang)),
            1 => style(value(opened.kind), value(opened.lang)),
            _ => None,
        };
        match part {
            Some(part) => embed(self.text, at, i, part, self.depth, self.out),
            None => self.text_of(at, i, k >= 2),
        }
        if i < len { self.close(i) } else { len }
    }

    /// The text from `at` to `to`, whitespace as whitespace is anywhere, its entities constants
    /// where `entities` is set.
    fn text_of(&mut self, at: usize, to: usize, entities: bool) {
        let b = self.b;
        let mut k = at;
        while k < to {
            if is_space(b[k]) {
                k = space_end(b, k, to);
                self.out.space(k);
            } else if entities
                && b[k] == b'&'
                && let Some(end) = entity(b, k).filter(|&end| end <= to)
            {
                self.out.push(end, Class::Constant);
                k = end;
            } else {
                let mut e = k + 1;
                while e < to && !is_space(b[e]) && !(entities && b[e] == b'&') {
                    e += 1;
                }
                self.out.push(e, Class::Plain);
                k = e;
            }
        }
    }
}

/// The language a `<script>` holds, by its `lang`, else its `type`: none for a type no browser
/// runs, whose text is a template's.
fn script(kind: Option<&str>, named: Option<&str>) -> Option<Lang> {
    if let Some(named) = named.map(str::trim).filter(|l| !l.is_empty()) {
        return Some(lang(named));
    }
    let Some(kind) = kind.map(str::trim).filter(|t| !t.is_empty()) else {
        return Some(lang("javascript"));
    };
    let kind = kind.to_ascii_lowercase();
    match kind.as_str() {
        "application/ecmascript"
        | "application/javascript"
        | "application/x-javascript"
        | "module"
        | "text/babel"
        | "text/ecmascript"
        | "text/javascript"
        | "text/jsx" => Some(lang("javascript")),
        "application/typescript" | "text/typescript" => Some(lang("typescript")),
        // Data, an import map, and speculation rules.
        "application/json" | "importmap" | "speculationrules" => Some(lang("json")),
        _ if kind.ends_with("+json") => Some(lang("json")),
        _ => None,
    }
}

/// The language a `<style>` holds, by its `lang`, else its `type`: SCSS for Less, which writes as
/// SCSS does; none for a type that is not CSS.
fn style(kind: Option<&str>, named: Option<&str>) -> Option<Lang> {
    if let Some(named) = named.map(str::trim).filter(|l| !l.is_empty()) {
        return Some(if named.eq_ignore_ascii_case("less") { lang("scss") } else { lang(named) });
    }
    match kind.map(str::trim) {
        None | Some("") => Some(lang("css")),
        Some(kind) if kind.eq_ignore_ascii_case("text/css") => Some(lang("css")),
        Some(_) => None,
    }
}

/// Whether an attribute named `name` is a Vue directive: `v-if`, `:class`, `@click`, `#default`.
fn directive(name: &[u8]) -> bool {
    name.starts_with(b"v-") || matches!(name.first(), Some(b':' | b'@' | b'#'))
}

/// How long PHP's opener at `at` is: `<?php` in any case, then white space or the text's end;
/// `<?=`; or `<?` and white space. None where `<?` opens something else (`<?xml`).
fn php_open(b: &[u8], at: usize) -> Option<usize> {
    if !b[at..].starts_with(b"<?") {
        return None;
    }
    let after = byte(b, at + 2);
    let php = b.len() >= at + 5 && b[at + 2..at + 5].eq_ignore_ascii_case(b"php");
    if php && (at + 5 == b.len() || is_space(b[at + 5])) {
        Some(5)
    } else if after == b'=' {
        Some(3)
    } else if is_space(after) {
        Some(2)
    } else {
        None
    }
}

/// Whether PHP's code opens anywhere in `b`, by `<?php` or `<?=`.
fn php_opens(b: &[u8]) -> bool {
    let mut i = 0;
    while let Some(k) = find(b, i, b"<?") {
        if php_open(b, k).is_some_and(|len| len > 2) {
            return true;
        }
        i = k + 2;
    }
    false
}

/// Where PHP's code from `at` ends: at the first `?>` outside its strings, block comments, and
/// heredocs, in a line comment too, or at the text's end.
fn php_close(b: &[u8], at: usize) -> usize {
    let len = b.len();
    let mut i = at;
    while i < len {
        let next = byte(b, i + 1);
        match b[i] {
            b'?' if next == b'>' => return i,
            q @ (b'"' | b'\'' | b'`') => i = quoted_end(b, i, q),
            b'/' if next == b'*' => i = find(b, i + 2, b"*/").map_or(len, |k| k + 2),
            b'/' if next == b'/' => i = comment_end(b, i),
            b'#' if next != b'[' => i = comment_end(b, i),
            b'<' if b[i..].starts_with(b"<<<") => i = heredoc_end(b, i),
            _ => i += 1,
        }
    }
    len
}

/// Where the line comment at `at` ends: at its line's end, or at a `?>` in it, which closes PHP's
/// code.
fn comment_end(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while i < b.len() && !is_break(b[i]) && !(b[i] == b'?' && byte(b, i + 1) == b'>') {
        i += 1;
    }
    i
}

/// Where the string that opens with `quote` at `at` ends: past the quote that closes it, a
/// backslash escaping the byte after it, or at the text's end.
fn quoted_end(b: &[u8], at: usize, quote: u8) -> usize {
    let mut i = at + 1;
    while i < b.len() && b[i] != quote {
        i += if b[i] == b'\\' { 2 } else { 1 };
    }
    (i + 1).min(b.len())
}

/// Where the heredoc that opens at `at` ends: `<<<EOT`, `<<<"EOT"`, or `<<<'EOT'`, to the line
/// that starts with its identifier past spaces and tabs, past the identifier. Past its `<<<` where
/// no identifier follows it.
fn heredoc_end(b: &[u8], at: usize) -> usize {
    let len = b.len();
    let mut from = blanks_end(b, at + 3, len);
    from += usize::from(matches!(byte(b, from), b'"' | b'\''));
    if !(byte(b, from).is_ascii_alphabetic() || byte(b, from) == b'_') {
        return at + 3;
    }
    let to = word_end(b, from, len.min(from + IDENTIFIER));
    let mut line = line_next(b, to, len);
    while line < len {
        let first = blanks_end(b, line, len);
        if b[first..].starts_with(&b[from..to]) && !is_word(byte(b, first + to - from)) {
            return first + to - from;
        }
        line = line_next(b, line, len);
    }
    len
}

/// Where the expression from `at` ends: at the `}` that closes it, past the braces, strings, and
/// block comments in it, or at the text's end.
fn brace_end(b: &[u8], at: usize) -> usize {
    let len = b.len();
    let (mut depth, mut i) = (0u32, at);
    while i < len {
        match b[i] {
            q @ (b'"' | b'\'' | b'`') => i = quoted_end(b, i, q),
            b'/' if byte(b, i + 1) == b'*' => i = find(b, i + 2, b"*/").map_or(len, |k| k + 2),
            b'{' => {
                depth = depth.saturating_add(1);
                i += 1;
            }
            b'}' if depth == 0 => return i,
            b'}' => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    len
}

/// Whether a tag's name can start with `c`: a letter, `_`, `:`, or any byte above ASCII.
fn name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || matches!(c, b'_' | b':') || c >= 0x80
}

/// Where the name at `at` ends: `div`, `svg:rect`, `my-element`, `xsl:value-of`.
fn name_end(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while i < b.len() && (name_start(b[i]) || b[i].is_ascii_digit() || matches!(b[i], b'-' | b'.')) {
        i += 1;
    }
    i
}

/// Where `what` is first found in `b` from `at`, if it is.
fn find(b: &[u8], at: usize, what: &[u8]) -> Option<usize> {
    let first = what[0];
    let mut i = at;
    while i + what.len() <= b.len() {
        if b[i] == first && b[i..].starts_with(what) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Where the entity at `at` ends, past its `;`: a name, or a number in decimal or hexadecimal.
fn entity(b: &[u8], at: usize) -> Option<usize> {
    let limit = b.len().min(at + ENTITY);
    let mut i = at + 1;
    if byte(b, i) == b'#' {
        i += 1;
        let hex = matches!(byte(b, i), b'x' | b'X');
        i += usize::from(hex);
        let digits = i;
        while i < limit && (if hex { b[i].is_ascii_hexdigit() } else { b[i].is_ascii_digit() }) {
            i += 1;
        }
        if i == digits {
            return None;
        }
    } else {
        let name = i;
        while i < limit && b[i].is_ascii_alphanumeric() {
            i += 1;
        }
        if i == name || !b[name].is_ascii_alphabetic() {
            return None;
        }
    }
    (byte(b, i) == b';').then_some(i + 1)
}

/// Where a declaration from `at`, past its `<!`, ends: past the `>` that closes it, its quotes and
/// its internal subset's brackets, and the declarations and comments in them, passed over.
fn declaration(b: &[u8], at: usize) -> usize {
    let len = b.len();
    let (mut depth, mut i) = (0u32, at);
    while i < len {
        match b[i] {
            q @ (b'"' | b'\'') => i = (i + 1..len).find(|&k| b[k] == q).map_or(len, |k| k + 1),
            b'<' if b[i..].starts_with(b"<!--") => i = find(b, i + 4, b"-->").map_or(len, |k| k + 3),
            b'[' | b'<' => {
                depth += 1;
                i += 1;
            }
            b']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'>' if depth == 0 => return i + 1,
            b'>' => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    len
}
