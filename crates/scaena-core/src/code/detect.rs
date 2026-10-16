//! The language of a paste (PLAN 2.133, ADR-0029 decision 9): named by what came with it or what
//! it says of itself, else likely by its signals, scored in integers, else prose.
//!
//! [`detect`] reads, in order:
//! - **What names it.** Among the [`Hints`], a Markdown fence's info string, VS Code's mode, a
//!   `text/html` element's class, and a file's name; then a fence that opens the text, and a
//!   script's first line (`#!/usr/bin/env python3`). A hint that names plain text names nothing,
//!   but a fence's does: a fence says its words are code, so a fence that names no language Scaena
//!   knows gives the language its words score, or plain text.
//! - **One line.** A text of one line is prose unless something names it.
//! - **What it looks like.** Each of the first [`READ_LINES`] lines, within [`READ_BYTES`], adds
//!   points to the languages its shape belongs to: `fn ` first on a line to Rust, `def …:` to
//!   Python, `<!DOCTYPE html` to HTML, a prompt (`$ git status`) to a console session, and so on;
//!   a text that is one JSON object or array scores JSON [`JSON_WHOLE`]. A superset scores what
//!   its subset scores: TypeScript JavaScript's, SCSS CSS's, C++ C's.
//! - **The rule.** The best score is likely where it reaches [`LIKELY`] and the lines that read as
//!   code outnumber those that read as prose; languages with the best score alike go by the order
//!   of [`LANGUAGES`], so JavaScript before TypeScript and C before C++. Anything else is prose.
//!
//! It reads ASCII bytes alone, in one pass, with no table sorted at run time.

use super::{LANGUAGES, Language, language};

/// The most lines [`detect`] reads for signals.
pub const READ_LINES: usize = 200;

/// The most bytes [`detect`] reads for signals, and the most it checks as one JSON value.
pub const READ_BYTES: usize = 64 * 1024;

/// The score from which a language is likely.
pub const LIKELY: i32 = 8;

/// What a text that is one JSON object or array scores for JSON.
pub const JSON_WHOLE: i32 = 20;

/// What came with a paste that may name its language. Each is optional.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hints<'a> {
    /// A Markdown fence's info string: `rust`, `python title="app.py"`, `{.js}`, or empty.
    pub fence: Option<&'a str>,
    /// A file's name or path: `main.rs`, `src/Dockerfile`.
    pub file: Option<&'a str>,
    /// A `text/html` element's class attribute: `language-rust`, `hljs python`.
    pub class: Option<&'a str>,
    /// VS Code's mode, from its `vscode-editor-data` (see [`vscode_mode`]): `typescriptreact`.
    pub mode: Option<&'a str>,
}

/// The language of a paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detected {
    /// Named by a hint, a fence, or a script's first line.
    Named(&'static Language),
    /// Likely, by its signals.
    Likely(&'static Language),
    /// Words, not code.
    Prose,
}

impl Detected {
    /// The language named or likely, none for prose.
    pub fn language(self) -> Option<&'static Language> {
        match self {
            Detected::Named(l) | Detected::Likely(l) => Some(l),
            Detected::Prose => None,
        }
    }
}

/// The language of `text`, pasted with `hints`.
pub fn detect(text: &str, hints: &Hints) -> Detected {
    if let Some(named) = named(text, hints) {
        return named;
    }
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    if lines.next().is_none() || lines.next().is_none() {
        return Detected::Prose;
    }
    scored(text).map_or(Detected::Prose, Detected::Likely)
}

