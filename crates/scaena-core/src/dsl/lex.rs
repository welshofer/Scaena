//! The `.scn` lexer: source text to logical lines of tokens (SPEC §4.2).
//!
//! A logical line is one physical line, except that a triple-quoted string runs on until
//! its closing `"""`. Indentation is spaces only. `#` starts a comment, outside strings.

use super::DslError;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// A bare word: `[A-Za-z_][A-Za-z0-9_.-]*`.
    Ident(String),
    /// A string, quoted or triple-quoted, unescaped.
    Str(String),
    /// A number as its JSON text; `12cu` is `12`.
    Num(String),
    /// A time: the number as written, and whether its unit is `s` (else `ms`). Whether it
    /// is milliseconds or seconds in the deck depends on the field (SPEC §4.2).
    Time(String, bool),
    /// A percentage, kept as the string it stands for: `50%`.
    Percent(String),
    /// An aspect ratio, kept as the string it stands for: `16:9`.
    Ratio(String),
    /// `1920x1080`.
    Dim(String, String),
    /// `1-7`, inside a call.
    Range(String, String),
    /// `@name`.
    Ref(String),
    Colon,
    Comma,
    Minus,
    /// `=`: a choreography item written as its value.
    Equals,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    LParen,
    RParen,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    /// Byte offsets into the source.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Leading spaces.
    pub indent: usize,
    pub tokens: Vec<Token>,
    /// 1-based, the line the logical line starts on.
    pub line: usize,
    /// Byte offset of the line's first character.
    pub start: usize,
    /// A `#` comment: the whole line, or what follows its tokens. One space after `#`
    /// is the comment's syntax, not its text.
    pub comment: Option<String>,
}

/// The logical lines of `source`, blank lines dropped.
pub fn lex(source: &str) -> Result<Vec<Line>, DslError> {
    Lexer { src: source, pos: 0, line: 1 }.lines()
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    line: usize,
}

