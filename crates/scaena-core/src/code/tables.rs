//! The word tables the scanners read (ADR-0029, PLAN 2.132): what the editor's module is handed
//! rather than carrying, as it is a language's hyphenation patterns (ADR-0015).
//!
//! A scanner's words, its keywords, types, constants, and the like, are sorted tables
//! ([`Table`]), each written in the scanner's source with [`words!`] and listed in its module's
//! `WORDS`. Native builds and the player's module compile them in (this crate's `code-tables`
//! feature). The editor's module leaves them out: each scanner's tables are a file of their own,
//! `crates/scaena-core/code/<file>.txt`, which `just bless` writes from the tables compiled in
//! ([`written`]), and its page hands a file over the first time a code text in a language that
//! reads it lays out ([`hand`]). The engine holds the file to its SHA-256, and asks for every file
//! a language reads ([`files`]) before it colors a word of it, so a scanner never reads a table it
//! lacks; one never handed over reads as empty.
//!
//! A file is its tables in order, each a line that names it (`python/keywords`), then its words, a
//! line each, then a blank line.

use super::{Lang, Language, Scan};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Deref;

/// Whether this build has the tables compiled in: natively and in the player's module.
pub const COMPILED: bool = cfg!(feature = "code-tables");

/// The files the tables are handed over in, by name, in order: a scanner's each.
pub const FILES: [&str; 11] =
    ["clike", "css", "dockerfile", "graphql", "hcl", "makefile", "powershell", "python", "ruby", "shell", "sql"];

/// A sorted table of words a scanner reads: its own words where they are compiled in, else those
/// handed over in its file, which it names by its key (`python/keywords`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Table {
    key: &'static str,
    words: Option<&'static [&'static str]>,
}

impl Table {
    /// A table with no words, compiled in everywhere.
    pub(crate) const EMPTY: Table = Table { key: "", words: Some(&[]) };

    /// The table `key` names, with `words` where they are compiled in: what [`words!`] makes.
    pub(crate) const fn new(key: &'static str, words: Option<&'static [&'static str]>) -> Table {
        Table { key, words }
    }
}

impl Table {
    /// Its words: compiled in, or as its file handed them over; none where it was not.
    fn all(self) -> &'static [&'static str] {
        match self.words {
            Some(words) => words,
            None => HANDED.with(|h| h.borrow().get(self.key).copied().unwrap_or(&[])),
        }
    }
}

impl Table {
    /// Whether it holds `word`, as bytes: found by binary search, since it is sorted.
    pub(crate) fn holds(self, word: &[u8]) -> bool {
        self.all().binary_search_by(|w| w.as_bytes().cmp(word)).is_ok()
    }
}

impl Deref for Table {
    type Target = [&'static str];

    fn deref(&self) -> &[&'static str] {
        self.all()
    }
}

impl IntoIterator for Table {
    type Item = &'static &'static str;
    type IntoIter = std::slice::Iter<'static, &'static str>;