/// VS Code's mode in its `vscode-editor-data`, which Chromium puts among a copy's custom data:
/// `{"version":1,"isFromEmptySelection":false,"mode":"rust"}`. None where the data is not an
/// object whose `mode` is a string written with no escape.
pub fn vscode_mode(json: &str) -> Option<&str> {
    let b = json.as_bytes();
    let mut i = space(b, 0);
    if b.get(i) != Some(&b'{') {
        return None;
    }
    i += 1;
    let mut depth = 1usize;
    while i < b.len() {
        match b[i] {
            b'"' => {
                let end = string_end(b, i)?;
                let after = space(b, end);
                // Only a key is followed by a `:`.
                if depth == 1 && b.get(after) == Some(&b':') && &json[i + 1..end - 1] == "mode" {
                    let at = space(b, after + 1);
                    if b.get(at) != Some(&b'"') {
                        return None;
                    }
                    let value = &json[at + 1..string_end(b, at)? - 1];
                    return (!value.contains('\\')).then_some(value);
                }
                i = end;
            }
            b'{' | b'[' => {
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
                depth = depth.checked_sub(1)?;
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

/// The index past the spaces at `i`.
fn space(b: &[u8], mut i: usize) -> usize {
    while b.get(i).is_some_and(|c| c.is_ascii_whitespace()) {
        i += 1;
    }
    i
}

/// The index past the JSON string that opens at `i`, none where it never closes.
fn string_end(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'"' => return Some(j + 1),
            _ => j += 1,
        }
    }
    None
}

/// VS Code's modes that are not a name or an alias Scaena knows, and the language each is.
const MODES: [(&str, &str); 6] = [
    ("cuda-cpp", "cpp"),
    ("dockercompose", "yaml"),
    ("github-actions-workflow", "yaml"),
    ("javascriptreact", "jsx"),
    ("objective-cpp", "objc"),
    ("typescriptreact", "tsx"),
];

/// What names `text`'s language, where something does.
fn named(text: &str, hints: &Hints) -> Option<Detected> {
    if let Some(info) = hints.fence {
        return Some(fenced(info, text));
    }
    let known = |l: Option<&'static Language>| l.filter(|l| l.name != "plain").map(Detected::Named);
    let by_mode = hints.mode.and_then(|m| {
        let m = m.trim();
        let mapped = MODES.iter().find(|(mode, _)| mode.eq_ignore_ascii_case(m)).map_or(m, |(_, name)| name);
        language(mapped)
    });
    if let Some(found) = known(by_mode).or_else(|| known(hints.class.and_then(by_class))) {
        return Some(found);
    }
    if let Some(found) = known(hints.file.and_then(by_file)) {
        return Some(found);
    }
    let start = text.len() - text.trim_start_matches(['\n', '\r', ' ', '\t', '\u{feff}']).len();
    let start = text[..start].rfind('\n').map_or(0, |n| n + 1);
    let rest = &text[start..];
    let (first, body) = rest.split_once('\n').unwrap_or((rest, ""));
    let first = first.trim_end_matches('\r');
    if let Some((_, _, info)) = opener(first) {
        return Some(fenced(info, body));
    }
    shebang(first.trim_start_matches('\u{feff}')).map(Detected::Named)
}

/// A fenced text's language: the one its info string names, else the one its words score, else
/// plain text.
fn fenced(info: &str, body: &str) -> Detected {
    let word = info.trim().split([' ', '\t', ',', '}']).next().unwrap_or("");
    let word = word.trim_start_matches(['{', '.']);
    let word = word.strip_prefix("language-").unwrap_or(word);
    if let Some(found) = (!word.is_empty()).then(|| language(word)).flatten() {
        return Detected::Named(found);
    }
    match scored(body) {
        Some(found) => Detected::Likely(found),
        None => language("plain").map_or(Detected::Prose, Detected::Named),
    }
}

/// A Markdown fence that opens a code block: its character, its length, and its info string.
pub(super) fn opener(line: &str) -> Option<(u8, usize, &str)> {
    let t = line.trim_start_matches([' ', '\t']);
    let c = *t.as_bytes().first()?;
    if c != b'`' && c != b'~' {
        return None;
    }
    let n = t.bytes().take_while(|&b| b == c).count();
    let info = t[n..].trim();
    (n >= 3 && !(c == b'`' && info.contains('`'))).then_some((c, n, info))
}

/// Whether `line` closes a fence opened by `n` of `c`.
pub(super) fn closes(line: &str, c: u8, n: usize) -> bool {
    let t = line.trim_matches([' ', '\t']);
    t.len() >= n && t.bytes().all(|b| b == c)
}

/// The language a `text/html` element's class names: `language-rust`, `lang-py`, `hljs python`,
/// `highlight-source-js`, `highlight-text-html-basic`.
fn by_class(class: &str) -> Option<&'static Language> {
    const PREFIXES: [&str; 7] =
        ["language-", "lang-", "highlight-source-", "highlight-text-", "highlight-", "source-", "brush:"];
    for word in class.split_ascii_whitespace() {
        let Some(rest) = PREFIXES.iter().find_map(|p| word.strip_prefix(p)) else {
            if let Some(found) = language(word).filter(|l| l.name != "plain") {
                return Some(found);
            }
            continue;
        };
        let head = rest.split('-').next().unwrap_or(rest);
        if let Some(found) = language(rest).or_else(|| language(head)) {
            return Some(found);
        }
    }
    None
}

/// The language a file's name says: its whole name (`Dockerfile`), its extension (`.rs`), or the
/// name before its first point (`Dockerfile.prod`).
fn by_file(path: &str) -> Option<&'static Language> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path).trim();
    if name.is_empty() {
        return None;
    }
    let whole = |n: &str| LANGUAGES.iter().find(|l| l.files.contains(&n));
    if let Some(found) = whole(name) {
        return Some(found);
    }
    if let Some((stem, ext)) = name.rsplit_once('.')
        && !stem.is_empty()
    {
        let ext = ext.to_ascii_lowercase();
        if let Some(found) = LANGUAGES.iter().find(|l| l.extensions.contains(&ext.as_str())) {
            return Some(found);
        }
    }
    name.split_once('.').and_then(|(stem, _)| whole(stem))
}

/// The language a script's first line runs it with: `#!/bin/bash`, `#!/usr/bin/env -S deno run`,
/// `#!/usr/bin/python3.12`.
fn shebang(first: &str) -> Option<&'static Language> {
    let mut words = first.strip_prefix("#!")?.split_ascii_whitespace();
    let mut program = words.next()?.rsplit('/').next()?;
    if program == "env" {
        program = words.find(|w| !w.starts_with('-') && !w.contains('='))?;
    }
    let runs = |p: &str| LANGUAGES.iter().find(|l| l.interpreters.contains(&p));
    runs(program).or_else(|| runs(program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '-')))
}

// The languages scored, in `LANGUAGES`' order, so a tie goes to the one the table lists first,
// and the shared scores added to several at the end.
const SCORED: [&str; 29] = [
    "c",
    "console",
    "cpp",
    "csharp",
    "css",
    "diff",
    "dockerfile",
    "go",
    "graphql",
    "hcl",
    "html",
    "ini",
    "java",
    "javascript",
    "json",
    "kotlin",
    "makefile",
    "powershell",
    "python",
    "ruby",
    "rust",
    "scss",
    "shell",
    "sql",
    "swift",
    "toml",
    "typescript",
    "xml",
    "yaml",
];
const C: usize = 0;
const CONSOLE: usize = 1;
const CPP: usize = 2;
const CSHARP: usize = 3;
const CSS: usize = 4;
const DIFF: usize = 5;
const DOCKERFILE: usize = 6;
const GO: usize = 7;
const GRAPHQL: usize = 8;
const HCL: usize = 9;
const HTML: usize = 10;
const INI: usize = 11;
const JAVA: usize = 12;
const JAVASCRIPT: usize = 13;
const JSON: usize = 14;
const KOTLIN: usize = 15;
const MAKEFILE: usize = 16;
const POWERSHELL: usize = 17;
const PYTHON: usize = 18;
const RUBY: usize = 19;
const RUST: usize = 20;
const SCSS: usize = 21;
const SHELL: usize = 22;
const SQL: usize = 23;
const SWIFT: usize = 24;
const TOML: usize = 25;
const TYPESCRIPT: usize = 26;
const XML: usize = 27;
const YAML: usize = 28;
/// What scores C and C++ alike.
const C_FAMILY: usize = 29;
/// What scores JavaScript and TypeScript alike.
const SCRIPT: usize = 30;
/// What scores CSS and SCSS alike.
const STYLES: usize = 31;
/// A line that ends with `;`: C, C++, C#, Java, JavaScript, TypeScript, and Rust.
const SEMI: usize = 32;
/// A line that ends with `{`: those, Go, Kotlin, and Swift.
const BRACE: usize = 33;

/// HTML's elements most written first on a line.
const HTML_TAGS: [&str; 52] = [
    "a", "article", "aside", "body", "br", "button", "div", "footer", "form", "h1", "h2", "h3", "h4", "h5", "h6",
    "head", "header", "hr", "html", "iframe", "img", "input", "label", "li", "link", "main", "meta", "nav", "noscript",
    "ol", "option", "p", "pre", "script", "section", "select", "small", "span", "strong", "style", "svg", "table",
    "tbody", "td", "template", "textarea", "th", "thead", "title", "tr", "ul", "video",
];

