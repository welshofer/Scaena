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

/// The paragraphs a text's `paragraphs` draws of its `count` (PLAN 2.129), as indices from 0:
/// one, or the first and the last of a run, counted from 1 as written, as far as there are any;
/// all where it says none.
pub fn drawn(paragraphs: Option<&serde_json::Value>, count: usize) -> Range<usize> {
    let index = |v: &serde_json::Value| v.as_u64().map(|n| (n as usize).clamp(1, count.max(1)));
    let (first, last) = match paragraphs {
        Some(serde_json::Value::Array(span)) if span.len() == 2 => match (index(&span[0]), index(&span[1])) {
            (Some(a), Some(b)) => (a, b),
            _ => return 0..count,
        },
        Some(n) => match index(n) {
            Some(n) => (n, n),
            None => return 0..count,
        },
        None => return 0..count,
    };
    (first - 1).min(count)..last.min(count).max(first - 1)
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

/// `paragraphs`, which of a text's paragraphs a state draws (PLAN 2.129, as [`drawn`] reads it),
/// once bytes `range` of `written` are `text`, as [`edited`] keeps a list: the paragraphs before
/// the edit keep their places, the paragraphs it touches become those it leaves, drawn where one
/// of them was, so what is typed into a paragraph drawn is drawn too, and those after it move by
/// as many as it adds or takes away. None where it names no paragraph.
pub fn redrawn(
    paragraphs: &serde_json::Value,
    written: &str,
    range: Range<usize>,
    text: &str,
) -> Option<serde_json::Value> {
    let index = |v: &serde_json::Value| v.as_u64().filter(|&n| n >= 1).map(|n| n as usize - 1);
    let (a, b, one) = match paragraphs {
        serde_json::Value::Array(span) if span.len() == 2 => (index(&span[0])?, index(&span[1])?, false),
        n => (index(n)?, index(n)?, true),
    };
    let before = self::paragraphs(written);
    let first = before.iter().position(|p| range.start <= p.end).unwrap_or(before.len() - 1);
    let last = before.iter().position(|p| range.end <= p.end).unwrap_or(before.len() - 1);
    let typed = self::paragraphs(text).len();
    // Where paragraph `q` is once edited: the edit's first or last paragraph where it touched `q`.
    let moved = |q: usize, end: bool| match q {
        q if q < first => q,
        q if q > last => q - (last + 1) + first + typed,
        _ if end => first + typed - 1,
        _ => first,
    };
    let (a, b) = (moved(a, false), moved(b, true));
    Some(if one && a == b { serde_json::Value::from(a + 1) } else { serde_json::Value::from(vec![a + 1, b + 1]) })
}

/// A code text's `code.focus` (PLAN 2.140), the lines in focus, once bytes `range` of `written`
/// are `text`, each span kept as [`redrawn`] keeps the paragraphs a state draws: a line typed
/// into stays in focus with what is typed, and the lines after the edit move with those it adds
/// or takes away. None where it names no line.
pub fn refocused(
    focus: &serde_json::Value,
    written: &str,
    range: Range<usize>,
    text: &str,
) -> Option<serde_json::Value> {
    let spans = focus.as_array()?;
    let kept: Option<Vec<serde_json::Value>> =
        spans.iter().map(|span| redrawn(span, written, range.clone(), text)).collect();
    kept.map(serde_json::Value::Array)
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

    /// What a state draws of a text built by bullet (PLAN 2.129) follows an edit as its list does.
    #[test]
    fn an_edit_keeps_the_paragraphs_drawn_in_step() {
        use serde_json::json;
        let text = "one\ntwo\nthree";
        // Enter at the end of the first, drawn alone: the new paragraph is drawn with it, and
        // those after it move down one.
        assert_eq!(redrawn(&json!(1), text, 3..3, "\n"), Some(json!([1, 2])));
        assert_eq!(redrawn(&json!([1, 2]), text, 3..3, "\n"), Some(json!([1, 3])));
        assert_eq!(redrawn(&json!([2, 3]), text, 3..3, "\n"), Some(json!([3, 4])));
        // Words typed in a paragraph move nothing.
        assert_eq!(redrawn(&json!([1, 2]), text, 5..5, "x"), Some(json!([1, 2])));
        assert_eq!(redrawn(&json!(3), text, 0..0, "x"), Some(json!(3)));
        // The break after the first taken away: the two are one, drawn where either was.
        assert_eq!(redrawn(&json!(1), text, 3..4, ""), Some(json!(1)));
        assert_eq!(redrawn(&json!([2, 3]), text, 3..4, ""), Some(json!([1, 2])));
        assert_eq!(redrawn(&json!(3), text, 3..4, ""), Some(json!(2)));
        // Pasted lines in the last move none before them.
        assert_eq!(redrawn(&json!([1, 2]), text, 13..13, "\nfour\nfive"), Some(json!([1, 2])));
        assert_eq!(redrawn(&json!(0), text, 0..0, "x"), None);
    }

    #[test]
    fn a_focus_keeps_each_span_in_step_with_typed_lines() {
        use serde_json::json;
        let text = "one\ntwo\nthree";
        // A line broken at the end of the first, which is in focus: the line it makes is in focus
        // with it, and the spans after it move down.
        assert_eq!(refocused(&json!([1, [2, 3]]), text, 3..3, "\nnew"), Some(json!([[1, 2], [3, 4]])));
        // A line typed before the second: the span from it moves down.
        assert_eq!(refocused(&json!([[2, 3]]), text, 0..0, "zero\n"), Some(json!([[3, 4]])));
        // A line taken away: the span over it closes up.
        assert_eq!(refocused(&json!([[2, 3]]), text, 3..7, ""), Some(json!([[1, 2]])));
        // A line typed into a line in focus stays in focus with it.
        assert_eq!(refocused(&json!([2]), text, 7..7, "\nmore"), Some(json!([[2, 3]])));
        assert_eq!(refocused(&json!(2), text, 0..0, "x"), None);
        assert_eq!(refocused(&json!([0]), text, 0..0, "x"), None);
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
