//! Where a caret stands in a text at rest, and what a selection covers (ADR-0013): each
//! character of the text as written, on its line, between the edges of the glyphs the
//! engine set for it. A client that edits a text where it stands (the web editor, the Mac
//! app) draws its caret and selection from these and puts a caret where a pointer is. It
//! lays nothing out, and `hit` says the character under a point from the same table.
//!
//! A character is what a reader counts as one: a grapheme cluster (a letter with its
//! accents, an emoji with its modifiers, a flag), so a caret never stands inside one. Its
//! edges come from the shaper's clusters: the glyphs set for them, from the left of the
//! first to the advance of the last. A cluster of several characters that take room (a
//! ligature) shares its width among them, in the text's direction. A character that sets
//! no glyph of its own (a soft hyphen, a line break) stands at a point after the characters
//! before it. Text the engine sets in another case, or hyphenates, maps back to the
//! characters as written ([`TextLayout::offsets`]).

use crate::text::{SpanLook, TextAlign, TextLayout, sets_ink};
use icu_segmenter::GraphemeClusterSegmenter;
use scaena_core::displaylist::Rect;
use std::collections::BTreeMap;

/// A text's characters as written, where each stands, line by line.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Carets {
    /// The text as written: the node's `text`, or its runs' texts end to end. Offsets are
    /// its bytes.
    pub text: String,
    /// Its lines, top to bottom.
    pub lines: Vec<CaretLine>,
    /// Its spans as written, in order, and the weight each is set in (PLAN 2.38).
    pub looks: Vec<SpanLook>,
}

/// The weight from which a text reads as bold: CSS's `bold` is 700, and 600 is the first
/// weight a reader takes for bold.
pub const BOLD: f32 = 600.0;

/// One line of a text, as a caret sees it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CaretLine {
    /// Its line box's top and bottom, canvas units: what a caret on it spans.
    pub top: f32,
    pub bottom: f32,
    /// Where a caret stands on it when it holds no character: its start edge, where its
    /// alignment puts it.
    pub x: f32,
    /// Where it starts in the text as written, and where the next line starts.
    pub start: usize,
    pub end: usize,
    /// It ends at a line break the text sets: a caret after the break stands on the next
    /// line, never at this one's end.
    pub broken: bool,
    /// Its characters, in the text's order.
    pub chars: Vec<CaretChar>,
}

/// A character of the text as written, as a reader counts it (a grapheme cluster), where it
/// stands on its line.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct CaretChar {
    /// Its offset in the text as written.
    pub offset: usize,
    /// Where a caret before it stands, canvas x, and one after it: its left and right
    /// edges, or its right and left in right-to-left text. Both are one point for a
    /// character that sets no glyph of its own.
    pub lead: f32,
    pub trail: f32,
}

/// Where a caret at an offset stands.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Caret {
    pub line: usize,
    /// Canvas x, and the line box's top and bottom.
    pub x: f32,
    pub top: f32,
    pub bottom: f32,
}