/// CSS's properties whose names hold no `-`, which a declaration's name is when it has none.
const CSS_PROPERTIES: [&str; 26] = [
    "background",
    "border",
    "bottom",
    "clear",
    "color",
    "content",
    "cursor",
    "display",
    "fill",
    "filter",
    "flex",
    "float",
    "font",
    "gap",
    "grid",
    "height",
    "left",
    "margin",
    "opacity",
    "outline",
    "overflow",
    "padding",
    "position",
    "right",
    "top",
    "width",
];

/// The commands a shell script most runs first on a line.
const COMMANDS: [&str; 30] = [
    "apt", "apt-get", "brew", "cargo", "cat", "cd", "chmod", "chown", "cp", "curl", "docker", "echo", "git", "go",
    "grep", "kubectl", "ln", "ls", "mkdir", "mv", "npm", "npx", "pip", "pip3", "rm", "sed", "sudo", "tar", "wget",
    "yarn",
];

/// A Dockerfile's instructions, in capitals, and what each scores.
const INSTRUCTIONS: [(&str, i32); 17] = [
    ("ADD ", 2),
    ("ARG ", 2),
    ("CMD ", 4),
    ("COPY ", 3),
    ("ENTRYPOINT ", 4),
    ("ENV ", 2),
    ("EXPOSE ", 4),
    ("FROM ", 3),
    ("HEALTHCHECK ", 4),
    ("LABEL ", 2),
    ("ONBUILD ", 4),
    ("RUN ", 4),
    ("SHELL ", 3),
    ("STOPSIGNAL ", 4),
    ("USER ", 2),
    ("VOLUME ", 2),
    ("WORKDIR ", 4),
];

/// SQL's clauses first on a line, in capitals, and what each scores; written in lowercase, each
/// scores one.
const CLAUSES: [(&str, i32); 28] = [
    ("ALTER TABLE ", 5),
    ("AND ", 1),
    ("BEGIN;", 2),
    ("COMMIT;", 3),
    ("CREATE INDEX ", 5),
    ("CREATE OR REPLACE ", 5),
    ("CREATE TABLE ", 5),
    ("CREATE VIEW ", 5),
    ("DELETE FROM ", 5),
    ("DROP TABLE ", 5),
    ("FROM ", 2),
    ("FULL JOIN ", 3),
    ("GROUP BY ", 4),
    ("HAVING ", 3),
    ("INNER JOIN ", 3),
    ("INSERT INTO ", 5),
    ("JOIN ", 3),
    ("LEFT JOIN ", 3),
    ("LIMIT ", 2),
    ("ON CONFLICT ", 4),
    ("OR ", 1),
    ("ORDER BY ", 4),
    ("RIGHT JOIN ", 3),
    ("SELECT ", 4),
    ("SET ", 1),
    ("UNION", 3),
    ("VALUES", 3),
    ("WHERE ", 3),
];

/// The scores of a text, line by line.
struct Scores {
    points: [i32; 34],
    /// Whether the line being read scored.
    hit: bool,
    /// Lines that read as code: they scored, or have code's shape.
    code: usize,
    /// Lines that read as prose: words, mostly, and nothing that scored.
    prose: usize,
    /// The indentation of a YAML key with nothing after it, which a deeper line under it opens.
    key: Option<usize>,
    /// Whether the line before was a Makefile rule's head or its recipe.
    rule: bool,
    /// A diff's hunks, and its lines that add or take away.
    hunks: usize,
    changes: i32,
}

impl Scores {
    fn add(&mut self, i: usize, n: i32) {
        self.points[i] += n;
        self.hit = true;
    }
}

/// The language `text`'s signals make likely, none where none is (see the module's rule).
fn scored(text: &str) -> Option<&'static Language> {
    let mut s = Scores { points: [0; 34], hit: false, code: 0, prose: 0, key: None, rule: false, hunks: 0, changes: 0 };
    let mut read = 0usize;
    let mut first = true;
    for raw in text.split('\n').take(READ_LINES) {
        read += raw.len() + 1;
        if read > READ_BYTES {
            break;
        }
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let t = line.trim_start_matches([' ', '\t', '\u{feff}']);
        if t.trim_end().is_empty() {
            continue;
        }
        s.hit = false;
        let indent = line.len() - t.len();
        signals(&mut s, line, t.trim_end(), indent, first);
        first = false;
        let shaped = code_shaped(t.trim_end());
        if s.hit || shaped {
            s.code += 1;
        } else if prose_like(t) {
            s.prose += 1;
        }
    }
    let trimmed = text.trim();
    if text.len() <= READ_BYTES && (trimmed.starts_with('{') || trimmed.starts_with('[')) && json(trimmed).is_some() {
        s.points[JSON] += JSON_WHOLE;
    }
    if s.hunks > 0 {
        s.points[DIFF] += s.changes;
    }
    let p = &mut s.points;
    for (shared, to) in [
        (C_FAMILY, &[C, CPP][..]),
        (SCRIPT, &[JAVASCRIPT, TYPESCRIPT]),
        (STYLES, &[CSS, SCSS]),
        (SEMI, &[C, CPP, CSHARP, JAVA, JAVASCRIPT, TYPESCRIPT, RUST]),
        (BRACE, &[C, CPP, CSHARP, GO, JAVA, JAVASCRIPT, KOTLIN, RUST, SWIFT, TYPESCRIPT]),
    ] {
        for &i in to {
            p[i] += p[shared];
        }
    }
    // The first of the best, in the table's order.
    let mut best = 0;
    for i in 1..SCORED.len() {
        if p[i] > p[best] {
            best = i;
        }
    }
    (p[best] >= LIKELY && s.code > s.prose).then(|| language(SCORED[best])).flatten()
}

