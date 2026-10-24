//! Speaker notes as a presenter reads them (lift-up USE-1). A deck's notes are kept as written
//! (SPEC §3.11): a person's lines, or the markdown a generated deck's model writes, bullets each
//! on a line of its own and a GitHub-flavored table where the slide is grid-shaped (PLAN G.2).
//! Every presenter view reads them through [`read`], as blocks: lines, a list, or a table, each
//! word plain, bold, italic, or code, so the browser's presenter view, the Mac's and the iPad's,
//! the remote, and a rehearsal show one reading, and no `- ` or `|` reaches a speaker's eye.
//! Nothing here changes the deck.

use serde::{Deserialize, Serialize};

/// A run of words in one look.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub code: bool,
}

/// A line of words.
pub type Line = Vec<Span>;

/// An item of a list: how deep it is nested, from 0, and its words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub level: u8,
    pub spans: Line,
}

/// A block of the notes, as a presenter view lays it out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Block {
    /// Lines as written, each kept on its own, a blank one between them a blank line; a heading
    /// is a line set bold.
    Lines { lines: Vec<Line> },
    /// A list, bulleted or numbered, each item at its level.
    List { ordered: bool, items: Vec<Item> },
    /// A table: its rows, the first a row of headings where `header`.
    Table { header: bool, rows: Vec<Vec<Line>> },
}

/// A state's notes as a presenter reads them: the words as the deck holds them, and the blocks
/// they read as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notes {
    pub text: String,
    pub blocks: Vec<Block>,
}

/// The notes a presenter reads for `state` of `deck` (SPEC §2.6): its own, else its first beat's
/// (`spine::notes`), read as blocks; none for a state the deck lacks.
pub fn of(deck: &crate::document::Deck, state: &str) -> Option<Notes> {
    let text = crate::spine::notes(deck, state)?;
    Some(Notes { text: text.to_string(), blocks: read(text) })
}

/// `notes` read as blocks (SPEC §2.6). Plain notes are one block of their lines, as written.
pub fn read(notes: &str) -> Vec<Block> {
    let lines: Vec<&str> = notes.lines().map(str::trim_end).collect();
    let mut blocks = Vec::new();
    let mut plain: Vec<Line> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(width) = table_at(&lines, i) {
            flush(&mut blocks, &mut plain);
            let mut rows = vec![cells(line, width)];
            i += 2;
            while i < lines.len() && is_row(lines[i]) {
                rows.push(cells(lines[i], width));
                i += 1;
            }
            blocks.push(Block::Table { header: true, rows });
            continue;
        }
        if let Some((level, number, rest)) = item(line) {
            flush(&mut blocks, &mut plain);
            let ordered = number;
            let mut items = vec![Item { level, spans: inline(rest) }];
            i += 1;
            loop {
                // A blank line between items keeps the list, where an item follows it.
                let next = (i..lines.len()).find(|&j| !lines[j].trim().is_empty());
                match next.and_then(|j| item(lines[j]).map(|it| (j, it))) {
                    Some((j, (level, _, rest))) => {
                        items.push(Item { level, spans: inline(rest) });
                        i = j + 1;
                    }
                    None => break,
                }
            }
            blocks.push(Block::List { ordered, items });
            continue;
        }
        let trimmed = line.trim_start();
        if let Some(heading) = heading(trimmed) {
            plain.push(inline(heading).into_iter().map(|s| Span { bold: true, ..s }).collect());
        } else {
            plain.push(inline(trimmed));
        }
        i += 1;
    }
    flush(&mut blocks, &mut plain);
    blocks
}

