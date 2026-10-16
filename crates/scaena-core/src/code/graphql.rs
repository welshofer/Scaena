//! GraphQL, its queries and its schemas (PLAN 2.135).
//!
//! - **Definitions**: the words that begin one (`query`, `mutation`, `subscription`, `fragment`,
//!   `type`, `interface`, `union`, `enum`, `input`, `scalar`, `schema`, `extend`, `directive`) are
//!   keywords, and so are `on`, `implements`, and `repeatable` in a definition's head. An
//!   operation's or a fragment's name is a function, and a type's a type.
//! - **Fields**: a field selected or defined, and an alias, are properties; an argument's or a
//!   parameter's name an attribute; a variable (`$id`) a variable; a directive (`@include`) an
//!   attribute; and a fragment spread (`...UserFields`) a function.
//! - **Types**: names are types where a type stands: after a field's or a parameter's `:`
//!   (`id: ID!`, `$first: Int = 10`, `[User!]!`), and after `on`, `implements`, `&`, and a union's
//!   `=` and `|`.
//! - **Values**: numbers; strings, `"…"` to its close or its line's end and `"""…"""` over lines;
//!   `true`, `false`, `null`, an enum's values, and a directive's locations, constants; and an
//!   input object's keys, properties.
//! - `#` begins a comment; `!`, `=`, `|`, `&`, and `...` are operators.
//!
//! The kinds of the outermost [`MAX_NESTING`] brackets open inside one another are kept, and those
//! inside them are taken to be of the innermost kind kept.
//!
//! [`MAX_NESTING`]: super::MAX_NESTING

use super::Class;
use super::scan::{Nest, Spans, byte, is_break, is_space, is_word, is_word_start, line_end, number_end, space_end};
use super::tables::{Table, words};

/// What a bracket holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Within {
    /// A selection set, `{ user { name } }`: fields.
    Selection,
    /// A type's fields, `type User { name: String }`.
    Fields,
    /// An enum's values.
    Values,
    /// A schema's operations, `schema { query: Query }`.
    Schema,
    /// Arguments, `(id: 4)`: names, then values.
    Arguments,
    /// Parameters, `(first: Int = 10)` and `($id: ID!)`: names, then types and defaults.
    Parameters,
    /// An input object, `{ a: 1 }`: keys, then values.
    Object,
    /// A list of values, `[1, 2]`.
    List,
    /// A list type, `[String!]`.
    ListType,
}

/// What the definition being read is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Def {
    None,
    /// A query, a mutation, a subscription, or a fragment, whose braces hold a selection.
    Operation,
    /// A type, an interface, or an input, whose braces hold fields.
    Type,
    Enum,
    Union,
    Schema,
    Directive,
}

/// What the next name is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Want {
    /// A definition's keyword, at the top.
    Definition,
    /// A definition's name, of this class.
    Name(Class),
    /// A definition's head after its name: `on`, `implements`, `repeatable`.
    Head,
    /// What the bracket holds names: a field, an argument's name, a key.
    Key,
    Type,
    Value,
    /// After `...`: `on`, or a fragment's name.
    Spread,
    /// A directive's locations, after its `on`.
    Locations,
}

