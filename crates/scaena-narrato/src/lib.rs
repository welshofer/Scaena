//! # scaena-narrato
//!
//! Vzestup's Narrato, ported (ADR-0026, PLAN G.1): a deck's narrative from a brief, by two calls
//! to the user's own model. The first picks one of 13 narrative arcs; the second writes the
//! slides along it, one JSON document under a schema. Both prompts are Narrato's, byte for byte.
//!
//! It makes no HTTP call, reads no clock, and keeps no key: a [`Generation`] gives the client
//! each request to make and reads the answer it hands back, as `scaena-chat` does for the
//! assistant (ADR-0022). A failed arc selection falls back to the heuristic, as Vzestup's does;
//! a failed generation is an error that says why.

pub mod answer;
pub mod arcs;
pub mod beats;
pub mod brief;
pub mod containment;
pub mod duration;
pub mod fit;
pub mod health;
pub mod heuristic;
mod js;
pub mod layouts;
pub mod look;
pub mod notes;
pub mod one;
pub mod outline;
pub mod pictures;
pub mod prompt;
pub mod providers;
pub mod research;
pub mod schema;

pub use answer::{ChartData, Series, Slide};
pub use brief::{Brief, BriefError};
pub use heuristic::{ArcChoice, Candidate};
pub use outline::Outline;
pub use providers::{Answer, Document, Picture, Provider, Request, Stop, Usage};

use serde::{Deserialize, Serialize};

/// Room for arc selection's answer, thinking included (Vzestup's `ARC_SELECT_MAX_TOKENS`).
pub const ARC_MAX_TOKENS: u32 = 16_000;
/// Room for the slides, thinking included: a 45-slide deck is some 30 KB of JSON (Vzestup's
/// `SLIDES_MAX_TOKENS`).
pub const SLIDES_MAX_TOKENS: u32 = 64_000;
/// Room for Gemini's answers, either: the most it writes (Vzestup's `maxOutputTokens`).
pub const GEMINI_MAX_TOKENS: u32 = 65_536;
/// Room for the rewrites of texts that run past their place, thinking included.
pub const FIT_MAX_TOKENS: u32 = 16_000;
/// Room for what the model would change on the deck it sees, thinking included.
pub const LOOK_MAX_TOKENS: u32 = 16_000;
/// Room for a dozen slides' notes, each a few hundred words, in any language.
pub const NOTES_MAX_TOKENS: u32 = 16_000;

/// How many times a step is asked while its provider is busy or does not answer (narrative
/// REL-1): the pictures' six, so that one busy answer on the dearest call ends no run.
pub const TRIES: usize = 6;

/// How many times the outline is asked where its answer comes back but does not read: once more,
/// then the person decides (lift-up REL-3).
pub const OUTLINE_READS: usize = 2;

/// The longest wait before a step is asked again, in seconds.
pub const LONGEST_WAIT: f64 = 60.0;
/// How many times the rest of an answer cut short is asked for (lift-up REL-2): each a call
/// for the slides still to come, written on from those already in.
pub const RESTS: usize = 2;

/// The figures `text` writes, as numbers: "$1,140" is 1140, "4.9%" is 4.9, "2024" is 2024.
pub fn figures(text: &str) -> Vec<f64> {
    grouped(text)
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .map(|w| w.trim_matches(['.', ',']).replace(',', ""))
        .filter(|w| !w.is_empty())
        .filter_map(|w| w.parse().ok())
        .collect()
}

/// `text` with the spaces that group a number's thousands taken out, as [`figure_end`] reads a
/// number ("1 240 000", "184\u{202f}500"), so each is one figure, never 1, 240, and 0 (W1's
/// dogfood 19); a year and a count ("2025 120") stay apart.
fn grouped(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let starts =
            chars[i].is_ascii_digit() && (i == 0 || !(chars[i - 1].is_ascii_digit() || ".,".contains(chars[i - 1])));
        if !starts {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let end = figure_end(&chars, i).max(i + 1);
        out.extend(chars[i..end].iter().filter(|c| !GROUPING.contains(c)));
        i = end;
    }
    out
}

/// The spaces a number's thousands are grouped by: a plain space as French is typed, and the
/// no-break, narrow no-break, and thin spaces typesetters set.
pub const GROUPING: [char; 4] = [' ', '\u{a0}', '\u{202f}', '\u{2009}'];

/// Where the number whose first digit is at `start` ends: its digits, the marks between them,
/// and the narrow spaces a language groups thousands by ("1\u{202f}200"), none at its end. A
/// plain space groups them too, as French is typed ("1 240 000"), where every group before it is
/// of digits alone, the first of one to three and the rest of three: never a year and a count
/// ("2025 120"), nor a decimal and what follows it ("2,4 000"), nor a group a letter or a hyphen
/// runs on from ("5 100-metre") (agent dogfood 4). The builder sets a figure by it, and health and
/// a source read one by it, so all three read "1 240 000" as one figure (W1's dogfood 19).
pub fn figure_end(chars: &[char], start: usize) -> usize {
    let mut end = start;
    while end < chars.len() {
        let c = chars[end];
        let three = (1..=3).all(|k| chars.get(end + k).is_some_and(char::is_ascii_digit))
            && !chars.get(end + 4).is_some_and(char::is_ascii_digit);
        let grouped = matches!(c, '\u{a0}' | '\u{202f}' | '\u{2009}') && end > start && three;
        // A group of three a letter or a hyphen runs on from is a figure of its own: "5 100-metre
        // sprints" and "10 100m heats" are two, in English and French alike (agent dogfood 4).
        let ends = chars.get(end + 4).is_none_or(|n| !(n.is_alphabetic() || *n == '-'));
        let typed = c == ' ' && end > start && three && ends && !(start > 0 && chars[start - 1].is_alphanumeric()) && {
            // The groups before this space: digits alone, split by plain spaces, the first of one
            // to three and each after it of three.
            let before: String = chars[start..end].iter().collect();
            let groups: Vec<&str> = before.split(' ').collect();
            (groups.iter().all(|g| !g.is_empty() && g.chars().all(|c| c.is_ascii_digit())))
                && groups[0].len() <= 3
                && groups[1..].iter().all(|g| g.len() == 3)
        };
        if !(c.is_ascii_digit() || matches!(c, ',' | '.') || grouped || typed) {
            break;
        }
        end += 1;
    }
    while end > start && matches!(chars[end - 1], ',' | '.') {
        end -= 1;
    }
    end
}

/// Whether `now` writes the figures `was` does, each as often: none dropped, none added, none
/// changed (lift-up REL-2). A rewrite that rounds 41% to "about 40%" or drops a year says
/// something else, however much shorter it is.
pub fn same_figures(was: &str, now: &str) -> bool {
    let sorted = |text: &str| {
        let mut f = figures(text);
        f.sort_unstable_by(f64::total_cmp);
        f
    };
    sorted(was) == sorted(now)
}

/// Whether `now` writes every figure `was` does, each as often, and adds only figures `also`
/// writes: a slide's title rewritten to state a figure its own words give (W1 dogfood 13).
pub fn figures_from(was: &str, now: &str, also: &str) -> bool {
    let mut gained = figures(now);
    for f in figures(was) {
        match gained.iter().position(|g| *g == f) {
            Some(at) => {
                gained.remove(at);
            }
            None => return false,
        }
    }
    let also = figures(also);
    gained.iter().all(|g| also.contains(g))
}

/// Why `now` does not write the figures `was` does, in a person's words (W1 dogfood 9): "it
/// writes 3.4 where the slide wrote 3,400", "it drops 2,475, which the slide wrote", "it adds
/// 40, which the slide did not write". Each figure as `was` or `now` writes it.
pub fn unlike_figures(was: &str, now: &str) -> String {
    unlike_figures_from(was, now, "")
}