/// What `t`, a line with its indentation taken off and its end trimmed, scores.
fn signals(s: &mut Scores, line: &str, t: &str, indent: usize, first: bool) {
    let b = t.as_bytes();
    let word = t.split([' ', '\t']).next().unwrap_or(t);

    // A YAML key alone opens what a deeper line under it holds.
    if let Some(at) = s.key.take()
        && indent > at
        && (t.starts_with("- ") || yaml_key(t).is_some())
    {
        s.add(YAML, 2);
    }
    // A Makefile's recipe stands after its tab, under its rule.
    let recipe = s.rule && line.starts_with('\t');
    s.rule = recipe;
    if recipe {
        s.add(MAKEFILE, 4);
        return;
    }

    // A diff.
    if t.starts_with("diff --git ") {
        return s.add(DIFF, 10);
    }
    if indent == 0 && (t.starts_with("--- a/") || t.starts_with("+++ b/") || t.ends_with("/dev/null")) {
        return s.add(DIFF, 4);
    }
    if t.starts_with("@@ -") && t[4..].contains(" @@") {
        s.hunks += 1;
        return s.add(DIFF, 6);
    }
    if indent == 0 && (b[0] == b'+' || b[0] == b'-') && b.get(1) != Some(&b' ') {
        s.changes += 1;
    }

    // A console's prompt: the command after it is the session's, not a script's.
    if prompt(t) {
        return s.add(CONSOLE, 5);
    }

    // Markup.
    if starts_any_case(t, "<!doctype html") {
        return s.add(HTML, 10);
    }
    if t.starts_with("<?xml") {
        return s.add(XML, 10);
    }
    if t.contains("xmlns") {
        s.add(XML, 3);
    }
    if let Some(rest) = t.strip_prefix('<') {
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        let name: &str =
            &rest[..rest.bytes().take_while(|&c| c.is_ascii_alphanumeric() || b"-:._".contains(&c)).count()];
        if name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) {
            // JSX: an expression in braces, a `className`, or a component's capital.
            let jsx = t.contains('{') || t.contains("className=");
            if jsx || name.as_bytes()[0].is_ascii_uppercase() && t.ends_with("/>") {
                s.add(SCRIPT, 3);
            } else if HTML_TAGS.contains(&name) {
                s.add(HTML, if t.starts_with("</") { 1 } else { 3 });
            } else {
                s.add(XML, if t.starts_with("</") { 1 } else { 2 });
            }
            return;
        }
    }

    // The C family's preprocessor.
    if let Some(directive) = t.strip_prefix('#') {
        let directive = directive.trim_start();
        if let Some(header) = directive.strip_prefix("include") {
            s.add(C_FAMILY, 4);
            if header.contains(".h>") || header.contains(".h\"") {
                s.add(C, 2);
            } else if header.contains('<') {
                s.add(CPP, 3);
            }
            return;
        }
        for d in ["define ", "ifdef ", "ifndef ", "endif", "pragma ", "undef "] {
            if directive.starts_with(d) {
                return s.add(C_FAMILY, 3);
            }
        }
        if t.starts_with("#[") || t.starts_with("#![") {
            return s.add(RUST, 4);
        }
        return;
    }
    if t.starts_with("//") || t.starts_with("/*") || t.starts_with("* ") || t.starts_with("<!--") {
        return;
    }

    // Dockerfile's instructions and SQL's clauses, in capitals.
    for (instruction, n) in INSTRUCTIONS {
        if t.starts_with(instruction) {
            s.add(DOCKERFILE, n);
        }
    }
    for (clause, n) in CLAUSES {
        let head = t.get(..clause.len());
        if head == Some(clause) {
            s.add(SQL, n);
        } else if head.is_some_and(|h| h.bytes().all(|c| !c.is_ascii_uppercase()) && h.eq_ignore_ascii_case(clause)) {
            s.add(SQL, if clause == "SELECT " && t.contains(" from ") { 4 } else { 1 });
        }
    }

    // A Makefile.
    if t.starts_with(".PHONY") {
        return s.add(MAKEFILE, 6);
    }
    if indent == 0 && makefile_rule(t) {
        s.rule = true;
    }
    if t.contains("$@") || t.contains("$<") || t.contains("$^") {
        s.add(MAKEFILE, 3);
    }
    if indent == 0
        && let Some(at) = t.find(['=', ' '])
        && [":=", "?=", "+=", "!=", "::="].iter().any(|op| t[at..].trim_start().starts_with(op))
        && t[..at].bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        s.add(MAKEFILE, 3);
    }
    if indent == 0
        && (["ifeq ", "ifneq ", "ifdef ", "ifndef ", "define "].iter().any(|d| t.starts_with(d)) || t == "endef")
    {
        s.add(MAKEFILE, 3);
    }
    if t.contains("$(") {
        s.add(MAKEFILE, 1);
        s.add(SHELL, 1);
    }

    // Terraform's HCL and GraphQL.
    for block in ["resource \"", "variable \"", "provider \"", "module \"", "output \"", "data \""] {
        if t.starts_with(block) {
            return s.add(HCL, 5);
        }
    }
    if t == "terraform {" || t == "locals {" {
        return s.add(HCL, 5);
    }
    for op in ["query ", "mutation ", "subscription "] {
        if t.starts_with(op) && t.ends_with('{') {
            return s.add(GRAPHQL, 4);
        }
    }
    if t.starts_with("fragment ") && t.contains(" on ") {
        return s.add(GRAPHQL, 5);
    }
    if ["String!", "Int!", "ID!", ": [", "Boolean"].iter().any(|g| t.contains(g)) && t.ends_with(['!', ']', 'n', 'g']) {
        s.add(GRAPHQL, 1);
    }

    // Styles.
    if t.ends_with('{') && selector(t, word) {
        return s.add(STYLES, 3);
    }
    if let Some(rule) = t.strip_prefix('@') {
        for at in ["media ", "import ", "font-face", "keyframes ", "supports "] {
            if rule.starts_with(at) {
                return s.add(STYLES, 4);
            }
        }
        for at in ["mixin ", "include ", "extend ", "use ", "forward ", "each ", "if "] {
            if rule.starts_with(at) {
                return s.add(SCSS, 5);
            }
        }
    }
    if t.starts_with('&') {
        return s.add(SCSS, 4);
    }
    if t.starts_with('$') && t.ends_with(';') && t.contains(':') && !t.contains('=') {
        return s.add(SCSS, 4);
    }
    if declaration(t) {
        return s.add(STYLES, 3);
    }

    // JSON's keys, where the text is not one whole value.
    if t.starts_with('"') && t.contains("\": ") {
        s.add(JSON, 2);
    }

    // Rust.
    let mut bare = t;
    let mut public = false;
    for modifier in ["pub(crate) ", "pub(super) ", "pub ", "async ", "unsafe ", "const ", "extern \"C\" "] {
        if let Some(rest) = bare.strip_prefix(modifier) {
            public |= modifier.starts_with("pub");
            bare = rest;
        }
    }
    if bare.starts_with("fn ") {
        s.add(RUST, 5);
    } else if public && ["struct ", "enum ", "mod ", "use ", "trait ", "type "].iter().any(|k| bare.starts_with(k)) {
        s.add(RUST, 4);
    }
    if t.starts_with("let mut ") || t.starts_with("impl ") || t.starts_with("impl<") {
        s.add(RUST, 5);
    }
    if t.starts_with("use ") && t.contains("::") {
        s.add(RUST, 5);
    }
    if (t.starts_with("mod ") && t.ends_with(';')) || (t.starts_with("match ") && t.ends_with('{')) {
        s.add(RUST, 3);
    }
    if t.contains("&self") || t.contains("&mut ") {
        s.add(RUST, 3);
    }
    if macro_call(t) {
        s.add(RUST, 3);
    }
    if t.contains("Some(") || t.contains("Ok(") || t.contains("Err(") {
        s.add(RUST, 1);
    }
    if t.starts_with("if let ") || t.starts_with("guard let ") || t.starts_with("guard ") {
        if t.contains("Some(") || t.contains("Ok(") {
            s.add(RUST, 2);
        } else {
            s.add(SWIFT, 5);
        }
    }

    // Python and Ruby.
    if t.starts_with("def ") || t.starts_with("async def ") {
        if t.ends_with(':') { s.add(PYTHON, 5) } else { s.add(RUBY, 4) }
    }
    if t.starts_with("class ") {
        if t.ends_with(':') {
            s.add(PYTHON, 4);
        } else if t.contains(" < ") || !t.contains(['{', '(', ':']) && !t.ends_with(';') {
            s.add(RUBY, 3);
        }
    }
    if t.starts_with("from ") && t.contains(" import ") {
        s.add(PYTHON, 5);
    }
    if let Some(module) = t.strip_prefix("import ") {
        import(s, module);
    }
    if ["if ", "elif ", "while ", "for ", "with ", "except"].iter().any(|k| t.starts_with(k))
        && t.ends_with(':')
        && !t.contains('{')
    {
        s.add(PYTHON, 2);
    }
    if t.starts_with("elif ") || t.starts_with("except") || t.starts_with("lambda ") || t.starts_with("\"\"\"") {
        s.add(PYTHON, 2);
    }
    if t == "else:" || t == "try:" || t == "finally:" {
        s.add(PYTHON, 2);
    }
    if t.contains("__init__") || t.contains("__name__") || t.contains("self.") && !t.ends_with(';') {
        s.add(PYTHON, 2);
    }
    if t == "end" || t.starts_with("elsif ") || t.starts_with("unless ") {
        s.add(RUBY, 3);
    }
    if t.starts_with("require '") || t.starts_with("require \"") || t.starts_with("require_relative ") {
        s.add(RUBY, 4);
    }
    if t.starts_with("puts ") || t.starts_with("attr_") || t.contains(" do |") || t.ends_with(" do") && !t.contains(';')
    {
        s.add(RUBY, 3);
    }
    if t.starts_with("module ") && word_at(t, 7).is_some_and(|w| w.starts_with(|c: char| c.is_ascii_uppercase())) {
        s.add(RUBY, 3);
    }

    // Shell and PowerShell.
    for k in ["if [", "elif [", "while [", "until ["] {
        if t.starts_with(k) {
            s.add(SHELL, 5);
        }
    }
    if t.ends_with("; then") || t == "then" || t.ends_with("; do") || t == "do" {
        s.add(SHELL, 4);
    }
    if ["fi", "done", "esac", "fi;", "done;", ";;"].contains(&t) || t.starts_with("case ") && t.ends_with(" in") {
        s.add(SHELL, 4);
    }
    if t.starts_with("export ") && t.contains('=') || t.starts_with("set -e") || t.starts_with("set -o") {
        s.add(SHELL, 4);
    }
    if assignment(t) {
        s.add(SHELL, 2);
    }
    if COMMANDS.contains(&word) && t.len() > word.len() {
        s.add(SHELL, 2);
    }
    if t.contains("2>&1") || t.contains(" >/dev/null") {
        s.add(SHELL, 2);
    }
    let verb_noun = |w: &str| {
        let Some((verb, noun)) = w.split_once('-') else { return false };
        let capital =
            |p: &str| p.len() > 1 && p.as_bytes()[0].is_ascii_uppercase() && p.bytes().all(|c| c.is_ascii_alphabetic());
        capital(verb) && capital(noun)
    };
    if verb_noun(word) || t.starts_with("function ") && word_at(t, 9).is_some_and(verb_noun) {
        s.add(POWERSHELL, 5);
    } else if t.starts_with("function ") || t.starts_with("async function") {
        s.add(SCRIPT, 4);
    }
    if starts_any_case(t, "param(") || t.starts_with("[CmdletBinding") {
        s.add(POWERSHELL, 5);
    }
    if t.contains("-Object") || t.contains("$_") || t.contains("Write-Host") {
        s.add(POWERSHELL, 3);
    }
    if t.starts_with('$') && t.contains(" = ") && !t.ends_with(';') {
        s.add(POWERSHELL, 3);
    }
    if [" -eq ", " -ne ", " -gt ", " -lt ", " -le ", " -ge "].iter().any(|op| t.contains(op)) {
        if t.contains("[ ") || t.contains("[[") { s.add(SHELL, 2) } else { s.add(POWERSHELL, 3) }
    }
    if [" -like ", " -match ", " -and ", " -or ", " -not "].iter().any(|op| t.contains(op)) {
        s.add(POWERSHELL, 2);
    }
    if t.starts_with("[string]") || t.starts_with("[int]") || t.starts_with("[switch]") || t.starts_with("[bool]") {
        s.add(POWERSHELL, 4);
    }
    if t.starts_with("foreach (") {
        if t.contains('$') { s.add(POWERSHELL, 3) } else { s.add(CSHARP, 3) }
    }

    // Go.
    if let Some(name) = t.strip_prefix("package ") {
        if name.ends_with(';') {
            s.add(JAVA, 4);
        } else if name.contains('.') {
            s.add(KOTLIN, 4);
        } else if name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
            s.add(GO, 5);
        }
    }
    if t.starts_with("func ") {
        s.add(GO, 3);
        s.add(SWIFT, 3);
        if t.contains("->") {
            s.add(SWIFT, 2);
        } else if t.starts_with("func (") || t.ends_with('{') {
            s.add(GO, 2);
        }
    }
    if t.contains(" := ") || t.contains("fmt.") || t.contains("err != nil") {
        s.add(GO, 3);
    }
    if t.starts_with("defer ") || t.starts_with("go func") {
        s.add(GO, 3);
    }
    if t.starts_with("type ") {
        if t.ends_with(" struct {") || t.ends_with(" interface {") {
            s.add(GO, 4);
        } else if t.contains(" = ") {
            s.add(TYPESCRIPT, 4);
        } else if t.ends_with('{') {
            s.add(GRAPHQL, 2);
        }
    }

    // JavaScript and TypeScript.
    if t.starts_with("const ") || t.starts_with("let ") || t.starts_with("var ") {
        let rest = &t[t.find(' ').map_or(0, |n| n + 1)..];
        let name = rest.split([' ', ':', '=']).next().unwrap_or("");
        let single = rest.starts_with(['{', '[']) || rest[name.len()..].trim_start().starts_with(['=', ':']);
        if single && t.starts_with("const ") {
            s.add(SCRIPT, 2);
        } else if single && t.starts_with("var ") {
            s.add(SCRIPT, 1);
            s.add(SWIFT, 1);
            if t.ends_with(';') {
                s.add(CSHARP, 1);
            }
        } else if single {
            s.add(SCRIPT, 1);
            s.add(SWIFT, 1);
        }
    }
    if t.contains("=>") {
        s.add(SCRIPT, 2);
    }
    if t.contains("===") || t.contains("!==") || t.contains("require(") || t.contains("document.") {
        s.add(SCRIPT, 3);
    }
    if t.contains("console.log") || t.contains("module.exports") {
        s.add(SCRIPT, 5);
    }
    if let Some(rest) = t.strip_prefix("export ") {
        if rest.starts_with("interface ") || rest.starts_with("type ") {
            s.add(TYPESCRIPT, 5);
        } else {
            s.add(SCRIPT, 4);
        }
    }
    if [": string", ": number", ": boolean", ": any", ": void", ": Promise<", "<string>", "as const", "readonly "]
        .iter()
        .any(|k| t.contains(k))
    {
        s.add(TYPESCRIPT, 4);
    }
    if t.starts_with("interface ") && t.ends_with('{') || t.starts_with("declare ") {
        s.add(TYPESCRIPT, 3);
    }

    // Java, C#, and Kotlin.
    for modifier in ["public ", "private ", "protected "] {
        if t.starts_with(modifier) {
            s.add(JAVA, 2);
            s.add(CSHARP, 2);
            s.add(TYPESCRIPT, 1);
        }
    }
    if t.contains("System.out.") || t.contains("@Override") || t.contains("String[] ") || t.contains(" throws ") {
        s.add(JAVA, 5);
    }
    if t.contains(" extends ") || t.contains("boolean ") || t.contains(" final ") {
        s.add(JAVA, 2);
    }
    if let Some(name) = t.strip_prefix("using ") {
        if name.starts_with("namespace ") {
            s.add(CPP, 5);
        } else if name.starts_with("System") {
            s.add(CSHARP, 6);
        } else if name.ends_with(';') {
            s.add(CSHARP, 4);
        }
    }
    if t.starts_with("namespace ") {
        if t.contains('.') || t.ends_with(';') { s.add(CSHARP, 4) } else { s.add(CPP, 2) }
    }
    if t.contains("Console.Write") || t.contains("static void Main") || t.contains("{ get;") || t.contains("async Task")
    {
        s.add(CSHARP, 6);
    }
    if t.starts_with("string ") || t.starts_with("bool ") {
        s.add(CSHARP, 2);
    }
    if t.starts_with('[') && t.ends_with(')') && b.get(1).is_some_and(u8::is_ascii_uppercase) && t.contains("(\"") {
        s.add(CSHARP, 4);
    }
    let mut fun = t;
    for modifier in ["override ", "private ", "public ", "internal ", "suspend ", "inline ", "open ", "protected "] {
        fun = fun.strip_prefix(modifier).unwrap_or(fun);
    }
    if fun.starts_with("fun ") {
        s.add(KOTLIN, 5);
    }
    if t.starts_with("val ") || t.starts_with("data class ") || t.starts_with("companion object") {
        s.add(KOTLIN, 4);
    }
    if t.starts_with("when (") || t.starts_with("when {") || t.contains("?.let") || t.contains("listOf(") {
        s.add(KOTLIN, 3);
    }

    // Swift.
    if t.contains("some View") || t.contains(": View {") {
        s.add(SWIFT, 6);
    }
    for k in ["@State", "@Published", "@MainActor", "@objc", "@main", "extension ", "protocol ", "init("] {
        if t.starts_with(k) {
            s.add(SWIFT, 3);
        }
    }

    // C and C++.
    if t.contains("std::") || t.starts_with("template<") || t.starts_with("template <") {
        s.add(CPP, 5);
    }
    if ["public:", "private:", "protected:"].contains(&t) || t.contains("cout <<") || t.contains("nullptr") {
        s.add(CPP, 4);
    }
    if t.starts_with("virtual ") || t.starts_with("auto ") {
        s.add(CPP, 2);
    }
    if t.contains("printf(") || t.contains("malloc(") || t.contains("free(") {
        s.add(C, 3);
    }
    if t.contains("NULL")
        || t.starts_with("struct ") && word_at(t, 7).is_some_and(|w| w.starts_with(char::is_lowercase))
    {
        s.add(C, 1);
    }
    let types = ["char", "double", "float", "int", "long", "size_t", "static", "typedef", "unsigned", "void"];
    if types.contains(&word) && t.len() > word.len() && !t.contains(" = function") {
        s.add(C_FAMILY, 2);
    }
    if t.contains("int main(") {
        s.add(C_FAMILY, 3);
    }

    // YAML, TOML, and INI.
    if first && t == "---" {
        s.add(YAML, 3);
    }
    if let Some((key, value)) = yaml_key(t) {
        let dashed = t.starts_with("- ");
        if value.is_empty() {
            s.key = Some(indent);
        } else if key.as_bytes()[0].is_ascii_lowercase() {
            s.add(YAML, if dashed { 2 } else { 1 });
        }
        if ["apiVersion", "kind", "services", "jobs", "steps", "runs-on", "uses", "image"].contains(&key) {
            s.add(YAML, 2);
        }
    }
    if let Some(inner) = t.strip_prefix("[[").and_then(|r| r.strip_suffix("]]")) {
        if section(inner) {
            s.add(TOML, 5);
        }
    } else if let Some(inner) = t.strip_prefix('[').and_then(|r| r.strip_suffix(']'))
        && section(inner)
    {
        if inner.as_bytes()[0].is_ascii_uppercase() {
            s.add(INI, 2);
            s.add(CSHARP, 1);
        } else {
            s.add(INI, 3);
            s.add(TOML, 3);
        }
    }
    if let Some((key, value)) = t.split_once('=')
        && !t.ends_with(';')
        && !value.starts_with('=')
        && !key.ends_with(['!', '<', '>', ':', '+', '?', '='])
        && !value.contains('(')
    {
        let key = key.trim_end();
        let value = value.trim();
        let quoted = key.len() > 1 && key.starts_with('"') && key.ends_with('"');
        if (quoted || !key.is_empty() && key.bytes().all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c)))
            && !assignment(t)
            && !value.is_empty()
        {
            let typed = value.starts_with(['"', '\'', '[', '{'])
                || value.starts_with(|c: char| c.is_ascii_digit())
                || value.starts_with('-') && value[1..].starts_with(|c: char| c.is_ascii_digit())
                || value == "true"
                || value == "false";
            if typed {
                s.add(TOML, 2);
                s.add(INI, 1);
            } else {
                s.add(INI, 2);
            }
        }
    }

    // What ends a C-family statement or opens its block.
    if t.ends_with(';') {
        s.add(SEMI, 1);
    } else if t.ends_with('{') {
        s.add(BRACE, 1);
    }
}