/// Writes `text`'s tokens as GraphQL.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let b = text.as_bytes();
    let len = b.len();
    let mut pos = 0;
    let mut nest = Nest::new();
    let mut def = Def::None;
    let mut want = Want::Definition;
    // Whether a directive's name came last, so a `(` opens its arguments.
    let mut directive = false;
    while pos < len {
        let c = b[pos];
        if is_space(c) {
            let to = space_end(b, pos, len);
            out.space(to);
            pos = to;
            continue;
        }
        let next = byte(b, pos + 1);
        let after = std::mem::take(&mut directive);
        // What the bracket open goes on with, once a name or a value is read.
        let rest = |nest: &Nest<Within>, def: Def| match nest.top() {
            None if def == Def::None => Want::Definition,
            None => Want::Head,
            Some(Within::List) => Want::Value,
            Some(Within::ListType) => Want::Type,
            Some(_) => Want::Key,
        };
        let (to, class) = match c {
            b'#' => (line_end(b, pos, len), Class::Comment),
            b'"' => {
                if want == Want::Value {
                    want = rest(&nest, def);
                }
                (string_end(b, pos), Class::String)
            }
            b'$' | b'@' if is_word_start(next) => {
                directive = c == b'@';
                if want == Want::Value {
                    want = rest(&nest, def);
                }
                (word_end(b, pos + 1), if c == b'$' { Class::Variable } else { Class::Attribute })
            }
            b'.' if next == b'.' && byte(b, pos + 2) == b'.' => {
                want = Want::Spread;
                (pos + 3, Class::Operator)
            }
            b'-' | b'0'..=b'9' if c != b'-' || next.is_ascii_digit() => {
                if want == Want::Value {
                    want = rest(&nest, def);
                }
                (number_end(b, pos + usize::from(c == b'-'), len, false), Class::Number)
            }
            _ if is_word_start(c) => {
                let to = word_end(b, pos);
                let word = &b[pos..to];
                // A definition begins at the top, where the one before has no braces to end it too.
                let begins = matches!(want, Want::Definition | Want::Head | Want::Locations) && nest.is_empty();
                let class = match want {
                    _ if begins && DEFINITIONS.holds(word) => {
                        (def, want) = match word {
                            b"query" | b"mutation" | b"subscription" | b"fragment" => {
                                (Def::Operation, Want::Name(Class::Function))
                            }
                            b"type" | b"interface" | b"input" | b"scalar" => (Def::Type, Want::Name(Class::Type)),
                            b"enum" => (Def::Enum, Want::Name(Class::Type)),
                            b"union" => (Def::Union, Want::Name(Class::Type)),
                            b"schema" => (Def::Schema, Want::Head),
                            b"directive" => (Def::Directive, Want::Head),
                            _ => (Def::None, Want::Definition),
                        };
                        Class::Keyword
                    }
                    Want::Definition => Class::Plain,
                    Want::Name(class) => {
                        want = Want::Head;
                        class
                    }
                    Want::Head => match word {
                        b"on" => {
                            want = if def == Def::Directive { Want::Locations } else { Want::Type };
                            Class::Keyword
                        }
                        b"implements" => {
                            want = Want::Type;
                            Class::Keyword
                        }
                        b"repeatable" => Class::Keyword,
                        _ => Class::Plain,
                    },
                    Want::Type => {
                        want = rest(&nest, def);
                        Class::Type
                    }
                    Want::Value => {
                        want = rest(&nest, def);
                        Class::Constant
                    }
                    Want::Spread if word == b"on" => {
                        want = Want::Type;
                        Class::Keyword
                    }
                    Want::Spread => {
                        want = rest(&nest, def);
                        Class::Function
                    }
                    Want::Locations => Class::Constant,
                    Want::Key => match nest.top() {
                        Some(Within::Values) => Class::Constant,
                        Some(Within::Schema) => Class::Keyword,
                        Some(Within::Arguments | Within::Parameters) => Class::Attribute,
                        _ => Class::Property,
                    },
                };
                (to, class)
            }
            b':' => {
                want = match nest.top() {
                    Some(Within::Selection) => Want::Key,
                    Some(Within::Arguments | Within::Object) => Want::Value,
                    _ => Want::Type,
                };
                (pos + 1, Class::Punctuation)
            }
            b'=' => {
                want = if nest.is_empty() && def == Def::Union { Want::Type } else { Want::Value };
                (pos + 1, Class::Operator)
            }
            b'|' | b'&' => {
                if want != Want::Locations {
                    want = Want::Type;
                }
                (pos + 1, Class::Operator)
            }
            b'{' => {
                let within = if want == Want::Value {
                    Within::Object
                } else if nest.is_empty() {
                    match def {
                        Def::Type => Within::Fields,
                        Def::Enum => Within::Values,
                        Def::Schema => Within::Schema,
                        _ => Within::Selection,
                    }
                } else {
                    Within::Selection
                };
                if nest.is_empty() && def == Def::None {
                    def = Def::Operation;
                }
                nest.push(within);
                want = rest(&nest, def);
                (pos + 1, Class::Punctuation)
            }
            b'(' => {
                let parameters = nest.is_empty() || nest.top() == Some(Within::Fields);
                let within = if after && !(nest.is_empty() && def == Def::Directive) {
                    Within::Arguments
                } else if parameters {
                    Within::Parameters
                } else {
                    Within::Arguments
                };
                nest.push(within);
                want = Want::Key;
                (pos + 1, Class::Punctuation)
            }
            b'[' => {
                nest.push(if want == Want::Type { Within::ListType } else { Within::List });
                want = rest(&nest, def);
                (pos + 1, Class::Punctuation)
            }
            b'}' | b')' | b']' => {
                nest.pop();
                if c == b'}' && nest.is_empty() {
                    def = Def::None;
                }
                want = rest(&nest, def);
                (pos + 1, Class::Punctuation)
            }
            b',' => (pos + 1, Class::Punctuation),
            _ if c.is_ascii_punctuation() => (pos + 1, Class::Operator),
            _ => (pos + 1, Class::Plain),
        };
        let to = to.max(pos + 1);
        out.push(to, class);
        pos = to;
    }
}

/// Where the name at `at` ends.
fn word_end(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while i < b.len() && is_word(b[i]) {
        i += 1;
    }
    i
}

/// Where the string at `at` ends: a block string, `"""…"""`, over lines to its close, `\"""` a
/// quote in it; else past its close, a backslash escaping the byte after it, or at its line's end.
fn string_end(b: &[u8], at: usize) -> usize {
    let len = b.len();
    if b[at..].starts_with(b"\"\"\"") {
        let mut i = at + 3;
        while i < len {
            if b[i..].starts_with(b"\\\"\"\"") {
                i += 4;
            } else if b[i..].starts_with(b"\"\"\"") {
                return i + 3;
            } else {
                i += 1;
            }
        }
        return len;
    }
    let mut i = at + 1;
    while i < len {
        match b[i] {
            c if is_break(c) => return i,
            b'\\' if !is_break(byte(b, i + 1)) => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    len
}

/// The words that begin a definition, sorted, so one is found by binary search.
const DEFINITIONS: Table = words!(
    "graphql/definitions",
    [
        "directive",
        "enum",
        "extend",
        "fragment",
        "input",
        "interface",
        "mutation",
        "query",
        "scalar",
        "schema",
        "subscription",
        "type",
        "union"
    ]
);

/// The tables above, in the file they are handed over in (`code::tables`).
pub(super) const WORDS: &[Table] = &[DEFINITIONS];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_definitions_are_sorted() {
        assert!(DEFINITIONS.windows(2).all(|w| w[0] < w[1]));
    }
}