/// Why `now` does not write the figures `was` does, where it may also add any `also` writes
/// ([`figures_from`]): only a figure neither writes is said as added.
pub fn unlike_figures_from(was: &str, now: &str, also: &str) -> String {
    let also = figures(also);
    let written = |text: &str| -> Vec<(String, f64)> {
        text.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
            .map(|w| w.trim_matches(['.', ',']))
            .filter_map(|w| w.replace(',', "").parse().ok().map(|n| (w.to_string(), n)))
            .collect()
    };
    let (mut lost, mut gained) = (written(was), written(now));
    lost.retain(|(_, n)| match gained.iter().position(|(_, m)| m == n) {
        Some(at) => {
            gained.remove(at);
            false
        }
        None => true,
    });
    if lost.is_empty() {
        gained.retain(|(_, n)| !also.contains(n));
    }
    let said = |figures: &[(String, f64)]| -> String {
        let words: Vec<&str> = figures.iter().map(|(w, _)| w.as_str()).collect();
        match words.as_slice() {
            [rest @ .., last] if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
            _ => words.concat(),
        }
    };
    match (lost.is_empty(), gained.is_empty()) {
        (false, false) => format!("it writes {} where the slide wrote {}", said(&gained), said(&lost)),
        (false, true) => format!("it drops {}, which the slide wrote", said(&lost)),
        (true, false) => format!("it adds {}, which the slide did not write", said(&gained)),
        (true, true) => "it writes the slide's figures otherwise".to_string(),
    }
}

/// Whether `status` answers for now, not for good: no answer (0), a timeout, a rate limit, or a
/// server's failure, Anthropic's 529 and Cloudflare's edge among them.
pub fn busy(status: u16) -> bool {
    matches!(status, 0 | 408 | 429 | 500..=599)
}

/// What a provider says when the key is out of credit (Vzestup's signatures, and Anthropic's).
const NO_CREDIT: [&str; 5] = [
    "insufficient_quota",
    "billing_hard_limit_reached",
    "exceeded your current quota",
    "check your plan and billing",
    "credit balance is too low",
];

/// Whether a failed answer says the key is out of credit, which waiting does not mend, though it
/// comes as a 429 as a rate limit does (lift-up 20261110 REL-2): no step is asked again for it.
pub fn for_good(status: u16, body: &str) -> bool {
    let lower = body.to_lowercase();
    !(200..300).contains(&status) && NO_CREDIT.iter().any(|s| lower.contains(s))
}

/// How long to wait before asking a busy provider again for a look or a fit (lift-up REL-1),
/// as the slides and the pictures are asked: `tried` the times it was asked so far, from 0. None
/// where `status` answers for good, out of credit among it ([`for_good`]), or [`TRIES`] are spent.
pub fn retry_after(status: u16, body: &str, after: Option<&str>, tried: usize) -> Option<f64> {
    (busy(status) && !for_good(status, body) && tried + 1 < TRIES).then(|| providers::wait(after, body, tried + 1))
}

/// Who writes the deck: a provider, the model the user picked from those their key lists, and
/// where its API is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub provider: Provider,
    pub model: String,
    /// The API's address, where it is not the provider's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Whether a browser makes the calls.
    #[serde(default)]
    pub browser: bool,
}

/// The deck's narrative: its arc, why, its executive summary, and its slides, as Narrato writes
/// them; and the brief it was written for, its sources left out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Narrative {
    pub arc: String,
    pub arc_reasoning: String,
    pub executive_summary: String,
    pub slides: Vec<Slide>,
    /// Recovered from an answer cut short: what closed of it, enough for the length.
    pub truncated: bool,
    pub brief: Brief,
    /// The icons the slides' words name, each by the word (PLAN G.23), as the set that ships
    /// draws them: what the client found, which [`Generation::give_icons`] hands over. A word with
    /// none has no icon.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub icons: std::collections::BTreeMap<String, Icon>,
    /// What the deck was grounded in, each source as the prompt named it (narrative FUNC-3): what
    /// the client gave, which [`Generation::give_sources`] hands over. The deck keeps them beside
    /// its brief, and each beat cites the one its slide names ([`Slide::source_ref`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<Provenance>,
    /// The outline the slides were written to, where the person settled one first (narrative
    /// FUNC-1): the deck's sections follow its beats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Outline>,
}

/// Where a source the deck was grounded in came from (narrative FUNC-3): its name, as the prompt
/// named it; its kind, in a word (`markdown`, `pdf`, `page`, …); its address, where it has one;
/// and what of it the prompt was given, in bytes, and whether it was cut short to fit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub bytes: usize,
    pub truncated: bool,
    /// The figures its words and cells hold, each value once and in order, written plainly
    /// ("7560", "25.8"), never its words: what health holds a slide that cites it to (W1's
    /// dogfood 17). None where it holds more than [`MOST_FIGURES`], or where the deck was made
    /// before format 0.41.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub figures: Vec<String>,
}

/// The most figures a source keeps in the deck: a source that holds more keeps none, and health
/// reads the slides that cite it as it reads a deck made before it kept any.
pub const MOST_FIGURES: usize = 10_000;

/// The figures `text` holds, as a source keeps them ([`Provenance::figures`]): its words' and each
/// cell's, each value once, in order, written plainly; none past [`MOST_FIGURES`].
/// A line of cells, between commas, semicolons, or tabs, of four words at most each is a sheet's
/// row, read cell by cell, so `2021,182000,48` holds 2021, 182000, and 48, never one long number;
/// any other line is read whole, so "removed 7,560 street trees" holds 7560 alone.
/// A sheet keeps each column's total too, where two or more of its cells write a figure, to the
/// places its cells write, a column of years aside: a pilot's five months of litres served hold
/// the 66400 a slide sums them to (R1's agent dogfood 12).
/// Each figure is read in the marks the source writes its figures in, where they tell
/// ([`decimal_mark`]): a Spanish report's "3.800 €" is 3800 and its "46,2 °C" 46.2, an English
/// one's "3,800" 3800 (R1's agent dogfood 12). Where they do not, its words are read in
/// `deck`'s decimal mark, the language the deck is grounded in, and a sheet's cells as written
/// plainly, as a sheet's numbers are.
pub fn source_figures(text: &str, deck: Option<char>) -> Vec<String> {
    let own = decimal_mark(text);
    let (words, cells) = (own.or(deck).unwrap_or('.'), own.unwrap_or('.'));
    let mut values: Vec<f64> = Vec::new();
    // Each column of the sheet being read, by its place in the row: the sum of the cells that
    // write one figure, how many do, the most decimal places one writes, and whether each is a
    // year, whose sum is no figure.
    let mut columns: Vec<(f64, usize, usize, bool)> = Vec::new();
    let total = |columns: &mut Vec<(f64, usize, usize, bool)>, values: &mut Vec<f64>| {
        for (sum, count, places, years) in columns.drain(..) {
            if count >= 2 && !years {
                let scale = 10f64.powi(places as i32);
                values.push((sum * scale).round() / scale);
            }
        }
    };
    for line in text.lines() {
        let cut = split_cells(line, cells);
        match cut.len() > 1 && cut.iter().all(|c| c.split_whitespace().count() <= 4) {
            true => {
                // A sheet's header names its columns: its digits are no figure ("toitures_m2",
                // W1's dogfood 19).
                let header = columns.is_empty() && cut.iter().all(|c| c.chars().any(char::is_alphabetic));
                if header {
                    continue;
                }
                if columns.len() < cut.len() {
                    columns.resize(cut.len(), (0.0, 0, 0, true));
                }
                for (k, cell) in cut.iter().enumerate() {
                    // A date's parts are no figures: a month's 2025-05 keeps no 5 (R1's agent
                    // dogfood 12).
                    if date(cell.trim()) {
                        continue;
                    }
                    let read = figures_in_marks(cell, cells);
                    if let [one] = read.as_slice() {
                        let places = format!("{one}").split_once('.').map_or(0, |(_, d)| d.len());
                        let column = &mut columns[k];
                        let year = one.fract() == 0.0 && (1800.0..=2200.0).contains(one);
                        *column = (column.0 + one, column.1 + 1, column.2.max(places), column.3 && year);
                    }
                    values.extend(read);
                }
            }
            false => {
                // A line that is no row ends the sheet: its columns are summed.
                total(&mut columns, &mut values);
                values.extend(figures_in_marks(line, words));
            }
        }
    }
    total(&mut columns, &mut values);
    values.sort_unstable_by(f64::total_cmp);
    values.dedup();
    match values.len() > MOST_FIGURES {
        true => Vec::new(),
        false => values.iter().map(|v| format!("{v}")).collect(),
    }
}