/// What an `import` line scores, by what it imports.
fn import(s: &mut Scores, module: &str) {
    let m = module.trim_end();
    if m.starts_with(['"', '(']) {
        s.add(GO, 4);
    } else if m.contains(" from ") || m.starts_with(['{', '*', '\'']) {
        s.add(SCRIPT, 4);
    } else if m.ends_with(';') {
        s.add(JAVA, 3);
    } else if m.starts_with(|c: char| c.is_ascii_uppercase()) && !m.contains(['.', ' ']) {
        s.add(SWIFT, 4);
    } else if m.contains('.') && m.rsplit('.').next().is_some_and(|w| w.starts_with(|c: char| c.is_ascii_uppercase())) {
        s.add(KOTLIN, 3);
    } else if m.bytes().all(|c| c.is_ascii_alphanumeric() || b"_., ".contains(&c)) {
        s.add(PYTHON, 3);
    }
}

/// Whether `t` starts with `prefix`, in any case.
fn starts_any_case(t: &str, prefix: &str) -> bool {
    t.get(..prefix.len()).is_some_and(|h| h.eq_ignore_ascii_case(prefix))
}

/// The word that starts `at` bytes into `t`, if any.
fn word_at(t: &str, at: usize) -> Option<&str> {
    t.get(at..)?.split([' ', '(', '{', ':', '<']).next().filter(|w| !w.is_empty())
}