impl Lexer<'_> {
    fn lines(&mut self) -> Result<Vec<Line>, DslError> {
        let mut out = Vec::new();
        while self.pos < self.src.len() {
            let start = self.pos;
            let line = self.line;
            let indent = self.src[self.pos..].bytes().take_while(|b| *b == b' ').count();
            self.pos += indent;
            if self.peek() == Some('\t') {
                return Err(self.error(self.pos, "indent with spaces, not tabs"));
            }
            let mut tokens = Vec::new();
            let comment = loop {
                self.skip_spaces();
                match self.peek() {
                    None => break None,
                    Some('\n') => break None,
                    Some('\r') if self.src[self.pos..].starts_with("\r\n") => break None,
                    Some('#') => {
                        let end = self.src[self.pos..].find('\n').map_or(self.src.len(), |i| self.pos + i);
                        let text = self.src[self.pos + 1..end].trim_end_matches('\r');
                        self.pos = end;
                        break Some(text.strip_prefix(' ').unwrap_or(text).to_string());
                    }
                    Some(_) => tokens.push(self.token()?),
                }
            };
            if self.src[self.pos..].starts_with("\r\n") {
                self.pos += 2;
            } else if self.peek() == Some('\n') {
                self.pos += 1;
            }
            self.line += 1;
            if !tokens.is_empty() || comment.is_some() {
                out.push(Line { indent, tokens, line, start, comment });
            }
        }
        Ok(out)
    }

    fn token(&mut self) -> Result<Token, DslError> {
        let start = self.pos;
        let c = self.peek().expect("a character");
        let tok = match c {
            ':' => self.one(Tok::Colon),
            '=' => self.one(Tok::Equals),
            ',' => self.one(Tok::Comma),
            '[' => self.one(Tok::LBracket),
            ']' => self.one(Tok::RBracket),
            '{' => self.one(Tok::LBrace),
            '}' => self.one(Tok::RBrace),
            '(' => self.one(Tok::LParen),
            ')' => self.one(Tok::RParen),
            '"' if self.src[self.pos..].starts_with("\"\"\"") => Tok::Str(self.triple()?),
            '"' => Tok::Str(self.string()?),
            '@' => {
                self.pos += 1;
                let name = self.word();
                if name.is_empty() {
                    return Err(self.error(start, "`@` needs a name after it"));
                }
                Tok::Ref(format!("@{name}"))
            }
            '-' if self.after(1).is_some_and(|c| c.is_ascii_digit()) => self.number()?,
            '-' => self.one(Tok::Minus),
            c if c.is_ascii_digit() => self.number()?,
            c if c.is_ascii_alphabetic() || c == '_' => Tok::Ident(self.word()),
            '/' => {
                return Err(self.error(start, "unexpected `/`: a path is a string, so quote it, as in \"data/q3.csv\""));
            }
            c => return Err(self.error(start, &format!("unexpected `{c}`"))),
        };
        Ok(Token { tok, start, end: self.pos })
    }

    fn one(&mut self, tok: Tok) -> Tok {
        self.pos += 1;
        tok
    }

    /// `[A-Za-z0-9_.-]*` from here.
    fn word(&mut self) -> String {
        let len = self.src[self.pos..]
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
            .count();
        let word = self.src[self.pos..self.pos + len].to_string();
        self.pos += len;
        word
    }

    /// A number, with what may follow one: a unit, `%`, `x` and a height, `-` and a
    /// range's end, or `:` and a ratio's.
    fn number(&mut self) -> Result<Tok, DslError> {
        let start = self.pos;
        let text = self.numeral().to_string();
        let _: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| self.error(start, &format!("`{text}` is not a number")))?;
        let rest = &self.src[self.pos..];
        let ends_word = |n: usize| rest[n..].chars().next().is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
        if rest.starts_with('%') {
            self.pos += 1;
            return Ok(Tok::Percent(format!("{text}%")));
        }
        if rest.starts_with('x') && rest[1..].starts_with(|c: char| c.is_ascii_digit()) {
            self.pos += 1;
            let height = self.numeral().to_string();
            return Ok(Tok::Dim(text, height));
        }
        if rest.starts_with('-') && rest[1..].starts_with(|c: char| c.is_ascii_digit()) {
            self.pos += 1;
            let end = self.numeral().to_string();
            return Ok(Tok::Range(text, end));
        }
        if rest.starts_with(':') && rest[1..].starts_with(|c: char| c.is_ascii_digit()) {
            self.pos += 1;
            let den = self.numeral().to_string();
            return Ok(Tok::Ratio(format!("{text}:{den}")));
        }
        if rest.starts_with("cu") && ends_word(2) {
            self.pos += 2;
            return Ok(Tok::Num(text));
        }
        for (unit, seconds) in [("ms", false), ("s", true)] {
            if rest.starts_with(unit) && ends_word(unit.len()) {
                self.pos += unit.len();
                return Ok(Tok::Time(text, seconds));
            }
        }
        if rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            return Err(self.error(start, &format!("`{text}` has a unit this language does not know (ms, s, cu, %)")));
        }
        Ok(Tok::Num(text))
    }

    /// `-?[0-9]+(.[0-9]+)?([eE][+-]?[0-9]+)?` from here.
    fn numeral(&mut self) -> &str {
        let start = self.pos;
        let b = self.src.as_bytes();
        let mut i = self.pos;
        if b.get(i) == Some(&b'-') {
            i += 1;
        }
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if b.get(i) == Some(&b'.') && b.get(i + 1).is_some_and(u8::is_ascii_digit) {
            i += 1;
            while b.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
        }
        if matches!(b.get(i), Some(b'e' | b'E')) {
            let mut j = i + 1;
            if matches!(b.get(j), Some(b'+' | b'-')) {
                j += 1;
            }
            if b.get(j).is_some_and(u8::is_ascii_digit) {
                while b.get(j).is_some_and(u8::is_ascii_digit) {
                    j += 1;
                }
                i = j;
            }
        }
        self.pos = i;
        &self.src[start..i]
    }

    /// A JSON string on one line.
    fn string(&mut self) -> Result<String, DslError> {
        let start = self.pos;
        let mut i = self.pos + 1;
        let b = self.src.as_bytes();
        loop {
            match b.get(i) {
                None | Some(b'\n') => return Err(self.error(start, "this string is not closed on its line")),
                Some(b'\\') => i += 2,
                Some(b'"') => break,
                Some(_) => i += 1,
            }
        }
        self.pos = i + 1;
        serde_json::from_str(&self.src[start..self.pos]).map_err(|e| self.error(start, &format!("bad string: {e}")))
    }

    /// `"""`, the rest of its line empty, then lines up to one that holds only `"""`. That
    /// line's indentation comes off every line before it; the lines join with newlines.
    fn triple(&mut self) -> Result<String, DslError> {
        let start = self.pos;
        self.pos += 3;
        let eol = self.src[self.pos..].find('\n').map_or(self.src.len(), |i| self.pos + i);
        if !self.src[self.pos..eol].trim().is_empty() {
            return Err(
                self.error(start, "a `\"\"\"` string starts on the next line: nothing may follow the opening `\"\"\"`")
            );
        }
        self.pos = (eol + 1).min(self.src.len());
        let mut lines: Vec<(usize, &str)> = Vec::new();
        loop {
            if self.pos >= self.src.len() {
                return Err(self.error(start, "this `\"\"\"` string is not closed"));
            }
            self.line += 1;
            let eol = self.src[self.pos..].find('\n').map_or(self.src.len(), |i| self.pos + i);
            let text = self.src[self.pos..eol].trim_end_matches('\r');
            let line_start = self.pos;
            self.pos = eol;
            if text.trim_start() == "\"\"\"" {
                let strip = text.len() - text.trim_start().len();
                let mut out = Vec::new();
                for (at, line) in lines {
                    if line.is_empty() {
                        out.push(String::new());
                    } else if line.len() >= strip && line[..strip].bytes().all(|b| b == b' ') {
                        out.push(line[strip..].to_string());
                    } else {
                        return Err(self.error(at, "this line is indented less than the closing `\"\"\"`"));
                    }
                }
                return Ok(out.join("\n"));
            }
            lines.push((line_start, text));
            self.pos = (eol + 1).min(self.src.len());
        }
    }

    fn skip_spaces(&mut self) {
        while self.peek() == Some(' ') {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn after(&self, n: usize) -> Option<char> {
        self.src[self.pos..].chars().nth(n)
    }

    fn error(&self, at: usize, message: &str) -> DslError {
        DslError::at(self.src, at, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().flat_map(|l| l.tokens).map(|t| t.tok).collect()
    }

    #[test]
    fn units_and_shapes_keep_their_meaning() {
        assert_eq!(
            toks("40ms 1.5s 12cu 50% 1920x1080 1-7 16:9 -3 1.0 2e3"),
            [
                Tok::Time("40".into(), false),
                Tok::Time("1.5".into(), true),
                Tok::Num("12".into()),
                Tok::Percent("50%".into()),
                Tok::Dim("1920".into(), "1080".into()),
                Tok::Range("1".into(), "7".into()),
                Tok::Ratio("16:9".into()),
                Tok::Num("-3".into()),
                Tok::Num("1.0".into()),
                Tok::Num("2e3".into()),
            ]
        );
    }

    #[test]
    fn words_refs_strings_and_comments() {
        let lines = lex("  title text \"Q3 \\\"Review\\\"\" role:x-height @q3 # a note\n# alone\n").unwrap();
        assert_eq!(lines[0].indent, 2);
        assert_eq!(lines[0].comment.as_deref(), Some("a note"));
        assert_eq!(lines[0].tokens.len(), 7);
        assert_eq!(lines[0].tokens[2].tok, Tok::Str("Q3 \"Review\"".into()));
        assert_eq!(lines[0].tokens[5].tok, Tok::Ident("x-height".into()));
        assert_eq!(lines[1].tokens, []);
        assert_eq!(lines[1].comment.as_deref(), Some("alone"));
    }

    #[test]
    fn triple_quoted_strings_lose_the_closing_lines_indent() {
        let lines = lex("notes \"\"\"\n    one\n\n      two\n    \"\"\"\nnext\n").unwrap();
        assert_eq!(lines[0].tokens[1].tok, Tok::Str("one\n\n  two".into()));
        assert_eq!(lines[1].line, 6);
    }

    #[test]
    fn mistakes_say_where() {
        let e = lex("a\n\tb").unwrap_err();
        assert_eq!((e.line, e.col), (2, 1));
        assert!(lex("x 12px").unwrap_err().message.contains("unit"));
        assert!(lex("x \"open").unwrap_err().message.contains("not closed"));
        assert!(lex("theme:themes/dusk.json").unwrap_err().message.contains("quote it"));
    }
}