/// Whether a sheet's cell is a date: a year then its month and day, set apart by hyphens or
/// slashes ("2025-05", "2025-05-14", "2025/05"), or a day and month then a year ("14.05.2025",
/// "14/05/25"); never a decimal ("1530.2", "2025.5").
fn date(cell: &str) -> bool {
    let parts: Vec<&str> = cell.split(['-', '/', '.']).collect();
    let digits = |p: &str, lens: std::ops::RangeInclusive<usize>| {
        lens.contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit())
    };
    let same =
        cell.chars().filter(|c| matches!(c, '-' | '/' | '.')).collect::<std::collections::BTreeSet<_>>().len() == 1;
    // A year first is set apart by a hyphen or a slash: "1530.2" is a decimal, never a month.
    let dotted = cell.contains('.');
    same && match parts.as_slice() {
        [y, m] => !dotted && digits(y, 4..=4) && digits(m, 1..=2),
        [y, m, d] if y.len() == 4 => !dotted && digits(y, 4..=4) && digits(m, 1..=2) && digits(d, 1..=2),
        [d, m, y] => digits(d, 1..=2) && digits(m, 1..=2) && (digits(y, 2..=2) || digits(y, 4..=4)),
        _ => false,
    }
}

/// A line's cells, between semicolons, tabs, and commas: where a sheet writes its decimals with a
/// comma, a comma between two digits is a figure's, never a cell's end.
fn split_cells(line: &str, decimal: char) -> Vec<&str> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut cells = Vec::new();
    let mut from = 0;
    for (k, &(at, c)) in chars.iter().enumerate() {
        let between =
            k > 0 && chars[k - 1].1.is_ascii_digit() && chars.get(k + 1).is_some_and(|(_, n)| n.is_ascii_digit());
        if matches!(c, ';' | '\t') || (c == ',' && !(decimal == ',' && between)) {
            cells.push(&line[from..at]);
            from = at + c.len_utf8();
        }
    }
    cells.push(&line[from..]);
    cells
}

/// The figures `text` writes, as [`figures`] reads them, each read with `decimal` as its decimal
/// mark: with a comma, a point between digits groups thousands ("3.800" is 3800) and a comma
/// between them is the decimal point ("46,2" is 46.2).
pub fn figures_in_marks(text: &str, decimal: char) -> Vec<f64> {
    if decimal != ',' {
        return figures(text);
    }
    let chars: Vec<char> = text.chars().collect();
    let between =
        |i: usize| i > 0 && chars[i - 1].is_ascii_digit() && chars.get(i + 1).is_some_and(char::is_ascii_digit);
    let plain: String = (chars.iter().enumerate())
        .filter_map(|(i, &c)| match c {
            '.' if between(i) => None,
            ',' if between(i) => Some('.'),
            c => Some(c),
        })
        .collect();
    figures(&plain)
}

/// The mark `text` writes its decimals with, where its figures tell: a figure with both marks
/// ends on its decimal ("1.234,5", "1,234.5"), one mark written more than once between groups of
/// three groups thousands ("1.234.567"), and one written once before other than three digits is
/// the decimal ("46,2", "25.8"). "3.800" and "3,800" alone tell nothing. None where its figures
/// tell nothing, or tell both ways as often (R1's agent dogfood 12).
pub fn decimal_mark(text: &str) -> Option<char> {
    let (mut point, mut comma) = (0usize, 0usize);
    for line in text.lines() {
        // A row of a sheet separated by commas holds no comma of a figure's: its cells are read
        // alone, so "2021,48" tells nothing. One separated by semicolons or tabs keeps its commas.
        let cells: Vec<&str> = match line.contains([';', '\t']) {
            true => line.split([';', '\t']).collect(),
            false => line.split(',').collect(),
        };
        let row = cells.len() > 1 && cells.iter().all(|c| c.split_whitespace().count() <= 4);
        let pieces: Vec<&str> = if row { cells } else { vec![line] };
        for piece in pieces {
            for told in piece_marks(piece) {
                match told {
                    '.' => point += 1,
                    _ => comma += 1,
                }
            }
        }
    }
    match point.cmp(&comma) {
        std::cmp::Ordering::Greater => Some('.'),
        std::cmp::Ordering::Less => Some(','),
        std::cmp::Ordering::Equal => None,
    }
}

/// The decimal mark each figure in `text` tells, where it tells one ([`decimal_mark`]).
fn piece_marks(text: &str) -> Vec<char> {
    let chars: Vec<char> = text.chars().collect();
    let mut told = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let from = i;
        while i < chars.len()
            && (chars[i].is_ascii_digit()
                || (matches!(chars[i], '.' | ',') && chars.get(i + 1).is_some_and(char::is_ascii_digit)))
        {
            i += 1;
        }
        let run: String = chars[from..i].iter().collect();
        let marks: Vec<char> = run.chars().filter(|c| matches!(c, '.' | ',')).collect();
        let groups: Vec<&str> = run.split(['.', ',']).collect();
        let mark = match marks.as_slice() {
            [] => None,
            [.., last] if marks.iter().any(|m| m != last) => Some(*last),
            [one] => (groups[1].len() != 3 || groups[0] == "0").then_some(*one),
            [m, ..] => {
                let grouped = groups[0].len() <= 3 && groups[1..].iter().all(|g| g.len() == 3);
                grouped.then_some(if *m == '.' { ',' } else { '.' })
            }
        };
        told.extend(mark);
    }
    told
}

/// An icon a slide's word names (PLAN G.23): its name in its set, and its outline on its view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    pub name: String,
    /// SVG path data.
    pub path: String,
    /// The box of the path's own coordinates it is drawn on, `[x, y, w, h]`.
    pub view: [f64; 4],
}

impl Narrative {
    /// Its arc's definition.
    pub fn narrative_arc(&self) -> &'static arcs::NarrativeArc {
        arcs::arc(&self.arc)
    }

    /// The words its slides name icons by (PLAN G.23), each once, as written but trimmed and in
    /// lower case: what [`Narrative::icons`] keys the icons found by. One with no letter or digit
    /// (`-`, `—`, `$`, an emoji), which a model writes for an item it cannot picture, names none.
    pub fn icon_words(&self) -> Vec<String> {
        let mut words: Vec<String> = Vec::new();
        for word in self.slides.iter().flat_map(|s| &s.icons).map(|w| w.trim().to_lowercase()) {
            if word.chars().any(char::is_alphanumeric) && !words.contains(&word) {
                words.push(word);
            }
        }
        words
    }
}

/// Why a deck could not be generated.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NarratoError {
    /// Its message alone, never the brief's again as its cause (W1 dogfood 12).
    #[error(transparent)]
    Brief(#[from] BriefError),
    /// What the provider said: a status and its words, a refusal, a block.
    #[error("{0}")]
    Provider(String),
    /// An answer that does not read as a deck.
    #[error("{0}")]
    Answer(String),
    /// Too few slides came back to be a deck of the length asked.
    #[error("the model wrote {delivered} slides, and a deck of this length has at least {minimum}")]
    Incomplete { delivered: usize, minimum: usize },
    #[error("the deck is written: nothing more is asked")]
    Done,
}

