//! The `.scn` lexer: source text to logical lines of tokens (SPEC §4.2).
//!
//! A logical line is one physical line, except that a block runs on until its fence closes
//! it: a run of three or more `"`, lines, then the same run alone on a line. Indentation is
//! spaces only. `#` starts a comment, outside strings.

use super::DslError;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// A bare word: `[A-Za-z_][A-Za-z0-9_.-]*`.
    Ident(String),
    /// A string, quoted or a block, unescaped.
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
            '"' => match self.src[self.pos..].bytes().take_while(|&b| b == b'"').count() {
                run @ 3.. => Tok::Str(self.block(run)?),
                _ => Tok::Str(self.string()?),
            },
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
        let text = self.checked_numeral()?;
        let rest = &self.src[self.pos..];
        let ends_word = |n: usize| rest[n..].chars().next().is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
        if rest.starts_with('%') {
            self.pos += 1;
            return Ok(Tok::Percent(format!("{text}%")));
        }
        if rest.starts_with('x') && rest[1..].starts_with(|c: char| c.is_ascii_digit()) {
            self.pos += 1;
            let height = self.checked_numeral()?;
            return Ok(Tok::Dim(text, height));
        }
        if rest.starts_with('-') && rest[1..].starts_with(|c: char| c.is_ascii_digit()) {
            self.pos += 1;
            let end = self.checked_numeral()?;
            return Ok(Tok::Range(text, end));
        }
        if rest.starts_with(':') && rest[1..].starts_with(|c: char| c.is_ascii_digit()) {
            self.pos += 1;
            let den = self.checked_numeral()?;
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

    /// A numeral from here, if it is a number JSON holds: every number a token carries is
    /// one, so the parser reads each without asking again.
    fn checked_numeral(&mut self) -> Result<String, DslError> {
        let start = self.pos;
        let text = self.numeral().to_string();
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(_) => Ok(text),
            Err(_) => Err(self.error(start, &format!("`{text}` is not a number"))),
        }
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

    /// A block: its fence, a run of `run` quotes with the rest of its line blank, then lines up
    /// to the first that closes it ([`closes`]). That line's indentation comes off every line
    /// before it; the lines join with line feeds.
    fn block(&mut self, run: usize) -> Result<String, DslError> {
        let start = self.pos;
        let src = self.src;
        let fence = &src[start..start + run];
        self.pos += run;
        let eol = src[self.pos..].find('\n').map_or(src.len(), |i| self.pos + i);
        if !src[self.pos..eol].trim().is_empty() {
            return Err(self.error(
                start,
                &format!("a `{fence}` string starts on the next line: nothing may follow the opening `{fence}`"),
            ));
        }
        self.pos = (eol + 1).min(src.len());
        let mut lines: Vec<(usize, &str)> = Vec::new();
        loop {
            if self.pos >= src.len() {
                // Where it opened: the rest of the source is the string's.
                let message = format!("this `{fence}` string is not closed: a line of `{fence}` alone closes it");
                return Err(DslError::span(src, start, run, &message));
            }
            self.line += 1;
            let eol = src[self.pos..].find('\n').map_or(src.len(), |i| self.pos + i);
            let text = src[self.pos..eol].trim_end_matches('\r');
            let line_start = self.pos;
            self.pos = eol;
            if closes(text, run) {
                let strip = text.len() - run;
                let mut out = Vec::new();
                for (at, line) in lines {
                    if line.is_empty() {
                        out.push(String::new());
                    } else if line.len() >= strip && line.as_bytes()[..strip].iter().all(|&b| b == b' ') {
                        out.push(line[strip..].to_string());
                    } else {
                        return Err(self.error(at, &format!("this line is indented less than the closing `{fence}`")));
                    }
                }
                return Ok(out.join("\n"));
            }
            lines.push((line_start, text));
            self.pos = (eol + 1).min(src.len());
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

/// Whether a line of a block, its carriage return taken off, closes the block, whose fence is
/// `run` quotes: after its indentation of spaces, it is that run and nothing else. A run of
/// another length is a line of the text.
fn closes(line: &str, run: usize) -> bool {
    let rest = line.trim_start_matches(' ');
    rest.len() == run && rest.bytes().all(|b| b == b'"')
}

/// How many quotes fence `text` as a block that reads back as exactly `text`: the fewest, three
/// or more, that no line of it is after its indentation, since that line would close the block.
/// So code with a line of `"""` alone, as a docstring may open on, takes a fence of four. `None`
/// where a block would not keep `text` as it is: a tab or a carriage return in it, or a line
/// that ends in white space, which editors change. Such text is a JSON string.
pub fn fence(text: &str) -> Option<usize> {
    if text.contains(['\t', '\r']) {
        return None;
    }
    // The runs a line of quotes alone rules out, from three up: at most one for each line, so
    // one of these is free.
    let mut taken = vec![false; text.bytes().filter(|&b| b == b'\n').count() + 2];
    for line in text.split('\n') {
        if line != line.trim_end() {
            return None;
        }
        let rest = line.trim_start_matches(' ');
        if rest.len() >= 3 && rest.bytes().all(|b| b == b'"') && rest.len() - 3 < taken.len() {
            taken[rest.len() - 3] = true;
        }
    }
    taken.iter().position(|t| !t).map(|free| free + 3)
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
    fn blocks_lose_the_closing_lines_indent() {
        let lines = lex("notes \"\"\"\n    one\n\n      two\n    \"\"\"\nnext\n").unwrap();
        assert_eq!(lines[0].tokens[1].tok, Tok::Str("one\n\n  two".into()));
        assert_eq!(lines[1].line, 6);
    }

    /// A fence is a run of three or more quotes, and only a line that is the same run after its
    /// indentation closes its block: a run of another length, or one with more on its line, is a
    /// line of the text.
    #[test]
    fn a_block_closes_only_on_its_own_run() {
        let q = |n: usize| "\"".repeat(n);
        let source = format!(
            "notes {}\n  {}\n  {}\n  x {}\n  {} # no comment\n  {}\nnext\n",
            q(4),
            q(3),
            q(5),
            q(4),
            q(4),
            q(4)
        );
        let lines = lex(&source).unwrap();
        let text = format!("{}\n{}\nx {}\n{} # no comment", q(3), q(5), q(4), q(4));
        assert_eq!(lines[0].tokens[1].tok, Tok::Str(text));
        assert_eq!((lines.len(), lines[1].line), (2, 7));
        // Six quotes open a fence of six, which a line of six closes, CRLF and all.
        let lines = lex(&format!("notes {}\r\n  a\r\n  {}\r\n", q(6), q(6))).unwrap();
        assert_eq!(lines[0].tokens[1].tok, Tok::Str("a".into()));
        // Two are an empty string.
        assert_eq!(toks("notes \"\""), [Tok::Ident("notes".into()), Tok::Str(String::new())]);
    }

    /// The fence the printer picks is the fewest quotes, three or more, that no line of the text
    /// is after its indentation; text a block would not keep as it is has none.
    #[test]
    fn a_fence_is_the_shortest_run_no_line_is() {
        let q = |n: usize| "\"".repeat(n);
        assert_eq!(fence("one\ntwo"), Some(3));
        let docstring = format!("def f():\n    {}Return one.{}\n    return 1", q(3), q(3));
        assert_eq!(fence(&docstring), Some(3), "a docstring on its line");
        let docstring = format!("def f():\n    {}\n    Return one.\n    {}\n    return 1", q(3), q(3));
        assert_eq!(fence(&docstring), Some(4), "a docstring's quotes alone on a line");
        assert_eq!(fence(&format!("{}\n{}", q(3), q(4))), Some(5));
        assert_eq!(fence(&q(4)), Some(3), "a line of four leaves three free");
        assert_eq!(fence(&format!("x = {}\nhello{}", q(3), q(3))), Some(3), "quotes at a line's end");
        assert_eq!(fence(&format!("{}\n{}\n{}\n{}", q(6), q(3), q(5), q(4))), Some(7));
        assert_eq!(fence(&format!("\u{a0}{}", q(3))), Some(3), "only spaces indent a line");
        assert_eq!(fence(""), Some(3));
        for kept in ["a\tb", "a\r\nb", "a\rb", "a \nb", "a\nb ", "a\n \nb", "a\u{a0}\nb", "a\u{3000}"] {
            assert_eq!(fence(kept), None, "{kept:?}");
        }
    }

    /// Text a fence is found for reads back from its block as itself, its lines indented as
    /// deep as its closing fence, or deeper.
    #[test]
    fn a_fenced_text_reads_back_as_itself() {
        let q = |n: usize| "\"".repeat(n);
        let texts = [
            "one".to_string(),
            "one\ntwo".into(),
            "\n\nafter two blank lines\n".into(),
            "  indented\n    more\nnone".into(),
            format!("def f():\n    {}\n    Return one.\n    {}\n    return 1", q(3), q(3)),
            format!("{}\n  {}\n{}", q(3), q(4), q(5)),
            format!("x = {}\nquotes at the end {}", q(3), q(4)),
            "# not a comment\n\u{a0}\"\"\"\nuni ✓ é 漢字 🇺🇸".into(),
        ];
        for text in &texts {
            let fence = q(fence(text).unwrap());
            for (open, deeper) in [(0, 0), (0, 2), (2, 4), (4, 4)] {
                let pad = " ".repeat(deeper);
                let lines: String =
                    text.split('\n').map(|l| if l.is_empty() { "\n".into() } else { format!("{pad}{l}\n") }).collect();
                let source = format!("{}notes {fence}\n{lines}{pad}{fence}\nnext\n", " ".repeat(open));
                let lines = lex(&source).unwrap_or_else(|e| panic!("{e}\n{source}"));
                assert_eq!(lines[0].tokens[1].tok, Tok::Str(text.clone()), "{source}");
                assert_eq!(lines[1].tokens[0].tok, Tok::Ident("next".into()), "{source}");
            }
        }
    }

    #[test]
    fn mistakes_say_where() {
        let e = lex("a\n\tb").unwrap_err();
        assert_eq!((e.line, e.col), (2, 1));
        assert!(lex("x 12px").unwrap_err().message.contains("unit"));
        assert!(lex("x \"open").unwrap_err().message.contains("not closed"));
        assert!(lex("theme:themes/dusk.json").unwrap_err().message.contains("quote it"));
    }

    /// A block left open is an error at its fence, which says what closes it; a line of a
    /// shorter or longer run does not.
    #[test]
    fn a_block_left_open_says_where_it_opened() {
        let source = "a\n  notes \"\"\"\"\n    one\n    \"\"\"\n    \"\"\"\"\"\n";
        let e = lex(source).unwrap_err();
        assert_eq!((e.line, e.col), (2, 9), "{e}");
        assert_eq!(&source[e.offset..e.offset + e.len], "\"\"\"\"");
        assert_eq!(e.message, "this `\"\"\"\"` string is not closed: a line of `\"\"\"\"` alone closes it");
        let e = lex("notes \"\"\"\"\"").unwrap_err();
        assert!(e.message.contains("not closed") && e.message.contains("`\"\"\"\"\"`"), "{e}");
        let e = lex("notes \"\"\"\" one\n  two\n  \"\"\"\"\n").unwrap_err();
        assert_eq!((e.line, e.col), (1, 7), "{e}");
        assert!(e.message.contains("nothing may follow the opening `\"\"\"\"`"), "{e}");
        let e = lex("notes \"\"\"\"\n    one\n  two\n   \"\"\"\"\n").unwrap_err();
        assert_eq!((e.line, e.col), (3, 1), "{e}");
        assert!(e.message.contains("indented less than the closing `\"\"\"\"`"), "{e}");
    }
}
