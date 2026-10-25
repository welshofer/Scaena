//! Code on a slide, colored by its language (ADR-0029, PLAN 2.130): the languages Scaena knows by
//! name, and the scanners that split a code text into tokens of 16 kinds for the theme to color.
//!
//! [`highlight`] reads a text in a language and gives its [`Token`]s: in order, each a run of the
//! text in one [`Class`], together the whole text. A deck stores no token and no color. The engine
//! colors a code text from its words and its language each time it lays the text out, so a fix to
//! a scanner changes how code draws, and the spans in `tests/code/` say how.
//!
//! The scanners are written by hand from tables, with no dependency and no regular expression:
//! - **The C family**: one scanner, which a table drives for each language. C, C++, C#, Go, Java,
//!   JavaScript with JSX, TypeScript and TSX, Kotlin, Swift, Rust, Dart, Scala, Zig, Objective-C,
//!   and Protocol Buffers.
//! - **JSON**, which reads JSON with comments and JSON5 as leniently.
//! - **Python, Ruby, Shell, PowerShell, and SQL**, each by a scanner of its own (PLAN 2.134).
//! - **A console session**: its prompts, the commands after them read as Shell or as PowerShell,
//!   and what they print.
//! - **Data, config, and markup** (PLAN 2.135): YAML, TOML, INI, HCL, and GraphQL; a Dockerfile,
//!   which reads its commands as Shell or PowerShell, and a Makefile, which reads its recipes as
//!   Shell; a diff; HTML and XML, by one scanner; and CSS and SCSS, by one.
//! - **Embedded languages** (PLAN 2.145): HTML's `<script>` and `<style>` read as JavaScript and
//!   CSS, or as their `type` and `lang` say; Markdown, each fenced block in its own language; PHP,
//!   its code between `<?php` and `?>` and HTML around it; and Vue's and Svelte's components,
//!   each part in its language and each expression as JavaScript. A part in another language is
//!   colored as that language colors it alone ([`embed`]), at most [`MAX_NESTING`] deep.
//! - **The generic scanner**, for a language Scaena has no scanner for: its strings, numbers,
//!   comments, and brackets.
//!
//! What each keeps to:
//! - **ASCII classes.** It tells bytes apart by ASCII alone, and any other character continues a
//!   word. So a token ends on an ASCII byte or at the text's end, always a character's boundary,
//!   and nothing depends on the toolchain's Unicode tables.
//! - **Linear and total.** Each step reads a byte, or closes what a byte read before opened, so a
//!   scanner neither panics nor runs on. What never closes runs to the end in its class; a string
//!   that cannot run past its line ends at the line's end.
//! - **Bounded.** At most [`MAX_NESTING`] interpolations, JSX elements, and JSX expressions stand
//!   open inside one another, and as many heredocs wait for their bodies; as many parts in another
//!   language stand inside one another, the next one in its container's text; the lines past the
//!   first [`MAX_LINES`] are one plain token.
//! - **Few tokens.** Neighbors of one class are one token, and whitespace goes to the token before
//!   it.
//!
//! A paste's language is [`detect`]ed, and the paste made [`tidy`], by the editor, never at layout
//! (PLAN 2.133).

mod clike;
mod console;
mod css;
mod detect;
mod diff;
mod dockerfile;
mod generic;
mod graphql;
mod hcl;
mod ini;
mod json;
mod keyed;
mod makefile;
mod markdown;
mod markup;
mod powershell;
mod python;
mod ruby;
mod scan;
mod shell;
mod sql;
pub mod tables;
mod tidy;
mod toml;
mod yaml;

use markup::Markup;

pub use detect::{Detected, Hints, JSON_WHOLE, LIKELY, READ_BYTES, READ_LINES, detect, vscode_mode};
pub use keyed::{Key, Keyed, keyed};
pub use tidy::{Tidied, tidy};

use std::fmt;

/// The most lines of a code text that are colored: the lines after them are one plain token.
pub const MAX_LINES: usize = 1_000;

/// The most interpolations (`${…}` in a template and in HCL, `%{…}` in HCL, `\(…)` in a Swift
/// string, `{…}` in an f-string, `#{…}` in Ruby and SCSS, `$(…)` in Shell, PowerShell, and Make),
/// JSX elements, and JSX expressions that stand open inside one another, and the most heredocs
/// that wait for their bodies. What would open one more is read as what it stands in: a ninth
/// `${` as its string's own characters.
pub const MAX_NESTING: usize = 8;

