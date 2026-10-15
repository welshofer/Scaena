//! Syllables and morae, counted by the engine (PLAN O.4a, ADR-0030 decision 27): what a haiku's
//! lines count, so a poem a model wrote is asked for once more, offered with its counts, and
//! judged by lint (I404), never refused for its count.
//!
//! - **English** goes first to a table of the 2,000 most frequent words the rules miss, from
//!   CMUdict ranked by Google Books Ngram's 1-grams (`verse/words.txt`, which
//!   `scripts/build_syllables.py` writes; the notices stand beside it in `verse/NOTICE.md`), then
//!   to rules after words/syllable 5.0.1's (MIT, Titus Wormer), written here over bytes. Capitals
//!   of five letters or fewer are read letter by letter (`NASA`, four). A digit, or a letter of
//!   another script, leaves its line uncounted.
//! - **Japanese** counts morae in a kana reading: each kana, ん, っ, and ー one, a small ゃ, ゅ, ょ,
//!   or a loanword's small vowel joining the kana before it. A line with a kanji is uncounted.
//! - **Any other language** is uncounted.
//!
//! A word said two ways counts either (`fire`, one or two), so a count is a range. Everything is
//! in integers, with no dependency and no regular expression, and total: any text counts or is
//! uncounted, in time linear in its length.
//!
//! The table is compiled in with the `syllables` feature: in the CLI, the MCP server, and the C
//! ABI. The editor's module leaves it out (SPEC §15, ADR-0015): its page hands it over the first
//! time a line counts ([`set_loader`]), held to [`TABLE_SHA256`] by whoever hands it over. Without
//! it, an English line is uncounted, never counted by the rules alone.

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;

/// The table's SHA-256, in hex: what a page hands over is held to it.
pub const TABLE_SHA256: &str = "3b801c5b078a3f5addaba8dc56a245878a1b79d64af368e23950c2a5d13db70c";

/// A line's count: the fewest and the most syllables (or morae) it may be said in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Count {
    pub low: u32,
    pub high: u32,
}

impl Count {
    const fn one(n: u32) -> Count {
        Count { low: n, high: n }
    }

    /// How far it falls from `target`: none where either way of saying it meets it.
    pub fn distance(self, target: u32) -> u32 {
        if target < self.low { self.low - target } else { target.saturating_sub(self.high) }
    }
}

impl std::ops::Add for Count {
    type Output = Count;
    fn add(self, other: Count) -> Count {
        Count { low: self.low.saturating_add(other.low), high: self.high.saturating_add(other.high) }
    }
}

/// How a poem keeps to 5-7-5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kept {
    /// Three lines, each counting its target.
    Kept,
    /// Three lines, two syllables off in all at most.
    Near,
    /// Counted, and further off, or not three lines.
    Off,
    /// A line could not be counted, or the language is not one the engine counts.
    Uncounted,
}

/// One line of a poem as [`check`] counts it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub text: String,
    /// Its count; none where it is uncounted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<Count>,
    /// What a haiku's line counts there: 5, 7, and 5; none past the third line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<u32>,
}

/// A poem counted: each line, and how it keeps to 5-7-5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checked {
    pub lines: Vec<Line>,
    pub kept: Kept,
}

impl Checked {
    /// How far the poem falls from 5-7-5, in syllables, all lines together; none where it is not
    /// three counted lines, which is as far as a poem falls.
    pub fn distance(&self) -> Option<u32> {
        if self.lines.len() != 3 {
            return None;
        }
        self.lines.iter().map(|l| Some(l.count?.distance(l.target?))).sum()
    }
}

/// The targets of a haiku's lines.
pub const HAIKU: [u32; 3] = [5, 7, 5];

/// The languages the engine counts, by a BCP 47 tag's language subtag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Counted {
    English,
    Japanese,
}

fn counted(lang: &str) -> Option<Counted> {
    let code = lang.split(['-', '_']).next().unwrap_or("");
    if code.eq_ignore_ascii_case("en") {
        Some(Counted::English)
    } else if code.eq_ignore_ascii_case("ja") {
        Some(Counted::Japanese)
    } else {
        None
    }
}

/// A line's count in language `lang` (a BCP 47 tag): syllables in English, morae in a Japanese
/// kana reading; none where the line or its language is not one the engine counts.
pub fn count(line: &str, lang: &str) -> Option<Count> {
    match counted(lang)? {
        Counted::English => syllables(line),
        Counted::Japanese => morae(line).map(Count::one),
    }
}

