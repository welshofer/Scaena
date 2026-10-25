//! A deck's narrative health (PLAN G.9, ADR-0026): Vzestup's measures of a talk, ported
//! (`scaena_narrato::health`), over the deck's slides as they stand: whether it is paced for the
//! length its brief asked, how many points each slide makes and how much its notes give the
//! speaker, how each slide follows from the one before, how the intensity each declares follows
//! the arc it was written to, whether its words carry the intensity and the tone it declares,
//! and which slides now stand in another beat of the arc than the one they were written for.
//!
//! A slide is read as its spine tells it: its headline the beat's claim, its words what its last
//! state shows (each item of a layout's set, or each item of a list, a point of its own), its
//! notes the beat's and its states', and what the narrative declared of it the beat's
//! `media.narrato`. Pure arithmetic over the deck: no layout, no call. What the words read is a
//! designed prior, not a calibrated model, so each finding asks a person to look, and a gap
//! between the words and the plan is the words' to close: the one fix offered is the beat a
//! moved slide now serves.

use crate::OpsError;
use scaena_core::document::{Beat, NodeType, Props};
use scaena_core::tracking::resolve_states;
use scaena_core::{Deck, Severity};
use scaena_narrato::health::constants::{
    COMPOSITE_FALLBACK_WEIGHT_ARC, COMPOSITE_FALLBACK_WEIGHT_BALANCE, COMPOSITE_FALLBACK_WEIGHT_PACING,
    INTENSITY_GAP_THRESHOLD,
};
use scaena_narrato::health::{self, LocalHealthMetrics, NarrativeMeta, PacingStatus, SlideForHealth, arc};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;

/// A deck's narrative health: its scores, each slide as health reads it, and what to look at.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Health {
    /// 0–100, as Vzestup weighs it: pacing, balance, and arc fidelity. Coherence stays out of
    /// it, since the slides' words are compared, not their meanings.
    pub score: u32,
    /// The arc the deck was written to (`meta.narrative.arc`), where it names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arc: Option<String>,
    /// The deck's beats, by name, in order: its settled outline's where it has one, else its
    /// arc's; none for an arc the catalog does not name and no outline.
    pub beats: Vec<String>,
    /// The slides against the count its brief asked for.
    pub pacing: Measure,
    /// The points each slide makes, and what its notes give the speaker.
    pub balance: Measure,
    /// How each slide follows from the one before: by their words, a guide only.
    pub coherence: Measure,
    /// How closely the intensity the slides declare follows a talk's curve.
    pub fidelity: Measure,
    /// How closely their words carry the intensity they declare.
    pub intensity: Measure,
    /// How closely their words carry the tone they declare.
    pub tone: Measure,
    /// Each slide, in order.
    pub slides: Vec<Read>,
    /// What to look at, slide by slide.
    pub findings: Vec<Finding>,
    /// The big idea the deck's settled outline names (`meta.narrative.outline`): the one sentence
    /// the audience should remember, which the editors show above the curve.
    #[serde(rename = "bigIdea", skip_serializing_if = "Option::is_none")]
    pub big_idea: Option<String>,
}

/// One measure: 0–100, and what it says.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Measure {
    /// None where it is not scored: coherence read by words alone, which reads low on any deck,
    /// as Vzestup shows no number for it.
    pub score: Option<u32>,
    pub says: String,
}

/// A slide as health reads it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Read {
    /// Its first state: where an editor goes to show it.
    pub state: String,
    /// The beat that tells it, where one does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beat: Option<String>,
    /// Its headline.
    pub title: String,
    /// Where the curve of the arc it was written to stands at its place, 0–1.
    pub curve: f64,
    /// The intensity it declares, 0–1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared: Option<f64>,
    /// The intensity its words read, 0–1; none where they are too few, or not English.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read: Option<f64>,
    /// The tone it declares, −1 grave to +1 hopeful.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone: Option<f64>,
    /// The tone its words read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone_read: Option<f64>,
    /// How like the slide before it its words are, 0–1; none for the first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<f64>,
    pub standing: Standing,
    /// The sources its beat cites of those the deck was grounded in, by name (narrative FUNC-3).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
}

/// How a slide stands in its arc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Standing {
    Ok,
    /// Its intensity strays from the curve at its place.
    Bends,
    /// It stands in another beat than the one it was written for.
    Drifted,
}

/// What a finding is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The slide has no headline.
    Title,
    /// It says nothing past its headline, and has no notes.
    Empty,
    /// Its words share little with the slide's before it.
    Turn,
    /// Its intensity strays from the curve at its place.
    Arc,
    /// Its words read far from the intensity it declares.
    Intensity,
    /// Its words read in another tone than it declares.
    Tone,
    /// It stands in another beat than the one it was written for.
    Drift,
    /// A state of it has an error lint finds: it does not stand as it is to be read.
    Layout,
    /// The beat of the settled outline it opens says nothing of the claim the person settled for
    /// it (`meta.narrative.outline`).
    Claim,
    /// The closing it opens never lands the big idea the outline settled.
    #[serde(rename = "bigIdea")]
    BigIdea,
    /// A break in its story the narrative lint finds: no claim shown, evidence with no claim,
    /// more than one claim, or the claim of the slide before it again (W420, W421, W424, W425).
    Story,
    /// It makes more points, or shows more words, than a room takes in.
    Dense,
    /// It shows something, and gives the speaker nothing to say about it.
    Notes,
    /// The deck runs past the range of slides its brief asked for; this slide turns least from
    /// the one before, and is the first to cut. Or its beats' time runs short of, or past, the
    /// slot its brief's slides are given; this slide's beat runs shortest, or longest.
    Pacing,
    /// A change the model asked once it saw the slide drawn was not kept, lint having found the
    /// slide worse for it (`meta.narrative.refused`, lift-up USE-4).
    Look,
}

/// Something to look at on one slide: what it says, what to ask the assistant, and, where one
/// is safe, a fix.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Finding {
    /// The slide's first state.
    pub state: String,
    /// The slide's number, from 1.
    pub slide: usize,
    pub severity: Severity,
    pub kind: Kind,
    pub says: String,
    /// The question an editor puts to the assistant for it.
    pub ask: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
}

/// A fix: what it does, in a few words, and the patch that does it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Fix {
    pub says: String,
    pub patch: Vec<Value>,
}

/// A slide of the deck: its states in order, and the beat that tells it with its place in the
/// spine.
struct Slide<'d> {
    states: Vec<usize>,
    beat: Option<(usize, usize, &'d Beat)>,
}

/// The slides of `deck`: each run of states one slide holds (`slide`, or a state its own), with
/// the beat of the first state a beat tells.
fn slides(deck: &Deck) -> Vec<Slide<'_>> {
    let mut told: HashMap<&str, (usize, usize, &Beat)> = HashMap::new();
    for (i, section) in deck.spine.iter().flat_map(|s| s.sections.iter().enumerate()) {
        for (j, beat) in section.beats.iter().enumerate() {
            for state in &beat.states {
                told.entry(state.as_str()).or_insert((i, j, beat));
            }
        }
    }
    let mut out: Vec<(&str, Slide)> = Vec::new();
    for (k, state) in deck.states.iter().enumerate() {
        let id = deck.slide_of(state);
        match out.last_mut() {
            Some((last, slide)) if *last == id => slide.states.push(k),
            _ => out.push((id, Slide { states: vec![k], beat: None })),
        }
    }
    (out.into_iter())
        .map(|(_, mut slide)| {
            slide.beat = slide.states.iter().find_map(|k| told.get(deck.states[*k].id.as_str()).copied());
            slide
        })
        .collect()
}

/// The ordinal a layout's slot begins with (`first-title` → `first`): the item of a set its node
/// is part of.
fn ordinal(slot: &str) -> Option<&str> {
    const ORDINALS: [&str; 12] = [
        "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth", "eleventh",
        "twelfth",
    ];
    let head = slot.split('-').next()?;
    ORDINALS.contains(&head).then_some(head)
}

/// A text node's words: its `text`, or its runs' together.
fn words(props: &Props) -> String {
    match (props.get("text"), props.get("runs")) {
        (Some(Value::String(text)), _) => text.clone(),
        (_, Some(Value::Array(runs))) => runs.iter().filter_map(|r| r.get("text").and_then(Value::as_str)).collect(),
        _ => String::new(),
    }
}

/// What the narrative declared of a beat (`media.narrato`).
fn declared(beat: &Beat) -> Option<NarrativeMeta> {
    let narrato = beat.media.as_ref()?.get("narrato")?;
    let text = |k: &str| narrato.get(k).and_then(Value::as_str).map(str::to_string);
    Some(NarrativeMeta {
        beat_index: narrato.get("beatIndex").and_then(Value::as_u64).map(|b| b as usize),
        beat_name: text("beatName"),
        purpose: text("purpose"),
        intent: text("intent"),
        expected_intensity: narrato.get("intensity").and_then(Value::as_f64),
        expected_valence: narrato.get("valence").and_then(Value::as_f64),
    })
}

/// The shipped themes' layouts that make one point by design, whose slides are left out of the
/// points a slide makes on average (ADR-0032 item 3, Jay's call, 2026-10-25).
const ONE_IDEA: [&str; 9] = [
    "big-number",
    "number-photo",
    "stat",
    "quote",
    "photo-quote",
    "quote-portrait",
    "statement",
    "thesis",
    "big-question",
];

/// The slides of `deck` as health reads them, each by its first state's id, and the most points
/// any one of its texts makes: its list's items, or its paragraphs (lift-up 10-24b FUNC-2). A
/// grid of short places, a SWOT or a timeline, makes few in each.
fn readings(deck: &Deck, slides: &[Slide]) -> Result<(Vec<SlideForHealth>, Vec<usize>), OpsError> {
    let snaps = resolve_states(deck).map_err(|e| OpsError::new(e.to_string()))?;
    let mut out = Vec::with_capacity(slides.len());
    let mut most = Vec::with_capacity(slides.len());
    for slide in slides {
        let (Some(&first), Some(&last)) = (slide.states.first(), slide.states.last()) else { continue };
        let beat = slide.beat.map(|(.., b)| b);
        let shown = &snaps[last].nodes;
        let texts: Vec<(&String, &Props)> = (shown.iter())
            .filter(|(id, _)| deck.nodes.get(id.as_str()).is_some_and(|n| n.node_type == NodeType::Text))
            .collect();
        let headline = |p: &Props| {
            matches!(p.get("semantic").and_then(Value::as_str), Some("claim"))
                || matches!(p.get("role").and_then(Value::as_str), Some("display" | "headline" | "title"))
        };
        let title = match beat.map(|b| b.claim.trim()).filter(|c| !c.is_empty()) {
            Some(claim) => claim.to_string(),
            None => texts.iter().find(|t| headline(t.1)).map(|t| words(t.1).trim().to_string()).unwrap_or_default(),
        };
        // Each item of a set its own point, its parts together; each item of a list one too.
        let mut lines: Vec<(Option<&str>, String)> = Vec::new();
        let mut points = 0;
        for &(_, props) in &texts {
            let said = words(props);
            let said = said.trim();
            let aside = matches!(props.get("semantic").and_then(Value::as_str), Some("navigation" | "decoration"))
                || props.get("role").and_then(Value::as_str) == Some("kicker");
            if said.is_empty() || said == title || aside {
                continue;
            }
            points = points.max(scaena_core::lists::paragraphs(said).len());
            let listed: Vec<Option<Value>> = match props.get("list") {
                Some(Value::Array(items)) => items.iter().map(|i| (!i.is_null()).then(|| i.clone())).collect(),
                _ => Vec::new(),
            };
            if listed.iter().any(Option::is_some) {
                for (k, range) in scaena_core::lists::paragraphs(said).into_iter().enumerate() {
                    let item = said[range].trim();
                    let marked = listed.get(k).is_some_and(Option::is_some);
                    lines.push((None, if marked { format!("\u{2022} {item}") } else { item.to_string() }));
                }
                continue;
            }
            let item = props.get("at").and_then(|a| a.get("in")).and_then(Value::as_str).and_then(ordinal);
            match lines.iter_mut().find(|(k, _)| item.is_some() && *k == item) {
                Some((_, line)) => {
                    line.push_str(" \u{2014} ");
                    line.push_str(said);
                }
                None => lines.push((item, if item.is_some() { format!("\u{2022} {said}") } else { said.to_string() })),
            }
        }
        let mut notes: Vec<&str> = Vec::new();
        let written = beat.and_then(|b| b.notes.as_deref()).into_iter();
        for note in written.chain(slide.states.iter().filter_map(|k| deck.states[*k].notes.as_deref())) {
            let note = note.trim();
            if !note.is_empty() && !notes.contains(&note) {
                notes.push(note);
            }
        }
        out.push(SlideForHealth {
            id: deck.states[first].id.clone(),
            title,
            body: lines.into_iter().map(|(_, l)| l).collect::<Vec<_>>().join("\n"),
            notes: notes.join("\n"),
            narrative_meta: beat.and_then(declared),
            one_idea: snaps[last].layout.as_deref().is_some_and(|l| ONE_IDEA.contains(&l)),
            drawn: (shown.keys()).any(|id| {
                deck.nodes
                    .get(id.as_str())
                    .is_some_and(|n| matches!(n.node_type, NodeType::Image | NodeType::Chart | NodeType::Table))
            }),
            ..SlideForHealth::default()
        });
        most.push(points);
    }
    Ok((out, most))
}

