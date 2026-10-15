//! SQL, read by a scanner of its own (PLAN 2.134): what the databases write alike, and keywords
//! in any case.
//!
//! - **Strings** in `'…'`, a quote doubled in them (`'it''s'`), maybe after a prefix: `N'…'`,
//!   `X'…'`, `B'…'`, `U&'…'`, and `E'…'`, in which a backslash escapes too. Each runs on over
//!   lines.
//! - **Dollar-quoted bodies**, `$$…$$` and `$tag$…$tag$`, are one string each, whatever they
//!   hold: a function's body, comments and quotes and all.
//! - **Quoted names**, `"…"`, `` `…` ``, and `[…]`, a quote doubled in them, are plain names,
//!   or properties after a `.`; each ends at its line's end where it does not close sooner.
//! - **Comments**: `--` and `/* */`, and `#` first on a line, as MySQL writes them.
//! - **Words**: keywords, types, and constants (`TRUE`, `NULL`, `CURRENT_DATE`) in any case. A
//!   name before a `(` is a function (`COUNT(*)`), and so is a keyword a function shares its name
//!   with, a `(` just after it (`LEFT(name, 3)`); a type with its size is a type
//!   (`VARCHAR(255)`). But a table's name before its columns is a name: after `TABLE`, `INTO`,
//!   `REFERENCES`, `AS`, and the like, and after `FROM`, `JOIN`, and `ON` where a space comes
//!   before the `(` (`FROM generate_series(1, 3)` calls). After `::` a word is a type (`'5'::int`), and
//!   after a `.` a property.
//! - `@name`, `@@name`, `$1`, `:name`, `?1`, and a `?` that stands for a value are variables; a
//!   `?` before a quote, and `?|` and `?&`, are Postgres's operators.

use super::Class;
use super::scan::{
    Spans, blanks_end, block_comment_end, holds, is_break, is_space, is_word, is_word_start, line_end, number_end,
    space_end, word_end,
};

/// The longest tag a dollar quote takes: `$body$`.
const TAG: usize = 64;

/// How far a name in brackets is looked through for its `]`: `[Order Details]`.
const BRACKETED: usize = 128;

/// What the keyword before a word says it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Names {
    /// Nothing: a word before a `(` is a function.
    Nothing,
    /// A table, before its columns: after `TABLE`, `INTO`, `REFERENCES`, `AS`.
    Table,
    /// A table, or a function that gives rows: after `FROM`, `JOIN`, `ON`.
    Rows,
}

/// Writes `text`'s tokens as SQL.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let b = text.as_bytes();
    let mut pos = 0;
    // What came last: a `.`, after which a word is a property; a `::`, after which it is a type;
    // a keyword that names a table.
    let (mut dot, mut cast, mut names) = (false, false, Names::Nothing);
    // Whether only whitespace has come since the line began.
    let mut first = true;
    while pos < b.len() {
        let c = b[pos];
        if is_space(c) {
            let to = space_end(b, pos, b.len());
            first |= b[pos..to].iter().any(|&c| is_break(c));
            out.space(to);
            pos = to;
            continue;
        }
        let (to, class) = token(text, pos, dot, cast, names, first);
        let next = at(b, pos + 1);
        dot = c == b'.' && to == pos + 1;
        cast = c == b':' && next == b':';
        names = match text.get(pos..to) {
            Some(word) if class == Class::Keyword && holds(TABLES, word) => Names::Table,
            Some(word) if class == Class::Keyword && holds(ROWS, word) => Names::Rows,
            _ => Names::Nothing,
        };
        first = false;
        let to = to.max(pos + 1);
        out.push(to, class);
        pos = to;
    }
}

/// The byte at `i`, or 0 past the text's end.
fn at(b: &[u8], i: usize) -> u8 {
    b.get(i).copied().unwrap_or(0)
}