/// A poem's lines counted against 5-7-5, blank lines left out. A Japanese poem is checked by its
/// kana reading.
pub fn check(poem: &str, lang: &str) -> Checked {
    let lines: Vec<Line> = poem
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .enumerate()
        .map(|(i, text)| Line { text: text.to_string(), count: count(text, lang), target: HAIKU.get(i).copied() })
        .collect();
    let kept = if lines.is_empty() || lines.iter().any(|l| l.count.is_none()) {
        Kept::Uncounted
    } else if lines.len() != 3 {
        Kept::Off
    } else {
        let checked = Checked { lines: lines.clone(), kept: Kept::Off };
        match checked.distance() {
            Some(0) => Kept::Kept,
            Some(1..=2) => Kept::Near,
            _ => Kept::Off,
        }
    };
    Checked { lines, kept }
}

// --- Japanese -----------------------------------------------------------------------------------

/// Morae in a kana reading: each kana, ん, っ, and ー one, and a small ゃ, ゅ, ょ, ゎ, or a loanword's
/// small vowel joining the kana before it. Spaces and punctuation count nothing; anything else
/// (a kanji, a Latin letter, a digit) leaves the line uncounted.
pub fn morae(reading: &str) -> Option<u32> {
    let mut morae = 0u32;
    let mut after_kana = false;
    for c in reading.chars() {
        let c = c as u32;
        let kana = (0x3041..=0x3096).contains(&c) || (0x30A1..=0x30FA).contains(&c) || c == 0x30FC;
        if kana {
            if !(small(c) && after_kana) {
                morae += 1;
            }
            after_kana = true;
        } else if c <= 0x7F && !(c as u8).is_ascii_alphanumeric()
            || (0x3000..=0x303F).contains(&c)
            || (0x2000..=0x206F).contains(&c)
            || c == 0x30FB
            || (0xFF01..=0xFF0F).contains(&c)
            || (0xFF1A..=0xFF20).contains(&c)
        {
            after_kana = false;
        } else {
            return None;
        }
    }
    Some(morae)
}

/// A small kana that joins the one before it: ぁぃぅぇぉ, ゃゅょ, ゎ, and their katakana.
fn small(c: u32) -> bool {
    let hiragana = if (0x30A1..=0x30F6).contains(&c) { c - 0x60 } else { c };
    matches!(hiragana, 0x3041 | 0x3043 | 0x3045 | 0x3047 | 0x3049 | 0x3083 | 0x3085 | 0x3087 | 0x308E)
}

// --- English ------------------------------------------------------------------------------------

/// The longest word looked up and counted by the rules, which look back and ahead within it; a
/// longer one, no word of any language, counts its groups of vowels, so counting stays linear.
const LONGEST: usize = 64;

/// Syllables in an English line, or none where a digit or a letter of another script is in it.
fn syllables(line: &str) -> Option<Count> {
    let mut total = Count::one(0);
    let mut word: Vec<u8> = Vec::new();
    let mut capitals = true;
    let table = table()?;
    for c in line.chars() {
        if c == '\'' || c == '\u{2019}' {
            continue; // an apostrophe joins a word's letters (`don't`, `dont`)
        }
        if c.is_ascii_alphabetic() {
            capitals &= c.is_ascii_uppercase();
            word.push(c.to_ascii_lowercase() as u8);
        } else if let Some((base, capital)) = latin(c) {
            capitals &= capital;
            word.extend_from_slice(base.as_bytes());
        } else if c.is_ascii_digit() || !separator(c) {
            return None;
        } else {
            total = total + one(&word, capitals, &table);
            word.clear();
            capitals = true;
        }
    }
    Some(total + one(&word, capitals, &table))
}

/// Whether `c` is a space or a mark of punctuation, which ends a word and counts nothing.
fn separator(c: char) -> bool {
    let c = c as u32;
    c <= 0x7F || (0x80..=0xBF).contains(&c) || c == 0xD7 || c == 0xF7 || (0x2000..=0x206F).contains(&c)
}

