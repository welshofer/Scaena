//! A text's paragraphs as a list's items (SPEC §3.5, ADR-0018): where its paragraphs are,
//! how its items count, what marks each, and how an edit of its text keeps its `list` in
//! step with its paragraphs.

use crate::model::theme::Lists;
use crate::model::values::{ListItem, ListKind};
use std::ops::Range;

/// The paragraphs of `text`, as byte ranges without the break that ends each: a hard line
/// break ends one (`\n`, `\r`, `\r\n`, U+2028, U+2029). A text has one paragraph more than
/// it has breaks, so an empty text has one, empty.
pub fn paragraphs(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let end = match c {
            '\r' if chars.peek().is_some_and(|&(_, n)| n == '\n') => {
                chars.next();
                i + 2
            }
            '\n' | '\r' | '\u{2028}' | '\u{2029}' => i + c.len_utf8(),
            _ => continue,
        };
        out.push(start..i);
        start = end;
    }
    out.push(start..text.len());
    out
}

/// Each paragraph's item, `list` read for `count` paragraphs: one past its end is none.
pub fn items(list: &[Option<ListItem>], count: usize) -> Vec<Option<ListItem>> {
    (0..count).map(|k| list.get(k).copied().flatten()).collect()
}

/// Each numbered item's number, from 1: it counts among the numbered items of its level that
/// follow one another. An item at a shallower level, or a paragraph that is no item, starts
/// the count again below it; a bullet at a level starts that level's again.
pub fn numbers(items: &[Option<ListItem>]) -> Vec<Option<u32>> {
    let mut counts = [0u32; 9];
    items
        .iter()
        .map(|item| {
            let Some(item) = item else {
                counts = [0; 9];
                return None;
            };
            let level = usize::from(item.depth()).min(8);
            for deeper in &mut counts[level + 1..] {
                *deeper = 0;
            }
            match item.kind {
                ListKind::Bullet => {
                    counts[level] = 0;
                    None
                }
                ListKind::Number => {
                    counts[level] += 1;
                    Some(counts[level])
                }
            }
        })
        .collect()
}

/// What marks `item`: its level's bullet, or its number `n` in its level's pattern.
pub fn marker(lists: &Lists, item: ListItem, n: Option<u32>) -> String {
    let (_, _, bullet, pattern) = lists.at(item.depth());
    match item.kind {
        ListKind::Bullet => bullet.to_string(),
        ListKind::Number => numbered(pattern, n.unwrap_or(1)),
    }
}

/// `pattern` with its first `1`, `a`, `A`, `i`, or `I` written as `n`: in digits, letters
/// (`z` then `aa`), or roman numerals. A pattern with none of them is `n` and a stop.
pub fn numbered(pattern: &str, n: u32) -> String {
    let Some(at) = pattern.find(['1', 'a', 'A', 'i', 'I']) else { return format!("{n}.") };
    let number = match &pattern[at..at + 1] {
        "1" => n.to_string(),
        "a" => alphabetic(n),
        "A" => alphabetic(n).to_uppercase(),
        "i" => roman(n),
        _ => roman(n).to_uppercase(),
    };
    format!("{}{number}{}", &pattern[..at], &pattern[at + 1..])
}