/// Where a generation stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    /// The arc is to be chosen.
    Arc,
    /// The outline is to be written, the person having asked to see it before the slides
    /// (narrative FUNC-1).
    Outline,
    /// The slides are to be written.
    Slides,
    /// The rest of the slides are to be written, the answer that wrote the first of them having
    /// been cut short (lift-up REL-2).
    Rest,
    /// The narrative is written.
    Done,
}

/// One deck's generation, step by step: the client asks for each [`Generation::request`],
/// makes it, and hands the response to [`Generation::answer`], until it is done.
#[derive(Debug, Clone)]
pub struct Generation {
    brief: Brief,
    settings: Settings,
    step: Step,
    arc: Option<ArcChoice>,
    /// Why the arc was chosen by the heuristic, where the model's choice did not come back.
    fallback: Option<String>,
    /// The outline the person settled, where they asked to see one (narrative FUNC-1): what the
    /// slides are written to.
    outline: Option<Outline>,
    /// How many of the outline's answers came back but did not read, and why the last did not,
    /// once it has been asked twice (lift-up REL-3): the person then writes without one, or stops.
    misread: usize,
    unread: Option<String>,
    /// The claims the person wrote in an outline shown, which the next outline keeps word for
    /// word in the beats of their names (lift-up 20261110 FUNC-2).
    kept: Vec<outline::Beat>,
    narrative: Option<Narrative>,
    /// How many times the step was asked and answered for now, not for good.
    tried: usize,
    /// How long to wait before the step is asked again, where its last answer was for now.
    again: Option<f64>,
    /// The slides as the model wrote them, the text [`Generation::written`] read.
    written: Option<String>,
    /// The slides written so far, where the answer that wrote them was cut short, and how many
    /// times the rest has been asked for (lift-up REL-2).
    partial: Option<answer::Slides>,
    rests: usize,
    /// Why the rest did not come back, where it did not.
    unrested: Option<String>,
    /// What the deck is grounded in, as the client gave it ([`Generation::give_sources`]).
    sources: Vec<Provenance>,
    /// Words the brief's packed sources do not carry, whose figures notes may say all the same:
    /// a PDF's, which goes to the model whole ([`Generation::hold`], W1's dogfood 17).
    held: String,
}

impl Generation {
    /// A generation of `brief`, read as Narrato reads one, by `settings`' model.
    pub fn new(brief: Brief, settings: Settings) -> Result<Generation, NarratoError> {
        Ok(Generation {
            brief: brief.validated()?,
            settings,
            step: Step::Arc,
            arc: None,
            fallback: None,
            outline: None,
            misread: 0,
            unread: None,
            kept: Vec::new(),
            narrative: None,
            tried: 0,
            again: None,
            written: None,
            partial: None,
            rests: 0,
            unrested: None,
            sources: Vec::new(),
            held: String::new(),
        })
    }

    pub fn brief(&self) -> &Brief {
        &self.brief
    }

    pub fn step(&self) -> Step {
        self.step
    }

    /// The slides the deck is asked for at least.
    pub fn target(&self) -> usize {
        brief::slide_count(&self.brief.length)
    }

    /// The fewest and the most slides an outline's beats may take in all, the most none for
    /// "41+": the length's own range, or, for a length that names none, its count to a fifth past
    /// it. Counts a person sets outside it are scaled to its nearer end ([`outline::counted`]).
    pub fn slides_range(&self) -> (usize, Option<usize>) {
        let target = self.target();
        match brief::minimum_slides(&self.brief.length) {
            Some(least) => (least, brief::maximum_slides(&self.brief.length)),
            None => (target, Some(target + (target / 5).max(1))),
        }
    }

    /// The slides the slides' prompt asks for: the outline's in all where one stands, as the
    /// person settled it, else the length's ([`Generation::target`]); what the progress says and
    /// the rest of slides cut short is held to (lift-up 20261110 USE-3).
    pub fn slides_asked(&self) -> usize {
        match &self.outline {
            Some(outline) if !outline.beats.is_empty() => outline.beats.iter().map(|b| b.slides).sum::<usize>().max(1),
            _ => self.target(),
        }
    }

    /// The arc chosen, once it is.
    pub fn arc(&self) -> Option<&ArcChoice> {
        self.arc.as_ref()
    }

    /// Why the heuristic chose the arc, where the model's choice did not come back.
    pub fn fallback(&self) -> Option<&str> {
        self.fallback.as_deref()
    }

    pub fn narrative(&self) -> Option<&Narrative> {
        self.narrative.as_ref()
    }

    /// The outline, once the model wrote it or the person settled one (narrative FUNC-1).
    pub fn outline(&self) -> Option<&Outline> {
        self.outline.as_ref()
    }

    /// The beats the slides are spread over at this length, as the slides' prompt plans them:
    /// the outline's where one is settled, else the arc's ([`beats::plan`]); none before the
    /// arc is chosen.
    pub fn plan(&self) -> Option<Vec<outline::Beat>> {
        let arc = arcs::arc(&self.arc.as_ref()?.selection);
        if let Some(outline) = &self.outline {
            return Some(outline.beats.clone());
        }
        let planned = beats::plan(arc, self.target());
        Some(
            (planned.iter().filter(|&&(_, from, to)| to >= from))
                .map(|&(beat, from, to)| outline::Beat {
                    name: arc.beats[beat].clone(),
                    claim: String::new(),
                    slides: to - from + 1,
                })
                .collect(),
        )
    }

    /// The claims the person wrote in the outline shown, kept by the next outline asked
    /// ([`Generation::outline_first`]): word for word in the beats of their names, and named to
    /// the model so the rest agree with them (lift-up 20261110 FUNC-2).
    pub fn keep_claims(&mut self, kept: Vec<outline::Beat>) {
        self.kept = kept.into_iter().filter(|b| !b.claim.trim().is_empty()).collect();
    }

    /// The request that asks for beat `beat` (from 0) of `outline`, the person's as it stands,
    /// again, every other claim fixed (lift-up 20261110 FUNC-2): Scaena's own short call, made
    /// while the outline is shown, which leaves the step as it was. None before the arc is chosen
    /// or for a beat the outline lacks.
    pub fn claim_request(&self, key: &str, outline: &Outline, beat: usize) -> Option<Request> {
        let arc = arcs::arc(&self.arc.as_ref()?.selection);
        let prompt = outline::claim_prompt(&self.brief, arc, outline, beat)?;
        let s = &self.settings;
        let max_tokens = if s.provider == Provider::Gemini { GEMINI_MAX_TOKENS } else { FIT_MAX_TOKENS };
        Some(s.provider.structured(&providers::Call {
            model: &s.model,
            key,
            base: s.base.as_deref(),
            browser: s.browser,
            system: outline::CLAIM_SYSTEM,
            prompt: &prompt,
            shape: providers::Shape::Claim,
            anthropic_schema: true,
            max_tokens,
            temperature: Some(0.8),
            documents: &[],
            pictures: &[],
        }))
    }

    /// The claim the answer to [`Generation::claim_request`] gives; an error that says why where
    /// it gives none, or came back refused, out of credit among it.
    pub fn claim_answer(&self, status: u16, status_text: &str, body: &str) -> Result<String, NarratoError> {
        if status == 0 {
            return Err(NarratoError::Provider(format!("the request did not complete: {status_text}")));
        }
        let read = self.settings.provider.read_answer(status, status_text, body).map_err(NarratoError::Provider)?;
        outline::read_claim(&read.text).map_err(NarratoError::Answer)
    }

