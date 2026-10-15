//! A console session (PLAN 2.134): prompts, the commands typed after them, and what the commands
//! print.
//!
//! - **A prompt** at a line's start is meta: a sign, `$`, `#`, `%`, or `>`, then a space or the
//!   line's end, maybe after who and where (`user@host:~/code$`, `[user@host code]$`,
//!   `user@host ~ %`, `~/code $`, `bash-5.2$`) and a virtual environment's name (`(venv) $`); or
//!   PowerShell's, `PS`, maybe a path, and `>` (`PS C:\Users\me>`).
//! - **The command** after a prompt is read as Shell, or as PowerShell after a PowerShell prompt,
//!   over the lines what it opens holds open: a quote, a heredoc, a `\` or a `|` at a line's end,
//!   an `if` before its `fi`. A continuation prompt that starts one of those lines (`> `, `>> `)
//!   is meta. `>>` at a line's start is a prompt too, read as the command before it was.
//! - **Every other line** is what a command printed, and plain.

use super::scan::{Spans, blanks_end, is_space, line_end, line_next};
use super::{Class, powershell, shell};

/// How far a prompt is looked through for its sign.
const PROMPT: usize = 128;

/// What reads a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reader {
    Shell,
    PowerShell,
}

/// Writes `text`'s tokens as a console session.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let b = text.as_bytes();
    let mut pos = 0;
    let mut last = Reader::Shell;
    while pos < b.len() {
        let next = line_next(b, pos, b.len());
        if let Some((at, reader)) = prompt(b, pos, next, last) {
            out.push(at, Class::Meta);
            last = reader;
            pos = match reader {
                Reader::Shell => shell::command(text, out, at),
                Reader::PowerShell => powershell::command(text, out, at),
            };
        } else {
            // What a command printed: its indentation goes to the token before, as whitespace does.
            out.space(blanks_end(b, pos, next));
            if b[pos..next].iter().all(|&c| is_space(c)) {
                out.space(next);
            } else {
                out.push(next, Class::Plain);
            }
            pos = next;
        }
    }
}

/// The prompt that starts the line from `pos` to `next`, if one does: where it ends, past one
/// space after it, and what reads the command after it. `last` reads after `>>`.
fn prompt(b: &[u8], pos: usize, next: usize, last: Reader) -> Option<(usize, Reader)> {
    let line = line_end(b, pos, next);
    let limit = line.min(pos + PROMPT);
    let at = |i: usize| if i < line { b[i] } else { 0 };
    // A run of bytes that are no spaces, from `i`.
    let run = |i: usize| (i..limit).find(|&k| is_space(b[k])).unwrap_or(limit);
    // Where a prompt ends whose sign is at `k`, past one space after it.
    let sign = |k: usize| {
        (matches!(at(k), b'$' | b'#' | b'%' | b'>') && matches!(at(k + 1), b' ' | 0))
            .then(|| k + 1 + usize::from(at(k + 1) == b' '))
    };
    let mut i = pos;
    // A virtual environment's name: `(venv) `.
    if at(i) == b'(' {
        let close = run(i);
        if close > i + 2 && b[close - 1] == b')' && at(close) == b' ' {
            i = close + 1;
        }
    }
    // PowerShell's: `PS`, maybe a path, and `>`.
    if b[i..line].starts_with(b"PS")
        && matches!(at(i + 2), b' ' | b'>')
        && let Some(k) = (i + 2..limit).find(|&k| b[k] == b'>')
    {
        return Some((k + 1 + usize::from(at(k + 1) == b' '), Reader::PowerShell));
    }
    // PowerShell's continuation, read as the command before it was.
    if b[i..line].starts_with(b">>") && matches!(at(i + 2), b' ' | 0) {
        return Some((i + 2 + usize::from(at(i + 2) == b' '), last));
    }
    if let Some(end) = sign(i) {
        return Some((end, Reader::Shell));
    }
    // Who and where before the sign: `[user@host code]`, or a run that holds an `@` or a `:`,
    // starts at home (`~/code`), or names a shell (`bash-5.2`), the sign maybe last in it.
    let head = if at(i) == b'[' {
        (i + 1..limit).find(|&k| b[k] == b']')? + 1
    } else {
        let e = run(i);
        let word = &b[i..e];
        let shell = ["bash", "sh", "zsh", "ksh", "dash"].iter().any(|s| {
            word.starts_with(s.as_bytes()) && word.get(s.len()).is_none_or(|&c| matches!(c, b'-' | b'$' | b'#' | b'%'))
        });
        if !(shell || word.first() == Some(&b'~') || word.iter().any(|&c| matches!(c, b'@' | b':'))) {
            return None;
        }
        if e > i + 1
            && let Some(end) = sign(e - 1)
        {
            return Some((end, Reader::Shell));
        }
        e
    };
    // Past spaces, the sign, or a path and then the sign: `user@host ~ %`, `[me@box]~/code$`.
    let mut k = head;
    while k < limit && at(k) == b' ' {
        k += 1;
    }
    if let Some(end) = sign(k) {
        return Some((end, Reader::Shell));
    }
    let e = run(k);
    if e > k + 1
        && let Some(end) = sign(e - 1)
    {
        return Some((end, Reader::Shell));
    }
    let mut k = e;
    while k < limit && at(k) == b' ' {
        k += 1;
    }
    sign(k).filter(|_| e > head).map(|end| (end, Reader::Shell))
}