/// A kind of token, which the theme gives a color, a weight, and an italic (ADR-0029).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    /// What no kind claims: names, a JSX element's words, and the lines past [`MAX_LINES`].
    Plain,
    /// A word the language keeps for itself: `fn`, `class`, `return`.
    Keyword,
    /// A type: one the language names (`int`, `u8`), one a declaration names (`struct point`),
    /// or a name that starts with a capital where the language writes types so (`HashMap`).
    Type,
    /// A function: one a declaration names, one called (`parse(`), or a Rust macro (`println!`).
    Function,
    /// A string, a character, or a regular expression, with its quotes.
    String,
    /// A number, in any base, with its suffix: `0x1F`, `1_000u32`, `1.5e-3`.
    Number,
    /// A constant: `true`, `null`, or a name in capitals (`MAX_LEN`).
    Constant,
    /// A comment.
    Comment,
    /// An operator: `+`, `=`, `&&`, `->`.
    Operator,
    /// Brackets, commas, semicolons, colons, and points.
    Punctuation,
    /// A member reached by a `.` (`point.x`), or a key in JSON.
    Property,
    /// An attribute or an annotation (`#[derive`, `@Override`), or a JSX element's attribute.
    Attribute,
    /// A markup element's name: `div` in JSX's `<div>`.
    Tag,
    /// A variable the language defines (`this`, `self`), or one a string interpolates (`$name`).
    Variable,
    /// A directive: `#include`, `#if`.
    Meta,
    /// A line a diff adds.
    Inserted,
    /// A line a diff takes away.
    Deleted,
}

impl Class {
    /// The 16 kinds a theme colors, every class but plain, in the order a theme lists them.
    pub const KINDS: [Class; 16] = [
        Class::Keyword,
        Class::Type,
        Class::Function,
        Class::String,
        Class::Number,
        Class::Constant,
        Class::Comment,
        Class::Operator,
        Class::Punctuation,
        Class::Property,
        Class::Attribute,
        Class::Tag,
        Class::Variable,
        Class::Meta,
        Class::Inserted,
        Class::Deleted,
    ];

    /// Its name, as a theme keys its look: `keyword`, `type`, and `plain`.
    pub fn name(self) -> &'static str {
        match self {
            Class::Plain => "plain",
            Class::Keyword => "keyword",
            Class::Type => "type",
            Class::Function => "function",
            Class::String => "string",
            Class::Number => "number",
            Class::Constant => "constant",
            Class::Comment => "comment",
            Class::Operator => "operator",
            Class::Punctuation => "punctuation",
            Class::Property => "property",
            Class::Attribute => "attribute",
            Class::Tag => "tag",
            Class::Variable => "variable",
            Class::Meta => "meta",
            Class::Inserted => "inserted",
            Class::Deleted => "deleted",
        }
    }
}

/// A run of a text in one class. It ends `end` bytes into the text, and starts where the token
/// before it ends, or at the text's start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub end: usize,
    pub class: Class,
}

/// A language Scaena knows by name.
pub struct Language {
    /// Its name, which a deck writes, in lowercase: `rust`, `cpp`.
    pub name: &'static str,
    /// The other names it goes by, lowercase: `rs`, `c++`. A name or an alias names one language.
    pub aliases: &'static [&'static str],
    /// What a person calls it: `C++`, `Plain Text`.
    pub said: &'static str,
    /// The extensions of its files, lowercase and without their point.
    pub extensions: &'static [&'static str],
    /// The names its files go by whole, as written: `Dockerfile`, `Makefile`.
    pub files: &'static [&'static str],
    /// The programs a script's first line runs it with: `node` in `#!/usr/bin/env node`.
    pub interpreters: &'static [&'static str],
    /// What colors it.
    pub(crate) scan: Scan,
}

impl PartialEq for Language {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for Language {}

impl fmt::Debug for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Language").field(&self.name).finish()
    }
}

