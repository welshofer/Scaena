//! The keys a code text answers as a code editor does (PLAN 2.139, ADR-0029): Tab, Shift+Tab, and
//! Return, each given the text and its selection and giving them back changed, which an editor
//! then makes a `replace_text` of, as it makes one of what is typed.
//!
//! - **Tab** with the caret on one line, or the selection within it, puts spaces in its place to
//!   the next tab stop, counted from the line's start; with a selection that holds a line break,
//!   it indents each line it touches one stop, but a line of white space alone, which it leaves.
//! - **Shift+Tab** takes one stop's indentation, or as much as there is, off each line the
//!   selection touches: spaces up to a stop, or one tab.
//! - **Return** puts a line break in the selection's place, the new line indented as the line
//!   it breaks, one stop more after an opening bracket, and between an opening bracket and its
//!   closing one, the closing one on a line of its own at the first's indentation. The white space
//!   before the caret on the line it leaves is taken off, so no line of indentation alone stays.
//!
//! Offsets are bytes of the text, on its characters' boundaries; a tab stop is `tab` columns, and
//! a tab in the text stands to the next stop, as the engine lays code out.

/// A text and its selection after a key: `from` and `to` are where the selection starts and
/// ends, a caret where they meet.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Keyed {
    pub text: String,
    pub from: usize,
    pub to: usize,
}

/// A key a code text answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Tab,
    ShiftTab,
    Return,
}

impl Key {
    /// The key a name gives: `Tab`, `Shift+Tab`, or `Return` (`Enter` too).
    pub fn named(name: &str) -> Option<Key> {
        match name {
            "Tab" => Some(Key::Tab),
            "Shift+Tab" => Some(Key::ShiftTab),
            "Return" | "Enter" => Some(Key::Return),
            _ => None,
        }
    }
}

/// `text` with `from` to `to` selected after `key`, tab stops every `tab` columns. None where
/// the selection is not on the text's characters.
pub fn keyed(text: &str, from: usize, to: usize, key: Key, tab: usize) -> Option<Keyed> {
    let (from, to) = (from.min(to), from.max(to));
    if to > text.len() || !text.is_char_boundary(from) || !text.is_char_boundary(to) {
        return None;
    }
    let tab = tab.max(1);
    Some(match key {
        Key::Tab if text[from..to].contains('\n') => indent(text, from, to, tab),
        Key::Tab => {
            let column = column(text, line_start(text, from), from, tab);
            let spaces = " ".repeat(tab - column % tab);
            let at = from + spaces.len();
            Keyed { text: format!("{}{spaces}{}", &text[..from], &text[to..]), from: at, to: at }
        }
        Key::ShiftTab => dedent(text, from, to, tab),
        Key::Return => broken(text, from, to, tab),
    })
}

/// Where the line `at` stands on starts.
fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |k| k + 1)
}

/// Where the line `at` stands on ends, before its break.
fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |k| at + k)
}

/// The column `at` stands at on the line that starts at `start`, a tab to the next stop.
fn column(text: &str, start: usize, at: usize, tab: usize) -> usize {
    text[start..at].chars().fold(0, |col, c| if c == '\t' { col + tab - col % tab } else { col + 1 })
}

/// Where each line the selection from `from` to `to` touches starts: a selection that ends at a
/// line's start does not touch that line, unless it is a caret there.
fn lines_of(text: &str, from: usize, to: usize) -> Vec<usize> {
    let last = if to > from && line_start(text, to) == to { to - 1 } else { to };
    let mut starts = vec![line_start(text, from)];
    let mut at = starts[0];
    while let Some(k) = text[at..last.max(at)].find('\n') {
        at += k + 1;
        starts.push(at);
    }
    starts
}

/// The indentation at the start of the line at `start`: its spaces and tabs.
fn indentation(text: &str, start: usize) -> &str {
    let rest = &text[start..line_end(text, start)];
    &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
}

/// Each line the selection touches indented one stop, but a line of white space alone; the
/// selection grown with them.
fn indent(text: &str, from: usize, to: usize, tab: usize) -> Keyed {
    let stop = " ".repeat(tab);
    edit_lines(text, from, to, |line| if line.trim().is_empty() { (0, String::new()) } else { (0, stop.clone()) })
}

/// One stop's indentation taken off each line the selection touches, or as much as it has: spaces
/// up to a stop, or one tab.
fn dedent(text: &str, from: usize, to: usize, tab: usize) -> Keyed {
    edit_lines(text, from, to, |line| {
        let taken = if line.starts_with('\t') { 1 } else { line.bytes().take(tab).take_while(|&c| c == b' ').count() };
        (taken, String::new())
    })
}

/// Each line the selection touches given what `edit` says: how many bytes to take off its start,
/// and what to put there. The selection moves with what it stands after, and a caret at a line's
/// start stays at its line's start where something is taken.
fn edit_lines(text: &str, from: usize, to: usize, edit: impl Fn(&str) -> (usize, String)) -> Keyed {
    let starts = lines_of(text, from, to);
    let mut out = String::with_capacity(text.len() + starts.len() * 8);
    let (mut new_from, mut new_to) = (from, to);
    let mut copied = 0;
    let mut shift: isize = 0;
    for &start in &starts {
        out.push_str(&text[copied..start]);
        let line = &text[start..line_end(text, start)];
        let (taken, put) = edit(line);
        // Where `offset`, on this line or past it, goes, `moved` by the lines before.
        let at = |offset: usize, moved: isize| -> usize {
            if offset < start + taken {
                // Inside what is taken: it stands where it went, at the line's start.
                start.saturating_add_signed(moved) + put.len()
            } else {
                offset.saturating_add_signed(moved + put.len() as isize - taken as isize)
            }
        };
        if from >= start {
            new_from = at(from, shift);
        }
        if to >= start {
            new_to = at(to, shift);
        }
        out.push_str(&put);
        copied = start + taken;
        shift += put.len() as isize - taken as isize;
    }
    out.push_str(&text[copied..]);
    Keyed { text: out, from: new_from, to: new_to }
}

/// A line break in the selection's place, the new line indented as the line it breaks and one stop
/// more after an opening bracket; between brackets, the closing one on a line of its own. The
/// white space before the caret on the line it leaves is taken off.
fn broken(text: &str, from: usize, to: usize, tab: usize) -> Keyed {
    let start = line_start(text, from);
    let indent = indentation(text, start);
    // What stays on the line it leaves: up to the caret, its trailing white space taken off.
    let kept = text[start..from].trim_end_matches([' ', '\t']).len() + start;
    let before = text[start..kept].chars().last();
    let after = text[to..].chars().next();
    let opens = matches!(before, Some('{' | '[' | '('));
    let closes = matches!((before, after), (Some('{'), Some('}')) | (Some('['), Some(']')) | (Some('('), Some(')')));
    let inner = if opens { format!("{indent}{}", " ".repeat(tab)) } else { indent.to_string() };
    // The rest of the line after the caret, its leading white space taken off, so it stands at
    // the new line's indentation.
    let rest_at = text.len() - text[to..].trim_start_matches([' ', '\t']).len();
    let mut out = String::with_capacity(text.len() + inner.len() * 2 + 2);
    out.push_str(&text[..kept]);
    out.push('\n');
    out.push_str(&inner);
    let caret = out.len();
    if closes {
        out.push('\n');
        out.push_str(indent);
    }
    out.push_str(&text[rest_at..]);
    Keyed { text: out, from: caret, to: caret }
}