/// One word's count: capitals of five letters or fewer letter by letter, else the table, else
/// the rules.
fn one(word: &[u8], capitals: bool, table: &Table) -> Count {
    if word.is_empty() {
        return Count::one(0);
    }
    if capitals && word.len() <= 5 {
        return Count::one(word.iter().map(|&b| if b == b'w' { 3 } else { 1 }).sum());
    }
    if word.len() > LONGEST {
        return Count::one(word.split(|b| !VOWELS_Y.contains(b)).filter(|g| !g.is_empty()).count().max(1) as u32);
    }
    table.find(word).unwrap_or_else(|| Count::one(rules(word)))
}

/// A Latin letter with a mark, Latin-1's or Latin Extended-A's, as its base letters and whether
/// it is a capital: what a word's letters count as (`café`, `cafe`).
fn latin(c: char) -> Option<(&'static str, bool)> {
    // Each letter from U+00C0 to U+017F as its base, in its case; a space where it is none, or
    // more than one letter.
    const BASES: &str = "AAAAAA CEEEEIIIIDNOOOOO OUUUUY  aaaaaa ceeeeiiiidnooooo ouuuuy yAaAaAaCcCcCcCcDdDdEeEeEeEeEeGgGgGgGgHhHhIiIiIiIiIi  JjKkkLlLlLlLlLlNnNnNnnNnOoOoOo  RrRrRrSsSsSsSsTtTtTtUuUuUuUuUuUuWwYyYZzZzZzs";
    const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
    let n = c as u32;
    let two = match n {
        0xC6 => Some(("ae", true)),
        0xE6 => Some(("ae", false)),
        0xDE => Some(("th", true)),
        0xFE => Some(("th", false)),
        0xDF => Some(("ss", false)),
        0x132 => Some(("ij", true)),
        0x133 => Some(("ij", false)),
        0x152 => Some(("oe", true)),
        0x153 => Some(("oe", false)),
        _ => None,
    };
    if two.is_some() {
        return two;
    }
    let b = *BASES.as_bytes().get(n.checked_sub(0xC0)? as usize)?;
    let i = usize::from(b.to_ascii_lowercase().checked_sub(b'a')?);
    (i < 26).then(|| (&LOWER[i..i + 1], b.is_ascii_uppercase()))
}

// --- the table ----------------------------------------------------------------------------------

/// The table's lines: a word and its count, `fire12`, sorted by their bytes.
#[derive(Clone)]
struct Table(Rc<str>);

impl Table {
    fn text(&self) -> &str {
        &self.0
    }

    /// `word`'s count, searched by halves: each probe falls back to the start of its line.
    fn find(&self, word: &[u8]) -> Option<Count> {
        let text = self.text().as_bytes();
        let (mut low, mut high) = (0usize, text.len());
        while low < high {
            let mid = low + (high - low) / 2;
            let start = text[..mid].iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
            let end = text[start..].iter().position(|&b| b == b'\n').map_or(text.len(), |i| start + i);
            let line = &text[start..end];
            let letters = line.iter().position(u8::is_ascii_digit).unwrap_or(line.len());
            match line[..letters].cmp(word) {
                std::cmp::Ordering::Equal => {
                    let digits = &line[letters..];
                    let low = u32::from(*digits.first()? - b'0');
                    let high = digits.get(1).map_or(low, |d| u32::from(d - b'0'));
                    return Some(Count { low, high });
                }
                std::cmp::Ordering::Less => low = end + 1,
                std::cmp::Ordering::Greater => high = start,
            }
        }
        None
    }
}

/// What gives the table where it is not compiled in.
type Loader = Box<dyn Fn() -> Option<String>>;

thread_local! {
    /// The table, once it is had.
    static HANDED: RefCell<Option<Rc<str>>> = const { RefCell::new(None) };
    static LOADER: RefCell<Option<Loader>> = const { RefCell::new(None) };
}

/// Where this thread gets the table where it is not compiled in, the first time an English line
/// counts: `loader` gives `verse/words.txt`'s text, held to [`TABLE_SHA256`], or none.
pub fn set_loader(loader: impl Fn() -> Option<String> + 'static) {
    LOADER.with(|l| *l.borrow_mut() = Some(Box::new(loader)));
}

/// The table: compiled in, else handed over by the loader the first time it is asked for.
fn table() -> Option<Table> {
    if let Some(text) = HANDED.with(|h| h.borrow().clone()) {
        return Some(Table(text));
    }
    let text: Rc<str> = compiled()
        .map(Rc::from)
        .or_else(|| LOADER.with(|l| l.borrow().as_ref().and_then(|load| load())).map(Rc::from))?;
    HANDED.with(|h| *h.borrow_mut() = Some(text.clone()));
    Some(Table(text))
}