/// Whether `t` opens with a console's prompt: `$ ls`, `% make`, `user@host:~/code$ git pull`,
/// `[user@host code]# make`, or `PS C:\> dir`.
fn prompt(t: &str) -> bool {
    let command = |rest: &str| rest.starts_with(|c: char| c.is_ascii_alphabetic() || ".~/".contains(c));
    if let Some(rest) = t.strip_prefix("$ ").or_else(|| t.strip_prefix("% ")) {
        return command(rest);
    }
    if t.starts_with("PS ") && t.get(..64.min(t.len())).is_some_and(|h| h.contains(">")) {
        return true;
    }
    let head = &t.as_bytes()[..t.len().min(64)];
    for (i, w) in head.windows(2).enumerate() {
        if (w == b"$ " || w == b"# ") && i > 0 {
            let who = &t[..i];
            let user = who.contains('@') && !who.contains(' ') || who.starts_with('[') && who.ends_with(']');
            return user && command(&t[i + 2..]);
        }
    }
    false
}

/// Whether `t`, a line at the margin, is a Makefile rule's head: its targets, a `:`, and what
/// they need, but no `=` after.
fn makefile_rule(t: &str) -> bool {
    let Some(at) = t.find(':') else { return false };
    let targets = &t[..at];
    let after = &t[at + 1..];
    let keyword = ["class", "else", "try", "finally", "if", "for", "while", "with", "def", "elif", "except", "case"];
    !targets.is_empty()
        && !after.starts_with('=')
        && !after.starts_with(":=")
        && !after.contains('=')
        && (after.is_empty() || after.starts_with([' ', '\t', ':']))
        && !keyword.contains(&targets.split(' ').next().unwrap_or(""))
        && targets.bytes().all(|c| c.is_ascii_alphanumeric() || b"_./%$() -".contains(&c))
        && (!targets.contains('(') || targets.contains("$("))
}