    /// Ask the model for the outline before the slides (narrative FUNC-1): the next request is
    /// the outline's, and its answer puts the outline where [`Generation::outline`] reads it, to
    /// be settled or taken as it is. Only once the arc is chosen and before the slides are.
    pub fn outline_first(&mut self) -> Result<(), NarratoError> {
        match self.step {
            Step::Slides | Step::Outline if self.arc.is_some() => {
                self.step = Step::Outline;
                self.misread = 0;
                self.unread = None;
                Ok(())
            }
            Step::Done => Err(NarratoError::Done),
            _ => Err(NarratoError::Answer("the arc is chosen before the outline is asked".into())),
        }
    }

    /// Why the outline did not read, once it was asked twice and twice came back unreadable
    /// (lift-up REL-3): the arc paid for stands, and the person writes the slides without an
    /// outline ([`Generation::skip_outline`]) or stops. None while an outline may still come.
    pub fn unread(&self) -> Option<&str> {
        self.unread.as_deref()
    }

    /// Write the slides without an outline, the arc chosen standing (lift-up REL-3): where the
    /// outline was asked and did not read, or the person passes it over. Only once the arc is
    /// chosen and before the slides are written.
    pub fn skip_outline(&mut self) -> Result<(), NarratoError> {
        match self.step {
            Step::Slides | Step::Outline if self.arc.is_some() => {}
            Step::Done => return Err(NarratoError::Done),
            _ => return Err(NarratoError::Answer("the arc is chosen before the outline is passed over".into())),
        }
        self.outline = None;
        self.unread = None;
        self.misread = 0;
        self.step = Step::Slides;
        Ok(())
    }

    /// The outline as the person settled it (narrative FUNC-1), which the slides are written to:
    /// its claims as they stand, its beats in their order, blank claims passed over, and the
    /// slides spread over them again ([`outline::settled`]). Only once the arc is chosen and
    /// before the slides are written; an outline with no claim left is refused.
    pub fn settle(&mut self, outline: &Outline) -> Result<(), NarratoError> {
        match self.step {
            Step::Slides | Step::Outline if self.arc.is_some() => {}
            Step::Done => return Err(NarratoError::Done),
            _ => return Err(NarratoError::Answer("the arc is chosen before the outline is settled".into())),
        }
        let target = self.target();
        let range = self.slides_range();
        self.outline = Some(outline::settled(outline, target, range).map_err(NarratoError::Answer)?);
        self.step = Step::Slides;
        Ok(())
    }

    /// How long to wait, in seconds, before the step is asked again ([`Generation::request`]
    /// gives the same request): where the last answer was for now, not for good. None where
    /// it was read, and after the last of [`TRIES`].
    pub fn again(&self) -> Option<f64> {
        self.again
    }

    /// How many times the step has been asked and answered for now, not for good.
    pub fn tried(&self) -> usize {
        self.tried
    }

    /// The slides as the model wrote them, the text [`Generation::written`] read, once it has:
    /// what a client keeps so a run that fails after them resumes from it, as `--slides` takes
    /// it, with no call paid for again (narrative REL-2).
    pub fn written_text(&self) -> Option<&str> {
        self.written.as_deref()
    }

    /// The words the narrative's slides name icons by ([`Narrative::icon_words`]); none before
    /// the narrative is written. The client finds each in the set that ships and hands back what
    /// it found ([`Generation::give_icons`]).
    pub fn icon_words(&self) -> Vec<String> {
        self.narrative.as_ref().map(Narrative::icon_words).unwrap_or_default()
    }

    /// The icons the client found for [`Generation::icon_words`], each by its word, kept with the
    /// narrative for the deck it makes (PLAN G.23).
    pub fn give_icons(&mut self, icons: std::collections::BTreeMap<String, Icon>) {
        if let Some(narrative) = &mut self.narrative {
            narrative.icons = icons;
        }
    }

    /// What the deck is grounded in, each source as the prompt names it (narrative FUNC-3): kept
    /// with the narrative, written or yet to be, for the deck it makes.
    pub fn give_sources(&mut self, sources: Vec<Provenance>) {
        if let Some(narrative) = &mut self.narrative {
            narrative.sources = sources.clone();
        }
        self.sources = sources;
    }

    pub fn into_narrative(self) -> Option<Narrative> {
        self.narrative
    }

    /// Choose the arc without asking the model: `name`, one of the catalog's, or the heuristic's
    /// choice for the brief.
    pub fn choose_arc(&mut self, name: Option<&str>) {
        let choice = match name {
            Some(name) => {
                let arc = arcs::arc(name);
                ArcChoice {
                    selection: arc.arc.clone(),
                    reasoning: "Chosen by the user.".into(),
                    candidates: Vec::new(),
                }
            }
            None => heuristic::choose(&self.brief),
        };
        self.arc = Some(choice);
        self.step = Step::Slides;
    }

    /// The request the step asks for, `key` in its headers, `documents` (PDFs) beside the
    /// outline's prompt and the slides'; none once the narrative is written. Arc selection sees the sources' first
    /// 500 characters, never their documents.
    pub fn request(&self, key: &str, documents: &[Document]) -> Option<Request> {
        use providers::Shape;
        let s = &self.settings;
        // Gemini is given the most it writes, as Vzestup gives it (`maxOutputTokens`).
        let room = |own: u32| if s.provider == Provider::Gemini { GEMINI_MAX_TOKENS } else { own };
        let (system, prompt, shape, max_tokens, temperature, documents) = match self.step {
            Step::Arc => (
                prompt::ARC_SELECT_SYSTEM,
                prompt::arc_selection(&self.brief),
                Shape::Arc,
                room(ARC_MAX_TOKENS),
                0.2,
                &[][..],
            ),
            Step::Outline => {
                let chosen = self.arc.as_ref()?;
                let arc = arcs::arc(&chosen.selection);
                let plan = beats::plan(arc, self.target());
                let text = outline::prompt(&self.brief, arc, &chosen.reasoning, &plan, &self.kept);
                // The documents beside it, as the slides have them (lift-up FUNC-3).
                (outline::OUTLINE_SYSTEM, text, Shape::Outline, room(outline::OUTLINE_MAX_TOKENS), 0.3, documents)
            }
            Step::Slides => {
                let arc = arcs::arc(&self.arc.as_ref()?.selection);
                let text = prompt::generation_outlined(&self.brief, arc, self.slides_asked(), self.outline.as_ref());
                (prompt::SLIDES_SYSTEM, text, Shape::Slides, room(SLIDES_MAX_TOKENS), 0.7, documents)
            }
            // The slides' own prompt, the sources with it, and after it those written so far.
            Step::Rest => {
                let arc = arcs::arc(&self.arc.as_ref()?.selection);
                let mut text =
                    prompt::generation_outlined(&self.brief, arc, self.slides_asked(), self.outline.as_ref());
                text.push_str(&prompt::rest(&self.partial.as_ref()?.slides, self.slides_asked()));
                (prompt::SLIDES_SYSTEM, text, Shape::Slides, room(SLIDES_MAX_TOKENS), 0.7, documents)
            }
            Step::Done => return None,
        };
        Some(s.provider.structured(&providers::Call {
            model: &s.model,
            key,
            base: s.base.as_deref(),
            browser: s.browser,
            system,
            prompt: &prompt,
            shape,
            // Vzestup holds Anthropic's arc selection to its shape by the rider alone.
            anthropic_schema: shape != Shape::Arc,
            max_tokens,
            temperature: Some(temperature),
            documents,
            pictures: &[],
        }))
    }