/// A character that ends a line wherever it stands (UAX #14's mandatory breaks).
pub fn breaks(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{B}' | '\u{C}' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// Whether `c` takes room on its line: it sets ink, or it is a space.
fn takes_room(c: char) -> bool {
    sets_ink(c) || (c.is_whitespace() && !breaks(c))
}

impl TextLayout {
    /// Each character of the text as written, where it stands, the text's top-left corner
    /// at `origin` (canvas units).
    pub fn carets(&self, origin: [f32; 2]) -> Carets {
        let [ox, oy] = origin;
        // Each cluster's left and right, its direction, and its line, by where it starts in
        // the text as laid out. The hyphen drawn at a break is no character's.
        let mut clusters: BTreeMap<usize, (f32, f32, bool)> = BTreeMap::new();
        for run in self.runs.iter().filter(|r| !r.hyphen) {
            for ((glyph, &start), &advance) in run.glyphs.iter().zip(&run.clusters).zip(&run.advances) {
                let (left, right) = (glyph.x + ox, glyph.x + advance + ox);
                let edges = clusters.entry(start).or_insert((left, right, run.rtl));
                (edges.0, edges.1) = (edges.0.min(left), edges.1.max(right));
            }
        }
        // Each character as laid out: its offset, its edges, and its line.
        let mut laid: Vec<(usize, f32, f32, usize)> = Vec::with_capacity(self.text.len());
        for (index, line) in self.lines.iter().enumerate() {
            let range = line.text.start.min(self.text.len())..line.text.end.min(self.text.len());
            let starts: Vec<usize> = clusters.range(range.clone()).map(|(&s, _)| s).collect();
            // What stands before the line's first cluster sets nothing: where that cluster
            // starts, or where the line's alignment puts an empty line.
            let first = match starts.first().map(|s| clusters[s]) {
                Some((left, right, rtl)) => {
                    if rtl {
                        right
                    } else {
                        left
                    }
                }
                None => self.anchor(index) + ox,
            };
            let lead_in = starts.first().copied().unwrap_or(range.end);
            for (i, _) in self.text[range.start..lead_in].char_indices() {
                laid.push((range.start + i, first, first, index));
            }
            for (k, &start) in starts.iter().enumerate() {
                let end = starts.get(k + 1).copied().unwrap_or(range.end);
                let (left, right, rtl) = clusters[&start];
                let cluster = &self.text[start..end];
                let room = cluster.chars().filter(|&c| takes_room(c)).count().max(1) as f32;
                let share = (right - left) / room * if rtl { -1.0 } else { 1.0 };
                let mut at = if rtl { right } else { left };
                for (i, c) in cluster.char_indices() {
                    let lead = at;
                    if takes_room(c) {
                        at += share;
                    }
                    laid.push((start + i, lead, at, index));
                }
            }
        }
        // The characters as written, as a reader counts them, each from where the first
        // character case set it as stands to the far edge of the last.
        let mut lines: Vec<CaretLine> = self
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| CaretLine {
                top: line.top + oy,
                bottom: line.top + line.height + oy,
                x: self.anchor(index) + ox,
                start: self.written_at(line.text.start),
                end: self.written_at(line.text.end),
                broken: self.text.get(line.text.clone()).and_then(|t| t.chars().next_back()).is_some_and(breaks),
                chars: Vec::new(),
            })
            .collect();
        let laid_at = |written: usize| {
            let i = self.offsets.partition_point(|&(w, _)| w < written);
            self.offsets.get(i).map_or(self.text.len(), |&(_, l)| l)
        };
        let bounds: Vec<usize> = GraphemeClusterSegmenter::new().segment_str(&self.written).collect();
        let mut at = 0;
        for pair in bounds.windows(2) {
            let (offset, from, to) = (pair[0], laid_at(pair[0]), laid_at(pair[1]));
            while laid.get(at).is_some_and(|c| c.0 < from) {
                at += 1;
            }
            let Some(&(_, lead, _, line)) = laid.get(at) else { break };
            let mut trail = lead;
            for &(_, _, end, on) in laid[at..].iter().take_while(|c| c.0 < to) {
                if on == line {
                    trail = end;
                }
            }
            lines[line].chars.push(CaretChar { offset, lead, trail });
        }
        if let Some(last) = lines.last_mut() {
            last.end = self.written.len();
        }
        Carets { text: self.written.clone(), lines, looks: self.looks.clone() }
    }

    /// Where line `index` is anchored, from the text's left edge: its start edge, its middle,
    /// or its end edge, as it aligns. A caret on an empty line stands there.
    fn anchor(&self, index: usize) -> f32 {
        let line = &self.lines[index];
        match (self.align, self.rtl) {
            (TextAlign::Start, false) | (TextAlign::End, true) => line.x,
            (TextAlign::Center, _) => line.x + 0.5 * line.width,
            (TextAlign::Start, true) | (TextAlign::End, false) => line.x + line.width,
        }
    }

    /// The offset in the text as written of what starts at `laid` in the text as laid out:
    /// the first character that starts there or after.
    fn written_at(&self, laid: usize) -> usize {
        let i = self.offsets.partition_point(|&(_, l)| l < laid);
        self.offsets.get(i).map_or(self.written.len(), |&(w, _)| w)
    }
}

impl CaretLine {
    /// Whether a caret at `offset` can stand on this line: from its start to its end, but
    /// not past the line break it ends with.
    pub fn holds(&self, offset: usize) -> bool {
        self.start <= offset && (offset < self.end || (offset == self.end && !self.broken))
    }

    /// Where the line ends for a caret: before the line break it ends with, if it does.
    pub fn last(&self) -> usize {
        match (self.broken, self.chars.last()) {
            (true, Some(c)) => c.offset,
            _ => self.end,
        }
    }
}

impl Carets {
    /// What ⌘B gives the characters from `from` to `to` (bytes of the text as written),
    /// as `style_text`'s `look` (ADR-0013, PLAN 2.38): bold, `style/weight` 700, unless
    /// every one of them is bold already ([`BOLD`]). Then their weight is taken away
    /// (`null`) where that leaves each of them below bold, and is 400 where their role is
    /// bold itself.
    pub fn bolding(&self, from: usize, to: usize) -> serde_json::Value {
        let covered = self.covered(from, to);
        let weight = if covered.is_empty() || covered.iter().any(|l| l.weight < BOLD) {
            serde_json::json!(700)
        } else if covered.iter().all(|l| l.base < BOLD) {
            serde_json::Value::Null
        } else {
            serde_json::json!(400)
        };
        serde_json::json!({ "style/weight": weight })
    }