#[cfg(feature = "syllables")]
fn compiled() -> Option<&'static str> {
    Some(include_str!("verse/words.txt"))
}

#[cfg(not(feature = "syllables"))]
fn compiled() -> Option<&'static str> {
    None
}

// --- the rules ----------------------------------------------------------------------------------

/// Syllables in one word of letters a–z by words/syllable 5.0.1's rules, its expressions written
/// as patterns over bytes ([`Pattern`]).
pub fn rules(word: &[u8]) -> u32 {
    if word.is_empty() {
        return 0;
    }
    if word.len() < 3 {
        return 1;
    }
    if let Some(n) = problematic(word) {
        return n;
    }
    for end in [&b"es"[..], b"s"] {
        if let Some(stem) = word.strip_suffix(end)
            && let Some(n) = problematic(stem)
        {
            return n;
        }
    }
    let mut count: i32 = 0;
    let mut word = word.to_vec();
    for (patterns, n) in [(TRIPLE, 3), (DOUBLE, 2), (SINGLE, 1)] {
        let (removed, matches) = remove(&word, patterns);
        word = removed;
        count += n * matches as i32;
    }
    count += word.split(|b| !VOWELS_Y.contains(b)).filter(|part| !part.is_empty()).count() as i32;
    count -= matches(&word, MONO_ONE) as i32 + matches(&word, MONO_TWO) as i32;
    count += (matches(&word, DOUBLE_ONE) + matches(&word, DOUBLE_TWO) + matches(&word, DOUBLE_THREE)) as i32;
    // Its last expression has no `g`, so it adds one at most.
    count += i32::from(matches(&word, DOUBLE_FOUR) > 0);
    if count <= 0 { 1 } else { count as u32 }
}

const VOWELS_Y: &[u8] = b"aeiouy";

/// words/syllable's list of hard words, sorted.
const PROBLEMATIC: &[(&str, u8)] = &[
    ("abalone", 4),
    ("abare", 3),
    ("abbruzzese", 4),
    ("abed", 2),
    ("aborigine", 5),
    ("abruzzese", 4),
    ("acreage", 3),
    ("adame", 3),
    ("adieu", 2),
    ("adobe", 3),
    ("anemone", 4),
    ("anyone", 3),
    ("apache", 3),
    ("aphrodite", 4),
    ("apostrophe", 4),
    ("ariadne", 4),
    ("cafe", 2),
    ("calliope", 4),
    ("catastrophe", 4),
    ("chile", 2),
    ("chloe", 2),
    ("circe", 2),
    ("coyote", 3),
    ("daphne", 2),
    ("epitome", 4),
    ("eurydice", 4),
    ("euterpe", 3),
    ("every", 2),
    ("everywhere", 3),
    ("forever", 3),
    ("gethsemane", 4),
    ("guacamole", 4),
    ("hermione", 4),
    ("hyperbole", 4),
    ("jesse", 2),
    ("jukebox", 2),
    ("karate", 3),
    ("machete", 3),
    ("maybe", 2),
    ("naive", 2),
    ("newlywed", 3),
    ("penelope", 4),
    ("people", 2),
    ("persephone", 4),
    ("phoebe", 2),
    ("pulse", 1),
    ("queue", 1),
    ("recipe", 3),
    ("riverbed", 3),
    ("sesame", 3),
    ("shoreline", 2),
    ("simile", 3),
    ("snuffleupagus", 5),
    ("sometimes", 2),
    ("syncope", 3),
    ("tamale", 3),
    ("waterbed", 3),
    ("wednesday", 2),
    ("yosemite", 4),
    ("zoe", 2),
];

fn problematic(word: &[u8]) -> Option<u32> {
    let i = PROBLEMATIC.binary_search_by(|(w, _)| w.as_bytes().cmp(word)).ok()?;
    Some(u32::from(PROBLEMATIC[i].1))
}