    /// The request that has `longs`, texts the deck set that run past their place, rewritten to
    /// fit ([`fit`]): Scaena's own call, made once the narrative is written; none before.
    pub fn fit_request(&self, key: &str, longs: &[fit::Long]) -> Option<Request> {
        let narrative = self.narrative.as_ref()?;
        let s = &self.settings;
        let prompt = fit::prompt(longs, narrative.brief.language.as_deref());
        let max_tokens = if s.provider == Provider::Gemini { GEMINI_MAX_TOKENS } else { FIT_MAX_TOKENS };
        Some(s.provider.structured(&providers::Call {
            model: &s.model,
            key,
            base: s.base.as_deref(),
            browser: s.browser,
            system: fit::FIT_SYSTEM,
            prompt: &prompt,
            shape: providers::Shape::Fit,
            anthropic_schema: true,
            max_tokens,
            temperature: Some(0.2),
            documents: &[],
            pictures: &[],
        }))
    }

    /// The request that shows the model the deck as it is drawn, `slides` each with what it may
    /// take ([`look::Seen`]) and `drawn` its picture, in their order, and asks what it would
    /// change ([`look`]): Scaena's own call, made once the narrative is written; none before, or
    /// for no slide. `slides` may be a part of the deck, a dozen at a time (narrative PERF-5):
    /// each slide is numbered as it stands in the deck, and the prompt says which are shown and,
    /// from `edges`, the slides just before and after them (lift-up PERF-1).
    pub fn look_request(
        &self,
        key: &str,
        slides: &[look::Seen],
        drawn: &[Picture],
        edges: [Option<&look::Seen>; 2],
    ) -> Option<Request> {
        let narrative = self.narrative.as_ref()?;
        if slides.is_empty() || slides.len() != drawn.len() {
            return None;
        }
        let s = &self.settings;
        let count = narrative.slides.len();
        let pictures: Vec<Picture> = (slides.iter().zip(drawn))
            .map(|(seen, d)| Picture { caption: look::caption(seen, count), ..d.clone() })
            .collect();
        let brief = &narrative.brief;
        let shown = (slides[0].slide, slides[slides.len() - 1].slide);
        let big_idea = narrative.outline.as_ref().map(|o| o.big_idea.as_str());
        let around = look::edges(edges);
        let prompt =
            look::prompt(&brief.topic, &brief.audience, count, brief.language.as_deref(), shown, big_idea, &around);
        let max_tokens = if s.provider == Provider::Gemini { GEMINI_MAX_TOKENS } else { LOOK_MAX_TOKENS };
        Some(s.provider.structured(&providers::Call {
            model: &s.model,
            key,
            base: s.base.as_deref(),
            browser: s.browser,
            system: look::LOOK_SYSTEM,
            prompt: &prompt,
            shape: providers::Shape::Look,
            anthropic_schema: true,
            max_tokens,
            temperature: Some(0.3),
            documents: &[],
            pictures: &pictures,
        }))
    }

    /// The request that asks for `spoken`'s speaker notes, written for the talk ([`notes`]):
    /// Scaena's own call, made once the narrative is written; none before, or for no slide.
    /// `spoken` may be a part of the deck, a dozen at a time, each slide numbered as it stands in
    /// it, and carrying the headlines either side of it.
    pub fn notes_request(&self, key: &str, spoken: &[notes::Spoken]) -> Option<Request> {
        let narrative = self.narrative.as_ref()?;
        if spoken.is_empty() {
            return None;
        }
        let s = &self.settings;
        let brief = &narrative.brief;
        let talk = notes::Talk {
            topic: &brief.topic,
            audience: &brief.audience,
            modality: brief.modality.as_deref(),
            minutes: notes::slot(brief.minutes, &brief.length),
            language: brief.language.as_deref(),
            voice: brief.voice.as_deref(),
            big_idea: narrative.outline.as_ref().map(|o| o.big_idea.as_str()),
        };
        let prompt = notes::prompt(spoken, narrative.slides.len(), &talk);
        let max_tokens = if s.provider == Provider::Gemini { GEMINI_MAX_TOKENS } else { NOTES_MAX_TOKENS };
        Some(s.provider.structured(&providers::Call {
            model: &s.model,
            key,
            base: s.base.as_deref(),
            browser: s.browser,
            system: notes::NOTES_SYSTEM,
            prompt: &prompt,
            shape: providers::Shape::Notes,
            anthropic_schema: true,
            max_tokens,
            temperature: Some(0.5),
            documents: &[],
            pictures: &[],
        }))
    }

    /// The notes the response to [`Generation::notes_request`] gives, each a slide of `spoken`'s
    /// and its words, and each slide of `spoken` whose notes were not taken, with why
    /// ([`notes::read_all`], W1's dogfood 6), each figure the brief's sources hold counted as
    /// written ([`Generation::held`], W1's dogfood 17).
    pub fn notes_answer(
        &self,
        status: u16,
        status_text: &str,
        body: &str,
        spoken: &[notes::Spoken],
    ) -> Result<notes::Read, NarratoError> {
        self.notes_answer_held(status, status_text, body, spoken, &self.held())
    }

    /// [`Generation::notes_answer`], each figure `held` holds counted as written: the sources'
    /// and the data files' the deck is made with.
    pub fn notes_answer_held(
        &self,
        status: u16,
        status_text: &str,
        body: &str,
        spoken: &[notes::Spoken],
        held: &notes::Held,
    ) -> Result<notes::Read, NarratoError> {
        if status == 0 {
            return Err(NarratoError::Provider(format!("the request did not complete: {status_text}")));
        }
        let read = self.settings.provider.read_answer(status, status_text, body).map_err(NarratoError::Provider)?;
        notes::read_held(&read.text, spoken, held).map_err(NarratoError::Answer)
    }

    /// The figures the brief's sources hold, as packed for the prompt and as [`Generation::hold`]
    /// gave them, which notes may say though no slide writes them (W1's dogfood 17).
    pub fn held(&self) -> notes::Held {
        let mut held = notes::Held::of(self.brief.source_content.as_deref().unwrap_or_default());
        held.add(&self.held);
        held
    }

    /// `words`, a source's the brief does not pack (a PDF's, which the model is handed whole),
    /// held for their figures: notes may say one though no slide writes it (W1's dogfood 17).
    pub fn hold(&mut self, words: &str) {
        self.held.push_str(words);
        self.held.push('\n');
    }

    /// The changes the response to [`Generation::look_request`] asks for, each one `slides`
    /// may take.
    pub fn look_answer(
        &self,
        status: u16,
        status_text: &str,
        body: &str,
        slides: &[look::Seen],
    ) -> Result<Vec<look::Change>, NarratoError> {
        if status == 0 {
            return Err(NarratoError::Provider(format!("the request did not complete: {status_text}")));
        }
        let read = self.settings.provider.read_answer(status, status_text, body).map_err(NarratoError::Provider)?;
        look::read(&read.text, slides).map_err(NarratoError::Answer)
    }

    /// The rewrites the response to [`Generation::fit_request`] gives: each a text asked about,
    /// shorter than it was.
    pub fn fit_answer(
        &self,
        status: u16,
        status_text: &str,
        body: &str,
        longs: &[fit::Long],
    ) -> Result<Vec<(String, String)>, NarratoError> {
        if status == 0 {
            return Err(NarratoError::Provider(format!("the request did not complete: {status_text}")));
        }
        let read = self.settings.provider.read_answer(status, status_text, body).map_err(NarratoError::Provider)?;
        fit::read(&read.text, longs).map_err(NarratoError::Answer)
    }