/// The token at `pos`: where it ends, and its class.
fn token(text: &str, pos: usize, dot: bool, cast: bool, names: Names, first: bool) -> (usize, Class) {
    let b = text.as_bytes();
    let end = b.len();
    let c = b[pos];
    let next = at(b, pos + 1);
    let after_word = pos > 0 && is_word(b[pos - 1]);
    let name = if dot { Class::Property } else { Class::Plain };
    if (c == b'-' && next == b'-') || (c == b'#' && first) {
        return (line_end(b, pos, end), Class::Comment);
    }
    if c == b'/' && next == b'*' {
        return (block_comment_end(b, pos, end, false), Class::Comment);
    }
    if c == b'\'' {
        return (quoted(b, pos + 1, false), Class::String);
    }
    if c == b'"' || c == b'`' {
        return (identifier(b, pos + 1, c), name);
    }
    if c == b'['
        && let Some(close) = bracketed(b, pos)
    {
        return (close, name);
    }
    if c == b'$' && !after_word {
        if next.is_ascii_digit() {
            return (digits_end(b, pos + 1), Class::Variable);
        }
        if let Some(open) = tag(b, pos) {
            return (dollar_end(b, pos, open), Class::String);
        }
    }
    if c == b'@' && (is_word_start(next) || (next == b'@' && is_word_start(at(b, pos + 2)))) {
        return (word_end(b, pos + 1 + usize::from(next == b'@'), end), Class::Variable);
    }
    if c == b':' && next == b':' {
        return (pos + 2, Class::Operator);
    }
    if c == b':' && is_word_start(next) && !after_word {
        return (word_end(b, pos + 1, end), Class::Variable);
    }
    if c == b'?' && next.is_ascii_digit() {
        return (digits_end(b, pos + 1), Class::Variable);
    }
    if c == b'?' && !matches!(next, b'|' | b'&') && at(b, blanks_end(b, pos + 1, end)) != b'\'' {
        return (pos + 1, Class::Variable);
    }
    if c.is_ascii_digit() || (c == b'.' && next.is_ascii_digit() && !after_word) {
        return (number_end(b, pos, end, false), Class::Number);
    }
    if is_word_start(c) {
        return word(text, pos, dot, cast, names);
    }
    match c {
        b'(' | b')' | b'[' | b']' | b',' | b';' | b'.' => (pos + 1, Class::Punctuation),
        _ if c.is_ascii_punctuation() => (pos + 1, Class::Operator),
        _ => (pos + 1, Class::Plain),
    }
}

/// A word: a string after its prefix, a keyword, a type, a constant, a function, or a name.
fn word(text: &str, pos: usize, dot: bool, cast: bool, names: Names) -> (usize, Class) {
    let b = text.as_bytes();
    let end = b.len();
    let mut j = word_end(b, pos, end);
    // A name may hold `$` after its first byte: `price$usd`.
    while at(b, j) == b'$' && is_word(at(b, j + 1)) {
        j = word_end(b, j + 1, end);
    }
    // A string after its prefix: `N'…'`, `E'…'`, `X'…'`, `B'…'`, and `U&'…'`.
    let letter = b[pos].to_ascii_lowercase();
    if j == pos + 1 && at(b, j) == b'\'' && matches!(letter, b'n' | b'e' | b'x' | b'b') {
        return (quoted(b, j + 1, letter == b'e'), Class::String);
    }
    if j == pos + 1 && letter == b'u' && at(b, j) == b'&' && at(b, j + 1) == b'\'' {
        return (quoted(b, j + 2, false), Class::String);
    }
    let word = text.get(pos..j).unwrap_or_default();
    let called = at(b, j) == b'(';
    let call = match names {
        Names::Nothing => at(b, blanks_end(b, j, end)) == b'(',
        Names::Table => false,
        Names::Rows => called,
    };
    let class = if dot {
        if call { Class::Function } else { Class::Property }
    } else if cast {
        Class::Type
    } else if holds(TYPES, word) {
        // Called with no size, a type's name is a function's: `DATE('now')`.
        if called && !at(b, blanks_end(b, j + 1, end)).is_ascii_digit() { Class::Function } else { Class::Type }
    } else if holds(KEYWORDS, word) {
        if called && holds(CALLED, word) { Class::Function } else { Class::Keyword }
    } else if holds(CONSTANTS, word) {
        Class::Constant
    } else if call {
        Class::Function
    } else {
        Class::Plain
    };
    (j, class)
}