/// What colors a language.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Scan {
    /// Nothing: plain text.
    Plain,
    /// The C family's scanner, by the language's table.
    CLike(&'static clike::Spec),
    /// JSON's.
    Json,
    /// Python's.
    Python,
    /// Ruby's.
    Ruby,
    /// Shell's: bash, sh, and zsh.
    Shell,
    /// A console session's, which reads its commands as Shell or PowerShell.
    Console,
    /// PowerShell's.
    PowerShell,
    /// SQL's.
    Sql,
    /// YAML's.
    Yaml,
    /// TOML's.
    Toml,
    /// A Dockerfile's, which reads its commands as Shell or PowerShell.
    Dockerfile,
    /// A diff's.
    Diff,
    /// INI's.
    Ini,
    /// HCL's.
    Hcl,
    /// GraphQL's.
    GraphQl,
    /// A Makefile's, which reads its recipes as Shell.
    Makefile,
    /// HTML's.
    Html,
    /// XML's.
    Xml,
    /// CSS's.
    Css,
    /// SCSS's.
    Scss,
    /// Markdown's, which reads each fenced block in its own language.
    Markdown,
    /// PHP's: its code between `<?php` and `?>`, HTML around it.
    Php,
    /// A Vue component's: HTML, its `{{ }}` and directives JavaScript.
    Vue,
    /// A Svelte component's: HTML, its `{ }` JavaScript.
    Svelte,
}

/// How a code text's language colors it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// By the language's own scanner.
    Known(&'static Language),
    /// Not at all: plain text is one plain token.
    Plain,
    /// By the generic scanner, which colors what most languages write alike: a language Scaena
    /// has no scanner for.
    Generic,
}