/// A YAML key first on `t`, maybe after a sequence's `- `, and the value after it.
fn yaml_key(t: &str) -> Option<(&str, &str)> {
    let r = t.strip_prefix("- ").unwrap_or(t);
    let at = r.find(':')?;
    let (key, rest) = (&r[..at], &r[at + 1..]);
    let ok = key.as_bytes().first().is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        && key.bytes().all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        && (rest.is_empty() || rest.starts_with(' '))
        && !t.ends_with([';', '{', ','])
        && !["else", "try", "finally", "default", "public", "private", "protected"].contains(&key);
    ok.then(|| (key, rest.trim()))
}

/// Whether `inner`, what a line's brackets hold, names a TOML table or an INI section.
fn section(inner: &str) -> bool {
    inner.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
        && inner.bytes().all(|c| c.is_ascii_alphanumeric() || b"_.-\" ".contains(&c))
}

/// Whether `t` sets a shell variable: `NAME=value`, in capitals, with no space before its value.
fn assignment(t: &str) -> bool {
    let Some((name, value)) = t.split_once('=') else { return false };
    !name.is_empty()
        && name.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
        && name.as_bytes()[0].is_ascii_uppercase()
        && !value.starts_with([' ', '='])
}

/// Whether `t` calls a Rust macro: `println!(`, `vec![`.
fn macro_call(t: &str) -> bool {
    let b = t.as_bytes();
    b.windows(3).enumerate().any(|(i, w)| {
        w[1] == b'!' && (w[2] == b'(' || w[2] == b'[') && (w[0].is_ascii_alphanumeric() || w[0] == b'_') && {
            let start = b[..=i].iter().rposition(|c| !(c.is_ascii_alphanumeric() || *c == b'_')).map_or(0, |p| p + 1);
            b[start].is_ascii_lowercase()
        }
    })
}

