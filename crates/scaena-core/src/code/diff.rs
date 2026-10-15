//! A diff, as `diff -u` and `git diff` write one, and a patch (PLAN 2.135).
//!
//! - **Headers are meta**: `diff --git a/x b/x`, `index …`, the modes, renames, and copies git
//!   writes, `--- a/x` and `+++ b/x`, `Index:` and the rule of `=`s under it, `Only in`, and
//!   `Binary files … differ`.
//! - **A hunk's header**, `@@ -1,5 +1,6 @@`, is meta, and the words after it, the function the
//!   hunk is in, are plain. Its counts say how many lines the hunk holds, so a line in it that
//!   starts with `---` or `+++` is one taken away or added, not a file's header.
//! - **A line is what its first character makes it**: `+` inserted, `-` deleted, and a space,
//!   or nothing in a hunk, context, which is plain; `\ No newline at end of file` is a comment.
//! - Outside a hunk, and in one whose header gives no counts (a combined diff's `@@@`), a line
//!   that starts with `+` or `-` is inserted or deleted all the same, but for `--- ` and `+++ `,
//!   which begin a file's header, `-- `, which begins a mail's signature, and the lines of a mail's
//!   message, from its headers (`From `, `Subject: `) to its patch. Any other line is plain: a
//!   commit's message, a patch's mail.
//!
//! A line's token holds the whole line, its first character and its break.

use super::Class;
use super::scan::{Spans, is_space, line_end, line_next};

/// The starts of the lines of a file's header, git's and others', in the order of their bytes.
const HEADERS: &[&[u8]] = &[
    b"Binary files ",
    b"GIT binary patch",
    b"Index: ",
    b"Only in ",
    b"copy from ",
    b"copy to ",
    b"deleted file mode ",
    b"diff ",
    b"dissimilarity index ",
    b"index ",
    b"new file mode ",
    b"new mode ",
    b"old mode ",
    b"rename from ",
    b"rename to ",
    b"similarity index ",
];

/// The starts of a mail's headers, before the message a patch's mail holds (`git format-patch`),
/// in the order of their bytes.
const MAIL: &[&[u8]] = &[b"From ", b"From: ", b"Subject: "];

/// Writes `text`'s tokens as a diff.
pub(super) fn scan<'t>(text: &'t str, out: &mut Spans<'t>) {
    let b = text.as_bytes();
    let len = b.len();
    let mut pos = 0;
    // The lines the hunk being read has yet to hold, on the old side and on the new.
    let (mut old, mut new) = (0u64, 0u64);
    // Whether a mail's headers came, and no file's header since: the lines are its message.
    let mut mail = false;
    while pos < len {
        let next = line_next(b, pos, len);
        let line = &b[pos..line_end(b, pos, len)];
        let counted = old > 0 || new > 0;
        let class = match line.first() {
            // The line that begins a mail's signature, after its patch.
            Some(b'-') if !counted && line == b"-- " => Class::Plain,
            // A line of a mail's message, before its patch: `- fixes the cache`.
            Some(b'+' | b'-') if mail && !counted && !file(line) => Class::Plain,
            Some(b'+') if counted || !file(line) => {
                new = new.saturating_sub(1);
                Class::Inserted
            }
            Some(b'-') if counted || !file(line) => {
                old = old.saturating_sub(1);
                Class::Deleted
            }
            Some(b' ') | None if counted => {
                old = old.saturating_sub(1);
                new = new.saturating_sub(1);
                Class::Plain
            }
            Some(b'\\') => Class::Comment,
            Some(b'@') if line.starts_with(b"@@") => {
                mail = false;
                (old, new) = counts(line).unwrap_or((0, 0));
                let meta = pos + hunk(line);
                out.push(meta, Class::Meta);
                let words = pos + line.len();
                if line[meta - pos..].iter().any(|&c| !is_space(c)) {
                    out.space(meta + line[meta - pos..].iter().take_while(|&&c| is_space(c)).count());
                    out.push(words, Class::Plain);
                }
                out.space(next);
                pos = next;
                continue;
            }
            _ if file(line) || HEADERS.iter().any(|h| line.starts_with(h)) || rule(line) => {
                (old, new, mail) = (0, 0, false);
                Class::Meta
            }
            _ => {
                (old, new) = (0, 0);
                mail |= MAIL.iter().any(|h| line.starts_with(h));
                Class::Plain
            }
        };
        if class == Class::Plain && line.iter().all(|&c| is_space(c)) {
            out.space(next);
        } else {
            out.push(next, class);
        }
        pos = next;
    }
}