/// One alternative of an expression, written over bytes:
/// - a letter is itself, and `.` any byte;
/// - `V` is one of `aeiouy`, `A` one of `aeiou`, `C` any byte but `aeiouy`;
/// - `[abc]` is one of them, and `[^abc]` any byte but them;
/// - `*` after any of these repeats it as often as it can, giving back as the rest needs;
/// - `=` is the byte before it again;
/// - `!x` (or `![abc]`) is a look ahead: what follows is not that, or nothing follows;
/// - `^` and `$` are the word's start and end.
type Pattern = &'static str;

/// An expression: its alternatives, tried in order at each position.
type Expression = &'static [Pattern];

const TRIPLE: Expression = &["creations$", "creation$", "ology$", "ologist$", "onomy$", "onomist$"];

const DOUBLE: Expression = &[
    "^above",
    "^anti",
    "^ante",
    "^counter",
    "^hyper",
    "^afore",
    "^agri",
    "^infra",
    "^intra",
    "^inter",
    "^over",
    "^semi",
    "^ultra",
    "^under",
    "^extra",
    "^dia",
    "^micro",
    "^mega",
    "^kilo",
    "^pico",
    "^nano",
    "^macro",
    "^somer",
    "fully$",
    "berry$",
    "woman$",
    "women$",
    "edly$",
    "union$",
    "[bcdfghjklmnpqrstvwxzaeiou]yeing$",
    "[bcdfghjklmnpqrstvwxzaeiou]ying$",
];

const SINGLE: Expression = &[
    "^un",
    "^fore",
    "^ware",
    "^none",
    "^non",
    "^out",
    "^post",
    "^sub",
    "^pre",
    "^pro",
    "^dis",
    "^side",
    "^some",
    "ly$",
    "less$",
    "some$",
    "ful$",
    "ers$",
    "er$",
    "ness$",
    "cians$",
    "cian$",
    "ments$",
    "ment$",
    "ettes$",
    "ette$",
    "villes$",
    "ville$",
    "ships$",
    "ship$",
    "sides$",
    "side$",
    "ports$",
    "port$",
    "shires$",
    "shire$",
    "[gnst]ioned$",
    "[gnst]ions$",
    "[gnst]ion$",
];

const MONO_ONE: Expression = &[
    "awe$",
    "awed",
    "aweso",
    "cial",
    "cia$",
    "tia",
    "cius",
    "cious",
    "[^aeiou]giu",
    "VCion",
    "iou",
    "sia$",
    "eous$",
    "[oa]gue$",
    ".[^aeiuoycgltdb][^aeiuoycgltdb][^aeiuoycgltdb]*ed$",
    ".ely$",
    "^jua",
    "uai",
    "eau",
    "^busi$",
    "V[bcfgklmnprsvwxyz]ed$",
    "Vched$",
    "Vdged$",
    "Vg[hn]ed$",
    "Vlched$",
    "Vl[lv]ed$",
    "Vmmed$",
    "Vnched$",
    "Vn[cgn]ed$",
    "Vr[bcnsv]ed$",
    "Vsqued$",
    "Vs[chkls]ed$",
    "Vthed$",
    "V[bdfklmnprstvy]es$",
    "Vches$",
    "Vg[hn]es$",
    "Vlches$",
    "Vl[lv]es$",
    "Vmmes$",
    "Vnches$",
    "Vnnes$",
    "Vr[nsv]es$",
    "Vsques$",
    "Vs[cklst]es$",
    "Vthes$",
];

const MONO_TWO: Expression = &[
    "V[bcdfgklmnprstvyz]e$",
    "Vche$",
    "Vdge$",
    "Vg[hn]e$",
    "Vl[lv]e$",
    "Vmme$",
    "Vn[cgns]e$",
    "Vr[cnsv]e$",
    "Vsque$",
    "Vs[cklst]e$",
    "Vthe$",
];

const DOUBLE_ONE: Expression = &[
    "C=l$",
    "Cier$",
    "Ciest$",
    "Ciet$",
    "[aeiouym]bl$",
    "eo$",
    "ism$",
    "asm$",
    "thm$",
    "dnt$",
    "snt$",
    "uity$",
    "dea$",
    "gean$",
    "oa$",
    "ua$",
    "react$",
    "reac$",
    "orbed$",
    "shred$",
    "eings$",
    "eing$",
    "Vshe[rs]$",
    "Vse[rs]$",
];

const DOUBLE_TWO: Expression = &[
    "creat!u",
    "[^gq]ua[^auieo]",
    "AAA",
    "^ia",
    "^mc",
    "^coa[dglx].",
    "^reapp",
    "^rees",
    "^reim",
    "^reus",
    "theist",
    "deist",
];