/// Whether `t`, a line that ends with `{`, is a CSS rule's selector.
fn selector(t: &str, word: &str) -> bool {
    let head = t.trim_end_matches('{').trim_end();
    if head.contains(['=', ';']) || head.contains("()") {
        return false;
    }
    let tag = word.split([',', ':', '.', '>', '[', '{']).next().unwrap_or("");
    head.starts_with(['.', ':', '*'])
        || head.starts_with('#') && head[1..].starts_with(|c: char| c.is_ascii_alphabetic()) && !head.contains('(')
        || HTML_TAGS.contains(&tag) && !head.contains('(')
}

/// Whether `t` is a CSS declaration: `color: red;`, `--gap: 4px;`.
fn declaration(t: &str) -> bool {
    let Some((name, value)) = t.split_once(':') else { return false };
    let custom = name.starts_with("--");
    let name = name.trim_start_matches('-');
    !name.is_empty()
        && t.ends_with(';')
        && !value.trim().is_empty()
        && name.bytes().all(|c| c.is_ascii_lowercase() || c == b'-')
        && (custom || name.contains('-') || CSS_PROPERTIES.contains(&name))
}

/// Whether `t` has code's shape, whatever its language: it ends or starts as code does, or holds
/// an operator prose rarely does.
fn code_shaped(t: &str) -> bool {
    t.ends_with([';', '{', '}', '(', '[', ')'])
        || t.ends_with("=>")
        || t.starts_with(['}', ')', ']', '<', '@', '#'])
        || t.starts_with("//")
        || t.starts_with("/*")
        || t.starts_with("return")
        || ["()", "==", "!=", "->", "::", "+=", "&&", "||", " = "].iter().any(|op| t.contains(op))
}

/// Whether `t` reads as prose: four words or more, three in four of them words of letters.
fn prose_like(t: &str) -> bool {
    let mut words = 0;
    let mut lettered = 0;
    for w in t.split_whitespace() {
        words += 1;
        let w = w.trim_matches(|c: char| "()\"'“”‘’.,;:!?*_".contains(c));
        if !w.is_empty() && w.chars().all(|c| c.is_alphabetic() || c == '\'' || c == '’' || c == '-') {
            lettered += 1;
        }
    }
    words >= 4 && lettered >= 4 && lettered * 4 >= words * 3
}

/// The tokens of `text` where it is one JSON value and nothing else: each token's range, its
/// strings, numbers, and words as written. None where it is not.
pub(super) fn json(text: &str) -> Option<Vec<(usize, usize)>> {
    #[derive(PartialEq)]
    enum Want {
        Value,
        ValueOrClose,
        KeyOrClose,
        Key,
        Colon,
        CommaOrClose,
        Done,
    }
    let b = text.as_bytes();
    let mut tokens = Vec::new();
    let mut stack: Vec<u8> = Vec::new();
    let mut want = Want::Value;
    let mut i = space(b, 0);
    while i < b.len() {
        let c = b[i];
        let end = match c {
            b'{' | b'[' | b'}' | b']' | b':' | b',' => i + 1,
            b'"' => {
                let end = string_end(b, i)?;
                if b[i..end].iter().any(|&c| c < 0x20) {
                    return None;
                }
                end
            }
            b'-' | b'0'..=b'9' => {
                let end = i + 1 + b[i + 1..].iter().take_while(|c| c.is_ascii_digit() || b".eE+-".contains(c)).count();
                if !b[end - 1].is_ascii_digit() {
                    return None;
                }
                end
            }
            b'a'..=b'z' => {
                let end = i + b[i..].iter().take_while(|c| c.is_ascii_lowercase()).count();
                if !matches!(&text[i..end], "true" | "false" | "null") {
                    return None;
                }
                end
            }
            _ => return None,
        };
        let close = |stack: &mut Vec<u8>| if stack.is_empty() { Want::Done } else { Want::CommaOrClose };
        want = match (want, c) {
            (Want::Value | Want::ValueOrClose, b'{') => {
                stack.push(b'{');
                Want::KeyOrClose
            }
            (Want::Value | Want::ValueOrClose, b'[') => {
                stack.push(b'[');
                Want::ValueOrClose
            }
            (Want::Value | Want::ValueOrClose, b'"' | b'-' | b'0'..=b'9' | b'a'..=b'z') => close(&mut stack),
            (Want::KeyOrClose | Want::Key, b'"') => Want::Colon,
            (Want::Colon, b':') => Want::Value,
            (Want::CommaOrClose, b',') if stack.last() == Some(&b'{') => Want::Key,
            (Want::CommaOrClose, b',') => Want::Value,
            (Want::KeyOrClose | Want::CommaOrClose, b'}') if stack.last() == Some(&b'{') => {
                stack.pop();
                close(&mut stack)
            }
            (Want::ValueOrClose | Want::CommaOrClose, b']') if stack.last() == Some(&b'[') => {
                stack.pop();
                close(&mut stack)
            }
            _ => return None,
        };
        tokens.push((i, end));
        i = space(b, end);
    }
    (want == Want::Done).then_some(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each scored name is a language's, in the table's order, so a tie goes by that order.
    #[test]
    fn the_scored_languages_are_in_the_tables_order() {
        let at = |n: &str| LANGUAGES.iter().position(|l| l.name == n).unwrap_or_else(|| panic!("{n} is no language"));
        assert!(SCORED.windows(2).all(|w| at(w[0]) < at(w[1])));
    }

    #[test]
    fn json_reads_one_value_and_nothing_else() {
        assert!(json("{\"a\": [1, -2.5e3, true, null, {}], \"b\": \"x\\\"y\"}").is_some());
        assert!(json("[]").is_some());
        for bad in ["{\"a\" 1}", "{\"a\": 1,}", "[1 2]", "{} {}", "{\"a\": tru}", "{a: 1}", "[\"a\nb\"]", "[1-]"] {
            assert!(json(bad).is_none(), "{bad}");
        }
    }
}