/// The slides whose beat breaks the order the deck's beats run in: each outside the longest run
/// of slides, in the deck's order, whose beats never go back, with the beat it was written for and
/// the beat it now stands in, the run's slide before it, else after it. A deck as Narrato writes
/// it, its beats in the plan's order, has none; a slide moved out of its beat stands among slides
/// written for another. Vzestup reads drift against where its allocator would put each slide,
/// which a deck of another length than its brief asked, or made by hand, does not follow.
fn moved(slides: &[SlideForHealth]) -> Vec<(usize, usize, usize)> {
    let declared: Vec<(usize, usize)> =
        (slides.iter().enumerate()).filter_map(|(i, s)| Some((i, s.narrative_meta.as_ref()?.beat_index?))).collect();
    // The longest run whose beats never go back: for each slide, the longest that ends with it,
    // and the slide before it there. The first of equally long runs ends it.
    let mut longest = vec![1usize; declared.len()];
    let mut before: Vec<Option<usize>> = vec![None; declared.len()];
    for j in 0..declared.len() {
        for i in 0..j {
            if declared[i].1 <= declared[j].1 && longest[i] + 1 > longest[j] {
                longest[j] = longest[i] + 1;
                before[j] = Some(i);
            }
        }
    }
    let mut run = vec![false; declared.len()];
    let mut last = (0..declared.len()).fold(None, |best: Option<usize>, j| match best {
        Some(b) if longest[b] >= longest[j] => Some(b),
        _ => Some(j),
    });
    while let Some(j) = last {
        run[j] = true;
        last = before[j];
    }
    (0..declared.len())
        .filter(|&j| !run[j])
        .filter_map(|j| {
            let now = (0..j).rev().chain(j + 1..declared.len()).find(|&r| run[r]).map(|r| declared[r].1)?;
            Some((declared[j].0, declared[j].1, now))
        })
        .collect()
}

/// The slides a brief's length asks for (`meta.narrative.brief.length`): `16-25 slides` from 16 to
/// 25, `41+ slides` 41 or more.
fn asked(length: &str) -> Option<(usize, Option<usize>)> {
    let range = length.trim().strip_suffix("slides")?.trim();
    if let Some(least) = range.strip_suffix('+') {
        return Some((least.trim().parse().ok()?, None));
    }
    let (lo, hi) = range.split_once(['-', '\u{2013}'])?;
    Some((lo.trim().parse().ok()?, Some(hi.trim().parse().ok()?)))
}

/// How near neutral a reading may stand and still have read nothing the lexicons know: a tone
/// within it of 0, an intensity within it of [`health::intensity::NEUTRAL`] (ADR-0032 item 2).
const SILENT: f64 = 0.05;

/// More points than a slide makes for a room to take in, as Vzestup's balance reads two to five
/// as full (lift-up 10-24b FUNC-2).
const DENSE_POINTS: usize = 5;
/// More words than a room reads on a slide while the speaker talks.
const DENSE_WORDS: usize = 75;

/// A range of slides, as a person writes it: "16–25", "41 or more".
fn range(lo: usize, hi: Option<usize>) -> String {
    match hi {
        Some(hi) => format!("{lo}\u{2013}{hi}"),
        None => format!("{lo} or more"),
    }
}

/// `n` and the noun, plural but for one.
fn counted(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// A share of the scale, to two places: `0.35`.
fn level(v: f64) -> String {
    format!("{v:.2}")
}

/// A tone, signed: `+0.40`, `−0.25`.
fn signed(v: f64) -> String {
    format!("{}{:.2}", if v >= 0.0 { "+" } else { "\u{2212}" }, v.abs())
}

/// The pacing of `count` slides against the brief's `asked` range, where it asked one: every
/// slide outside it five points off, as Vzestup's pacing takes five a slide from its target.
/// Else Vzestup's, against the 18 slides a talk of no length is paced for.
fn pacing(count: usize, asked: Option<(usize, Option<usize>)>, m: &LocalHealthMetrics) -> Measure {
    let Some((lo, hi)) = asked else {
        let target = m.pacing.target_slide_count;
        let says = match m.pacing.status {
            PacingStatus::Under => {
                format!("{}: a talk with no length asked for is paced for about {target}.", counted(count, "slide"))
            }
            PacingStatus::Optimal => format!("{}, paced as a talk of its kind is.", counted(count, "slide")),
            PacingStatus::Over => {
                format!("{}: a talk with no length asked for is paced for about {target}.", counted(count, "slide"))
            }
        };
        return Measure { score: Some(m.pacing.score as u32), says };
    };
    let outside = if count < lo { lo - count } else { hi.map_or(0, |hi| count.saturating_sub(hi)) };
    let asked = range(lo, hi);
    let says = match () {
        _ if outside == 0 => format!("{}, as the brief asked ({asked}).", counted(count, "slide")),
        _ if count < lo => format!("{} where the brief asked for {asked}: {outside} short.", counted(count, "slide")),
        _ => format!("{} where the brief asked for {asked}: {outside} over.", counted(count, "slide")),
    };
    Measure { score: Some(100u32.saturating_sub(5 * outside as u32)), says }
}

/// How fast a talk is spoken: about 140 words a minute, a presenter's pace with its pauses; and
/// in a script set without spaces between its words (Chinese, Japanese, Korean), about 300 of its
/// characters a minute (lift-up FUNC-1).
const WORDS_A_MINUTE: f64 = 140.0;
const CHARACTERS_A_MINUTE: f64 = 300.0;

/// Whether `c` is a letter of a script read by the character: kana, Han, and Hangul.
fn by_character(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF)
}

/// The seconds `notes` take to say aloud: each word at [`WORDS_A_MINUTE`], and each letter of a
/// script read by the character at [`CHARACTERS_A_MINUTE`].
fn spoken(notes: &str) -> f64 {
    let (mut words, mut characters) = (0usize, 0usize);
    for word in notes.split_whitespace() {
        match word.chars().filter(|&c| by_character(c)).count() {
            0 => words += 1,
            n => characters += n,
        }
    }
    60.0 * (words as f64 / WORDS_A_MINUTE + characters as f64 / CHARACTERS_A_MINUTE)
}

/// What gives a beat its time: the `duration` it is given, its states' holds, or its notes said
/// aloud, whichever comes to most.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Timed {
    Given,
    Held,
    Noted,
}

/// A beat as the talk times it: the index of the first slide it tells, its seconds, and what
/// gives them.
type Beaten = (usize, f64, Timed);

/// How long the talk runs, by its beats (lift-up FUNC-1): each beat the most of its `duration`,
/// the holds of its states, as a video paces it, and its notes and its states' own said aloud,
/// so that notes deepened lengthen the talk; and each beat's seconds, and what gives them, by the
/// index of the first slide it tells. None where a beat has neither a `duration` nor a hold on
/// every state, or a slide neither a beat nor holds, as a deck written by hand with no spine.
fn talk(deck: &Deck, slides: &[Slide]) -> Option<(f64, Vec<Beaten>)> {
    let state = |id: &str| deck.states.iter().find(|s| s.id == id);
    let mut told: Vec<((usize, usize), usize, f64, Timed)> = Vec::new();
    let mut seconds = 0.0;
    for (k, slide) in slides.iter().enumerate() {
        match slide.beat {
            Some((i, j, _)) if told.iter().any(|(at, ..)| *at == (i, j)) => {}
            Some((i, j, beat)) => {
                let holds: Vec<Option<f64>> = beat.states.iter().map(|s| state(s).and_then(|s| s.hold)).collect();
                let held = holds.iter().flatten().sum::<f64>() / 1000.0;
                let given = match beat.duration {
                    Some(d) => d,
                    None if holds.iter().all(Option::is_some) => 0.0,
                    None => return None,
                };
                let noted = spoken(beat.notes.as_deref().unwrap_or_default())
                    + (beat.states.iter())
                        .filter_map(|s| state(s).and_then(|s| s.notes.as_deref()))
                        .map(spoken)
                        .sum::<f64>();
                let (own, by) = [(held, Timed::Held), (noted, Timed::Noted)]
                    .into_iter()
                    .fold((given, Timed::Given), |most, (t, by)| if t > most.0 { (t, by) } else { most });
                told.push(((i, j), k, own, by));
                seconds += own;
            }
            None => {
                let holds: Option<Vec<f64>> = slide.states.iter().map(|&s| deck.states[s].hold).collect();
                seconds += holds?.iter().sum::<f64>() / 1000.0;
            }
        }
    }
    (!told.is_empty()).then(|| (seconds, told.into_iter().map(|(_, k, s, by)| (k, s, by)).collect()))
}

/// A talk's minutes, rounded, as a person says them: "about 8 minutes", "under a minute".
fn minutes(seconds: f64) -> String {
    match (seconds / 60.0).round() as usize {
        0 => "under a minute".to_string(),
        m => format!("about {}", counted(m, "minute")),
    }
}

/// A beat's time: "40 seconds" under a minute and a half, else "about 3 minutes".
fn span(seconds: f64) -> String {
    if seconds < 90.0 { counted(seconds.round() as usize, "second") } else { minutes(seconds) }
}

/// The slot a brief's slides are given, as a person says it: "a 16–30 minute slot", "a slot of
/// up to 15 minutes", "a slot of 46 minutes or more".
fn slot((lo, hi): (u32, Option<u32>)) -> String {
    match (lo, hi) {
        (0, Some(hi)) => format!("a slot of up to {hi} minutes"),
        (lo, Some(hi)) => format!("a {lo}\u{2013}{hi} minute slot"),
        (lo, None) => format!("a slot of {lo} minutes or more"),
    }
}

/// Whether a deck in `language` is in English, by its name or its tag's first part, as the
/// lexicons judge a language named (`scaena_narrato::health`'s `english`): one that names none is.
fn in_english(language: Option<&str>) -> bool {
    let Some(name) = language.map(str::trim).filter(|l| !l.is_empty()) else { return true };
    let name = name.to_lowercase();
    let head = name.split(['-', '_', ' ']).next().unwrap_or_default();
    let en = |t: &str| matches!(t, "en" | "eng" | "english");
    en(&name) || en(head)
}

/// `language` as a person names it: a name as written, or a tag's language by its English name
/// where it is one a made deck is written in; none for a tag of another.
fn language_named(language: &str) -> Option<String> {
    let tag = language.trim().to_lowercase();
    let head = tag.split(['-', '_']).next().unwrap_or_default();
    let named = match head {
        "de" | "deu" | "ger" => "German",
        "fr" | "fra" | "fre" => "French",
        "es" | "spa" => "Spanish",
        "it" | "ita" => "Italian",
        "pt" | "por" => "Portuguese",
        "nl" | "nld" | "dut" => "Dutch",
        "ja" | "jpn" => "Japanese",
        "zh" | "zho" | "chi" => "Chinese",
        "ko" | "kor" => "Korean",
        "ar" | "ara" => "Arabic",
        "hi" | "hin" => "Hindi",
        "ru" | "rus" => "Russian",
        "pl" | "pol" => "Polish",
        "sv" | "swe" => "Swedish",
        _ if head.len() > 3 => return Some(language.trim().to_string()),
        _ => return None,
    };
    Some(named.to_string())
}

/// The layouts a talk closes in (ADR-0032 item 6).
const CLOSING: [&str; 4] = ["closing", "call-to-action", "takeaways", "poster"];

/// Headlines that thank the room or open the floor, in the languages a made deck is written in:
/// the first words of a closing slide that a layout of any kind may hold.
const THANKS: [&str; 16] = [
    "thank",
    "questions",
    "q&a",
    "q & a",
    "danke",
    "vielen dank",
    "fragen",
    "merci",
    "gracias",
    "preguntas",
    "grazie",
    "domande",
    "obrigad",
    "perguntas",
    "ありがとう",
    "質問",
];

/// The first states of the deck's closing slides: the last run of slides in a closing layout,
/// or headed with thanks or questions (ADR-0032 item 6).
fn closing(deck: &Deck, slides: &[Slide], read: &[SlideForHealth]) -> Vec<String> {
    let closes = |i: usize| {
        let layout = slides[i].states.iter().find_map(|&k| deck.states[k].layout.as_deref());
        let title = read[i].title.trim().to_lowercase();
        layout.is_some_and(|l| CLOSING.contains(&l)) || THANKS.iter().any(|t| title.starts_with(t))
    };
    let n = read.len().min(slides.len());
    (0..n).rev().take_while(|&i| closes(i)).map(|i| read[i].id.clone()).collect()
}

/// The opening of a slide's notes: their first line or item, cut at its first sentence's end, a
/// stop that white space or the line's end follows, so a figure's point ("3.5%") ends nothing.
fn opening(notes: &str) -> &str {
    let first = notes.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    let mut chars = first.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if matches!(c, '.' | '!' | '?') && chars.peek().is_none_or(|&(_, next)| next.is_whitespace()) {
            return &first[..at + c.len_utf8()];
        }
    }
    first
}

/// Whether `next`'s notes open by carrying `before` across (narrative dogfood H4): a word of
/// `before`'s headline and body that `next`'s do not use, as coherence reads words, in the
/// opening of `next`'s notes. A word both slides use, a product's name, carries nothing over.
fn bridges(before: &SlideForHealth, next: &SlideForHealth) -> bool {
    let words = |s: &SlideForHealth| health::coherence::keywords(&format!("{} {}", s.title, s.body));
    let carried: Vec<String> = words(before).difference(&words(next)).cloned().collect();
    let opened = health::coherence::keywords(opening(&next.notes));
    carried.iter().any(|w| opened.contains(w))
}

/// The narrative health of `deck`, as its words and its plan read: [`health_in`] with nothing
/// lint found.
pub fn health(deck: &Deck) -> Result<Health, OpsError> {
    health_in(deck, &[])
}