/// Whether `line` begins a file's header: `--- a/x` or `+++ b/x`.
fn file(line: &[u8]) -> bool {
    (line.starts_with(b"---") || line.starts_with(b"+++")) && line.get(3).is_none_or(|&c| c == b' ' || c == b'\t')
}

/// Whether `line` is a rule of `=`s, as one stands under `Index:`.
fn rule(line: &[u8]) -> bool {
    line.len() >= 3 && line.iter().all(|&c| c == b'=')
}

/// How long a hunk's header is that starts `line`: its `@`s, its ranges, and as many `@`s again
/// after them, or the whole line where none close it.
fn hunk(line: &[u8]) -> usize {
    let n = line.iter().take_while(|&&c| c == b'@').count();
    let mut k = n;
    while k < line.len() {
        if line[k] == b'@' {
            let run = line[k..].iter().take_while(|&&c| c == b'@').count();
            if run >= n {
                return k + n;
            }
            k += run;
        } else {
            k += 1;
        }
    }
    line.len()
}

/// The lines a hunk's header says it holds, on the old side and on the new (`@@ -1,5 +1,6 @@`), a
/// count left out being 1; `None` for a header of another shape, as a combined diff's `@@@`.
fn counts(line: &[u8]) -> Option<(u64, u64)> {
    let (old, rest) = range(line.strip_prefix(b"@@ -")?)?;
    let (new, rest) = range(rest.strip_prefix(b" +")?)?;
    rest.starts_with(b" @@").then_some((old, new))
}

/// A range, `start,count` or `start`, at the start of `s`: its count, and what follows it.
fn range(s: &[u8]) -> Option<(u64, &[u8])> {
    let digits = |s: &[u8]| s.iter().take_while(|c| c.is_ascii_digit()).count();
    let n = digits(s);
    if n == 0 {
        return None;
    }
    let Some(rest) = s[n..].strip_prefix(b",") else {
        return Some((1, &s[n..]));
    };
    let m = digits(rest);
    if m == 0 {
        return None;
    }
    let count = rest[..m].iter().fold(0u64, |a, &c| a.saturating_mul(10).saturating_add(u64::from(c - b'0')));
    Some((count, &rest[m..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The headers are sorted, so a reader finds each in order.
    #[test]
    fn the_headers_are_sorted() {
        for table in [HEADERS, MAIL] {
            assert!(table.windows(2).all(|w| w[0] < w[1]), "not sorted: {table:?}");
        }
    }

    #[test]
    fn a_hunks_header_gives_its_counts() {
        assert_eq!(counts(b"@@ -1,5 +1,6 @@ fn main() {"), Some((5, 6)));
        assert_eq!(counts(b"@@ -3 +3,0 @@"), Some((1, 0)));
        assert_eq!(counts(b"@@@ -1,2 -1,2 +1,3 @@@"), None);
        assert_eq!(counts(b"@@ -x +1 @@"), None);
        assert_eq!(hunk(b"@@ -1 +1 @@ f"), 11);
        assert_eq!(hunk(b"@@@ -1,2 -1,2 +1,3 @@@ g"), 22);
        assert_eq!(hunk(b"@@ never closed"), 15);
    }
}
