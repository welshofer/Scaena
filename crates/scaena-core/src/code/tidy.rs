//! A paste made tidy as code (PLAN 2.133, ADR-0029 decision 9): typesetting, never a language's
//! own formatter, so the speaker's code stays as they wrote it.
//!
//! [`tidy`], in order:
//! - takes off a byte-order mark, makes every line end a line feed, and U+00A0 a space;
//! - takes off a Markdown fence about the text (three or more `` ` `` or `~`, its info string
//!   kept in [`Tidied::fence`], closed by as many or more of the same, or never);
//! - turns tabs into spaces to the next stop, a stop every `tab` columns, columns counted in
//!   characters (a Makefile keeps the tabs that open its lines: they tell a recipe);
//! - drops the spaces that end each line, and the blank lines at either end;
//! - takes off the indentation the lines share (a diff keeps its own: a line's first character
//!   is what it is), and that of a first line cut at its first character (see [`dedent`]);
//! - reindents JSON with two spaces where it is one minified value, every token as written.
//!
//! It reports the lines that hold curly quotes, which a word processor puts in code. Tidying a
//! tidy text changes nothing.

use super::detect::{closes, json, opener};
use super::{Lang, Language};

/// A paste made tidy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tidied {
    /// The text.
    pub text: String,
    /// The info string of the fence taken off, empty where it had none; none where there was no
    /// fence.
    pub fence: Option<String>,
    /// The lines of [`Tidied::text`], counted from 1, that hold a curly quote (U+2018, U+2019,
    /// U+201C, U+201D).
    pub curly: Vec<usize>,
}

/// The width a minified JSON line runs past, where a value goes on after a comma.
const MINIFIED: usize = 80;