    /// Read the response to [`Generation::request`]: `status` 0 where it never came back, and
    /// `after` its `Retry-After`, where it has one. An answer for now, not for good (`busy`: no
    /// answer, a timeout, a rate limit, a server's failure) leaves the step standing to be asked
    /// again, after the wait [`Generation::again`] says, up to [`TRIES`] times in all (narrative
    /// REL-1). After the last, or any other failure, arc selection falls back to the heuristic,
    /// saying why; the slides that fail are an error, and the step stands to be asked again.
    pub fn answer(
        &mut self,
        status: u16,
        status_text: &str,
        body: &str,
        after: Option<&str>,
    ) -> Result<Step, NarratoError> {
        if self.step == Step::Done {
            return Err(NarratoError::Done);
        }
        // Out of credit is said at once, at any step, and asked again at none: the arc falls back
        // to no heuristic, since the slides after it would fail the same way (lift-up 20261110
        // REL-2).
        if for_good(status, body) {
            self.again = None;
            self.tried = 0;
            let said =
                providers::read(status, status_text, body).err().unwrap_or_else(|| format!("{status} {status_text}"));
            return Err(NarratoError::Provider(format!(
                "no credit is left on this key: top up its balance, or use another key. It said: {said}"
            )));
        }
        if busy(status) && self.tried + 1 < TRIES {
            self.tried += 1;
            self.again = Some(providers::wait(after, body, self.tried));
            return Ok(self.step);
        }
        self.again = None;
        let tried = std::mem::take(&mut self.tried) + 1;
        let asked = |why: String| if tried > 1 { format!("{why} (asked {tried} times)") } else { why };
        match self.step {
            Step::Arc => {
                let chosen = match status {
                    0 => Err(format!("the request did not complete: {status_text}")),
                    _ => {
                        self.settings.provider.read_answer(status, status_text, body).and_then(|a| answer::arc(&a.text))
                    }
                };
                match chosen {
                    Ok(choice) => self.arc = Some(choice),
                    Err(why) => {
                        self.arc = Some(heuristic::choose(&self.brief));
                        self.fallback = Some(asked(why));
                    }
                }
                self.step = Step::Slides;
                Ok(self.step)
            }
            // An outline that fails to come back is an error that says why, and the step stands
            // to be asked again, or passed over for the slides at once. One that comes back but
            // does not read (prose around its JSON, no claims) is asked once more, at once; the
            // second stands as `unread`, for the person to write without one or stop, the arc
            // paid for kept (lift-up REL-3).
            Step::Outline => {
                if status == 0 {
                    return Err(NarratoError::Provider(asked(format!("the request did not complete: {status_text}"))));
                }
                let read = (self.settings.provider.read_answer(status, status_text, body))
                    .map_err(|why| NarratoError::Provider(asked(why)))?;
                let chosen = self.arc.as_ref().expect("the arc is chosen before the outline is asked");
                let arc = arcs::arc(&chosen.selection);
                let plan = beats::plan(arc, self.target());
                match outline::read(&read.text, arc, &plan) {
                    Ok(mut outline) => {
                        // The person's own claims stand as they wrote them, whatever the model
                        // gave their beats (lift-up 20261110 FUNC-2).
                        for beat in &mut outline.beats {
                            if let Some(own) = self.kept.iter().find(|k| k.name.trim() == beat.name.trim()) {
                                beat.claim = own.claim.trim().to_string();
                            }
                        }
                        self.outline = Some(outline);
                        self.misread = 0;
                        self.step = Step::Slides;
                    }
                    Err(why) => {
                        self.misread += 1;
                        if self.misread >= OUTLINE_READS {
                            self.unread = Some(format!("{why} (asked {} times)", self.misread));
                        }
                    }
                }
                Ok(self.step)
            }
            Step::Slides => {
                if status == 0 {
                    return Err(NarratoError::Provider(asked(format!("the request did not complete: {status_text}"))));
                }
                let read = (self.settings.provider.read_answer(status, status_text, body))
                    .map_err(|why| NarratoError::Provider(asked(why)))?;
                self.wrote(&read.text, read.stop == Stop::Length, true)
            }
            // The rest that fails to come back leaves the slides as far as they were written.
            Step::Rest => {
                let more = match status {
                    0 => Err(format!("the request did not complete: {status_text}")),
                    _ => (self.settings.provider.read_answer(status, status_text, body))
                        .and_then(|a| answer::slides(&a.text).map(|s| (s, a.stop == Stop::Length))),
                };
                self.rested(more.map_err(asked))
            }
            Step::Done => Err(NarratoError::Done),
        }
    }

    /// The slides as the model wrote them, `text`, the JSON the slides' request asks for, cut
    /// short where `stopped` says the model ran out of room: what [`Generation::answer`] reads
    /// from an answer, handed over by a client that read the answer itself, or written elsewhere.
    /// The arc is chosen first; too few slides for the length asked are refused.
    pub fn written(&mut self, text: &str, stopped: bool) -> Result<Step, NarratoError> {
        self.wrote(text, stopped, false)
    }

    /// How many slides are written so far, where the answer that wrote them was cut short and
    /// the rest is asked for ([`Step::Rest`]).
    pub fn partial(&self) -> Option<usize> {
        self.partial.as_ref().map(|p| p.slides.len())
    }

    /// Why the rest of an answer cut short did not come back, where it did not (lift-up REL-2):
    /// the deck is made of the slides written before it.
    pub fn unrested(&self) -> Option<&str> {
        self.unrested.as_deref()
    }

    /// [`Generation::written`], the rest asked for where `rest` lets it and the answer was cut
    /// short of the length asked (lift-up REL-2): the minimum is held once the rest is in.
    fn wrote(&mut self, text: &str, stopped: bool, rest: bool) -> Result<Step, NarratoError> {
        match (self.step, &self.arc) {
            (Step::Slides | Step::Outline, Some(_)) => {}
            (Step::Done, _) => return Err(NarratoError::Done),
            _ => return Err(NarratoError::Answer("the arc is chosen before the slides are written".into())),
        }
        let written = answer::slides(text).map_err(NarratoError::Answer)?;
        let cut = written.recovered || stopped;
        if rest && cut && written.slides.len() < self.slides_asked() {
            (self.partial, self.rests, self.unrested) = (Some(written), 0, None);
            self.step = Step::Rest;
            return Ok(self.step);
        }
        self.finish(written, cut, text.to_string())
    }

    /// The rest of the slides, `more`, after those written so far, as many as the length asks
    /// for at most, or why they did not come back; asked for again while they come back cut short
    /// and there are too few, up to [`RESTS`] times.
    fn rested(&mut self, more: Result<(answer::Slides, bool), String>) -> Result<Step, NarratoError> {
        let mut partial = self.partial.take().expect("the rest is asked for after slides cut short");
        self.rests += 1;
        let cut = match more {
            Ok((more, stopped)) => {
                let room = self.slides_asked().saturating_sub(partial.slides.len());
                let before = partial.slides.len();
                partial.slides.extend(more.slides.into_iter().take(room));
                if partial.executive_summary.is_empty() {
                    partial.executive_summary = more.executive_summary;
                }
                // A rest that added nothing is not asked for again.
                (more.recovered || stopped) && partial.slides.len() > before
            }
            Err(why) => {
                self.unrested = Some(why);
                false
            }
        };
        if cut && partial.slides.len() < self.slides_asked() && self.rests < RESTS {
            self.partial = Some(partial);
            return Ok(self.step);
        }
        let truncated = cut || self.unrested.is_some();
        // What `--slides` takes again: the slides as they came back, in one answer.
        let text = serde_json::json!({ "executiveSummary": partial.executive_summary, "slides": partial.slides });
        self.finish(partial, truncated, text.to_string())
    }