/// Where a string from `from` ends: past the quote that closes it, a quote doubled staying in
/// it; with `escapes`, a backslash escaping the byte after it.
fn quoted(b: &[u8], from: usize, escapes: bool) -> usize {
    let mut i = from;
    while i < b.len() {
        match b[i] {
            b'\\' if escapes => i += 2,
            b'\'' if at(b, i + 1) == b'\'' => i += 2,
            b'\'' => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

/// Where a name in `quote` from `from` ends: past the quote that closes it, a quote doubled
/// staying in it, or at its line's end.
fn identifier(b: &[u8], from: usize, quote: u8) -> usize {
    let mut i = from;
    while i < b.len() && !is_break(b[i]) {
        if b[i] == quote {
            if at(b, i + 1) != quote {
                return i + 1;
            }
            i += 1;
        }
        i += 1;
    }
    i
}

/// Where a name in brackets at `pos` ends, past its `]`, if it closes on its line within
/// [`BRACKETED`] bytes and holds no comma, quote, or bracket: `[Order Details]`, not
/// `ARRAY[a, b]` or `prices[1]`.
fn bracketed(b: &[u8], pos: usize) -> Option<usize> {
    if !is_word_start(at(b, pos + 1)) {
        return None;
    }
    let limit = b.len().min(pos + BRACKETED);
    let close =
        (pos + 1..limit).find(|&k| matches!(b[k], b']' | b'[' | b',' | b'(' | b')' | b'\'' | b'"' | b'\n' | b'\r'))?;
    (b[close] == b']').then_some(close + 1)
}

/// Where the digits from `from` end.
fn digits_end(b: &[u8], from: usize) -> usize {
    let mut i = from;
    while at(b, i).is_ascii_digit() {
        i += 1;
    }
    i
}

/// Where a dollar quote that opens at `pos` ends its opener, past the second `$`: `$$`, or a tag
/// of at most [`TAG`] bytes between two (`$body$`).
fn tag(b: &[u8], pos: usize) -> Option<usize> {
    let mut k = pos + 1;
    if is_word_start(at(b, k)) {
        let limit = b.len().min(k + TAG);
        while k < limit && is_word(b[k]) {
            k += 1;
        }
    }
    (at(b, k) == b'$').then_some(k + 1)
}

/// Where the body a dollar quote opens, from `pos` to `open`, ends: past the same quote again, or
/// at the text's end.
fn dollar_end(b: &[u8], pos: usize, open: usize) -> usize {
    let quote = &b[pos..open];
    let mut i = open;
    while i < b.len() {
        if b[i] == b'$' && b[i..].starts_with(quote) {
            return i + quote.len();
        }
        i += 1;
    }
    b.len()
}

// The tables, each sorted and in lowercase, so a word is found by binary search in any case
// (`every_table_is_sorted`).

const KEYWORDS: &[&str] = &[
    "add",
    "after",
    "all",
    "alter",
    "always",
    "analyze",
    "and",
    "any",
    "array",
    "as",
    "asc",
    "at",
    "auto_increment",
    "autoincrement",
    "before",
    "begin",
    "between",
    "by",
    "call",
    "cascade",
    "case",
    "check",
    "collate",
    "column",
    "commit",
    "conflict",
    "constraint",
    "create",
    "cross",
    "current",
    "cursor",
    "database",
    "declare",
    "default",
    "deferrable",
    "deferred",
    "delete",
    "desc",
    "describe",
    "distinct",
    "do",
    "drop",
    "duplicate",
    "each",
    "else",
    "elsif",
    "end",
    "engine",
    "escape",
    "except",
    "exception",
    "exclude",
    "exec",
    "execute",
    "exists",
    "explain",
    "extension",
    "fetch",
    "filter",
    "first",
    "following",
    "for",
    "foreign",
    "from",
    "full",
    "function",
    "generated",
    "glob",
    "go",
    "grant",
    "group",
    "having",
    "identity",
    "if",
    "ilike",
    "immediate",
    "immutable",
    "in",
    "index",
    "initially",
    "inner",
    "insert",
    "instead",
    "intersect",
    "into",
    "is",
    "isnull",
    "join",
    "key",
    "language",
    "last",
    "lateral",
    "left",
    "like",
    "limit",
    "loop",
    "matched",
    "materialized",
    "merge",
    "natural",
    "next",
    "no",
    "not",
    "nothing",
    "notnull",
    "nulls",
    "of",
    "offset",
    "on",
    "only",
    "or",
    "order",
    "ordinality",
    "others",
    "outer",
    "over",
    "partition",
    "pragma",
    "preceding",
    "prepare",
    "primary",
    "procedure",
    "raise",
    "range",
    "recursive",
    "references",
    "regexp",
    "release",
    "rename",
    "replace",
    "restrict",
    "return",
    "returning",
    "returns",
    "revoke",
    "right",
    "rollback",
    "row",
    "rows",
    "savepoint",
    "schema",
    "select",
    "sequence",
    "set",
    "show",
    "similar",
    "some",
    "stored",
    "table",
    "temp",
    "temporary",
    "then",
    "ties",
    "to",
    "top",
    "transaction",
    "trigger",
    "truncate",
    "unbounded",
    "union",
    "unique",
    "unsigned",
    "update",
    "use",
    "using",
    "vacuum",
    "values",
    "view",
    "virtual",
    "volatile",
    "when",
    "where",
    "while",
    "window",
    "with",
    "within",
    "without",
    "zone",
];
/// The keywords a function shares its name with, which a `(` just after makes the function's.
const CALLED: &[&str] = &["if", "left", "replace", "right"];
/// The keywords after which a word names a table, maybe before its columns.
const TABLES: &[&str] =
    &["as", "exists", "index", "into", "recursive", "references", "table", "update", "view", "with"];
/// The keywords after which a word names a table, or calls a function that gives rows.
const ROWS: &[&str] = &["from", "join", "lateral", "on"];
const TYPES: &[&str] = &[
    "bigint",
    "bigserial",
    "binary",
    "bit",
    "blob",
    "bool",
    "boolean",
    "bytea",
    "char",
    "character",
    "cidr",
    "clob",
    "date",
    "datetime",
    "datetime2",
    "decimal",
    "double",
    "float",
    "float4",
    "float8",
    "inet",
    "int",
    "int2",
    "int4",
    "int8",
    "integer",
    "interval",
    "json",
    "jsonb",
    "longtext",
    "macaddr",
    "mediumint",
    "mediumtext",
    "money",
    "nchar",
    "ntext",
    "numeric",
    "nvarchar",
    "precision",
    "real",
    "serial",
    "smallint",
    "smallserial",
    "text",
    "time",
    "timestamp",
    "timestamptz",
    "timetz",
    "tinyint",
    "tinytext",
    "uuid",
    "varbinary",
    "varchar",
    "varying",
    "xml",
];
const CONSTANTS: &[&str] = &[
    "current_date",
    "current_time",
    "current_timestamp",
    "false",
    "localtime",
    "localtimestamp",
    "null",
    "true",
    "unknown",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Each table is sorted and in lowercase, with no word twice, so binary search finds what it
    /// holds in any case; no word is in two of them; and a keyword a function shares its name
    /// with is a keyword.
    #[test]
    fn every_table_is_sorted() {
        let tables = [KEYWORDS, CALLED, TABLES, ROWS, TYPES, CONSTANTS];
        for table in tables {
            assert!(table.windows(2).all(|w| w[0] < w[1]), "not sorted: {table:?}");
            assert!(table.iter().all(|w| w.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')));
        }
        for (k, table) in [KEYWORDS, TYPES, CONSTANTS].into_iter().enumerate() {
            for other in [KEYWORDS, TYPES, CONSTANTS].into_iter().skip(k + 1) {
                assert!(table.iter().all(|w| !other.contains(w)), "a word in two tables: {table:?}");
            }
        }
        assert!(CALLED.iter().chain(TABLES).chain(ROWS).all(|w| KEYWORDS.contains(w)));
    }
}