const DOUBLE_THREE: Expression = &[
    "[^aeiou]y[ae]",
    "[^l]lien",
    "riet",
    "dien",
    "iu",
    "io",
    "ii",
    "uen",
    "[aeilotu]real",
    "real[aeilotu]",
    "iell",
    "eo[^aeiou]",
    "[aeiou]y[aeiou]",
];

const DOUBLE_FOUR: Expression = &["[^s]ia"];

/// The matches of `expression` in `word`, left to right, none overlapping.
fn matches(word: &[u8], expression: Expression) -> usize {
    let mut found = 0;
    let mut i = 0;
    while i < word.len() {
        match expression.iter().find_map(|p| matched(p.as_bytes(), word, i)) {
            Some(end) => {
                found += 1;
                i = end.max(i + 1);
            }
            None => i += 1,
        }
    }
    found
}

/// `word` with each match of `expression` taken out, and how many there were.
fn remove(word: &[u8], expression: Expression) -> (Vec<u8>, usize) {
    let mut kept = Vec::with_capacity(word.len());
    let mut found = 0;
    let mut i = 0;
    while i < word.len() {
        match expression.iter().find_map(|p| matched(p.as_bytes(), word, i)) {
            Some(end) => {
                found += 1;
                i = end.max(i + 1);
            }
            None => {
                kept.push(word[i]);
                i += 1;
            }
        }
    }
    (kept, found)
}

/// Where a match of `pattern` at `at` in `word` ends, if it matches there.
fn matched(pattern: &[u8], word: &[u8], at: usize) -> Option<usize> {
    walk(pattern, word, at, at)
}

/// One class of bytes in a pattern, and the pattern after it.
fn class(pattern: &[u8]) -> Option<(&[u8], bool, &[u8])> {
    match *pattern.first()? {
        b'[' => {
            let close = pattern.iter().position(|&b| b == b']')?;
            let (negated, set) = match pattern.get(1) {
                Some(b'^') => (true, &pattern[2..close]),
                _ => (false, &pattern[1..close]),
            };
            Some((set, negated, &pattern[close + 1..]))
        }
        b'V' => Some((VOWELS_Y, false, &pattern[1..])),
        b'A' => Some((b"aeiou", false, &pattern[1..])),
        b'C' => Some((VOWELS_Y, true, &pattern[1..])),
        b'.' => Some((b"", true, &pattern[1..])),
        _ => Some((&pattern[..1], false, &pattern[1..])),
    }
}

fn walk(pattern: &[u8], word: &[u8], start: usize, i: usize) -> Option<usize> {
    let Some(&first) = pattern.first() else {
        return Some(i);
    };
    match first {
        b'^' => (i == 0).then_some(())?,
        b'$' => return (i == word.len()).then_some(i),
        b'=' => {
            if i == start || word.get(i) != word.get(i - 1) {
                return None;
            }
            return walk(&pattern[1..], word, start, i + 1);
        }
        b'!' => {
            let (set, negated, rest) = class(&pattern[1..])?;
            if word.get(i).is_some_and(|b| set.contains(b) != negated) {
                return None;
            }
            return walk(rest, word, start, i);
        }
        _ => {
            let (set, negated, rest) = class(pattern)?;
            let takes = |i: usize| word.get(i).is_some_and(|b| set.contains(b) != negated);
            if rest.first() == Some(&b'*') {
                let mut end = i;
                while takes(end) {
                    end += 1;
                }
                return (i..=end).rev().find_map(|j| walk(&rest[1..], word, start, j));
            }
            return if takes(i) { walk(rest, word, start, i + 1) } else { None };
        }
    }
    walk(&pattern[1..], word, start, i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_match_as_written() {
        assert_eq!(matched(b"C=l$", b"bottl", 2), Some(5));
        assert_eq!(matched(b"creat!u", b"creature", 0), None);
        assert_eq!(matched(b"creat!u", b"create", 0), Some(5));
        assert_eq!(matched(b"creat!u", b"creat", 0), Some(5));
        assert_eq!(matched(b".[^b][^b][^b]*ed$", b"xmmmed", 0), Some(6));
        assert_eq!(remove(b"unhappily", SINGLE), (b"happi".to_vec(), 2));
    }
}