    /// What ⌘I gives the characters from `from` to `to` (bytes of the text as written), as
    /// `style_text`'s `look` (PLAN 2.40): italic, `style/italic` true, unless every one of
    /// them asks for it already. Then their own italic is taken away (`null`) where that
    /// leaves each of them upright, and is `false` where their role is italic itself. Italic
    /// as asked, not as set: a family without an italic face sets it upright (lint W231).
    pub fn italicizing(&self, from: usize, to: usize) -> serde_json::Value {
        let covered = self.covered(from, to);
        let italic = if covered.is_empty() || covered.iter().any(|l| !l.italic) {
            serde_json::json!(true)
        } else if covered.iter().all(|l| !l.base_italic) {
            serde_json::Value::Null
        } else {
            serde_json::json!(false)
        };
        serde_json::json!({ "style/italic": italic })
    }

    /// The looks of the spans that hold any of the characters from `from` to `to`.
    fn covered(&self, from: usize, to: usize) -> Vec<&SpanLook> {
        let mut start = 0;
        let mut covered = Vec::new();
        for look in &self.looks {
            if start < look.end && start < to && look.end > from {
                covered.push(look);
            }
            start = look.end;
        }
        covered
    }

    /// The line a caret at `offset` stands on: `on`, if it can stand there; otherwise the
    /// last line that holds it, so a caret where a line wraps starts the next one.
    pub fn line_of(&self, offset: usize, on: Option<usize>) -> usize {
        if let Some(l) = on.filter(|&l| self.lines.get(l).is_some_and(|line| line.holds(offset))) {
            return l;
        }
        let last = self.lines.len().saturating_sub(1);
        self.lines.iter().rposition(|line| line.holds(offset)).unwrap_or(if offset == 0 { 0 } else { last })
    }

    /// Where a caret at `offset` stands, on `on` if it can stand there (a caret moved by a
    /// line keeps to it). An offset inside a character stands before it.
    pub fn caret(&self, offset: usize, on: Option<usize>) -> Option<Caret> {
        let l = self.line_of(offset, on);
        let line = self.lines.get(l)?;
        let i = line.chars.partition_point(|c| c.offset <= offset);
        let x = match i.checked_sub(1).map(|i| line.chars[i]) {
            None => line.chars.first().map_or(line.x, |c| c.lead),
            Some(c) if c.offset < offset && i == line.chars.len() && offset >= line.end => c.trail,
            Some(c) => c.lead,
        };
        Some(Caret { line: l, x, top: line.top, bottom: line.bottom })
    }

    /// The caret nearest `point`: on the line whose box holds its y (the first above them
    /// all, the last below), where it stands nearest the point's x ([`Carets::on_line`]).
    /// Its offset, and the line it stands on.
    pub fn at(&self, [x, y]: [f32; 2]) -> (usize, usize) {
        let l = self.lines.iter().position(|line| y < line.bottom).unwrap_or(self.lines.len().saturating_sub(1));
        (self.on_line(l, x), l)
    }

    /// The offset on line `l` whose caret stands nearest canvas `x`: before one of its
    /// characters, or after the last, the first in the text's order where two are as near,
    /// or its start when it holds none. Where right-to-left and left-to-right text meet, an
    /// edge can be two offsets' or none's: the caret goes where it is drawn. A caret moving
    /// up or down a line asks here.
    pub fn on_line(&self, l: usize, x: f32) -> usize {
        let Some(line) = self.lines.get(l) else { return self.text.len() };
        let after = line.chars.last().filter(|_| line.holds(line.end)).map(|c| (c.trail, line.end));
        let stops = line.chars.iter().map(|c| (c.lead, c.offset)).chain(after);
        let mut best = (f32::INFINITY, line.start);
        for (at, offset) in stops {
            if (at - x).abs() < best.0 {
                best = ((at - x).abs(), offset);
            }
        }
        best.1
    }

    /// What a selection from `from` to `to` covers: a rectangle per stretch of adjacent
    /// characters on a line, left to right, top to bottom. Characters that set no glyph
    /// cover nothing.
    pub fn selection(&self, from: usize, to: usize) -> Vec<Rect> {
        let (from, to) = (from.min(to), from.max(to));
        let mut out = Vec::new();
        for line in &self.lines {
            let mut spans: Vec<(f32, f32)> = line
                .chars
                .iter()
                .filter(|c| from <= c.offset && c.offset < to && c.lead != c.trail)
                .map(|c| (c.lead.min(c.trail), c.lead.max(c.trail)))
                .collect();
            scaena_core::sort::by(&mut spans, |a, b| a.0.total_cmp(&b.0));
            let mut merged: Vec<(f32, f32)> = Vec::new();
            for (left, right) in spans {
                match merged.last_mut() {
                    Some(last) if left <= last.1 + 0.5 => last.1 = last.1.max(right),
                    _ => merged.push((left, right)),
                }
            }
            out.extend(merged.into_iter().map(|(l, r)| [l, line.top, r - l, line.bottom - line.top]));
        }
        out
    }
}