    /// The narrative of `written`, the slides read, held to the length's minimum.
    fn finish(&mut self, written: answer::Slides, truncated: bool, text: String) -> Result<Step, NarratoError> {
        if let Some(minimum) = brief::minimum_slides(&self.brief.length)
            && written.slides.len() < minimum
        {
            // Too few: the slides are asked for again, whole.
            if self.step == Step::Rest {
                self.step = Step::Slides;
            }
            return Err(NarratoError::Incomplete { delivered: written.slides.len(), minimum });
        }
        let arc = self.arc.clone().expect("the arc is chosen before the slides are written");
        self.written = Some(text);
        self.narrative = Some(Narrative {
            arc: arc.selection,
            arc_reasoning: arc.reasoning,
            executive_summary: written.executive_summary,
            slides: written.slides,
            truncated,
            brief: Brief { source_content: None, ..self.brief.clone() },
            icons: Default::default(),
            sources: self.sources.clone(),
            outline: self.outline.clone(),
        });
        self.step = Step::Done;
        Ok(self.step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source keeps each figure its words and cells hold once, a sheet's row cell by cell and
    /// prose whole (W1's dogfood 17): never one long number of a row's cells run together.
    #[test]
    fn a_source_keeps_its_figures_and_never_a_row_read_as_one() {
        let csv = "jahr,restmuell_t,gebuehr_eur_t\n2021,182000,48\n2022,186500,52\n";
        // Each column's total too, its years' aside: 48 + 52 and 182000 + 186500.
        assert_eq!(source_figures(csv, None), ["48", "52", "100", "2021", "2022", "182000", "186500", "368500"]);
        let prose = "Over ten years the city removed 7,560 street trees and planted 4,370, 9 degrees hotter.";
        assert_eq!(source_figures(prose, None), ["9", "4370", "7560"]);
        // A CSV's line that is a sentence with commas in it reads as words, not as its cells: its
        // 7,560 is one figure, never a 7 and a 560.
        let said = "year,note\n2025,\"Over ten years the city removed 7,560 street trees, and planted 4,370\"\n";
        assert_eq!(source_figures(said, None), ["2025", "4370", "7560"]);
        let many: String = (0..=MOST_FIGURES).map(|n| format!("{n}\n")).collect();
        assert!(source_figures(&many, None).is_empty(), "past the most, none");
    }

    /// W1's dogfood 19 and R1's agent dogfood 12: a number its thousands are grouped in by a space
    /// is one figure, in a slide's words and a source's alike, however the space is set; a year
    /// and a count stay two, and so does a group a letter or a hyphen runs on from. A sheet's
    /// header gives no figure, nor does a date's cell.
    #[test]
    fn a_number_grouped_by_spaces_is_one_figure_and_a_header_or_a_date_none() {
        for spaced in ["1 240 000", "1\u{a0}240\u{a0}000", "1\u{202f}240\u{202f}000"] {
            assert_eq!(figures(&format!("{spaced} relevés")), [1_240_000.0], "{spaced:?}");
            assert_eq!(source_figures(&format!("Nous avons {spaced} relevés."), Some(',')), ["1240000"]);
        }
        assert_eq!(figures("184 500 m² de toits verts"), [184_500.0]);
        assert_eq!(figures("En 2025 120 toits"), [2025.0, 120.0], "a year and a count");
        assert_eq!(figures("5 100-metre sprints"), [5.0, 100.0], "a group a hyphen runs on from");
        // A decimal and what follows it stay apart (read plainly, "2,4" is 24): never 24000.
        assert_eq!(figures("2,4 000"), [24.0, 0.0]);
        let sheet = "quartier,toitures_m2,baisse_c\nConfluence,184500,2.4\nGerland,97300,1.9\n";
        let kept = source_figures(sheet, Some(','));
        assert!(!kept.contains(&"2".to_string()), "the header's m2 is no figure: {kept:?}");
        assert_eq!(kept, ["1.9", "2.4", "4.3", "97300", "184500", "281800"]);
        let pilot = "mes,litros\n2025-05,6200\n2025-06,11800\n";
        assert_eq!(source_figures(pilot, Some(',')), ["6200", "11800", "18000"], "a month keeps no 5 or 6");
        assert!(date("2025-05") && date("2025-05-14") && date("14.05.2025") && date("14/05/25"));
        assert!(!date("2025") && !date("6200") && !date("2,4") && !date("2025-05/14"));
        assert!(!date("1530.2") && !date("2025.5") && !date("1234.56"), "a decimal is no date");
        let losses = "anio,perdidas_pct,litros_millones\n2025,19.6,1279.4\n2026,18.9,1234.5\n";
        assert!(source_figures(losses, Some(',')).contains(&"1234.5".to_string()), "Valdemar's litres stay");
    }

    /// A sheet keeps each column's total (R1's agent dogfood 12): the pilot's five months of
    /// litres and of bottles sum to the 66.400 and the 132.800 a slide writes, a column of years
    /// sums to none, and a table that prose ends is summed apart from the one after it.
    #[test]
    fn a_sheet_keeps_each_columns_total() {
        let pilot = "mes,litros_servidos,botellas_evitadas\n2025-05,6200,12400\n2025-06,11800,23600\n\
            2025-07,18400,36800\n2025-08,17900,35800\n2025-09,12100,24200\n";
        let kept = source_figures(pilot, Some(','));
        assert!(kept.contains(&"66400".to_string()) && kept.contains(&"132800".to_string()), "{kept:?}");
        let years = "year,share\n2021,23.4\n2022,22.8\n2023,21.5\n";
        assert_eq!(source_figures(years, None), ["21.5", "22.8", "23.4", "67.7", "2021", "2022", "2023"]);
        let apart = "a,1\nb,2\nThe second table:\nc,10\nd,20\n";
        assert_eq!(source_figures(apart, None), ["1", "2", "3", "10", "20", "30"]);
        // One cell is no column: a lone row keeps its cells alone.
        assert_eq!(source_figures("a,5,6\n", None), ["5", "6"]);
    }

    /// A source's figures are read in the marks it writes them in (R1's agent dogfood 12): a
    /// Spanish report's "3.800 €" is 3800 and its "46,2 °C" 46.2, an English report's "3,800"
    /// 3800 and "46.2" 46.2, a sheet's cells as written plainly, and a sheet whose decimals take a
    /// comma read as it writes them. Where a source's figures tell nothing ("3.800" alone), its
    /// words are read in the deck's mark.
    #[test]
    fn a_source_keeps_its_figures_in_its_own_marks() {
        let es = "Cada fuente cuesta 3.800 € instalada. La máxima llegó a 46,2 °C.";
        assert_eq!(decimal_mark(es), Some(','));
        assert_eq!(source_figures(es, Some('.')), ["46.2", "3800"]);
        let en = "Each fountain costs $3,800 installed. The high reached 46.2 °C.";
        assert_eq!(decimal_mark(en), Some('.'));
        assert_eq!(source_figures(en, Some(',')), ["46.2", "3800"]);
        // A sheet's cells say nothing of marks: read plainly, whatever the deck's language.
        let csv = "mes,litros\n2025-05,6200\n2025-07,18400\n";
        assert_eq!(decimal_mark(csv), None);
        assert_eq!(source_figures(csv, Some(',')), ["6200", "18400", "24600"]);
        // Small figures in a row's cells are its cells, never a decimal: "1,2" tells nothing.
        assert_eq!(decimal_mark("a,1,2\nb,3,4\n"), None);
        // A sheet that writes its decimals with a comma, its cells between semicolons.
        let semi = "mes;litros;max\nmayo;6.200;38,5\njulio;18.400;46,2\n";
        assert_eq!(decimal_mark(semi), Some(','));
        assert_eq!(source_figures(semi, None), ["38.5", "46.2", "84.7", "6200", "18400", "24600"]);
        // Nothing told: the words are read in the deck's mark, a sheet's cells plainly.
        assert_eq!(source_figures("Cuesta 3.800 €.", Some(',')), ["3800"]);
        assert_eq!(source_figures("Costs 3.800 €.", Some('.')), ["3.8"]);
        assert_eq!(figures_in_marks("18.400 litros y 46,2 °C", ','), [18400.0, 46.2]);
    }
}