/// The slides at `indices`, from 0, as a person names them: "slide 3", or "slides 3, 5 and 8";
/// and whether there are several.
fn slides_named(indices: &[usize]) -> (String, bool) {
    let named: Vec<String> = indices.iter().map(|i| (i + 1).to_string()).collect();
    match named.split_last() {
        Some((last, [])) => (format!("slide {last}"), false),
        Some((last, rest)) => (format!("slides {} and {last}", rest.join(", ")), true),
        None => (String::new(), false),
    }
}

/// `text` with its first letter a capital.
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The narrative health of `deck`, with what lint finds in it (`lint`, over its bundle): health
/// lays nothing out, so a slide that does not stand as it is to be read is lint's to say. Each
/// slide with an error in one of its states is a finding of its own, ahead of every other: what
/// lint says of it, and the assistant asked to fix it, and the score is capped by the slides that
/// stand (ADR-0032 item 1).
pub fn health_in(deck: &Deck, lint: &[scaena_core::lint::Finding]) -> Result<Health, OpsError> {
    let narrative = deck.meta.as_ref().and_then(|m| m.extra.get("narrative"));
    let arc_name = narrative.and_then(|n| n.get("arc")).and_then(Value::as_str).map(str::to_string);
    let asked =
        (narrative.and_then(|n| n.get("brief")).and_then(|b| b.get("length")).and_then(Value::as_str)).and_then(asked);
    let slides = slides(deck);
    let (read, most) = readings(deck, &slides)?;
    let n = read.len();
    let m = health::metrics(&read, None);
    // The language the deck is written in, its brief's, else its own: the lexicons read English
    // alone, and German's "die" and "will" are English words too, so a deck in another language
    // is not read for intensity or tone, whatever script it is in (dogfood DF7-4).
    let language = (narrative.and_then(|n| n.get("brief")).and_then(|b| b.get("language")).and_then(Value::as_str))
        .or_else(|| deck.meta.as_ref().and_then(|m| m.lang.as_deref()));
    let english = in_english(language);

    // The arc's own curve and beats, as Vzestup's graph draws them. Fidelity is read against the
    // arc the deck was written to, where it names one, as that graph reads it; else against a
    // talk's curve, as Vzestup's score is.
    let curve = arc::curve(arc_name.as_deref(), n);
    // An outlined deck's beats are its outline's, each by the name the person settled, as its
    // slides were written to them (`beatIndex`, `beatName`); else the arc's (lift-up 10-25b
    // STAB-1). A beat moved, added, or taken away in the outline is the deck's, not the arc's.
    let settled: Vec<String> = (narrative.and_then(|n| n.get("outline")).and_then(|o| o.get("beats")))
        .and_then(Value::as_array)
        .map(|beats| {
            (beats.iter())
                .map(|b| b.get("name").and_then(Value::as_str).unwrap_or_default().trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    let beats: Vec<String> = match settled.is_empty() {
        true => arc::beat_names(arc_name.as_deref()).into_iter().map(str::to_string).collect(),
        false => settled,
    };
    let beat_name = |k: usize| beats.get(k).cloned().unwrap_or_else(|| format!("beat {}", k + 1));
    let (fidelity_score, bends) = match &arc_name {
        Some(_) => {
            let own = arc::fidelity(&read, &curve, &arc::FidelityOptions::default());
            ((own.fidelity * 100.0).round(), own.deviant_slides)
        }
        None => (m.arc_fidelity.score, m.arc_fidelity.deviant_slides.clone()),
    };
    let drifted = moved(&read);
    // A calm close is no bend (ADR-0032 item 6, Jay's call, 2026-10-25): the deck's last slides,
    // a closing, a call to action, takeaways, a poster, or thanks or questions, come down from
    // the climax as a talk does, so a plan cooler than the curve there is not found.
    let closing = closing(deck, &slides, &read);
    let bends: Vec<_> = bends.into_iter().filter(|d| !(d.delta < 0.0 && closing.contains(&d.slide_id))).collect();

    // Pacing against the brief's range, where it asked one, and the overall again with it.
    let mut pacing = pacing(n, asked, &m);
    // The time its beats take, beside the count (lift-up FUNC-1): Vzestup's score is the count's.
    let talk = talk(deck, &slides);
    let given = asked.map(|(lo, hi)| (range(lo, hi), scaena_narrato::brief::minutes_for_slides(lo)));
    if let Some((seconds, _)) = talk {
        let said = capitalized(&minutes(seconds));
        pacing.says = match &given {
            Some((count, given)) => {
                format!("{} {said} of talk, where {count} slides is {}.", pacing.says, slot(*given))
            }
            None => format!("{} {said} of talk.", pacing.says),
        };
    }
    let score = match (asked, &arc_name) {
        (None, None) => m.overall_score as u32,
        _ => {
            let weighed = f64::from(pacing.score.unwrap_or(0)) * COMPOSITE_FALLBACK_WEIGHT_PACING
                + m.content_balance.score * COMPOSITE_FALLBACK_WEIGHT_BALANCE
                + fidelity_score * COMPOSITE_FALLBACK_WEIGHT_ARC;
            let weights =
                COMPOSITE_FALLBACK_WEIGHT_PACING + COMPOSITE_FALLBACK_WEIGHT_BALANCE + COMPOSITE_FALLBACK_WEIGHT_ARC;
            (weighed / weights).round() as u32
        }
    };

    let balance = &m.content_balance;
    let mut short = Vec::new();
    if balance.avg_bullets_per_slide < 2.0 {
        short
            .push(format!("slides make {} points each, where two to five read as full", balance.avg_bullets_per_slide));
    }
    if balance.avg_bullets_per_slide > 5.0 {
        short.push(format!("slides make {} points each, more than a room takes in", balance.avg_bullets_per_slide));
    }
    if balance.avg_speaker_notes_length < 30.0 {
        short.push(format!("their notes run {} characters, too few to speak from", balance.avg_speaker_notes_length));
    }
    let balance = Measure {
        score: Some(balance.score as u32),
        says: match short.is_empty() {
            true => "Each slide makes two to five points, and its notes give the speaker enough to say.".into(),
            false => {
                let said = short.join("; ");
                format!("{}{}.", said[..1].to_uppercase(), &said[1..])
            }
        },
    };
    // A turn the next slide's notes open by carrying the slide before it across is bridged: the
    // speaker says what the slides' words do not (narrative dogfood H4).
    let bridged: Vec<&String> = (m.coherence.weak_transitions.iter())
        .filter(|id| read.iter().position(|s| &s.id == *id).is_some_and(|i| i > 0 && bridges(&read[i - 1], &read[i])))
        .collect();
    let hard = m.coherence.breaks.iter().filter(|b| b.severity == health::coherence::Severity::Hard).count();
    let soft = m.coherence.breaks.len() - hard;
    // By words alone, neighbouring slides share few even where one follows from the other: no
    // number, only where the talk turns.
    let coherence = Measure {
        score: match m.coherence.method {
            health::coherence::Method::Embedding => Some(m.coherence.score as u32),
            health::coherence::Method::Keyword => None,
        },
        says: match (hard, soft) {
            (0, 0) => "No slide breaks from the one before it, read by their words.".into(),
            _ => format!(
                "Read by their words: {} where the talk turns sharply, {} at a beat's turn{}.",
                counted(hard, "slide"),
                soft,
                match bridged.len() {
                    0 => String::new(),
                    n => format!("; the notes bridge {n} of them"),
                }
            ),
        },
    };
    let curve_of = match &arc_name {
        Some(arc) => format!("{arc}'s curve"),
        None => "a talk's curve".to_string(),
    };
    let fidelity = Measure {
        score: Some(fidelity_score as u32),
        says: match bends.len() {
            0 => format!("The intensity the slides declare follows {curve_of}."),
            k => format!("{} bend the intensity away from {curve_of}.", counted(k, "slide")),
        },
    };
    // Slides that declare what `of` reads, and of those, the ones read: a deck whose words the
    // lexicons do not read (another language than English) is not measured, rather than read clean.
    let declaring = |of: fn(&NarrativeMeta) -> Option<f64>| {
        (read.iter().filter(|s| s.eligible()))
            .filter(|s| s.narrative_meta.as_ref().and_then(of).is_some_and(f64::is_finite))
            .count()
    };
    // A gap is found only where the words read something: words within `SILENT` of neutral read
    // nothing the lexicons know, and a lexicon's silence says nothing of how a slide sounds
    // (ADR-0032 item 2, Jay's call, 2026-10-25). Gaps the English lexicons read in another
    // language's words are not the deck's at all.
    let heard_intensity: Vec<&health::IntensityGap> = (m.intensity_gaps.iter())
        .filter(|g| english && (g.detected - health::intensity::NEUTRAL).abs() >= SILENT)
        .collect();
    let heard_tone: Vec<&health::ToneGap> =
        m.tone_gaps.iter().filter(|g| english && g.detected.abs() >= SILENT).collect();
    let silent = |all: usize, heard: usize| if english { all - heard } else { 0 };
    let matched = |r: &health::MatchReport, gaps: usize, unread: usize, declared: usize, what: &str| match (
        if english { r.measured_slides } else { 0 },
        gaps,
    ) {
        (0, _) if declared > 0 => Measure {
            score: None,
            says: format!(
                "{} {what}, but health reads it only in English{}.",
                if declared == 1 { "One slide declares".to_string() } else { format!("{declared} slides declare") },
                // A deck named in another language says so; one in another script reads no words.
                match language.filter(|_| !english).map(language_named) {
                    Some(Some(name)) => format!(", and this deck is in {name}"),
                    Some(None) => ", and this deck is in another language".to_string(),
                    None => " words, three or more, and none has them".to_string(),
                }
            ),
        },
        // Nothing measured, nothing scored: a full mark would read as praise (lift-up STAB-4).
        (0, _) => Measure { score: None, says: format!("No slide declares {what}: a deck made from a prompt does.") },
        (_, 0) if unread > 0 => Measure {
            score: Some(r.score as u32),
            says: format!(
                "The words carry the {what} each slide declares, where they read any: on {} the lexicons read none.",
                counted(unread, "slide")
            ),
        },
        (_, 0) => {
            Measure { score: Some(r.score as u32), says: format!("The words carry the {what} each slide declares.") }
        }
        _ => Measure {
            score: Some(r.score as u32),
            says: match unread {
                0 => format!("{} read far from the {what} they declare.", counted(gaps, "slide")),
                _ => format!(
                    "{} read far from the {what} they declare; on {} the lexicons read none.",
                    counted(gaps, "slide"),
                    counted(unread, "slide")
                ),
            },
        },
    };
    let intensity = matched(
        &m.intensity_match,
        heard_intensity.len(),
        silent(m.intensity_gaps.len(), heard_intensity.len()),
        declaring(|n| n.expected_intensity),
        "intensity",
    );
    let tone = matched(
        &m.tone_match,
        heard_tone.len(),
        silent(m.tone_gaps.len(), heard_tone.len()),
        declaring(|n| n.expected_valence),
        "tone",
    );

    // Each slide.
    let place: HashMap<&str, usize> = read.iter().enumerate().map(|(i, s)| (s.id.as_str(), i)).collect();
    let grounded = scaena_core::files::grounded(deck);
    let slides_read: Vec<Read> = (read.iter().enumerate())
        .map(|(i, s)| {
            let meta = s.narrative_meta.as_ref();
            let text = s.text();
            let standing = if drifted.iter().any(|d| d.0 == i) {
                Standing::Drifted
            } else if bends.iter().any(|d| d.slide_id == s.id) {
                Standing::Bends
            } else {
                Standing::Ok
            };
            let beat = slides[i].beat.map(|(.., b)| b.id.clone());
            let sources = (grounded.iter())
                .filter(|g| beat.as_ref().is_some_and(|b| g.cited.contains(b)))
                .map(|g| g.name.clone())
                .collect();
            Read {
                state: s.id.clone(),
                beat,
                title: s.title.clone(),
                curve: curve.get(i).copied().unwrap_or(0.5),
                declared: meta.and_then(|m| m.expected_intensity).filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0)),
                read: health::intensity::detected(&text, language),
                tone: meta.and_then(|m| m.expected_valence).filter(|v| v.is_finite()).map(|v| v.clamp(-1.0, 1.0)),
                tone_read: health::valence::detected(&text, language),
                after: i.checked_sub(1).and_then(|p| m.coherence.pair_similarities.get(p).copied()),
                standing,
                sources,
            }
        })
        .collect();

    // What to look at, in the order health finds it.
    let mut findings = Vec::new();
    let mut find = |id: &str, severity: Severity, kind: Kind, says: String, ask: String, fix: Option<Fix>| {
        if let Some(&i) = place.get(id) {
            findings.push(Finding { state: id.to_string(), slide: i + 1, severity, kind, says, ask, fix });
        }
    };
    // Where in the spine a slide's narrative is declared.
    let at = |id: &str, key: &str| -> Option<String> {
        let (i, j, _) = slides[*place.get(id)?].beat?;
        Some(format!("/spine/sections/{i}/beats/{j}/media/narrato/{key}"))
    };
    for (i, s) in read.iter().enumerate() {
        let k = i + 1;
        if s.title.trim().is_empty() {
            find(
                &s.id,
                Severity::Error,
                Kind::Title,
                format!("Slide {k} has no headline."),
                format!("Give slide {k} a headline that states its point in a sentence."),
                None,
            );
        }
        if s.body.trim().is_empty() && s.notes.trim().is_empty() {
            find(
                &s.id,
                Severity::Warning,
                Kind::Empty,
                format!("Slide {k} says nothing past its headline, and has no notes."),
                format!("Add to slide {k} what bears out its headline, or notes that say what to tell the room."),
                None,
            );
        }
    }
    for id in m.coherence.weak_transitions.iter().filter(|id| !bridged.contains(id)) {
        let Some(&i) = place.get(id.as_str()) else { continue };
        let (k, before) = (i + 1, i);
        find(
            id,
            Severity::Warning,
            Kind::Turn,
            format!("Slide {k} shares little with slide {before}: the turn between them may lose the room."),
            format!("Open slide {k}'s notes with the line that carries the talk from slide {before}."),
            None,
        );
    }
    // A bend judges the intensity a slide was planned at against the arc's curve where it stands,
    // so its plan, or its place, is what moves it, never its words, which the gap below judges
    // (ADR-0032, item 4). How far each slide's plan is from what its place calls for.
    let bent: HashMap<&str, f64> = bends.iter().map(|d| (d.slide_id.as_str(), d.delta)).collect();
    // Three or more neighbouring slides planned off the curve the same way are one pattern, found
    // once on the first of them (narrative dogfood H5): each slide's plan and what its place calls
    // for, as ranges, and where the talk peaks against where the arc does. The graph still marks
    // each slide that bends.
    let planned_at = |i: usize| {
        read[i]
            .narrative_meta
            .as_ref()
            .and_then(|n| n.expected_intensity)
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0.0, 1.0))
    };
    let mut placed: Vec<(usize, f64)> =
        bends.iter().filter_map(|d| place.get(d.slide_id.as_str()).map(|&i| (i, d.delta))).collect();
    scaena_core::sort::by_key(&mut placed, |&(i, _)| i);
    let mut runs: Vec<Vec<(usize, f64)>> = Vec::new();
    for (i, delta) in placed {
        match runs.last_mut() {
            Some(run) if run.last().is_some_and(|&(j, d)| j + 1 == i && (d > 0.0) == (delta > 0.0)) => {
                run.push((i, delta))
            }
            _ => runs.push(vec![(i, delta)]),
        }
    }
    let runs: Vec<Vec<(usize, f64)>> =
        runs.into_iter().filter(|run| run.len() >= 3 && run.iter().all(|&(i, _)| planned_at(i).is_some())).collect();
    let folded: Vec<&str> = runs.iter().flatten().map(|&(i, _)| read[i].id.as_str()).collect();
    // The first slide the talk is planned most intense at, and the first the arc's curve peaks at.
    let peak_of = |values: &[Option<f64>]| {
        (values.iter().enumerate()).filter_map(|(i, v)| v.map(|v| (i, v))).fold(
            None,
            |most: Option<(usize, f64)>, (i, v)| match most {
                Some((_, m)) if m >= v => most,
                _ => Some((i, v)),
            },
        )
    };
    let talk_peak = peak_of(&(0..read.len()).map(planned_at).collect::<Vec<_>>());
    let arc_peak = peak_of(&curve.iter().map(|&c| Some(c)).collect::<Vec<_>>()).map(|(i, _)| i);
    for run in &runs {
        let (first, last) = (run[0].0, run[run.len() - 1].0);
        let hotter = run[0].1 > 0.0;
        let span = |values: Vec<f64>| {
            let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if level(lo) == level(hi) { level(lo) } else { format!("{}\u{2013}{}", level(lo), level(hi)) }
        };
        let plans: Vec<f64> = run.iter().filter_map(|&(i, _)| planned_at(i)).collect();
        let calls: Vec<f64> = run.iter().filter_map(|&(i, d)| planned_at(i).map(|p| p - d)).collect();
        let slides = format!("Slides {}\u{2013}{}", first + 1, last + 1);
        let builds = match arc_peak {
            Some(p) if p + 1 == read.len() => format!("where {curve_of} builds to its end"),
            Some(p) => format!("where {curve_of} peaks at slide {}", p + 1),
            None => format!("against {curve_of}"),
        };
        let peaks = match talk_peak {
            Some((p, v)) => format!("it peaks at slide {} ({}), {builds}", p + 1, level(v)),
            None => builds,
        };
        let says = format!(
            "{slides} were planned {} than their places in the talk ({}, where {} is called for): {peaks}.",
            if hotter { "hotter" } else { "cooler" },
            span(plans),
            span(calls)
        );
        let later = match (talk_peak, arc_peak) {
            (Some((t, _)), Some(a)) if a > t => ", or move its peak later",
            (Some((t, _)), Some(a)) if a < t => ", or move its peak earlier",
            _ => "",
        };
        let ask = match hotter {
            true => format!(
                "Plan slides {}\u{2013}{} calmer, so the talk builds as its arc does{later}.",
                first + 1,
                last + 1
            ),
            false => format!(
                "Plan slides {}\u{2013}{} more intense, so the talk carries what their places call for{later}.",
                first + 1,
                last + 1
            ),
        };
        find(&read[first].id, Severity::Info, Kind::Arc, says, ask, None);
    }
    for d in bends.iter().filter(|d| !folded.contains(&d.slide_id.as_str())) {
        let Some(&i) = place.get(d.slide_id.as_str()) else { continue };
        let k = i + 1;
        let planned = read[i].narrative_meta.as_ref().and_then(|n| n.expected_intensity).filter(|v| v.is_finite());
        let (hotter, way, moved) = match d.delta > 0.0 {
            true => ("hotter", "calmer", "nearer the climax"),
            false => ("cooler", "more intense", "away from the climax"),
        };
        let (says, ask) = match planned {
            // What fidelity read is the plan, held to the scale: its place calls for that less the bend.
            Some(planned) => {
                let planned = planned.clamp(0.0, 1.0);
                let (at, calls) = (level(planned), level(planned - d.delta));
                let than = match d.delta > 0.0 {
                    true => "more intense than",
                    false => "calmer than",
                };
                (
                    format!("Slide {k} was planned {hotter} than its place in the talk."),
                    format!(
                        "Slide {k} is planned at {at}, {than} its place in the story calls for ({calls}): plan it {way}, or move it {moved}."
                    ),
                )
            }
            // A slide that declares none is read as a talk usually runs where it stands.
            None => (
                format!(
                    "Slide {k} declares no intensity, where its place in the talk calls for a {way} slide than most."
                ),
                format!("Plan slide {k}'s intensity for its place in the story."),
            ),
        };
        find(&d.slide_id, Severity::Info, Kind::Arc, says, ask, None);
    }
    for g in heard_intensity {
        let Some(&i) = place.get(g.slide_id.as_str()) else { continue };
        let k = i + 1;
        let (declared, detected) = (level(g.declared), level(g.detected));
        let says = match g.gap > 0.0 {
            true => format!("Slide {k} reads flatter than it was written to be."),
            false => format!("Slide {k} reads hotter than it was written to be."),
        };
        // In a person's words; the figures are for the assistant, which reads the deck's. Where
        // the plan bends from the arc, the words are to carry what the place calls for, not the
        // plan, and words that already do leave the plan to change (ADR-0032, item 4).
        let ask = match bent.get(g.slide_id.as_str()).map(|delta| g.declared - delta) {
            Some(calls) if (g.detected - calls).abs() <= INTENSITY_GAP_THRESHOLD => format!(
                "Slide {k}'s words read {detected}, as its place in the story calls for ({}): it is its plan ({declared}) that strays, so plan it for its place, not the words.",
                level(calls)
            ),
            Some(calls) if g.detected < calls => format!(
                "Make slide {k}'s words carry the intensity its place in the story calls for ({}): sharper verbs, a figure that lands.",
                level(calls)
            ),
            Some(calls) => format!(
                "Calm slide {k}'s words to the intensity its place in the story calls for ({}); they read {detected}.",
                level(calls)
            ),
            None if g.gap > 0.0 => format!(
                "Make slide {k}'s words carry the intensity it declares ({declared}): sharper verbs, a figure that lands."
            ),
            None => format!("Calm slide {k}'s words to the intensity it declares ({declared}); they read {detected}."),
        };
        // No fix: the plan's intensity is the arc's, which the words are to carry, not follow.
        find(&g.slide_id, Severity::Warning, Kind::Intensity, says, ask, None);
    }
    for g in heard_tone {
        let Some(&i) = place.get(g.slide_id.as_str()) else { continue };
        let k = i + 1;
        let (declared, detected) = (signed(g.declared), signed(g.detected));
        let (says, ask) = match g.gap > 0.0 {
            true => (
                format!("Slide {k} reads graver than it was written to be."),
                format!("Warm slide {k}'s words toward the tone it declares ({declared}); they read {detected}."),
            ),
            false => (
                format!("Slide {k} reads more hopeful than it was written to be."),
                format!("Ground slide {k}'s words in the tone it declares ({declared}); they read {detected}."),
            ),
        };
        // No fix: the plan's tone is the arc's, which the words are to carry, not follow.
        find(&g.slide_id, Severity::Warning, Kind::Tone, says, ask, None);
    }
    for &(i, was_beat, now_beat) in &drifted {
        let id = read[i].id.as_str();
        let (was, now) = (beat_name(was_beat), beat_name(now_beat));
        let k = i + 1;
        let fix = at(id, "beatIndex").zip(at(id, "beatName")).map(|(index, name)| Fix {
            says: format!("Make it part of {now}"),
            patch: vec![
                json!({ "op": "replace", "path": index, "value": now_beat }),
                json!({ "op": "replace", "path": name, "value": now }),
            ],
        });
        find(
            id,
            Severity::Warning,
            Kind::Drift,
            format!("Slide {k} was written for {was}, and now stands in {now}."),
            format!(
                "Slide {k} was written for the {was} part of the talk and now stands in {now}: rewrite it for its new place, or move it back."
            ),
            fix,
        );
    }
    // The settled outline (narrative FUNC-1, lift-up 10-24b FUNC-1): each beat's slides carry
    // its claim, and the closing beat lands the big idea, read by their words as coherence
    // reads them. A sentence with no word to read by (another script) says nothing either way.
    let outline = narrative.and_then(|n| n.get("outline"));
    let big_idea = (outline.and_then(|o| o.get("bigIdea")).and_then(Value::as_str))
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(str::to_string);
    let outlined: Vec<(String, String)> = (outline.and_then(|o| o.get("beats")).and_then(Value::as_array))
        .into_iter()
        .flatten()
        .map(|b| {
            let text = |k: &str| b.get(k).and_then(Value::as_str).unwrap_or_default().trim().to_string();
            (text("name"), text("claim"))
        })
        .collect();
    // Each outline beat's slides, by the beat each was written for.
    let of_beat = |k: usize| -> Vec<usize> {
        (0..read.len()).filter(|&i| read[i].narrative_meta.as_ref().and_then(|m| m.beat_index) == Some(k)).collect()
    };
    let shares = |sentence: &str, of: &[usize]| -> Option<bool> {
        let said = content_words(sentence);
        if said.is_empty() {
            return None;
        }
        Some(of.iter().any(|&i| !content_words(&format!("{} {}", read[i].title, read[i].body)).is_disjoint(&said)))
    };
    let spanned = |of: &[usize]| match (of.first(), of.last()) {
        (Some(a), Some(b)) if a != b => format!("slides {}\u{2013}{}", a + 1, b + 1),
        _ => format!("slide {}", of[0] + 1),
    };
    for (k, (name, claim)) in outlined.iter().enumerate() {
        let of = of_beat(k);
        if claim.is_empty() || of.is_empty() || shares(claim, &of) != Some(false) {
            continue;
        }
        let part = if name.is_empty() { format!("beat {}", k + 1) } else { name.clone() };
        let slides = spanned(&of);
        let (does, carry) = if of.len() > 1 { ("say", "they carry") } else { ("says", "it carries") };
        let mut said = slides.clone();
        said[..1].make_ascii_uppercase();
        find(
            &read[of[0]].id,
            Severity::Warning,
            Kind::Claim,
            format!("{said} {does} nothing of the claim settled for {part}: \u{201c}{claim}\u{201d}"),
            format!(
                "Check the claim: the outline settled that the {part} part of the talk leaves the audience believing \u{201c}{claim}\u{201d}. Rewrite {slides} so {carry} it, or tell me what to settle instead."
            ),
            None,
        );
    }
    if let Some(idea) = &big_idea {
        let closing = (0..outlined.len()).rev().map(of_beat).find(|of| !of.is_empty());
        if let Some(of) = closing.filter(|of| shares(idea, of) == Some(false)) {
            let slides = spanned(&of);
            find(
                &read[of[0]].id,
                Severity::Warning,
                Kind::BigIdea,
                format!("The close, {slides}, never lands the big idea: \u{201c}{idea}\u{201d}"),
                format!(
                    "The big idea is \u{201c}{idea}\u{201d}. Rewrite the closing {slides} so the talk ends by landing it."
                ),
                None,
            );
        }
    }
    // Coaching, slide by slide (lift-up 10-24b FUNC-2): the narrative lint's warnings where they
    // fall, a slide that says more than a room takes in, one that leaves the speaker nothing to
    // say, and the slides to cut first where the deck runs past what its brief asked.
    let slide_of_state = |id: &str| slides.iter().position(|sl| sl.states.iter().any(|&j| deck.states[j].id == id));
    let slide_of_beat = |path: &str| -> Option<usize> {
        let rest = path.strip_prefix("/spine/sections/")?;
        let mut parts = rest.split('/');
        let s: usize = parts.next()?.parse().ok()?;
        let b: usize = parts.nth(1)?.parse().ok()?;
        slides.iter().position(|sl| sl.beat.is_some_and(|(i, j, _)| i == s && j == b))
    };
    let slide_of_path = |path: &str| -> Option<usize> {
        let i: usize = path.strip_prefix("/states/")?.split('/').next()?.parse().ok()?;
        slide_of_state(&deck.states.get(i)?.id)
    };
    let mut told: Vec<(usize, &str)> = Vec::new();
    for f in lint.iter().filter(|f| matches!(f.code.as_str(), "W420" | "W421" | "W424" | "W425")) {
        let at = (f.state.as_deref().and_then(slide_of_state))
            .or_else(|| f.path.as_deref().and_then(|p| slide_of_beat(p).or_else(|| slide_of_path(p))));
        let Some(i) = at.filter(|&i| i < read.len()) else { continue };
        if told.contains(&(i, f.code.as_str())) {
            continue;
        }
        told.push((i, f.code.as_str()));
        let k = i + 1;
        let said = scaena_core::words::said(&f.code, f.measure.as_ref()).unwrap_or_else(|| f.message.clone());
        let ask = match f.code.as_str() {
            "W420" => format!("Say slide {k}'s claim on it: the one sentence it is there to prove, as its headline."),
            "W421" => {
                format!(
                    "Slide {k} ends on evidence: say on it what the evidence proves, so the room knows why it is there."
                )
            }
            "W424" => format!("Keep one claim on slide {k}: make the others the evidence for it, or split the slide."),
            _ => format!("Make slide {k} say something slide {} doesn't, or fold the two together.", i.max(1)),
        };
        find(&read[i].id, Severity::Warning, Kind::Story, format!("Slide {k}: {said}"), ask, None);
    }
    let mut unnoted = Vec::new();
    for (i, s) in read.iter().enumerate() {
        let k = i + 1;
        let points = most.get(i).copied().unwrap_or(0);
        let words = s.body.split_whitespace().count();
        if points > DENSE_POINTS {
            find(
                &s.id,
                Severity::Warning,
                Kind::Dense,
                format!("Slide {k} makes {points} points in one place, more than a room takes in."),
                format!("Tighten slide {k} to the {DENSE_POINTS} points that matter most, or split it in two."),
                None,
            );
        } else if words > DENSE_WORDS {
            find(
                &s.id,
                Severity::Warning,
                Kind::Dense,
                format!("Slide {k} shows {words} words, more than a room reads while you speak."),
                format!("Tighten slide {k}'s words to what the room must read, and move the rest to its notes."),
                None,
            );
        }
        if !s.body.trim().is_empty() && s.notes.trim().is_empty() {
            unnoted.push(i);
        }
    }
    // The slides with nothing for the presenter to say, as one finding at the first of them: a
    // deck written by hand has no notes on any, and a row for each would bury the rest.
    if let Some(&first) = unnoted.first() {
        let (named, many) = slides_named(&unnoted);
        let (have, each) = if many { ("have", "each") } else { ("has", "it") };
        find(
            &read[first].id,
            Severity::Info,
            Kind::Notes,
            format!("{} {have} no notes for you.", capitalized(&named)),
            format!("Draft notes for {named}, in order: what to tell the room beyond what {each} shows."),
            None,
        );
    }
    if let Some((lo, Some(hi))) = asked.filter(|_| n > 2) {
        let over = n.saturating_sub(hi);
        if over > 0 {
            // The slides that turn least from the one before, the first and the last kept.
            let pairs = &m.coherence.pair_similarities;
            let mut cut: Vec<usize> = (1..n.saturating_sub(1)).collect();
            scaena_core::sort::by_key(&mut cut, |&i| {
                std::cmp::Reverse((pairs.get(i - 1).copied().unwrap_or(0.0) * 1e6) as i64)
            });
            cut.truncate(over);
            scaena_core::sort::by_key(&mut cut, |&i| i);
            if let Some(&first) = cut.first() {
                let (named, many) = slides_named(&cut);
                let adds =
                    if many { "add least to the slides before them" } else { "adds least to the slide before it" };
                find(
                    &read[first].id,
                    Severity::Warning,
                    Kind::Pacing,
                    format!(
                        "The deck runs {} past the {} slides the brief asked for: {named} {adds}.",
                        counted(over, "slide"),
                        range(lo, Some(hi))
                    ),
                    format!(
                        "Bring the deck to the {} slides the brief asked for: {named} {adds}. Fold them in, or cut them.",
                        range(lo, Some(hi))
                    ),
                    None,
                );
            }
        }
    }
    // A talk that runs short of the slot its slides are given, or past it, found on its shortest
    // beat or its longest, with the beats that run so named (lift-up FUNC-1).
    if let (Some((seconds, beats)), Some((count, (lo, hi)))) = (&talk, &given) {
        let said = (seconds / 60.0).round() as u32;
        let under = said < *lo;
        if under || hi.is_some_and(|hi| said > hi) {
            let mut by: Vec<Beaten> = beats.clone();
            scaena_core::sort::by_key(&mut by, |&(_, s, _)| (s * 1000.0) as i64);
            let ((short, shortest, _), (long, longest, timed)) = (by[0], by[by.len() - 1]);
            if !under {
                by.reverse();
            }
            let mut first: Vec<usize> = by.iter().take(3).map(|&(k, ..)| k).collect();
            scaena_core::sort::by_key(&mut first, |&k| k);
            let (named, many) = slides_named(&first);
            let given = slot((*lo, *hi));
            let (at, says, ask) = if under {
                (
                    short,
                    format!(
                        "The talk runs {}, short of {given} for {count} slides. Its shortest beat, slide {}'s, runs {}; its longest, slide {}'s, {}.",
                        minutes(*seconds),
                        short + 1,
                        span(shortest),
                        long + 1,
                        span(longest)
                    ),
                    format!(
                        "Deepen the notes of {named}, the shortest {}, until the talk fills {given}: what to tell the room beyond what {} shows.",
                        if many { "beats" } else { "beat" },
                        if many { "each" } else { "it" }
                    ),
                )
            } else {
                (
                    long,
                    format!(
                        "The talk runs {}, past {given} for {count} slides. Its longest beat, slide {}'s, runs {} {}; its shortest, slide {}'s, {}.",
                        minutes(*seconds),
                        long + 1,
                        span(longest),
                        match timed {
                            Timed::Given => "by the time it is given",
                            Timed::Held => "by its holds",
                            Timed::Noted => "by its notes",
                        },
                        short + 1,
                        span(shortest)
                    ),
                    format!(
                        "Bring the talk within {given}, the longest {} first: tighten or cut {named}.",
                        if many { "beats" } else { "beat" }
                    ),
                )
            };
            find(&read[at].id, Severity::Warning, Kind::Pacing, says, ask, None);
        }
    }
    // A slide lint finds an error on: what the first says, and how many more there are.
    for (i, slide) in slides.iter().enumerate() {
        let k = i + 1;
        let ids: Vec<&str> = slide.states.iter().map(|&j| deck.states[j].id.as_str()).collect();
        let errors: Vec<&scaena_core::lint::Finding> = (lint.iter())
            .filter(|f| f.severity == Severity::Error && f.state.as_deref().is_some_and(|s| ids.contains(&s)))
            .collect();
        let Some(first) = errors.first() else { continue };
        let what =
            scaena_core::words::said(&first.code, first.measure.as_ref()).unwrap_or_else(|| first.message.clone());
        let more = match errors.len() - 1 {
            0 => String::new(),
            1 => " (1 more error)".to_string(),
            n => format!(" ({n} more errors)"),
        };
        find(
            ids[0],
            Severity::Error,
            Kind::Layout,
            format!("Slide {k} does not stand as it is to be read: {what}{more}"),
            format!("Fix what lint finds wrong on slide {k}, so it reads as it should: {what}"),
            None,
        );
    }
    // Each change the model asked once it saw the deck drawn that was let go, and why (lift-up
    // USE-4): what the person may try again, by hand or with the assistant.
    let refused = narrative.and_then(|n| n.get("refused")).and_then(Value::as_array).into_iter().flatten();
    for r in refused {
        let text = |key: &str| r.get(key).and_then(Value::as_str).unwrap_or_default();
        let Some(k) = r.get("slide").and_then(Value::as_u64).map(|k| k as usize) else { continue };
        let Some(slide) = k.checked_sub(1).and_then(|i| slides.get(i)) else { continue };
        let (what, why) = (text("what"), text("why"));
        find(
            &deck.states[slide.states[0]].id,
            Severity::Info,
            Kind::Look,
            format!("Slide {k} was kept as made: the model asked for {what}, and lint found it worse for that. {why}"),
            format!("The model asked slide {k} for {what}. Make that change without what lint found it caused: {why}"),
            None,
        );
    }
    // A slide that does not stand first, then in the order the slides stand, and on each slide in
    // the order health found them.
    scaena_core::sort::by_key(&mut findings, |f| (f.kind != Kind::Layout, f.slide));
    // A slide that does not stand caps the score at the clean slides' share, less ten for each
    // that does not (ADR-0032 item 1, Jay's call, 2026-10-25).
    let broken = findings.iter().filter(|f| f.kind == Kind::Layout).count();
    let score = match broken {
        0 => score,
        b => score.min(((100.0 * (n - b) as f64 / n as f64).round() as u32).saturating_sub(10 * b as u32)),
    };

    Ok(Health {
        score,
        arc: arc_name,
        beats,
        pacing,
        balance,
        coherence,
        fidelity,
        intensity,
        tone,
        slides: slides_read,
        findings,
        big_idea,
    })
}

