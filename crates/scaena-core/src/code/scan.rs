//! What every scanner shares: the bytes it tells apart, how far the commonest constructs run,
//! and the tokens it writes.
//!
//! A scanner tells bytes apart by ASCII alone. Whitespace is ASCII space, tab, line feed,
//! carriage return, and form feed; any byte above ASCII continues a word. So a token ends on an
//! ASCII byte or at the text's end, always a character's boundary.

use super::{Class, Token};

/// Whether `b` is whitespace: ASCII space, tab, line feed, carriage return, or form feed.
pub(super) fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0C)
}

/// Whether `b` ends a line: a line feed, or a carriage return.
pub(super) fn is_break(b: u8) -> bool {
    matches!(b, b'\n' | b'\r')
}

/// Whether a word can start with `b`: an ASCII letter, `_`, or any byte above ASCII.
pub(super) fn is_word_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b >= 0x80
}

/// Whether `b` continues a word: an ASCII letter or digit, `_`, or any byte above ASCII.
pub(super) fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

/// Where the whitespace at `at` ends.
pub(super) fn space_end(b: &[u8], at: usize, end: usize) -> usize {
    let mut i = at;
    while i < end && is_space(b[i]) {
        i += 1;
    }
    i
}

/// Where the line `at` stands on ends: at its break, which it leaves out, or at `end`.
pub(super) fn line_end(b: &[u8], at: usize, end: usize) -> usize {
    let mut i = at;
    while i < end && !is_break(b[i]) {
        i += 1;
    }
    i
}

/// Where the word at `at` ends.
pub(super) fn word_end(b: &[u8], at: usize, end: usize) -> usize {
    let mut i = at;
    while i < end && is_word(b[i]) {
        i += 1;
    }
    i
}

/// Where the number at `at` ends: its digits, letters, and underscores (`0x1F`, `10u8`,
/// `1_000`), a point before a digit (`1.5`, but not `1..2`), a sign after its exponent (`1e-9`,
/// and `0x1p-4` in hexadecimal), and, where `quotes` is set, a `'` between two digits (C++'s
/// `1'000`).
pub(super) fn number_end(b: &[u8], at: usize, end: usize, quotes: bool) -> usize {
    let hex = b[at] == b'0' && at + 1 < end && matches!(b[at + 1], b'x' | b'X');
    let digit_next = |i: usize| i + 1 < end && b[i + 1].is_ascii_digit();
    let mut i = at;
    while i < end {
        let c = b[i];
        let more = c.is_ascii_alphanumeric()
            || c == b'_'
            || (c == b'.' && digit_next(i) && (i == at || b[i - 1] != b'.'))
            || (matches!(c, b'+' | b'-')
                && i > at
                && digit_next(i)
                && if hex { matches!(b[i - 1], b'p' | b'P') } else { matches!(b[i - 1], b'e' | b'E') })
            || (quotes
                && c == b'\''
                && i > at
                && b[i - 1].is_ascii_hexdigit()
                && i + 1 < end
                && b[i + 1].is_ascii_hexdigit());
        if !more {
            break;
        }
        i += 1;
    }
    i
}

/// Where the block comment that opens at `at` (`/*`) ends: after the `*/` that closes it, or
/// at `end` where none does. Where `nested` is set, a `/*` inside it opens one more.
pub(super) fn block_comment_end(b: &[u8], at: usize, end: usize, nested: bool) -> usize {
    let mut depth = 1u32;
    let mut i = at + 2;
    while i < end {
        if b[i] == b'*' && i + 1 < end && b[i + 1] == b'/' {
            i += 2;
            depth -= 1;
            if depth == 0 {
                return i;
            }
        } else if nested && b[i] == b'/' && i + 1 < end && b[i + 1] == b'*' {
            i += 2;
            depth = depth.saturating_add(1);
        } else {
            i += 1;
        }
    }
    end
}

/// Where the string that opens with the quote at `at` ends: after the same quote, with a
/// backslash escaping the character after it, or at its line's end, or at `end`.
pub(super) fn string_end(b: &[u8], at: usize, end: usize) -> usize {
    let quote = b[at];
    let mut i = at + 1;
    while i < end {
        match b[i] {
            b'\\' => i += 2,
            c if c == quote => return i + 1,
            c if is_break(c) => return i,
            _ => i += 1,
        }
    }
    end
}

/// Where the first `lines` lines of `text` end: after the break that ends the last of them, or
/// at the text's end. A line feed ends a line, and so does a carriage return with no line feed
/// after it.
pub(super) fn cut(text: &str, lines: usize) -> usize {
    let b = text.as_bytes();
    let mut count = 0;
    for (i, &c) in b.iter().enumerate() {
        if c == b'\n' || (c == b'\r' && b.get(i + 1) != Some(&b'\n')) {
            count += 1;
            if count == lines {
                return i + 1;
            }
        }
    }
    b.len()
}

/// The tokens a scanner writes, in order: a token in the class of the one before it joins it.
pub(super) struct Spans<'t> {
    text: &'t str,
    tokens: Vec<Token>,
}

impl<'t> Spans<'t> {
    pub(super) fn new(text: &'t str) -> Self {
        Spans { text, tokens: Vec::new() }
    }

    /// Where the last token ends.
    pub(super) fn end(&self) -> usize {
        self.tokens.last().map_or(0, |t| t.end)
    }

    /// The text from where the last token ends up to `end`, in `class`. An end at or before
    /// the last token's adds nothing, and one inside a character moves to its end.
    pub(super) fn push(&mut self, end: usize, class: Class) {
        let end = self.boundary(end);
        if end <= self.end() {
            return;
        }
        match self.tokens.last_mut() {
            Some(last) if last.class == class => last.end = end,
            _ => self.tokens.push(Token { end, class }),
        }
    }

    /// Whitespace up to `end`, which goes to the token before it, or is plain at the text's
    /// start.
    pub(super) fn space(&mut self, end: usize) {
        let end = self.boundary(end);
        if end <= self.end() {
            return;
        }
        match self.tokens.last_mut() {
            Some(last) => last.end = end,
            None => self.tokens.push(Token { end, class: Class::Plain }),
        }
    }

    /// The tokens, the text's every byte in one: what no scanner reached is plain.
    pub(super) fn finish(mut self) -> Vec<Token> {
        self.push(self.text.len(), Class::Plain);
        self.tokens
    }

    fn boundary(&self, end: usize) -> usize {
        let mut end = end.min(self.text.len());
        while !self.text.is_char_boundary(end) {
            end += 1;
        }
        end
    }
}