/// The blocks as words alone: each line on its own, a list's items marked, and a table's cells
/// set apart by a middle dot; what a test compares, and what reads where nothing is drawn.
pub fn plain(blocks: &[Block]) -> String {
    let words = |line: &Line| line.iter().map(|s| s.text.as_str()).collect::<String>();
    let mut out = Vec::new();
    for block in blocks {
        match block {
            Block::Lines { lines } => out.extend(lines.iter().map(words)),
            Block::List { ordered, items } => {
                let mut counts = [0usize; 8];
                for item in items {
                    let level = (item.level as usize).min(7);
                    counts[level] += 1;
                    counts[level + 1..].fill(0);
                    let mark = if *ordered { format!("{}.", counts[level]) } else { "•".to_string() };
                    out.push(format!("{}{mark} {}", "  ".repeat(level), words(&item.spans)));
                }
            }
            Block::Table { rows, .. } => {
                out.extend(rows.iter().map(|row| row.iter().map(words).collect::<Vec<_>>().join(" · ")));
            }
        }
    }
    out.join("\n")
}

/// The lines gathered so far as one block, without the blank lines at either end.
fn flush(blocks: &mut Vec<Block>, plain: &mut Vec<Line>) {
    let blank = |line: &Line| line.iter().all(|s| s.text.trim().is_empty());
    let from = plain.iter().position(|l| !blank(l));
    let to = plain.iter().rposition(|l| !blank(l));
    if let (Some(from), Some(to)) = (from, to) {
        blocks.push(Block::Lines { lines: plain.drain(from..=to).collect() });
    }
    plain.clear();
}

/// Where `lines[i]` heads a table, how many columns it has: a row of cells over a row of dashes.
fn table_at(lines: &[&str], i: usize) -> Option<usize> {
    let head = lines[i];
    let rule = lines.get(i + 1)?;
    if !is_row(head) || !is_rule(rule) {
        return None;
    }
    let width = split(head).len();
    (width >= 1 && split(rule).len() == width).then_some(width)
}

/// A line of a table: it has a pipe between cells, or pipes at its ends.
fn is_row(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && t.contains('|') && (t.starts_with('|') || t.trim_end_matches('|').contains('|'))
}

/// The rule under a table's headings: cells of dashes, each a colon at either end or neither.
fn is_rule(line: &str) -> bool {
    let cells = split(line);
    is_row(line)
        && cells.iter().all(|c| {
            let c = c.trim().trim_start_matches(':').trim_end_matches(':');
            !c.is_empty() && c.bytes().all(|b| b == b'-')
        })
}

/// A row's cells as written, the pipes at its ends taken off, a `\|` kept as a pipe.
fn split(line: &str) -> Vec<String> {
    let mut t = line.trim();
    t = t.strip_prefix('|').unwrap_or(t);
    t = t.strip_suffix('|').filter(|_| !t.ends_with("\\|")).unwrap_or(t);
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cell.push('|');
                chars.next();
            }
            '|' => cells.push(std::mem::take(&mut cell)),
            c => cell.push(c),
        }
    }
    cells.push(cell);
    cells
}

/// A row's `width` cells read, a cell missing empty and one over the width dropped.
fn cells(line: &str, width: usize) -> Vec<Line> {
    let mut cells = split(line);
    cells.resize(width, String::new());
    cells.iter().map(|c| inline(c.trim())).collect()
}

/// A list item: its level from its indentation (two spaces, or a tab, a level), whether it is
/// numbered, and its words.
fn item(line: &str) -> Option<(u8, bool, &str)> {
    let indent: usize =
        line.chars().take_while(|c| *c == ' ' || *c == '\t').map(|c| if c == '\t' { 2 } else { 1 }).sum();
    let level = (indent / 2).min(7) as u8;
    let rest = line.trim_start();
    for mark in ["- ", "* ", "+ ", "• "] {
        if let Some(words) = rest.strip_prefix(mark) {
            return Some((level, false, words.trim_start()));
        }
    }
    if rest == "-" || rest == "*" || rest == "+" {
        return Some((level, false, ""));
    }
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=3).contains(&digits) {
        let after = &rest[digits..];
        if let Some(words) = after.strip_prefix(". ").or_else(|| after.strip_prefix(") ")) {
            return Some((level, true, words.trim_start()));
        }
    }
    None
}