/// The languages Scaena knows by name, in the order of their names, so [`language`] finds one by
/// binary search: each that a scanner of its own colors, and plain text, which nothing colors. A
/// name none of them goes by colors by the generic scanner.
pub static LANGUAGES: &[Language] = &[
    Language {
        name: "c",
        aliases: &[],
        said: "C",
        extensions: &["c", "h"],
        files: &[],
        interpreters: &[],
        scan: Scan::CLike(&clike::C),
    },
    Language {
        name: "console",
        aliases: &["sh-session", "shell-session", "shellsession"],
        said: "Console",
        extensions: &["sh-session"],
        files: &[],
        interpreters: &[],
        scan: Scan::Console,
    },
    Language {
        name: "cpp",
        aliases: &["c++", "cc", "cxx", "hpp"],
        said: "C++",
        extensions: &["c++", "cc", "cpp", "cxx", "h++", "hh", "hpp", "hxx", "ipp", "tpp"],
        files: &[],
        interpreters: &[],
        scan: Scan::CLike(&clike::CPP),
    },
    Language {
        name: "csharp",
        aliases: &["c#", "cs"],
        said: "C#",
        extensions: &["cs", "csx"],
        files: &[],
        interpreters: &[],
        scan: Scan::CLike(&clike::CSHARP),
    },
    Language {
        name: "css",
        aliases: &[],
        said: "CSS",
        extensions: &["css"],
        files: &[],
        interpreters: &[],
        scan: Scan::Css,
    },
    Language {
        name: "dart",
        aliases: &[],
        said: "Dart",
        extensions: &["dart"],
        files: &[],
        interpreters: &["dart"],
        scan: Scan::CLike(&clike::DART),
    },
    Language {
        name: "diff",
        aliases: &["patch", "udiff"],
        said: "Diff",
        extensions: &["diff", "patch", "rej"],
        files: &[],
        interpreters: &[],
        scan: Scan::Diff,
    },
    Language {
        name: "dockerfile",
        aliases: &["containerfile", "docker"],
        said: "Dockerfile",
        extensions: &["containerfile", "dockerfile"],
        files: &["Containerfile", "Dockerfile"],
        interpreters: &[],
        scan: Scan::Dockerfile,
    },
    Language {
        name: "go",
        aliases: &["golang"],
        said: "Go",
        extensions: &["go"],
        files: &[],
        interpreters: &[],
        scan: Scan::CLike(&clike::GO),
    },
    Language {
        name: "graphql",
        aliases: &["gql"],
        said: "GraphQL",
        extensions: &["gql", "graphql", "graphqls"],
        files: &[],
        interpreters: &[],
        scan: Scan::GraphQl,
    },
    Language {
        name: "hcl",
        aliases: &["terraform", "tf", "tfvars"],
        said: "HCL",
        extensions: &["hcl", "nomad", "tf", "tfvars"],
        files: &[],
        interpreters: &[],
        scan: Scan::Hcl,
    },
    Language {
        name: "html",
        aliases: &["htm", "xhtml"],
        said: "HTML",
        extensions: &["htm", "html", "xht", "xhtml"],
        files: &[],
        interpreters: &[],
        scan: Scan::Html,
    },
    Language {
        name: "ini",
        aliases: &["cfg", "conf", "dosini", "editorconfig", "properties"],
        said: "INI",
        extensions: &["cfg", "conf", "ini", "properties"],
        files: &[".editorconfig", ".gitconfig", ".gitmodules"],
        interpreters: &[],
        scan: Scan::Ini,
    },
    Language {
        name: "java",
        aliases: &[],
        said: "Java",
        extensions: &["java"],
        files: &[],
        interpreters: &[],
        scan: Scan::CLike(&clike::JAVA),
    },
    Language {
        name: "javascript",
        aliases: &["cjs", "js", "mjs"],
        said: "JavaScript",
        extensions: &["cjs", "js", "mjs"],
        files: &[],
        interpreters: &["bun", "node", "nodejs"],
        scan: Scan::CLike(&clike::JAVASCRIPT),
    },
    Language {
        name: "json",
        aliases: &["jsonl"],
        said: "JSON",
        extensions: &["geojson", "json", "jsonl"],
        files: &[],
        interpreters: &[],
        scan: Scan::Json,
    },
    Language {
        name: "json5",
        aliases: &[],
        said: "JSON5",
        extensions: &["json5"],
        files: &[],
        interpreters: &[],
        scan: Scan::Json,
    },
    Language {
        name: "jsonc",
        aliases: &[],
        said: "JSON with Comments",
        extensions: &["jsonc"],
        files: &[],
        interpreters: &[],
        scan: Scan::Json,
    },
    Language {
        name: "jsx",
        aliases: &[],
        said: "JSX",
        extensions: &["jsx"],
        files: &[],
        interpreters: &[],
        scan: Scan::CLike(&clike::JAVASCRIPT),
    },
    Language {
        name: "kotlin",
        aliases: &["kt", "kts"],
        said: "Kotlin",
        extensions: &["kt", "kts"],
        files: &[],
        interpreters: &[],
        scan: Scan::CLike(&clike::KOTLIN),
    },
    Language {
        name: "makefile",
        aliases: &["make", "mk"],
        said: "Makefile",
        extensions: &["mak", "make", "mk"],
        files: &["GNUmakefile", "Makefile", "makefile"],
        interpreters: &["gmake", "make"],
        scan: Scan::Makefile,
    },
    Language {
        name: "markdown",
        aliases: &["gfm", "md", "mdown", "mkd"],
        said: "Markdown",
        extensions: &["markdown", "md", "mdown", "mkd"],
        files: &[],
        interpreters: &[],
        scan: Scan::Markdown,
    },
    Language {
        name: "objc",
        aliases: &["obj-c", "objective-c", "objectivec"],
        said: "Objective-C",
        extensions: &["m", "mm"],
        files: &[],
        interpreters: &[],
        scan: Scan::CLike(&clike::OBJC),
    },
    Language {
        name: "php",
        aliases: &[],
        said: "PHP",
        extensions: &["php", "phtml"],
        files: &[],
        interpreters: &["php"],
        scan: Scan::Php,
    },
    Language {
        name: "plain",
        aliases: &["plaintext", "text", "txt"],
        said: "Plain Text",
        extensions: &["text", "txt"],
        files: &[],
        interpreters: &[],
        scan: Scan::Plain,
    },
    Language {
        name: "powershell",
        aliases: &["posh", "ps", "ps1", "pwsh"],
        said: "PowerShell",
        extensions: &["ps1", "psd1", "psm1"],
        files: &[],
        interpreters: &["powershell", "pwsh"],
        scan: Scan::PowerShell,
    },
    Language {
        name: "protobuf",
        aliases: &["proto"],
        said: "Protocol Buffers",
        extensions: &["proto"],
        files: &[],
        interpreters: &[],
        scan: Scan::CLike(&clike::PROTOBUF),
    },
    Language {
        name: "python",
        aliases: &["py", "python3"],
        said: "Python",
        extensions: &["py", "pyi", "pyw"],
        files: &[],
        interpreters: &["pypy", "pypy3", "python", "python2", "python3"],
        scan: Scan::Python,
    },
    Language {
        name: "ruby",
        aliases: &["rb"],
        said: "Ruby",
        extensions: &["gemspec", "rake", "rb", "ru"],
        files: &[],
        interpreters: &["jruby", "ruby"],
        scan: Scan::Ruby,
    },
    Language {
        name: "rust",
        aliases: &["rs"],
        said: "Rust",
        extensions: &["rs"],
        files: &[],
        interpreters: &[],
        scan: Scan::CLike(&clike::RUST),
    },
    Language {
        name: "scala",
        aliases: &["sc"],
        said: "Scala",
        extensions: &["sc", "scala"],
        files: &[],
        interpreters: &["scala"],
        scan: Scan::CLike(&clike::SCALA),
    },
    Language {
        name: "scss",
        aliases: &[],
        said: "SCSS",
        extensions: &["scss"],
        files: &[],
        interpreters: &[],
        scan: Scan::Scss,
    },
    Language {
        name: "shell",
        aliases: &["bash", "sh", "shell-script", "shellscript", "zsh"],
        said: "Shell",
        extensions: &["bash", "ksh", "sh", "zsh"],
        files: &[],
        interpreters: &["ash", "bash", "dash", "ksh", "sh", "zsh"],
        scan: Scan::Shell,
    },
    Language {
        name: "sql",
        aliases: &["mysql", "pgsql", "plpgsql", "postgres", "postgresql", "sqlite", "tsql"],
        said: "SQL",
        extensions: &["ddl", "pgsql", "sql"],
        files: &[],
        interpreters: &["psql", "sqlite3"],
        scan: Scan::Sql,
    },
    Language {
        name: "svelte",
        aliases: &[],
        said: "Svelte",
        extensions: &["svelte"],
        files: &[],
        interpreters: &[],
        scan: Scan::Svelte,
    },
    Language {
        name: "swift",
        aliases: &[],
        said: "Swift",
        extensions: &["swift"],
        files: &[],
        interpreters: &["swift"],
        scan: Scan::CLike(&clike::SWIFT),
    },
    Language {
        name: "toml",
        aliases: &[],
        said: "TOML",
        extensions: &["toml"],
        files: &["Cargo.lock", "Pipfile"],
        interpreters: &[],
        scan: Scan::Toml,
    },
    Language {
        name: "tsx",
        aliases: &[],
        said: "TSX",
        extensions: &["tsx"],
        files: &[],
        interpreters: &[],
        scan: Scan::CLike(&clike::TSX),
    },
    Language {
        name: "typescript",
        aliases: &["cts", "mts", "ts"],
        said: "TypeScript",
        extensions: &["cts", "mts", "ts"],
        files: &[],
        interpreters: &["deno", "ts-node"],
        scan: Scan::CLike(&clike::TYPESCRIPT),
    },
    Language {
        name: "vue",
        aliases: &[],
        said: "Vue",
        extensions: &["vue"],
        files: &[],
        interpreters: &[],
        scan: Scan::Vue,
    },
    Language {
        name: "xml",
        aliases: &["plist", "rss", "svg", "xsd", "xsl", "xslt"],
        said: "XML",
        extensions: &["atom", "csproj", "plist", "rss", "svg", "xml", "xsd", "xsl", "xslt"],
        files: &[],
        interpreters: &[],
        scan: Scan::Xml,
    },
    Language {
        name: "yaml",
        aliases: &["yml"],
        said: "YAML",
        extensions: &["yaml", "yml"],
        files: &[],
        interpreters: &[],
        scan: Scan::Yaml,
    },
    Language {
        name: "zig",
        aliases: &[],
        said: "Zig",
        extensions: &["zig"],
        files: &[],
        interpreters: &[],
        scan: Scan::CLike(&clike::ZIG),
    },
];