/// `text`, pasted as code in `lang`, made tidy, tabs standing every `tab` columns (one at least).
pub fn tidy(text: &str, lang: Lang, tab: usize) -> Tidied {
    let tab = tab.max(1);
    let name = match lang {
        Lang::Known(Language { name, .. }) => *name,
        _ => "",
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n").replace(['\r'], "\n").replace('\u{a0}', " ");
    let mut lines: Vec<&str> = text.split('\n').collect();
    blank_ends(&mut lines, |l| l.trim().is_empty());
    let mut fence = None;
    if let Some((c, n, info)) = lines.first().and_then(|l| opener(l)) {
        fence = Some(info.to_string());
        lines.remove(0);
        if lines.last().is_some_and(|l| closes(l, c, n)) {
            lines.pop();
        }
    }
    let mut lines: Vec<String> = lines.iter().map(|l| expand(l, tab, name == "makefile")).collect();
    blank_ends(&mut lines, String::is_empty);
    if name != "diff" {
        dedent(&mut lines);
    }
    let mut text = lines.join("\n");
    if name == "json"
        && let Some(tokens) = json(&text)
        && minified(&text, &tokens)
    {
        text = reindent(&text, &tokens);
    }
    let curly = (text.split('\n').enumerate())
        .filter(|(_, l)| l.contains(['\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}']))
        .map(|(i, _)| i + 1)
        .collect();
    Tidied { text, fence, curly }
}

/// Takes the blank lines off either end of `lines`.
fn blank_ends<T>(lines: &mut Vec<T>, blank: impl Fn(&T) -> bool) {
    while lines.last().is_some_and(&blank) {
        lines.pop();
    }
    let lead = lines.iter().take_while(|l| blank(l)).count();
    lines.drain(..lead);
}

/// `line` with its tabs turned into spaces to the next stop and its trailing spaces dropped;
/// with `keep`, the tabs that open it stay.
fn expand(line: &str, tab: usize, keep: bool) -> String {
    let mut out = String::with_capacity(line.len());
    let mut column = 0;
    let mut opening = true;
    for c in line.chars() {
        if c == '\t' && !(keep && opening) {
            let n = tab - column % tab;
            out.extend(std::iter::repeat_n(' ', n));
            column += n;
            continue;
        }
        opening &= c == '\t';
        out.push(c);
        column += 1;
    }
    out.truncate(out.trim_end_matches([' ', '\t']).len());
    out
}

/// The spaces that open `line`.
fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// Takes off the indentation `lines` share, then that of a first line cut at its first
/// character: a selection that starts where a line's words do leaves that line at the margin and
/// the rest indented. Four cases:
/// - **Every line indented.** What they share is taken off every line.
/// - **The first line cut, the rest indented alike.** What the rest share is taken off them where
///   the first line ends a statement (`;` or `}`), so the rest are not its own continuation
///   (`foo(a,` over `    b)`) or a block's body under it (`def f():` over `    return 1`).
/// - **The first line cut, the rest at several depths.** What the rest share is taken off them
///   where the first line ends a statement, or opens a block whose next line stands deeper than
///   the rest: a body under a cut `fn main() {`, its `}` at the rest's depth.
/// - **Nothing shared.** Nothing is taken off.
///
/// A first line that ends in a word is never read as cut: `x` over `    y` may be a whole line
/// and its continuation. Each case leaves the lines as the next tidying would, so it cannot apply
/// again.
fn dedent(lines: &mut [String]) {
    let least = |lines: &[String]| lines.iter().filter(|l| !l.is_empty()).map(|l| indent(l)).min().unwrap_or(0);
    let common = least(lines);
    strip(lines, common);
    let Some((first, rest)) = lines.split_first_mut() else { return };
    if indent(first) > 0 {
        return;
    }
    let shared = least(rest);
    let Some(next) = rest.iter().find(|l| !l.is_empty()) else { return };
    let ends = first.ends_with([';', '}']);
    let opens = first.ends_with(['{', '(', '[', ':']) || first.ends_with(" do") || first.ends_with(" then");
    if shared > 0 && (ends || opens && indent(next) > shared) {
        strip(rest, shared);
    }
}

/// Takes `n` spaces off the start of each line that is not blank.
fn strip(lines: &mut [String], n: usize) {
    if n == 0 {
        return;
    }
    for line in lines.iter_mut().filter(|l| !l.is_empty()) {
        line.drain(..n);
    }
}

/// Whether the JSON `text` is minified: one line, or a line longer than [`MINIFIED`] on which a
/// value goes on after a comma.
fn minified(text: &str, tokens: &[(usize, usize)]) -> bool {
    let container = tokens.first().is_some_and(|&(s, _)| matches!(text.as_bytes()[s], b'{' | b'['));
    if !container || tokens.len() < 3 {
        return false;
    }
    if !text.contains('\n') {
        return true;
    }
    tokens.windows(2).any(|w| {
        let ((comma, after), (next, _)) = (w[0], w[1]);
        if text.as_bytes()[comma] != b',' || text[after..next].contains('\n') {
            return false;
        }
        let start = text[..comma].rfind('\n').map_or(0, |n| n + 1);
        let end = text[comma..].find('\n').map_or(text.len(), |n| comma + n);
        text[start..end].chars().count() > MINIFIED
    })
}

/// The JSON `text` reindented by two spaces: each member and item on a line of its own, an empty
/// object or array kept on one, and every token as written.
fn reindent(text: &str, tokens: &[(usize, usize)]) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let mut depth = 0usize;
    let newline = |out: &mut String, depth: usize| {
        out.push('\n');
        out.extend(std::iter::repeat_n("  ", depth));
    };
    for (i, &(s, e)) in tokens.iter().enumerate() {
        let token = &text[s..e];
        match token {
            "{" | "[" => {
                out.push_str(token);
                let empty = tokens.get(i + 1).is_some_and(|&(n, _)| matches!(text.as_bytes()[n], b'}' | b']'));
                if !empty {
                    depth += 1;
                    newline(&mut out, depth);
                }
            }
            "}" | "]" => {
                let empty = i > 0 && matches!(text.as_bytes()[tokens[i - 1].0], b'{' | b'[');
                if !empty {
                    depth = depth.saturating_sub(1);
                    newline(&mut out, depth);
                }
                out.push_str(token);
            }
            "," => {
                out.push(',');
                newline(&mut out, depth);
            }
            ":" => out.push_str(": "),
            _ => out.push_str(token),
        }
    }
    out
}