    fn into_iter(self) -> Self::IntoIter {
        self.all().iter()
    }
}

/// A table of the words listed, under `key`: compiled in where this crate's `code-tables` feature
/// is on, and otherwise left out, to be handed over in its file.
#[cfg(feature = "code-tables")]
macro_rules! words {
    ($key:literal, [$($word:literal),* $(,)?]) => {
        $crate::code::tables::Table::new($key, Some(&[$($word),*]))
    };
}

/// A table of the words listed, under `key`: compiled in where this crate's `code-tables` feature
/// is on, and otherwise left out, to be handed over in its file.
#[cfg(not(feature = "code-tables"))]
macro_rules! words {
    ($key:literal, [$($word:literal),* $(,)?]) => {
        $crate::code::tables::Table::new($key, None)
    };
}

pub(crate) use words;

thread_local! {
    /// The tables handed over, by key, each kept for the thread's life.
    static HANDED: RefCell<BTreeMap<&'static str, &'static [&'static str]>> = RefCell::default();
    /// The files handed over, by name.
    static GIVEN: RefCell<Vec<&'static str>> = RefCell::default();
}

/// The files the tables `lang` is colored by are in: none for plain text, the generic scanner, and
/// the scanners that read no table of words (JSON, YAML, TOML, a diff, INI, and XML). A
/// Dockerfile reads its commands as Shell or PowerShell, and a Makefile its recipes as Shell.
/// HTML, PHP, Vue, and Svelte read their scripts and expressions by the C family's tables and
/// their styles by CSS's, and Markdown a fenced block in any language, so it is handed them all
/// (PLAN 2.145).
pub fn files(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Plain | Lang::Generic => &[],
        Lang::Known(Language { scan, .. }) => match scan {
            Scan::Plain | Scan::Json => &[],
            Scan::CLike(_) => &["clike"],
            Scan::Python => &["python"],
            Scan::Ruby => &["ruby"],
            Scan::Shell => &["shell"],
            Scan::Console => &["powershell", "shell"],
            Scan::PowerShell => &["powershell"],
            Scan::Sql => &["sql"],
            Scan::Dockerfile => &["dockerfile", "powershell", "shell"],
            Scan::Makefile => &["makefile", "shell"],
            Scan::Hcl => &["hcl"],
            Scan::GraphQl => &["graphql"],
            Scan::Css | Scan::Scss => &["css"],
            Scan::Html | Scan::Php | Scan::Vue | Scan::Svelte => &["clike", "css"],
            Scan::Markdown => &FILES,
            Scan::Yaml | Scan::Toml | Scan::Diff | Scan::Ini | Scan::Xml => &[],
        },
    }
}

/// Whether file `name`'s tables are here: compiled in, or handed over on this thread.
pub fn has(name: &str) -> bool {
    COMPILED || GIVEN.with(|g| g.borrow().contains(&name))
}

/// The tables of file `name`, as `text`: what a page hands over, held to its SHA-256 by the
/// engine, kept for the thread's life, and read in place of the tables a build leaves out. Refused
/// where `name` is no file of tables, or a table in `text` is not of it.
pub fn hand(name: &str, text: &str) -> Result<(), String> {
    let Some(&name) = FILES.iter().find(|f| **f == name) else {
        return Err(format!("`{name}` is no file of code tables"));
    };
    if has(name) {
        return Ok(());
    }
    if let Some((key, _)) = tables(text).find(|(key, _)| key.split_once('/').is_none_or(|(file, _)| file != name)) {
        return Err(format!("`{key}` is no table of `{name}`"));
    }
    // Kept for the thread's life, as the tables a build compiles in are: each file is taken once.
    let kept: &'static str = Box::leak(text.to_owned().into_boxed_str());
    for (key, words) in tables(kept) {
        let words: &'static [&'static str] = Box::leak(words.collect::<Vec<&'static str>>().into_boxed_slice());
        HANDED.with(|h| h.borrow_mut().insert(key, words));
    }
    GIVEN.with(|g| g.borrow_mut().push(name));
    Ok(())
}

/// The tables `text`, a file of them, holds: each its key, then its words.
fn tables(text: &str) -> impl Iterator<Item = (&str, std::str::Lines<'_>)> {
    text.split("\n\n").filter(|b| !b.trim().is_empty()).map(|block| {
        let mut lines = block.lines();
        (lines.next().unwrap_or_default(), lines)
    })
}

/// The tables `text`, a file of them, holds, each its key and its words, as [`hand`] reads them.
pub fn read(text: &str) -> Vec<(&str, Vec<&str>)> {
    tables(text).map(|(key, words)| (key, words.collect())).collect()
}

/// Each table file `name` holds as compiled in, its key and its words: what [`written`] writes.
/// None in a build without them.
pub fn compiled(name: &str) -> Option<Vec<(&'static str, &'static [&'static str])>> {
    let tables: &[Table] = match name {
        "clike" => super::clike::WORDS,
        "css" => super::css::WORDS,
        "dockerfile" => super::dockerfile::WORDS,
        "graphql" => super::graphql::WORDS,
        "hcl" => super::hcl::WORDS,
        "makefile" => super::makefile::WORDS,
        "powershell" => super::powershell::WORDS,
        "python" => super::python::WORDS,
        "ruby" => super::ruby::WORDS,
        "shell" => super::shell::WORDS,
        "sql" => super::sql::WORDS,
        _ => return None,
    };
    COMPILED.then(|| tables.iter().map(|t| (t.key, t.all())).collect())
}

/// File `name`'s text, as its tables compiled in write it: what `just bless` writes into
/// `crates/scaena-core/code/`. None in a build without them, or for no file of tables.
pub fn written(name: &str) -> Option<String> {
    let mut out = String::new();
    for (key, words) in compiled(name)? {
        out.push_str(key);
        out.push('\n');
        for word in words {
            out.push_str(word);
            out.push('\n');
        }
        out.push('\n');
    }
    Some(out)
}