/// The language `name` names, by its name or an alias, in any case.
pub fn language(name: &str) -> Option<&'static Language> {
    let name = name.to_ascii_lowercase();
    match LANGUAGES.binary_search_by(|l| l.name.cmp(name.as_str())) {
        Ok(i) => LANGUAGES.get(i),
        Err(_) => LANGUAGES.iter().find(|l| l.aliases.contains(&name.as_str())),
    }
}

/// How a code text in the language `name` is colored: by the language's own scanner, as plain
/// text, or, where Scaena knows no language by that name, by the generic scanner.
pub fn lang(name: &str) -> Lang {
    match language(name) {
        Some(language) if matches!(language.scan, Scan::Plain) => Lang::Plain,
        Some(language) => Lang::Known(language),
        None => Lang::Generic,
    }
}

/// The tokens of `text` in `lang`: in order, each a run of the text in one class, and together
/// every byte of it, the lines past [`MAX_LINES`] one plain token. An empty text has none.
pub fn highlight(text: &str, lang: Lang) -> Vec<Token> {
    let head = text.get(..scan::cut(text, MAX_LINES)).unwrap_or(text);
    let mut out = scan::Spans::new(head);
    color(head, lang, 0, &mut out);
    let mut tokens = out.finish();
    if head.len() < text.len() {
        match tokens.last_mut() {
            Some(last) if last.class == Class::Plain => last.end = text.len(),
            _ => tokens.push(Token { end: text.len(), class: Class::Plain }),
        }
    }
    tokens
}