/// The words of `text` that coherence reads a slide by: lowercased, past three letters, Latin
/// letters and figures alone, as `scaena_narrato::health::coherence::keyword_overlap` splits them.
fn content_words(text: &str) -> std::collections::BTreeSet<String> {
    (text.to_lowercase().split(|c: char| !c.is_ascii_lowercase() && !c.is_ascii_digit()))
        .filter(|w| w.len() > 3)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::narrative::{Pictures, deck};
    use scaena_core::model::theme::Theme;
    use scaena_narrato::{Brief, Narrative, answer};
    use std::path::{Path, PathBuf};

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// The climate talk as Narrato writes it, 25 slides asked for as 16 to 25, made in Dusk.
    fn climate() -> Value {
        let text = std::fs::read_to_string(root().join("crates/scaena-narrato/tests/answers/climate.json")).unwrap();
        let written = answer::slides(&text).unwrap();
        let narrative = Narrative {
            arc: "Data Storytelling".into(),
            arc_reasoning: "The evidence, then what it means.".into(),
            executive_summary: written.executive_summary,
            slides: written.slides,
            truncated: false,
            icons: Default::default(),
            sources: Vec::new(),
            outline: None,
            brief: Brief {
                topic: "Climate change, where we stand, for an audience of colleagues".into(),
                audience: "Colleagues".into(),
                goal: "Inform".into(),
                length: "16-25 slides".into(),
                ..Brief::default()
            },
        };
        let dusk = std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap();
        let theme: Theme = serde_json::from_str(&dusk).unwrap();
        deck(&narrative, &theme, &Pictures::new()).unwrap().deck
    }

    fn of(deck: &Value) -> Health {
        health(&Deck::from_value(deck).unwrap()).unwrap()
    }

    /// Every beat of `deck` given `seconds`.
    fn timed(deck: &mut Value, seconds: f64) {
        for section in deck["spine"]["sections"].as_array_mut().unwrap() {
            for beat in section["beats"].as_array_mut().unwrap() {
                beat["duration"] = json!(seconds);
            }
        }
    }

    /// The talk's time beside the count (lift-up FUNC-1): its beats' durations, or their states'
    /// holds where those come to more, said in the pacing line against the slot the brief's
    /// slides are given; a talk short of it found on its shortest beat, and one past it on its
    /// longest, with Vzestup's score as it was.
    #[test]
    fn health_paces_a_talk_by_its_time() {
        let mut deck = climate();
        // What the notes take to say is counted too: here, at first, none.
        for section in deck["spine"]["sections"].as_array_mut().unwrap() {
            for beat in section["beats"].as_array_mut().unwrap() {
                beat.as_object_mut().unwrap().remove("notes");
            }
        }
        for state in deck["states"].as_array_mut().unwrap() {
            state.as_object_mut().unwrap().remove("notes");
        }
        let n = of(&deck).slides.len();
        assert!((16..=25).contains(&n), "{n}");
        let timing =
            |h: &Health| h.findings.iter().filter(|f| f.says.starts_with("The talk runs")).cloned().collect::<Vec<_>>();
        let slot =
            |m: usize| format!(" About {m} minutes of talk, where 16\u{2013}25 slides is a 31\u{2013}45 minute slot.");

        // In the slot: said, and nothing found.
        timed(&mut deck, 38.0 * 60.0 / n as f64);
        let h = of(&deck);
        assert!(h.pacing.says.ends_with(&slot(38)), "{}", h.pacing.says);
        assert!(timing(&h).is_empty(), "{:?}", h.findings);
        let score = (h.score, h.pacing.score);

        // Short of it: found on the shortest beat, the three shortest named.
        timed(&mut deck, 30.0);
        let short = beat_of(&deck, &h.slides[4].state);
        *deck.pointer_mut(&format!("{short}/duration")).unwrap() = json!(10.0);
        let h = of(&deck);
        assert_eq!((h.score, h.pacing.score), score, "the score is Vzestup's");
        let found = timing(&h);
        assert_eq!(found.len(), 1, "{:?}", h.findings);
        assert_eq!((found[0].slide, found[0].kind, found[0].severity), (5, Kind::Pacing, Severity::Warning));
        assert!(found[0].says.starts_with(&format!("The talk runs about {} minutes, short of a 31\u{2013}45 minute slot for 16\u{2013}25 slides. Its shortest beat, slide 5's, runs 10 seconds; its longest, slide ", (10 + 30 * (n - 1)) / 60)), "{}", found[0].says);
        assert!(
            found[0].ask.starts_with("Deepen the notes of slides 1, 2 and 5, the shortest beats,"),
            "{}",
            found[0].ask
        );

        // Doing what it asks clears it: the shortest beat's notes deepened, 3,000 words at 140 a
        // minute, lift the talk into the slot, and the line's minutes with it.
        let words = |n: usize| "word ".repeat(n).trim_end().to_string();
        deck.pointer_mut(&short).unwrap()["notes"] = json!(words(3000));
        let h = of(&deck);
        assert!(timing(&h).is_empty(), "{:?}", h.findings);
        let m = ((30.0 * (n - 1) as f64 + 3000.0 * 60.0 / WORDS_A_MINUTE) / 60.0).round() as usize;
        assert!(h.pacing.says.ends_with(&slot(m)), "{} for {m}", h.pacing.says);
        // Pacing's score is Vzestup's count; the overall moves only as notes fill out balance.
        assert_eq!(h.pacing.score, score.1, "pacing's score is Vzestup's");

        // A beat long by its notes alone: past the slot, found on it, by its notes.
        deck.pointer_mut(&short).unwrap()["notes"] = json!(words(7000));
        let h = of(&deck);
        let found = timing(&h);
        assert_eq!(found.len(), 1, "{:?}", h.findings);
        assert_eq!(found[0].slide, 5);
        assert!(found[0].says.contains(", past a 31\u{2013}45 minute slot for 16\u{2013}25 slides. Its longest beat, slide 5's, runs about 50 minutes by its notes;"), "{}", found[0].says);
        assert!(
            found[0].ask.starts_with(
                "Bring the talk within a 31\u{2013}45 minute slot, the longest beats first: tighten or cut slides "
            ),
            "{}",
            found[0].ask
        );
        deck.pointer_mut(&short).unwrap().as_object_mut().unwrap().remove("notes");

        // A hold that comes to more wins, as a video paces it: past the slot, found on the
        // longest beat, by its holds.
        let state = h.slides[2].state.clone();
        let at = deck["states"].as_array().unwrap().iter().position(|s| s["id"] == state).unwrap();
        deck["states"][at]["hold"] = json!(50.0 * 60_000.0);
        let h = of(&deck);
        let found = timing(&h);
        assert_eq!(found.len(), 1, "{:?}", h.findings);
        assert_eq!(found[0].slide, 3);
        assert!(found[0].says.contains(", past a 31\u{2013}45 minute slot for 16\u{2013}25 slides. Its longest beat, slide 3's, runs about 50 minutes by its holds;"), "{}", found[0].says);

        // A beat with no time of its own: the talk's time is not known, and nothing is said.
        let beat = beat_of(&deck, &h.slides[0].state);
        deck.pointer_mut(&beat).unwrap().as_object_mut().unwrap().remove("duration");
        let h = of(&deck);
        assert!(!h.pacing.says.contains("of talk"), "{}", h.pacing.says);
        assert!(timing(&h).is_empty());

        // Notes are said a word at a time, and a script without spaces a character at a time.
        assert_eq!(spoken("one two  three\nfour"), 4.0 * 60.0 / WORDS_A_MINUTE);
        assert_eq!(spoken("ありがとうございます"), 10.0 * 60.0 / CHARACTERS_A_MINUTE);
        assert_eq!(spoken("감사합니다 여러분"), 8.0 * 60.0 / CHARACTERS_A_MINUTE);
    }

    /// The first slide past the cover whose words read an intensity, the lexicons not silent on
    /// them (ADR-0032 item 2), and the plan at the far end from what they read: the peak for words
    /// that read calmer than the middle, else rest.
    fn heard(h: &Health) -> Vec<(usize, f64)> {
        (h.slides.iter().enumerate().skip(1))
            .filter_map(|(i, s)| s.read.filter(|r| (r - health::intensity::NEUTRAL).abs() >= SILENT).map(|r| (i, r)))
            .map(|(i, r)| (i, if r < 0.5 { 1.0 } else { 0.0 }))
            .collect()
    }

    /// `deck` with the slide whose first state is `state` planned at `intensity`.
    fn planned(deck: &mut Value, state: &str, intensity: f64) {
        let beat = beat_of(deck, state);
        *deck.pointer_mut(&format!("{beat}/media/narrato/intensity")).unwrap() = json!(intensity);
    }

    /// A turn the notes bridge is not found (narrative dogfood H4): coherence reads a slide's
    /// headline and body, so the finding asks for the line that carries the talk over in the
    /// next slide's notes, and notes that open by carrying a word of the slide before across,
    /// one the next slide's words do not use, answer it. A word further in, or one both slides
    /// use, does not. No figure moves.
    #[test]
    fn a_turn_the_notes_bridge_is_not_found() {
        let mut deck = climate();
        let before = of(&deck);
        let turn = before.findings.iter().find(|f| f.kind == Kind::Turn).expect("a turn its words do not make").clone();
        let k = turn.slide;
        assert_eq!(
            turn.ask,
            format!("Open slide {k}'s notes with the line that carries the talk from slide {}.", k - 1)
        );
        let d = Deck::from_value(&deck).unwrap();
        let read = readings(&d, &slides(&d)).unwrap().0;
        let words = |s: &SlideForHealth| health::coherence::keywords(&format!("{} {}", s.title, s.body));
        let (was, next) = (words(&read[k - 2]), words(&read[k - 1]));
        let carried = was.difference(&next).next().cloned().expect("a word of the slide before");
        let shared = was.intersection(&next).next().cloned();
        let beat = beat_of(&deck, &turn.state);
        let at = deck["states"].as_array().unwrap().iter().position(|s| s["id"] == turn.state.as_str()).unwrap();
        let noted = |deck: &mut Value, notes: String| {
            deck.pointer_mut(&beat).unwrap()["notes"] = json!(notes);
            deck["states"][at].as_object_mut().unwrap().remove("notes");
            of(deck)
        };
        let found = |h: &Health| h.findings.iter().any(|f| f.kind == Kind::Turn && f.state == turn.state);

        let h = noted(&mut deck, format!("From {carried} we turn to what comes next.\nThen the rest."));
        assert!(!found(&h), "{:#?}", h.findings);
        assert!(h.coherence.says.contains("; the notes bridge "), "{}", h.coherence.says);
        assert_eq!(h.coherence.score, before.coherence.score, "no figure moves");
        // A figure's point ends no sentence: the opening runs on past "3.5" to the word carried.
        let h = noted(&mut deck, format!("Retention rose 3.5% since {carried}. Then the rest."));
        assert!(!found(&h), "{:#?}", h.findings);
        assert_eq!(opening("Rose 3.5% since then. Later."), "Rose 3.5% since then.");
        assert_eq!(opening("\n  - Up 2.1x!\nMore"), "- Up 2.1x!");
        let h = noted(&mut deck, format!("We turn to what comes next. Later, {carried}."));
        assert!(found(&h), "a word past the notes' opening carries nothing over");
        if let Some(shared) = shared {
            let h = noted(&mut deck, format!("{shared} again, and onward."));
            assert!(found(&h), "a word both slides use carries nothing over");
        }
    }

    /// One pattern, one finding (narrative dogfood H5): three or more neighbouring slides planned
    /// hotter than their places are found once, on the first, saying the run, its plans and what
    /// its places call for, and where the talk peaks against the arc; the graph still marks each
    /// slide that bends, and two in a row stay a finding each.
    #[test]
    fn a_run_of_bends_is_one_finding() {
        let mut deck = climate();
        let ids: Vec<String> = of(&deck).slides.iter().map(|s| s.state.clone()).collect();
        let plan = |deck: &mut Value, i: usize, v: f64| {
            let beat = beat_of(deck, &ids[i]);
            *deck.pointer_mut(&format!("{beat}/media/narrato/intensity")).unwrap() = json!(v);
        };
        for i in 1..=4 {
            plan(&mut deck, i, 1.0);
        }
        let h = of(&deck);
        let arcs: Vec<&Finding> = h.findings.iter().filter(|f| f.kind == Kind::Arc).collect();
        let run = arcs.iter().find(|f| f.says.starts_with("Slides 2\u{2013}")).expect("the run, found once");
        assert_eq!(run.slide, 2);
        // The plans as a range up to the 1.00 set here, a neighbour already bending joining it.
        assert!(run.says.contains(" were planned hotter than their places in the talk ("), "{}", run.says);
        assert!(run.says.contains("1.00, where "), "{}", run.says);
        assert!(run.says.contains("it peaks at slide 2 (1.00)"), "{}", run.says);
        assert!(run.ask.starts_with("Plan slides 2\u{2013}"), "{}", run.ask);
        let end: usize = run.says["Slides 2\u{2013}".len()..].split(' ').next().unwrap().parse().unwrap();
        assert!(end >= 4, "{}", run.says);
        for k in 2..=end {
            assert!(
                !arcs.iter().any(|f| f.says.starts_with(&format!("Slide {k} "))),
                "slide {k} is the run's: {arcs:#?}"
            );
            assert_eq!(h.slides[k - 1].standing, Standing::Bends, "the graph marks slide {k}");
        }
    }

    /// The spine's beat that tells the state `id`: its place, as a JSON pointer.
    fn beat_of(deck: &Value, id: &str) -> String {
        for (i, section) in deck["spine"]["sections"].as_array().unwrap().iter().enumerate() {
            for (j, beat) in section["beats"].as_array().unwrap().iter().enumerate() {
                if beat["states"].as_array().unwrap().iter().any(|s| s == id) {
                    return format!("/spine/sections/{i}/beats/{j}");
                }
            }
        }
        panic!("no beat tells {id}")
    }

    /// Coaching slide by slide (lift-up 10-24b FUNC-2): a narrative warning lint finds is a
    /// `story` finding on its slide in the editors' words; a slide of more than five points is
    /// `dense`; a content slide with no notes asks for them; and a deck past the range its brief
    /// asked names the slides that turn least as the first to cut.
    #[test]
    fn health_coaches_slide_by_slide() {
        use scaena_core::lint::Finding as Linted;
        let mut deck = climate();
        let d = Deck::from_value(&deck).unwrap();
        let id = |k: usize| deck["states"][k]["id"].as_str().unwrap().to_string();
        let lint = [Linted::new("W424", Severity::Warning, "two claims")
            .state(id(3))
            .measure(json!({ "claims": 2, "max": 1 }))];
        let h = health_in(&d, &lint).unwrap();
        let slide = h.slides.iter().position(|s| s.state == id(3)).map(|i| i + 1);
        let story = h.findings.iter().find(|f| f.kind == Kind::Story).expect("a story finding");
        assert_eq!(Some(story.slide), slide);
        assert_eq!(story.says, format!("Slide {}: More than one claim is on screen at once.", story.slide));
        assert!(story.ask.starts_with(&format!("Keep one claim on slide {}", story.slide)), "{}", story.ask);

        // Seven points on a slide, and no notes for it.
        let first = h.slides.iter().position(|s| !s.title.is_empty()).unwrap();
        let state = &h.slides[first].state;
        let points = (1..=7).map(|n| format!("Point {n}")).collect::<Vec<_>>().join("\n");
        let list: Vec<Value> = (1..=7).map(|_| json!({ "kind": "bullet" })).collect();
        deck["nodes"]["dense"] = json!({ "type": "text", "role": "body", "text": points, "list": list, "at": { "rect": [0.1, 0.4, 0.8, 0.5] } });
        // On every state of the slide, so its last shows it.
        for st in deck["states"].as_array_mut().unwrap() {
            if st["id"] == *state || st["slide"] == *state {
                st["props"]["dense"] = json!({});
            }
        }
        let h = of(&deck);
        let dense = h.findings.iter().find(|f| f.kind == Kind::Dense && f.slide == first + 1);
        assert!(
            dense.is_some_and(|f| f.says.ends_with("makes 7 points in one place, more than a room takes in.")),
            "{:?}",
            h.findings
        );
        assert!((h.findings.iter()).all(|f| f.kind != Kind::Notes), "a made deck writes notes on every slide");

        // Two slides with nothing for the presenter to say: one finding, at the first, naming both.
        let slides: Vec<String> = (deck["states"].as_array().unwrap().iter())
            .filter(|st| st.get("slide").is_none())
            .map(|st| st["id"].as_str().unwrap().to_string())
            .collect();
        let mut unnoted = deck.clone();
        for st in unnoted["states"].as_array_mut().unwrap() {
            let of = st.get("slide").cloned().unwrap_or_else(|| st["id"].clone());
            if of == slides[2] || of == slides[4] {
                st.as_object_mut().unwrap().remove("notes");
            }
        }
        for section in unnoted["spine"]["sections"].as_array_mut().unwrap() {
            for beat in section["beats"].as_array_mut().unwrap() {
                let tells = |id: &String| beat["states"].as_array().unwrap().iter().any(|s| s == id.as_str());
                if tells(&slides[2]) || tells(&slides[4]) {
                    beat.as_object_mut().unwrap().remove("notes");
                }
            }
        }
        let h = of(&unnoted);
        let noted: Vec<&Finding> = h.findings.iter().filter(|f| f.kind == Kind::Notes).collect();
        assert_eq!(noted.len(), 1, "{:?}", h.findings);
        assert_eq!((noted[0].slide, noted[0].severity), (3, Severity::Info));
        assert_eq!(noted[0].says, "Slides 3 and 5 have no notes for you.");
        assert_eq!(
            noted[0].ask,
            "Draft notes for slides 3 and 5, in order: what to tell the room beyond what each shows."
        );

        // Twenty-five slides where ten to fifteen were asked: ten to cut, named.
        deck["meta"]["narrative"]["brief"]["length"] = json!("10-15 slides");
        let h = of(&deck);
        let paced: Vec<&Finding> = h.findings.iter().filter(|f| f.kind == Kind::Pacing).collect();
        assert_eq!(paced.len(), 1, "{:?}", h.findings);
        assert!(
            paced[0]
                .says
                .starts_with("The deck runs 10 slides past the 10\u{2013}15 slides the brief asked for: slides "),
            "{}",
            paced[0].says
        );
        assert!(paced[0].slide > 1 && paced[0].slide < 25);
    }

    /// The settled outline read (lift-up 10-24b FUNC-1): a deck whose beats each say their claim
    /// and whose close says its big idea is found carrying both, and the big idea is given for the
    /// editors to show; a claim no slide of its beat says, and a big idea the close never says,
    /// are each found on the beat's first slide, with what to ask.
    #[test]
    fn health_reads_the_settled_outline() {
        let mut deck = climate();
        let h = of(&deck);
        // Each slide's arc beat, as the spine declares it.
        let written: Vec<usize> = (h.slides.iter())
            .map(|s| {
                let at = format!("{}/media/narrato/beatIndex", beat_of(&deck, &s.state));
                deck.pointer(&at).and_then(Value::as_u64).unwrap() as usize
            })
            .collect();
        let first_of = |k: usize| written.iter().position(|&w| w == k).unwrap();
        let beats: Vec<Value> = (h.beats.iter().enumerate())
            .map(|(k, name)| json!({ "name": name, "claim": h.slides[first_of(k)].title, "slides": 1 }))
            .collect();
        let close = first_of(h.beats.len() - 1);
        deck["meta"]["narrative"]["outline"] = json!({ "bigIdea": h.slides[close].title, "beats": beats });
        let carried = of(&deck);
        let outlined = |h: &Health| -> Vec<(Kind, usize)> {
            (h.findings.iter())
                .filter(|f| matches!(f.kind, Kind::Claim | Kind::BigIdea))
                .map(|f| (f.kind, f.slide))
                .collect()
        };
        assert_eq!(outlined(&carried), [], "{:?}", carried.findings);
        assert_eq!(carried.big_idea.as_deref(), Some(h.slides[close].title.as_str()));

        let second = h.beats[1].clone();
        deck["meta"]["narrative"]["outline"]["beats"][1]["claim"] = json!("Penguins juggle marmalade quietly");
        deck["meta"]["narrative"]["outline"]["bigIdea"] = json!("Zebras orchestrate symphonies underwater");
        let missed = of(&deck);
        assert_eq!(
            outlined(&missed),
            [(Kind::Claim, first_of(1) + 1), (Kind::BigIdea, close + 1)],
            "{:?}",
            missed.findings
        );
        let claim = missed.findings.iter().find(|f| f.kind == Kind::Claim).unwrap();
        assert!(
            claim.says.contains(&second) && claim.says.ends_with("\u{201c}Penguins juggle marmalade quietly\u{201d}"),
            "{}",
            claim.says
        );
        assert!(claim.ask.starts_with("Check the claim:"), "{}", claim.ask);
        let big = missed.findings.iter().find(|f| f.kind == Kind::BigIdea).unwrap();
        assert!(big.says.starts_with("The close, slide") && big.ask.starts_with("The big idea is"), "{big:?}");
        // A big idea in another script has no words to read by: nothing is found of it.
        deck["meta"]["narrative"]["outline"]["bigIdea"] = json!("気候変動は止められる");
        assert!(!of(&deck).findings.iter().any(|f| f.kind == Kind::BigIdea));
    }

    #[test]
    fn a_generated_talk_reads_as_its_beats_tell_it() {
        let deck = climate();
        let h = of(&deck);
        assert_eq!(h.slides.len(), 25);
        assert_eq!(h.arc.as_deref(), Some("Data Storytelling"));
        assert!(!h.beats.is_empty(), "the arc's beats, by name");
        // Each slide by its first state, its headline its beat's claim.
        for (read, state) in h.slides.iter().zip(deck["states"].as_array().unwrap()) {
            assert_eq!(read.state, state["id"]);
            let beat = deck.pointer(&beat_of(&deck, read.state.as_str())).unwrap();
            assert_eq!(read.title, beat["claim"]);
            assert_eq!(read.declared, beat["media"]["narrato"]["intensity"].as_f64());
        }
        // Asked for 16 to 25 slides, it has 25: paced as asked, its beats a short talk for them.
        assert_eq!(
            h.pacing,
            Measure {
                score: Some(100),
                says: "25 slides, as the brief asked (16\u{2013}25). About 8 minutes of talk, where 16\u{2013}25 slides is a 31\u{2013}45 minute slot.".into()
            }
        );
        // Read by its words alone, coherence gets no number.
        assert_eq!(h.coherence.score, None);
        // As written, its beats in the plan's order: nothing has drifted.
        assert!(!h.findings.iter().any(|f| f.kind == Kind::Drift), "{:#?}", h.findings);
        assert!((0..=100).contains(&h.score), "{}", h.score);
        // The curve the graph draws is the arc's own, one point a slide.
        assert!(h.slides.iter().all(|s| (0.0..=1.0).contains(&s.curve)));
        assert!(h.slides[0].after.is_none() && h.slides[1].after.is_some());
        // Every finding is on a slide, in order, and says what to ask.
        assert!(h.findings.windows(2).all(|w| w[0].slide <= w[1].slide), "{:#?}", h.findings);
        // A finding names its slide, or the run of slides it folds ("Slides 2–7"), or is the talk's
        // pacing.
        assert!(
            (h.findings.iter()).all(|f| !f.ask.is_empty() && (f.says.starts_with("Slide") || f.kind == Kind::Pacing)),
            "{:#?}",
            h.findings
        );
    }

    /// Each slide names the sources its beat cites of those the deck was grounded in (narrative
    /// FUNC-3): what the Story tab shows on its row; none where it cites none.
    #[test]
    fn each_slide_names_the_sources_its_beat_cites() {
        let mut deck = climate();
        assert!(of(&deck).slides.iter().all(|s| s.sources.is_empty()), "the talk keeps no sources");
        deck["meta"]["narrative"]["sources"] = json!([
            { "name": "ipcc.pdf", "kind": "pdf", "bytes": 900, "truncated": false },
            { "name": "https://example.org/noaa", "kind": "web page", "url": "https://example.org/noaa", "bytes": 300, "truncated": false },
        ]);
        let third = deck["states"][2]["id"].as_str().unwrap().to_string();
        let beat = beat_of(&deck, &third);
        deck.pointer_mut(&beat).unwrap()["evidence"] = json!(["source:ipcc.pdf", "https://example.org/noaa"]);
        let h = of(&deck);
        assert_eq!(h.slides[2].sources, ["ipcc.pdf", "https://example.org/noaa"]);
        assert!(h.slides.iter().enumerate().all(|(i, s)| i == 2 || s.sources.is_empty()));
    }

    #[test]
    fn a_slide_whose_words_do_not_carry_what_it_declares_is_asked_to_say_it() {
        let mut deck = climate();
        let before = of(&deck);
        // A slide whose words read something, planned at the far end from it.
        let (i, plan) = *heard(&before).first().expect("a slide whose words read an intensity");
        let state = before.slides[i].state.clone();
        planned(&mut deck, &state, plan);
        let h = of(&deck);
        let k = i + 1;
        let finding = h.findings.iter().find(|f| f.state == state && f.kind == Kind::Intensity).expect("a gap");
        assert_eq!(finding.slide, k);
        // In a person's words, the figures in the question for the assistant.
        let way = if plan > 0.5 { "flatter" } else { "hotter" };
        assert_eq!(finding.says, format!("Slide {k} reads {way} than it was written to be."));
        // Asked of the words on that slide, held to its plan or, where the plan bends, its place.
        assert!(finding.ask.to_lowercase().contains(&format!("slide {k}'s words")), "{}", finding.ask);
        // The plan is the arc's: the words are to carry it, so no fix declares what they read
        // in its place.
        assert_eq!(finding.fix, None);
    }

    /// Health says nothing it did not read (Jay's call, 2026-10-25). Words the lexicons read
    /// nothing of, within `SILENT` of neutral, are not found far from what a slide declares, while
    /// words that read something still are (ADR-0032 item 2); and a calm close is no bend: the
    /// deck's last run of closing slides is not found planned cooler than the curve (item 6). No
    /// figure moves.
    #[test]
    fn health_says_nothing_it_did_not_read() {
        let mut deck = climate();
        let before = of(&deck);
        let declare = |deck: &mut Value, state: &str, key: &str, v: f64| {
            let beat = beat_of(deck, state);
            *deck.pointer_mut(&format!("{beat}/media/narrato/{key}")).unwrap() = json!(v);
        };
        // Intensity: a slide whose words read nothing, planned at the peak, and one whose words
        // read something, planned at the far end from it.
        let silent = (before.slides.iter())
            .find(|s| s.read.is_some_and(|r| (r - health::intensity::NEUTRAL).abs() < SILENT))
            .expect("a slide whose words read no intensity")
            .state
            .clone();
        let (i, plan) = *heard(&before).first().expect("a slide whose words read an intensity");
        let heard = before.slides[i].state.clone();
        // Tone: a slide whose words read neither grave nor hopeful, and one whose words read
        // something, each declared at the far end from it.
        let toneless = (before.slides.iter())
            .find(|s| s.tone_read.is_some_and(|t| t.abs() < SILENT))
            .expect("a slide whose words read no tone")
            .state
            .clone();
        let (toned, read) = (before.slides.iter())
            .find_map(|s| s.tone_read.filter(|t| t.abs() >= SILENT).map(|t| (s.state.clone(), t)))
            .expect("a slide whose words read a tone");
        declare(&mut deck, &silent, "intensity", 1.0);
        declare(&mut deck, &heard, "intensity", plan);
        declare(&mut deck, &toneless, "valence", 1.0);
        declare(&mut deck, &toned, "valence", if read > 0.0 { -1.0 } else { 1.0 });
        let h = of(&deck);
        let on = |state: &str, kind: Kind| h.findings.iter().any(|f| f.state == state && f.kind == kind);
        assert!(!on(&silent, Kind::Intensity), "words that read nothing are not found flat: {:#?}", h.findings);
        assert!(
            on(&heard, Kind::Intensity),
            "words that read something are found far from the plan: {:#?}",
            h.findings
        );
        if toneless != toned {
            assert!(!on(&toneless, Kind::Tone), "words that read no tone are not found neutral: {:#?}", h.findings);
        }
        assert!(on(&toned, Kind::Tone), "words that read a tone are found far from it: {:#?}", h.findings);
        assert!(h.findings.iter().all(|f| !f.says.contains("reads neutral")), "{:#?}", h.findings);
        assert!(h.intensity.says.contains("the lexicons read none"), "{}", h.intensity.says);
        // The figures are Vzestup's, as without the change.
        let d = Deck::from_value(&deck).unwrap();
        let m = health::metrics(&readings(&d, &slides(&d)).unwrap().0, None);
        assert_eq!(
            (h.intensity.score, h.tone.score),
            (Some(m.intensity_match.score as u32), Some(m.tone_match.score as u32))
        );

        // A calm close: the last slide, in the Closing layout and planned at rest, is no bend.
        let mut deck = climate();
        let last = before.slides.last().unwrap().state.clone();
        let at = deck["states"].as_array().unwrap().iter().position(|s| s["id"] == last.as_str()).unwrap();
        deck["states"][at]["layout"] = json!("closing");
        declare(&mut deck, &last, "intensity", 0.0);
        let cooler = |h: &Health, state: &str| {
            h.findings.iter().any(|f| f.state == state && f.kind == Kind::Arc && f.says.contains("cooler"))
        };
        let closed = of(&deck);
        assert!(!cooler(&closed, &last), "a calm close is no bend: {:#?}", closed.findings);
        assert_eq!(closed.slides.last().unwrap().standing, Standing::Ok);
        // The same slide in a layout that does not close, and headed as no thanks, bends.
        deck["states"][at]["layout"] = json!("statement");
        let open = of(&deck);
        let title = open.slides.last().unwrap().title.to_lowercase();
        if !THANKS.iter().any(|t| title.starts_with(t)) {
            assert!(cooler(&open, &last), "the same plan away from a close bends: {:#?}", open.findings);
        }
        assert_eq!(closed.fidelity.score, open.fidelity.score, "no figure moves");
    }

    /// A slide planned hotter than its place in the story is asked to change its plan, or its
    /// place, never its words, and its words are held to what its place calls for, not to the
    /// plan that strays from it: the two findings on it point one way (ADR-0032, item 4). No
    /// figure moves.
    #[test]
    fn a_plan_that_bends_from_the_arc_is_asked_of_the_plan_not_the_words() {
        let climate = climate();
        let before = of(&climate);
        // The first slide whose words read something that, planned at the far end from them,
        // both bends from the arc and reads far from its plan.
        let (deck, h, state, plan) = (heard(&before).into_iter())
            .find_map(|(i, plan)| {
                let mut deck = climate.clone();
                let state = before.slides[i].state.clone();
                planned(&mut deck, &state, plan);
                let h = of(&deck);
                let on = |kind: Kind| h.findings.iter().any(|f| f.state == state && f.kind == kind);
                (on(Kind::Arc) && on(Kind::Intensity)).then_some((deck, h, state, plan))
            })
            .expect("a slide whose plan bends and whose words read far from it");
        let _ = deck;
        let on = |kind: Kind| h.findings.iter().find(|f| f.state == state && f.kind == kind);
        let bend = on(Kind::Arc).expect("the plan bends from the arc");
        let k = bend.slide;
        let (hotter, way) = if plan > 0.5 { ("hotter", "calmer") } else { ("cooler", "more intense") };
        assert_eq!(bend.says, format!("Slide {k} was planned {hotter} than its place in the talk."));
        assert!(
            bend.ask.contains(&format!("is planned at {plan:.2}")) && bend.ask.contains(&format!("plan it {way}")),
            "{}",
            bend.ask
        );
        assert!(!bend.ask.contains("words"), "{}", bend.ask);
        let gap = on(Kind::Intensity).expect("its words read far from its plan");
        assert!(!gap.ask.contains("intensity it declares"), "{}", gap.ask);
        // What the place calls for, as each says it: the same figure.
        let calls = |ask: &str| {
            let at = ask.find("calls for (").map(|i| i + "calls for (".len()).expect(ask);
            ask[at..at + 4].to_string()
        };
        assert_eq!(calls(&gap.ask), calls(&bend.ask), "{}\n{}", bend.ask, gap.ask);
        // A slide whose plan keeps to the arc is held to its plan, as before.
        for f in before.findings.iter().filter(|f| f.kind == Kind::Intensity) {
            let bends = before.findings.iter().any(|b| b.kind == Kind::Arc && b.state == f.state);
            assert!(bends || f.ask.contains("intensity it declares"), "{}", f.ask);
        }
        // Only what the findings say changed: every figure is as before but the plan's.
        assert_eq!((h.pacing.clone(), h.balance.clone()), (before.pacing, before.balance));
    }

    /// A slide lint finds an error on stands ahead of every other finding, said as lint says it,
    /// and the figures are Vzestup's as they were without it.
    #[test]
    fn a_slide_lint_finds_an_error_on_comes_first() {
        use scaena_core::lint::Finding as Linted;
        let deck = climate();
        let d = Deck::from_value(&deck).unwrap();
        let plain = health(&d).unwrap();
        let id = |k: usize| deck["states"][k]["id"].as_str().unwrap().to_string();
        let lint = [
            Linted::new("E100", Severity::Error, "text overflows").state(id(6)),
            Linted::new("E101", Severity::Error, "collision").state(id(6)),
            Linted::new("W301", Severity::Warning, "off the grid").state(id(2)),
            Linted::new("E100", Severity::Error, "text overflows").state(id(9)),
            Linted::new("E102", Severity::Error, "no such slot"),
        ];
        let h = health_in(&d, &lint).unwrap();
        let broken: Vec<(usize, Severity, &str)> = (h.findings.iter())
            .take_while(|f| f.kind == Kind::Layout)
            .map(|f| (f.slide, f.severity, f.says.as_str()))
            .collect();
        assert_eq!(
            broken,
            [
                (
                    7,
                    Severity::Error,
                    "Slide 7 does not stand as it is to be read: The text doesn't fit its box. (1 more error)"
                ),
                (10, Severity::Error, "Slide 10 does not stand as it is to be read: The text doesn't fit its box."),
            ]
        );
        assert!(h.findings[0].ask.starts_with("Fix what lint finds wrong on slide 7"), "{}", h.findings[0].ask);
        // Every other finding as it was, and every figure but the score.
        assert_eq!(h.findings[2..], plain.findings[..]);
        let figures =
            |h: &Health| (h.pacing.clone(), h.balance.clone(), h.fidelity.clone(), h.intensity.clone(), h.tone.clone());
        assert_eq!(figures(&h), figures(&plain));
        assert_eq!(h.slides, plain.slides);
        // The score capped by the slides that stand: 23 of 25, less ten for each of the two that
        // do not (ADR-0032 item 1).
        assert_eq!((plain.score, h.slides.len(), h.score), (84, 25, 72));
    }

    /// A slide that does not stand caps the score at the clean slides' share, less ten for each:
    /// one in 25 leaves a cap over the score, two bring it down, a warning moves nothing, and a
    /// deck no slide of which stands scores nothing (ADR-0032 item 1).
    #[test]
    fn the_score_is_capped_by_the_slides_that_stand() {
        use scaena_core::lint::Finding as Linted;
        let deck = climate();
        let d = Deck::from_value(&deck).unwrap();
        let id = |k: usize| deck["states"][k]["id"].as_str().unwrap().to_string();
        let one = [Linted::new("E100", Severity::Error, "text overflows").state(id(6))];
        // 24 of 25 stand: a cap of 86, over the score's 84.
        assert_eq!(health_in(&d, &one).unwrap().score, 84);
        let two = [one[0].clone(), Linted::new("E100", Severity::Error, "text overflows").state(id(9))];
        assert_eq!(health_in(&d, &two).unwrap().score, 72);
        let warned = [Linted::new("W210", Severity::Warning, "too many words").state(id(6))];
        assert_eq!(health_in(&d, &warned).unwrap().score, 84);
        let every: Vec<Linted> = (0..deck["states"].as_array().unwrap().len())
            .map(|k| Linted::new("E100", Severity::Error, "text overflows").state(id(k)))
            .collect();
        assert_eq!(health_in(&d, &every).unwrap().score, 0);
    }

    /// An outlined deck's beats are its outline's (lift-up 10-25b STAB-1): a person who renamed
    /// and reordered them sees a drift say the beats the deck has, and its fix write the outline's
    /// name, not the arc's.
    #[test]
    fn an_outlined_decks_beats_are_its_outlines() {
        let mut deck = climate();
        let arc = of(&deck).beats;
        let names: Vec<String> = (0..arc.len()).map(|k| format!("Part {} of ours", k + 1)).collect();
        let beats: Vec<Value> = names.iter().map(|n| json!({ "name": n, "claim": "", "slides": 1 })).collect();
        deck["meta"]["narrative"]["outline"] = json!({ "bigIdea": "", "beats": beats });
        // The agenda, written for the second beat, moved to the end.
        let states = deck["states"].as_array_mut().unwrap();
        let agenda = states.remove(1);
        let id = agenda["id"].as_str().unwrap().to_string();
        states.push(agenda);
        let h = of(&deck);
        assert_eq!(h.beats, names, "the outline's beats, not the arc's: {arc:?}");
        let drift = h.findings.iter().find(|f| f.state == id && f.kind == Kind::Drift).expect("it drifted");
        assert!(drift.says.contains("Part ") && drift.says.contains(" of ours"), "{}", drift.says);
        assert!(arc.iter().all(|a| !drift.says.contains(a.as_str())), "{}", drift.says);
        let fix = drift.fix.as_ref().expect("a fix");
        let named = fix.patch[1]["value"].as_str().unwrap();
        assert!(names.iter().any(|n| n == named), "the fix writes the outline's name: {named}");
        assert_eq!(fix.says, format!("Make it part of {named}"));
    }

    #[test]
    fn a_slide_moved_into_another_beat_has_drifted() {
        let mut deck = climate();
        // The agenda, written for the context, moved to the end.
        let states = deck["states"].as_array_mut().unwrap();
        let agenda = states.remove(1);
        let id = agenda["id"].as_str().unwrap().to_string();
        states.push(agenda);
        let h = of(&deck);
        assert_eq!(h.slides[24].state, id);
        assert_eq!(h.slides[24].standing, Standing::Drifted);
        let drift = h.findings.iter().find(|f| f.state == id && f.kind == Kind::Drift).expect("it drifted");
        assert_eq!(drift.slide, 25);
        let beat = beat_of(&deck, &id);
        let fix = drift.fix.as_ref().expect("a fix");
        assert_eq!(fix.patch[0]["path"], format!("{beat}/media/narrato/beatIndex"));
        assert_eq!(fix.patch[1]["path"], format!("{beat}/media/narrato/beatName"));
        assert!(fix.says.starts_with("Make it part of "), "{}", fix.says);
    }

    /// A deck whose slides declare their intensity and tone in words the lexicons do not read
    /// is not measured on them, and says why, rather than reading clean.
    #[test]
    fn a_deck_in_another_language_is_not_read_for_intensity_or_tone() {
        fn japanese(v: &mut Value) {
            match v {
                Value::Object(map) => {
                    for (k, v) in map.iter_mut() {
                        match (k.as_str(), v.is_string()) {
                            ("text" | "claim" | "notes", true) => *v = json!("港は嵐で失われ、町は一年で取り戻した。"),
                            _ => japanese(v),
                        }
                    }
                }
                Value::Array(items) => items.iter_mut().for_each(japanese),
                _ => {}
            }
        }
        let mut deck = climate();
        for key in ["nodes", "states", "spine"] {
            japanese(&mut deck[key]);
        }
        let h = of(&deck);
        assert!(h.slides.iter().all(|s| s.read.is_none() && s.tone_read.is_none()), "{:#?}", h.slides);
        let says =
            "25 slides declare intensity, but health reads it only in English words, three or more, and none has them.";
        assert_eq!(h.intensity, Measure { score: None, says: says.into() });
        assert_eq!(h.tone.score, None, "{}", h.tone.says);
    }

    /// A deck in another language written in Latin letters is not read for intensity or tone
    /// either (dogfood DF7-4): German's "die" and "will" are English words too, so the language
    /// its brief names, else its own `lang`, decides, not its script. English decks read as before.
    #[test]
    fn a_deck_named_in_another_language_is_not_read_for_intensity_or_tone() {
        let english = of(&climate());
        assert!(english.intensity.score.is_some() && english.tone.score.is_some());
        let mut by_brief = climate();
        by_brief["meta"]["narrative"]["brief"]["language"] = json!("German");
        let mut by_lang = climate();
        by_lang["meta"]["lang"] = json!("de-DE");
        for deck in [by_brief, by_lang] {
            let h = of(&deck);
            assert!(h.slides.iter().all(|s| s.read.is_none() && s.tone_read.is_none()), "{:#?}", h.slides);
            // Named, it says the deck's language, not that its words are too few.
            let says = "25 slides declare intensity, but health reads it only in English, and this deck is in German.";
            assert_eq!(h.intensity, Measure { score: None, says: says.into() });
            assert_eq!(h.tone.score, None, "{}", h.tone.says);
            assert!(h.findings.iter().all(|f| f.kind != Kind::Intensity && f.kind != Kind::Tone), "{:?}", h.findings);
            // Its other measures, and its score, are as an English deck's.
            assert_eq!((h.score, &h.pacing, &h.fidelity), (english.score, &english.pacing, &english.fidelity));
        }
        // A deck that names English, in any of its forms, reads as one that names none.
        for named in ["English", "en", "en-GB"] {
            let mut deck = climate();
            deck["meta"]["narrative"]["brief"]["language"] = json!(named);
            let h = of(&deck);
            assert_eq!(
                (h.intensity.clone(), h.tone.clone(), h.findings.len()),
                (english.intensity.clone(), english.tone.clone(), english.findings.len())
            );
        }
    }

    #[test]
    fn a_deck_made_by_hand_has_its_health_read_too() {
        let text = std::fs::read_to_string(root().join("docs/examples/revenue.deck.json")).unwrap();
        let deck: Value = serde_json::from_str(&text).unwrap();
        let h = of(&deck);
        assert!(h.arc.is_none() && h.beats.is_empty());
        assert!(!h.slides.is_empty());
        // No length asked: paced against the 18 slides of a talk of no length.
        assert!(h.pacing.says.contains("about 18"), "{}", h.pacing.says);
        // Nothing declared, nothing to hold the words to, and no number for what was not measured.
        assert!(h.intensity.says.starts_with("No slide declares intensity"), "{}", h.intensity.says);
        assert!(h.tone.says.starts_with("No slide declares tone"), "{}", h.tone.says);
        assert_eq!((h.intensity.score, h.tone.score), (None, None));
        assert!(!h.findings.iter().any(|f| f.fix.is_some()), "{:#?}", h.findings);
    }

    #[test]
    fn a_slide_out_of_its_beats_order_has_moved() {
        let deck = |beats: &[usize]| -> Vec<SlideForHealth> {
            (beats.iter().enumerate())
                .map(|(i, b)| SlideForHealth {
                    id: format!("s{i}"),
                    narrative_meta: Some(NarrativeMeta { beat_index: Some(*b), ..NarrativeMeta::default() }),
                    ..SlideForHealth::default()
                })
                .collect()
        };
        assert!(moved(&deck(&[0, 0, 1, 2, 2, 3])).is_empty());
        // Written for the third beat, it stands among the first's.
        assert_eq!(moved(&deck(&[0, 2, 1, 1, 3])), [(1, 2, 0)]);
        // Written for the first, it stands among the second's.
        assert_eq!(moved(&deck(&[0, 1, 1, 0, 2])), [(3, 0, 1)]);
        // Moved to the front, it stands before the run's first.
        assert_eq!(moved(&deck(&[3, 0, 1, 2])), [(0, 3, 0)]);
        // A slide that declares no beat stands outside the order.
        let mut some = deck(&[0, 1, 2]);
        some[1].narrative_meta = None;
        assert!(moved(&some).is_empty());
    }

    /// The climate talk's figures, Vzestup's to the last bit (PLAN G.9) but where ADR-0032 departs
    /// from them: what a change to how health reads a deck must not move without saying so.
    #[test]
    fn a_generated_talks_figures_are_vzestups() {
        let h = of(&climate());
        let scores = [&h.pacing, &h.balance, &h.fidelity, &h.intensity, &h.tone].map(|m| m.score);
        // Balance leaves out the one-idea slides' points and counts a slide that draws a figure as
        // showing something (ADR-0032 items 1 and 3, Jay's call, 2026-10-25): 69 and 83 were Vzestup's.
        assert_eq!((h.score, scores), (84, [Some(100), Some(71), Some(83), Some(83), Some(82)]));
    }

    #[test]
    fn a_brief_asks_for_a_range_of_slides() {
        assert_eq!(asked("16-25 slides"), Some((16, Some(25))));
        assert_eq!(asked("5\u{2013}10 slides"), Some((5, Some(10))));
        assert_eq!(asked("41+ slides"), Some((41, None)));
        assert_eq!(asked("soon"), None);
        assert_eq!(range(41, None), "41 or more");
    }
}