/// `n` in letters, as spreadsheets count columns: a … z, aa, ab, ….
fn alphabetic(mut n: u32) -> String {
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(b'a' + (n % 26) as u8);
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// `n` in lower-case roman numerals; past 3999, in digits.
fn roman(n: u32) -> String {
    if n == 0 || n > 3999 {
        return n.to_string();
    }
    const STEPS: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    let mut n = n;
    for (value, numeral) in STEPS {
        while n >= value {
            out.push_str(numeral);
            n -= value;
        }
    }
    out
}

/// `list` for `written` once bytes `range` of it are `text` (ADR-0018): the paragraphs before
/// the edit keep their items, and so do those after it; the paragraph the edit begins in keeps
/// its item, a paragraph a break it types splits off takes that item too, and paragraphs it
/// joins keep the first's. Trailing paragraphs that are no items are left out.
pub fn edited(list: &[Option<ListItem>], written: &str, range: Range<usize>, text: &str) -> Vec<Option<ListItem>> {
    let before = paragraphs(written);
    let first = before.iter().position(|p| range.start <= p.end).unwrap_or(before.len() - 1);
    let last = before.iter().position(|p| range.end <= p.end).unwrap_or(before.len() - 1);
    let old = items(list, before.len());
    let typed = paragraphs(text).len();
    let mut out: Vec<Option<ListItem>> = old[..first].to_vec();
    out.extend(std::iter::repeat_n(old[first], typed));
    out.extend_from_slice(&old[last + 1..]);
    while out.last().is_some_and(Option::is_none) {
        out.pop();
    }
    out
}

/// `list` with the paragraphs that characters `range` of `written` touch (bytes) marked as
/// `how` says: each set to `kind` (none takes it out of the list) at `level`, or, with `by`,
/// each item moved that many levels deeper, from 0 to 8. A paragraph that is no item and
/// moves deeper becomes a bullet. Trailing paragraphs that are no items are left out.
pub fn marked(list: &[Option<ListItem>], written: &str, range: Range<usize>, how: Marking) -> Vec<Option<ListItem>> {
    let paras = paragraphs(written);
    let first = paras.iter().position(|p| range.start <= p.end).unwrap_or(paras.len() - 1);
    let last = paras.iter().position(|p| range.end <= p.end).unwrap_or(paras.len() - 1).max(first);
    let mut out = items(list, paras.len());
    for item in &mut out[first..=last] {
        *item = match how {
            Marking::Kind(None, _) => None,
            Marking::Kind(Some(kind), level) => {
                let level = level.or(item.and_then(|i| i.level)).filter(|&l| l > 0);
                Some(ListItem { kind, level })
            }
            Marking::By(by) => {
                let now = item.unwrap_or(ListItem { kind: ListKind::Bullet, level: None });
                let level = (i32::from(now.depth()) + by).clamp(0, 8) as u8;
                match item {
                    None if by <= 0 => None,
                    _ => Some(ListItem { kind: now.kind, level: (level > 0).then_some(level) }),
                }
            }
        };
    }
    while out.last().is_some_and(Option::is_none) {
        out.pop();
    }
    out
}

/// How [`marked`] marks paragraphs: a kind (or none) at a level (or each its own), or levels
/// deeper (positive) or shallower.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marking {
    Kind(Option<ListKind>, Option<u8>),
    By(i32),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: ListKind, level: u8) -> Option<ListItem> {
        Some(ListItem { kind, level: (level > 0).then_some(level) })
    }
    const B: ListKind = ListKind::Bullet;
    const N: ListKind = ListKind::Number;

    #[test]
    fn paragraphs_end_at_hard_breaks() {
        assert_eq!(paragraphs(""), vec![Range { start: 0, end: 0 }]);
        assert_eq!(paragraphs("a\nb"), [0..1, 2..3]);
        assert_eq!(paragraphs("a\r\nb\n"), [0..1, 3..4, 5..5]);
        assert_eq!(paragraphs("a\u{2028}b"), [0..1, 4..5]);
    }

    #[test]
    fn numbers_count_by_level_and_start_again() {
        let list = [item(N, 0), item(N, 1), item(N, 1), item(N, 0), item(B, 1), item(N, 1), None, item(N, 0)];
        let got = numbers(&list);
        assert_eq!(got, [Some(1), Some(1), Some(2), Some(2), None, Some(1), None, Some(1)]);
    }

    #[test]
    fn markers_follow_the_pattern() {
        assert_eq!(numbered("1.", 12), "12.");
        assert_eq!(numbered("(a)", 28), "(ab)");
        assert_eq!(numbered("A.", 3), "C.");
        assert_eq!(numbered("i.", 14), "xiv.");
        assert_eq!(numbered("I)", 1999), "MCMXCIX)");
        assert_eq!(numbered("#", 4), "4.");
        let lists = Lists::default();
        assert_eq!(marker(&lists, ListItem { kind: B, level: Some(1) }, None), "\u{2013}");
        assert_eq!(marker(&lists, ListItem { kind: N, level: Some(7) }, Some(4)), "iv.");
    }

    #[test]
    fn an_edit_keeps_the_list_in_step() {
        let list = [item(B, 0), item(N, 1)];
        // Enter at the end of the first item: a new item like it.
        assert_eq!(edited(&list, "one\ntwo", 3..3, "\n"), [item(B, 0), item(B, 0), item(N, 1)]);
        // The break between them taken away: one item, the first's.
        assert_eq!(edited(&list, "one\ntwo", 3..4, ""), [item(B, 0)]);
        // Words typed in the second leave both.
        assert_eq!(edited(&list, "one\ntwo", 5..5, "x"), list);
        // A plain paragraph after them stays plain.
        assert_eq!(edited(&[None, item(B, 0)], "a\nb", 0..0, "x\ny"), [None, None, item(B, 0)]);
    }

    #[test]
    fn marking_sets_kinds_and_levels() {
        let text = "a\nb\nc";
        assert_eq!(marked(&[], text, 0..3, Marking::Kind(Some(B), None)), [item(B, 0), item(B, 0)]);
        let list = [item(B, 0), item(B, 2), item(N, 0)];
        assert_eq!(marked(&list, text, 2..2, Marking::By(1)), [item(B, 0), item(B, 3), item(N, 0)]);
        assert_eq!(marked(&list, text, 0..5, Marking::By(-1)), [item(B, 0), item(B, 1), item(N, 0)]);
        assert_eq!(marked(&list, text, 4..5, Marking::Kind(None, None)), [item(B, 0), item(B, 2)]);
        assert_eq!(marked(&[], text, 0..0, Marking::By(1)), [item(B, 1)]);
        assert_eq!(marked(&[], text, 0..0, Marking::By(-1)), Vec::<Option<ListItem>>::new());
    }
}