/// Writes the tokens of `text[from..to]`, a part of `text` in another language, as `lang` colors
/// that part alone: a `<script>`'s JavaScript, a fenced block's Rust. `depth` is how many parts
/// in another language stand around this one; at [`MAX_NESTING`], the part is its container's
/// text, plain.
fn embed<'t>(text: &'t str, from: usize, to: usize, lang: Lang, depth: usize, out: &mut scan::Spans<'t>) {
    embed_with(text, from, to, depth, out, |part, inner, depth| color(part, lang, depth, inner));
}

/// Writes the tokens of `text[from..to]` as `read` writes the part's alone, given the part, its
/// tokens, and how many parts in another language stand around it with it; plain at
/// [`MAX_NESTING`]. White space the part's scanner leaves plain at its start goes to the token
/// before it, as white space does anywhere.
fn embed_with<'t>(
    text: &'t str,
    from: usize,
    to: usize,
    depth: usize,
    out: &mut scan::Spans<'t>,
    read: impl for<'p> FnOnce(&'p str, &mut scan::Spans<'p>, usize),
) {
    let part = match text.get(from..to) {
        Some(part) if depth < MAX_NESTING => part,
        _ => return out.push(to, Class::Plain),
    };
    let mut inner = scan::Spans::new(part);
    read(part, &mut inner, depth + 1);
    out.space(from + part.bytes().take_while(|&c| scan::is_space(c)).count());
    for token in inner.finish() {
        out.push(from + token.end, token.class);
    }
}

/// Writes `text`'s tokens in `lang`, `depth` parts in another language deep.
fn color<'t>(head: &'t str, lang: Lang, depth: usize, out: &mut scan::Spans<'t>) {
    match lang {
        Lang::Plain | Lang::Known(Language { scan: Scan::Plain, .. }) => {}
        Lang::Generic => generic::scan(head, out),
        Lang::Known(Language { scan: Scan::CLike(spec), .. }) => clike::scan(head, spec, out),
        Lang::Known(Language { scan: Scan::Json, .. }) => json::scan(head, out),
        Lang::Known(Language { scan: Scan::Python, .. }) => python::scan(head, out),
        Lang::Known(Language { scan: Scan::Ruby, .. }) => ruby::scan(head, out),
        Lang::Known(Language { scan: Scan::Shell, .. }) => shell::scan(head, out),
        Lang::Known(Language { scan: Scan::Console, .. }) => console::scan(head, out),
        Lang::Known(Language { scan: Scan::PowerShell, .. }) => powershell::scan(head, out),
        Lang::Known(Language { scan: Scan::Sql, .. }) => sql::scan(head, out),
        Lang::Known(Language { scan: Scan::Yaml, .. }) => yaml::scan(head, out),
        Lang::Known(Language { scan: Scan::Toml, .. }) => toml::scan(head, out),
        Lang::Known(Language { scan: Scan::Dockerfile, .. }) => dockerfile::scan(head, out),
        Lang::Known(Language { scan: Scan::Diff, .. }) => diff::scan(head, out),
        Lang::Known(Language { scan: Scan::Ini, .. }) => ini::scan(head, out),
        Lang::Known(Language { scan: Scan::Hcl, .. }) => hcl::scan(head, out),
        Lang::Known(Language { scan: Scan::GraphQl, .. }) => graphql::scan(head, out),
        Lang::Known(Language { scan: Scan::Makefile, .. }) => makefile::scan(head, out),
        Lang::Known(Language { scan: Scan::Html, .. }) => markup::scan(head, out, Markup::Html, depth),
        Lang::Known(Language { scan: Scan::Xml, .. }) => markup::scan(head, out, Markup::Xml, depth),
        Lang::Known(Language { scan: Scan::Php, .. }) => markup::scan(head, out, Markup::Php, depth),
        Lang::Known(Language { scan: Scan::Vue, .. }) => markup::scan(head, out, Markup::Vue, depth),
        Lang::Known(Language { scan: Scan::Svelte, .. }) => markup::scan(head, out, Markup::Svelte, depth),
        Lang::Known(Language { scan: Scan::Css, .. }) => css::scan(head, out, false),
        Lang::Known(Language { scan: Scan::Scss, .. }) => css::scan(head, out, true),
        Lang::Known(Language { scan: Scan::Markdown, .. }) => markdown::scan(head, out, depth),
    }
}