/// A heading's words, where `line` is one.
fn heading(line: &str) -> Option<&str> {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        return Some(line[hashes..].trim().trim_end_matches('#').trim_end());
    }
    None
}

/// A line's words in their looks: `**bold**` and `__bold__`, `*italic*` and `_italic_`,
/// `` `code` ``, a link's words without its address, and `\` keeping the character after it.
fn inline(text: &str) -> Line {
    let chars: Vec<char> = text.chars().collect();
    let mut spans: Line = Vec::new();
    let mut run = String::new();
    let (mut bold, mut italic) = (false, false);
    let mut i = 0;
    let push = |spans: &mut Line, run: &mut String, bold: bool, italic: bool, code: bool| {
        if run.is_empty() {
            return;
        }
        let text = std::mem::take(run);
        match spans.last_mut() {
            Some(last) if last.bold == bold && last.italic == italic && last.code == code => last.text.push_str(&text),
            _ => spans.push(Span { text, bold, italic, code }),
        }
    };
    // Whether a run of `n` of `mark` closes ahead, past `from`.
    let closes = |from: usize, mark: char, n: usize| -> bool {
        let mut j = from;
        while j + n <= chars.len() {
            if chars[j..j + n].iter().all(|c| *c == mark)
                && (j == 0 || !chars[j - 1].is_whitespace())
                && (mark == '*' || j + n == chars.len() || !chars[j + n].is_alphanumeric())
            {
                return true;
            }
            j += 1;
        }
        false
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if i + 1 < chars.len() => {
                run.push(chars[i + 1]);
                i += 2;
            }
            '`' => {
                let end = (i + 1..chars.len()).find(|&j| chars[j] == '`');
                match end {
                    Some(end) => {
                        push(&mut spans, &mut run, bold, italic, false);
                        let mut code: String = chars[i + 1..end].iter().collect();
                        code = code.trim().to_string();
                        push(&mut spans, &mut code, bold, italic, true);
                        i = end + 1;
                    }
                    None => {
                        run.push(c);
                        i += 1;
                    }
                }
            }
            '*' | '_' => {
                let n = if i + 1 < chars.len() && chars[i + 1] == c { 2 } else { 1 };
                let opens = (i + n < chars.len() && !chars[i + n].is_whitespace())
                    && (c == '*' || i == 0 || !chars[i - 1].is_alphanumeric());
                let on = if n == 2 { bold } else { italic };
                let before = i > 0 && !chars[i - 1].is_whitespace();
                if on && before {
                    push(&mut spans, &mut run, bold, italic, false);
                    if n == 2 {
                        bold = false
                    } else {
                        italic = false
                    }
                    i += n;
                } else if !on && opens && closes(i + n, c, n) {
                    push(&mut spans, &mut run, bold, italic, false);
                    if n == 2 {
                        bold = true
                    } else {
                        italic = true
                    }
                    i += n;
                } else {
                    run.extend(std::iter::repeat_n(c, n));
                    i += n;
                }
            }
            '[' => {
                // `[words](address)`: the words alone.
                let close = (i + 1..chars.len()).find(|&j| chars[j] == ']');
                match close {
                    Some(close) if chars.get(close + 1) == Some(&'(') => {
                        if let Some(end) = (close + 2..chars.len()).find(|&j| chars[j] == ')') {
                            run.extend(chars[i + 1..close].iter());
                            i = end + 1;
                            continue;
                        }
                        run.push(c);
                        i += 1;
                    }
                    _ => {
                        run.push(c);
                        i += 1;
                    }
                }
            }
            c => {
                run.push(c);
                i += 1;
            }
        }
    }
    push(&mut spans, &mut run, bold, italic, false);
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> Span {
        Span { text: t.into(), bold: false, italic: false, code: false }
    }

    #[test]
    fn plain_notes_are_their_lines_as_written() {
        let blocks = read("Pause on the Q3 bar.\n\nThen the mix shift: Pro went from a fifth to a third.\n");
        assert_eq!(
            blocks,
            [Block::Lines {
                lines: vec![
                    vec![text("Pause on the Q3 bar.")],
                    vec![],
                    vec![text("Then the mix shift: Pro went from a fifth to a third.")]
                ]
            }]
        );
        assert_eq!(plain(&blocks), "Pause on the Q3 bar.\n\nThen the mix shift: Pro went from a fifth to a third.");
        assert!(read("").is_empty() && read("\n\n").is_empty());
    }

    #[test]
    fn bullets_are_a_list_and_a_table_is_a_table() {
        let notes = "Open with the number.\n- Lead with **why**: the *market* moved\n- Then `how`\n  - nested\n\n| Option | Cost | Speed |\n|---|:--:|---|\n| A | low | high |\n| B | high \\| mid | low |\nClose on the ask.";
        let blocks = read(notes);
        assert_eq!(blocks.len(), 4, "{blocks:?}");
        assert_eq!(blocks[0], Block::Lines { lines: vec![vec![text("Open with the number.")]] });
        let Block::List { ordered, items } = &blocks[1] else { panic!("{blocks:?}") };
        assert!(!ordered);
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[0].spans,
            [
                text("Lead with "),
                Span { text: "why".into(), bold: true, italic: false, code: false },
                text(": the "),
                Span { text: "market".into(), bold: false, italic: true, code: false },
                text(" moved")
            ]
        );
        assert_eq!(
            items[1].spans,
            [text("Then "), Span { text: "how".into(), bold: false, italic: false, code: true }]
        );
        assert_eq!((items[2].level, items[2].spans.as_slice()), (1, &[text("nested")][..]));
        let Block::Table { header, rows } = &blocks[2] else { panic!("{blocks:?}") };
        assert!(*header);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], [vec![text("Option")], vec![text("Cost")], vec![text("Speed")]]);
        assert_eq!(rows[2][1], [text("high | mid")]);
        assert_eq!(blocks[3], Block::Lines { lines: vec![vec![text("Close on the ask.")]] });
        assert_eq!(
            plain(&blocks),
            "Open with the number.\n• Lead with why: the market moved\n• Then how\n  • nested\nOption · Cost · Speed\nA · low · high\nB · high | mid · low\nClose on the ask."
        );
    }

    #[test]
    fn numbers_headings_links_and_lone_marks() {
        let blocks = read(
            "## Key points\n1. First\n2) Second\n\n3. Third\nSee [the brief](https://x.y) at 3 * 4 and snake_case_names.",
        );
        let Block::Lines { lines } = &blocks[0] else { panic!("{blocks:?}") };
        assert_eq!(lines[0], [Span { text: "Key points".into(), bold: true, italic: false, code: false }]);
        let Block::List { ordered, items } = &blocks[1] else { panic!("{blocks:?}") };
        assert!(*ordered && items.len() == 3, "{items:?}");
        assert_eq!(plain(&blocks[1..2]), "1. First\n2. Second\n3. Third");
        let Block::Lines { lines } = &blocks[2] else { panic!("{blocks:?}") };
        assert_eq!(lines[0], [text("See the brief at 3 * 4 and snake_case_names.")]);
        // A mark with no close stays as written; one escaped too.
        assert_eq!(read("a **b and \\*c\\*")[0], Block::Lines { lines: vec![vec![text("a **b and *c*")]] });
    }

    #[test]
    fn the_blocks_serialize_small() {
        let json = serde_json::to_value(read("- a\n\n| h |\n|-|\n| c |")).unwrap();
        assert_eq!(
            json,
            serde_json::json!([
                { "kind": "list", "ordered": false, "items": [{ "level": 0, "spans": [{ "text": "a" }] }] },
                { "kind": "table", "header": true, "rows": [[[{ "text": "h" }]], [[{ "text": "c" }]]] }
            ])
        );
    }
}
