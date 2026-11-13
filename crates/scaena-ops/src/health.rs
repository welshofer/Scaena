//! A deck's narrative health (PLAN G.9, ADR-0026): Vzestup's measures of a talk, ported
//! (`scaena_narrato::health`), over the deck's slides as they stand: whether it is paced for the
//! length its brief asked, how many points each slide makes and how much its notes give the
//! speaker, how each slide follows from the one before, how the intensity each declares follows
//! the arc it was written to, whether its words carry the intensity and the tone it declares,
//! and which slides now stand in another beat of the arc than the one they were written for.
//!
//! A slide is read as its spine tells it: its headline the beat's claim, its words what its last
//! state shows (each item of a layout's set, or each item of a list, a point of its own), its
//! notes as the presenter reads them (each state's own, else its slide's earlier step's, else its
//! beat's), and what the narrative
//! declared of it the beat's `media.narrato`. Pure arithmetic over the deck: no layout, no call. What the words read is a
//! designed prior, not a calibrated model, so each finding asks a person to look, and a gap
//! between the words and the plan is the words' to close: the one fix offered is the beat a
//! moved slide now serves.

use crate::OpsError;
use scaena_core::document::{Beat, NodeType, Props};
use scaena_core::tracking::resolve_states;
use scaena_core::{Deck, Severity};
use scaena_narrato::health::constants::{
    COMPOSITE_FALLBACK_WEIGHT_ARC, COMPOSITE_FALLBACK_WEIGHT_BALANCE, COMPOSITE_FALLBACK_WEIGHT_PACING,
    DEVIANT_INTENSITY_DELTA, INTENSITY_GAP_THRESHOLD,
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
    /// Where slides lint finds an error on cap the score (ADR-0032 item 1): those slides, the cap,
    /// and the score the deck would have had, so the editors can say why it fell (W1's dogfood 5).
    /// None where nothing caps it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capped: Option<Capped>,
    /// Where the deck's slides state three or more figures, the ask an empty conversation offers
    /// to have the assistant find two that seem the same measure with different values: words
    /// cannot tell a measure said twice from a goal, a step, or a before and after, and a model
    /// reading the deck can (`docs/examples/health-calibration.md`, change 5). One ask, so the
    /// browser and the Mac say the same.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compare: Option<Ask>,
}

/// An ask the assistant is offered to begin with: what its button says, and the question.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Ask {
    pub label: String,
    pub question: String,
}

/// The figures that ask the assistant whether any disagree: three distinct ones at least.
const COMPARED: usize = 3;

/// The ask that has the assistant compare the deck's figures, quoting each it finds at odds and
/// changing nothing until the person says which is right.
fn comparing() -> Ask {
    Ask {
        label: "Check the figures agree".into(),
        question: "Do any of this deck's figures disagree? Read every figure the slides state, each with its slide. Name any two that seem to be the same measure with different values: quote both, and say which slide each is on. Change nothing until I say which is right.".into(),
    }
}

/// The score held down by the slides that do not stand.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Capped {
    /// The slides lint finds an error on, by number from 1.
    pub slides: Vec<usize>,
    /// The cap they set: the clean slides' share, less ten for each.
    pub cap: u32,
    /// The score the deck would have had without the cap, over it.
    pub score: u32,
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
    /// The beat of the arc it was written for, by its place in [`Health::beats`], where it names
    /// one the arc has: the band the graph draws it in (lift-up ATTR-4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<usize>,
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
    /// A made deck links an address whose host neither its brief nor its sources name: where it
    /// sends the room was the model's word alone (lift-up 20261025 SEC-4).
    Link,
    /// A change the model asked once it saw the slide drawn was not kept, lint having found the
    /// slide worse for it (`meta.narrative.refused`, lift-up USE-4).
    Look,
    /// A deck made from sources writes a figure on this slide that it names no source for: its
    /// beat cites none, its words name none, and no slide before it sourced the figure (W1's
    /// dogfood 4 and 5). The finding says the figure.
    Source,
    /// The deck's slides that list points list fewer than balance's floor on average, and this
    /// slide, among those that list fewest, holds the average down; the finding names them (R1's
    /// MCP dogfood).
    Thin,
    /// Its headline names a topic ("Q3 by product") where a content slide states its point: in a
    /// deck health reads in English, three to six words with no verb and no question
    /// (lift-up 10-25b FUNC-4). Information, or a warning where a line under it says the claim,
    /// which belongs in the headline, a topic of a word or two as well ("Some history"; W1's
    /// dogfood 3).
    Label,
}

/// What a finding asks a person to edit, where it is one field of the slide an editor opens for
/// it (W2's editor dogfood 6), so an editor need not read the ask's words for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Field {
    /// The slide's headline.
    Headline,
    /// The notes the presenter reads for it.
    Notes,
    /// The intensity its beat plans it at (`media.narrato.intensity`, the finding's `path`).
    Intensity,
    /// The claim the settled outline holds for the part of the talk it opens.
    Claim,
}

/// Something to look at on one slide: what it says, what to ask the assistant, and, where one
/// is safe, a fix.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Finding {
    /// The step it was read at, which an editor shows: the slide's first state for a finding about
    /// its notes or its plan, or that it does not stand, else its last, built whole.
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
    /// The figure a `source` finding found no source for, as the slide writes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub figure: Option<String>,
    /// Where in `deck.json` the plan an `arc` finding asks changed is written, a JSON pointer to
    /// its beat's `media.narrato.intensity`, for an agent that edits the deck by patches; the
    /// editors set it by the slide's Intensity (R1's MCP dogfood). No patch: what to plan is
    /// the person's (ADR-0032, item 4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The one field the finding asks to edit, where it asks for one: a slide's headline, its
    /// notes, its intensity, or its part's claim; none where it asks for the slide's words at
    /// large, or for a cut, a move, or a source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<Field>,
    /// The earlier slide a slide that says again what it said may be folded into, by its first
    /// state's id: what a `story` finding of a repeat offers beside the cut, Fold Into Slide N
    /// (`scaena_ops::states::folding`'s `into`, R1's #1161).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub into: Option<String>,
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

/// How many cards a slide built by hand sets side by side (R1's dogfood 6), the most of any one
/// set, where it sets three or more: each card a point, as a list's item is, though no bullet
/// marks it. A card is a container's text, or the stack, shape, or frame its texts stand in, its
/// label, figure, and words one card; three or more under one container, or three or more texts
/// in one role in it, are a set. `texts` are the slide's texts shown; `title` its headline, which
/// is none.
fn cards<'a>(deck: &'a Deck, texts: &[(&'a String, &'a Props)], title: &str) -> usize {
    fn parent(props: &Props) -> Option<&str> {
        props.get("at").and_then(|a| a.get("parent")).and_then(Value::as_str)
    }
    let of = |id: &str| deck.nodes.get(id).and_then(|n| n.props.get("at")).and_then(|a| a.get("parent"));
    // Each card by the container it stands in: (container, card).
    let mut held: Vec<(&str, &str)> = Vec::new();
    // Each text in one role straight in a container: (container, role, text).
    let mut roles: Vec<(&str, &str, &str)> = Vec::new();
    for &(id, props) in texts {
        let said = words(props);
        let aside = matches!(props.get("semantic").and_then(Value::as_str), Some("navigation" | "decoration"));
        if said.trim().is_empty() || said.trim() == title || aside {
            continue;
        }
        let Some(card) = parent(props) else { continue };
        let role = props.get("role").and_then(Value::as_str).unwrap_or("body");
        roles.push((card, role, id));
        if let Some(container) = of(card).and_then(Value::as_str)
            && !held.contains(&(container, card))
        {
            held.push((container, card));
        }
    }
    let mut most = 0;
    for &(container, _) in &held {
        most = most.max(held.iter().filter(|(c, _)| *c == container).count());
    }
    for &(container, role, _) in &roles {
        most = most.max(roles.iter().filter(|(c, r, _)| *c == container && *r == role).count());
    }
    if most >= 3 { most } else { 0 }
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

/// The slides of `deck` as health reads them, each by its first state's id; the most points any
/// one of its texts makes: its list's items, or its paragraphs (lift-up 10-24b FUNC-2), a grid of
/// short places, a SWOT or a timeline, making few in each; and the cards it sets side by side,
/// [`cards`]; and the words its code shows, which make no points a room reads (R1's agent
/// dogfood 11).
type Readings = (Vec<SlideForHealth>, Vec<usize>, Vec<usize>, Vec<usize>);

fn readings(deck: &Deck, slides: &[Slide]) -> Result<Readings, OpsError> {
    let snaps = resolve_states(deck).map_err(|e| OpsError::new(e.to_string()))?;
    let mut out = Vec::with_capacity(slides.len());
    let mut most = Vec::with_capacity(slides.len());
    let mut set = Vec::with_capacity(slides.len());
    let mut coded = Vec::with_capacity(slides.len());
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
        let mut code_words = 0;
        for &(_, props) in &texts {
            let said = words(props);
            let said = said.trim();
            let aside = matches!(props.get("semantic").and_then(Value::as_str), Some("navigation" | "decoration"))
                || props.get("role").and_then(Value::as_str) == Some("kicker");
            if said.is_empty() || said == title || aside {
                continue;
            }
            // Code's lines are no points: a block of ten lines is one thing shown, as a chart is.
            if props.get("code").is_some_and(Value::is_object) {
                code_words += said.split_whitespace().count();
            } else {
                points = points.max(scaena_core::lists::paragraphs(said).len());
            }
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
        // Its notes as the presenter reads them, each state's own, else its slide's earlier step's, else
        // its beat's (SPEC §2.6), so
        // what health judges is what the editor's Notes field shows (editor dogfood 3).
        let mut notes: Vec<&str> = Vec::new();
        let read = slide.states.iter().filter_map(|k| scaena_core::spine::notes(deck, &deck.states[*k].id));
        for note in read {
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
        set.push(out.last().map_or(0, |read: &SlideForHealth| cards(deck, &texts, &read.title)));
        most.push(points);
        coded.push(code_words);
    }
    Ok((out, most, set, coded))
}

/// A figure a slide writes: as written, its value with its marks between digits taken out
/// ("184,000" is 184000), and its unit, a currency's sign, a percent, a scale, or the word it
/// counts, singular ("$184,000" and "184,000 dollars" are one; "40-mile" and "40 miles" too).
#[derive(Debug, Clone, PartialEq)]
struct Figure {
    shown: String,
    value: String,
    unit: String,
}

/// The unit a word names, singular and in one spelling: "dollars" is `$`, "miles" `mile`.
fn unit_of(word: &str) -> String {
    let word = word.to_lowercase();
    match word.as_str() {
        "dollar" | "dollars" | "usd" => "$".into(),
        "euro" | "euros" | "eur" => "€".into(),
        "pound" | "pounds" | "gbp" => "£".into(),
        "percent" | "per cent" => "%".into(),
        // Thousands, millions, and billions of euros, as German and French accounts write them
        // ("240 TEUR" is "240 k\u{20ac}", R2's Nordlicht).
        "teur" | "teuro" | "t\u{20ac}" | "keur" | "k\u{20ac}" => "k\u{20ac}".into(),
        "meur" | "m\u{20ac}" => "m\u{20ac}".into(),
        "md\u{20ac}" | "mrd\u{20ac}" | "mds\u{20ac}" => "md\u{20ac}".into(),
        w if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") => w[..w.len() - 1].to_string(),
        w => w.to_string(),
    }
}

/// The figures `text` writes, in order: each a word with a digit that no letter begins ("38%",
/// "$1.4M", "2.5×", "40-mile"), not a year ("2026") and not a count under ten standing alone
/// ("Week 1", "3 steps"), so a name's digits ("Q3", "p99") and a slide's numbering are no figure.
/// Nor is a time of day, hours and minutes ("1:10", "11:40"), which a timetable states and no
/// source backs (the ferry deck's "a fourth sailing at 1:10"); a ratio written so ("1:20") is
/// missed with it.
/// A figure's unit is its sign, its scale, the word joined to it ("40-mile"), or the word after it.
/// A percent, a currency, or a magnitude set apart from it by a space, as German and French set
/// them ("40 %", "48,6 Mio. €"), is shown with it, read as the builder reads a figure
/// (`narrative::figures_in`), so a finding names the figure the slide writes (R1's agent
/// dogfood 3: "writes 40" for "40 %").
fn figures(text: &str) -> Vec<Figure> {
    let trimmed =
        text.split_whitespace().map(|w| w.trim_matches(|c: char| !(c.is_alphanumeric() || "$€£¥%+−-".contains(c))));
    // A word of a script set without spaces is a phrase, its figures inside it ("建設費：3億円"),
    // so its words are the figures the builder reads in it, a magnitude and a counter with each
    // ("3億円", "4,500万円"), a percent set right after one kept with it ("18%増"), and none a
    // date's or a time's counter follows ("2026年", "12月10日", "9時"), as a year and a clock
    // are none (health read of a Japanese deck).
    let words: Vec<String> = trimmed
        .flat_map(|w| -> Vec<String> {
            if !w.chars().any(unspaced) {
                return vec![w.to_string()];
            }
            let mut found = Vec::new();
            let mut rest = w;
            for f in crate::narrative::figures_in(w) {
                let Some(at) = rest.find(f.as_str()) else { continue };
                let after = &rest[at + f.len()..];
                rest = after;
                if after.starts_with(['年', '月', '日', '時', '分', '秒']) {
                    continue;
                }
                match after.starts_with('%') && !f.ends_with('%') {
                    true => found.push(format!("{f}%")),
                    false => found.push(f),
                }
            }
            found
        })
        .collect();
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    // Each figure as the builder reads it, a mark set apart after it with it ("48,6 Mio. €").
    let spelled = crate::narrative::figures_in(text);
    let mut out = Vec::new();
    for (k, w) in words.iter().enumerate() {
        let starts = w.chars().next().is_some_and(|c| !c.is_alphabetic());
        if !starts || !w.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }
        let year = w.len() == 4 && w.parse::<u32>().is_ok_and(|y| (1900..=2100).contains(&y));
        let count = w.parse::<u32>().is_ok_and(|n| n < 10);
        if year || count || clock(w) || dated(&words, k) {
            continue;
        }
        let (number, joined) = match w.split_once('-') {
            Some((n, rest)) if n.chars().any(|c| c.is_ascii_digit()) && rest.chars().all(char::is_alphabetic) => {
                (n, Some(rest))
            }
            _ => (*w, None),
        };
        let sign: String = number.chars().take_while(|c| "$€£¥".contains(*c)).collect();
        let digits: String =
            number.chars().skip(sign.chars().count()).take_while(|c| c.is_ascii_digit() || ".,".contains(*c)).collect();
        let tail: String = number.chars().skip(sign.chars().count() + digits.chars().count()).collect();
        let value = digits.replace(',', "").trim_end_matches('.').to_string();
        let after = words.get(k + 1).filter(|n| n.chars().all(char::is_alphabetic) && !n.is_empty());
        let spaced = (spelled.iter())
            .find(|s| s.strip_prefix(number).is_some_and(|rest| rest.starts_with([' ', '\u{a0}', '\u{202f}'])));
        // What the builder reads set apart after the figure: a percent, or a magnitude and its
        // currency as one word, thousands of euros one unit however written ("240 TEUR",
        // "240 k\u{20ac}"), as the builder and binding read them.
        let suffix = spaced
            .and_then(|s| s.strip_prefix(number))
            .map(|rest| rest.trim_start_matches([' ', '\u{a0}', '\u{202f}']));
        let percent = spaced.is_some_and(|s| s.ends_with(['%', '\u{2030}']));
        let compound = suffix.map(unit_of).filter(|u| ["k\u{20ac}", "m\u{20ac}", "md\u{20ac}"].contains(&u.as_str()));
        let unit = if !sign.is_empty() {
            unit_of(&sign)
        } else if !tail.is_empty() {
            unit_of(&tail)
        } else if let Some(word) = joined {
            unit_of(word)
        } else if percent {
            "%".into()
        } else if let Some(compound) = compound {
            compound
        } else {
            after.map(|a| unit_of(a)).unwrap_or_default()
        };
        let shown = match joined {
            Some(word) => {
                format!("{number} {}", if word.ends_with('s') { word.to_string() } else { format!("{word}s") })
            }
            None => spaced.cloned().unwrap_or_else(|| w.to_string()),
        };
        out.push(Figure { shown, value, unit });
    }
    out
}

/// The names of the months, and their short forms, as a date writes them in English, Spanish,
/// Portuguese, French, German, and Italian, in lower case.
const MONTHS: [&str; 75] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
    "jan",
    "feb",
    "mar",
    "apr",
    "jun",
    "jul",
    "aug",
    "sep",
    "sept",
    "oct",
    "nov",
    "dec",
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "setiembre",
    "octubre",
    "noviembre",
    "diciembre",
    "janeiro",
    "fevereiro",
    "março",
    "maio",
    "junho",
    "julho",
    "setembro",
    "outubro",
    "novembro",
    "dezembro",
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "décembre",
    "januar",
    "jänner",
    "februar",
    "märz",
    "juni",
    "juli",
    "oktober",
    "dezember",
    "gennaio",
    "febbraio",
    "aprile",
    "maggio",
    "giugno",
    "luglio",
    "settembre",
    "ottobre",
    "dicembre",
];

/// Whether `words[k]` is the day of the month in a date, as each language writes one (W2's health
/// read: Valdemar's "Votación: 15 de diciembre" read as the figure 15): a day from 1 to 31, an
/// ordinal's ending aside ("15th", "1er", "15."), before its month ("15 December", "15. Dezember",
/// "15 décembre", "15 dicembre") or its month after "de" or "del" ("15 de diciembre", "15 de
/// dezembro"), or after its month in English ("December 15"). A day a counter follows in Chinese or
/// Japanese ("12月15日") is passed over where the phrase is read. A count is none: "15 de los 30
/// barrios" names no month.
fn dated(words: &[&str], k: usize) -> bool {
    let word = words[k];
    let digits: String = word.chars().take_while(char::is_ascii_digit).collect();
    let rest = &word[digits.len()..];
    let ordinal = ["", ".", "st", "nd", "rd", "th", "er", "e", "º", "ª", "o"].contains(&rest.to_lowercase().as_str());
    if digits.is_empty() || digits.len() > 2 || !ordinal || !digits.parse::<u32>().is_ok_and(|d| (1..=31).contains(&d))
    {
        return false;
    }
    let month = |w: Option<&&str>| w.is_some_and(|w| MONTHS.contains(&w.trim_end_matches('.').to_lowercase().as_str()));
    let english =
        |w: Option<&&str>| w.is_some_and(|w| MONTHS[..24].contains(&w.trim_end_matches('.').to_lowercase().as_str()));
    let of = |w: Option<&&str>| w.is_some_and(|w| matches!(w.to_lowercase().as_str(), "de" | "del"));
    month(words.get(k + 1)) || (of(words.get(k + 1)) && month(words.get(k + 2))) || (k > 0 && english(words.get(k - 1)))
}

/// Whether `word` is a time of day: hours to 23, a colon, and two digits of minutes to 59.
fn clock(word: &str) -> bool {
    let Some((hours, minutes)) = word.split_once(':') else { return false };
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    digits(hours)
        && hours.len() <= 2
        && hours.parse::<u32>().is_ok_and(|h| h < 24)
        && minutes.len() == 2
        && digits(minutes)
        && minutes.parse::<u32>().is_ok_and(|m| m < 60)
}

/// Whether `text` names one of `sources` by its name's words, its extension aside, in their order
/// ("the season report" names `season-report.pdf`), as a panel reads a slide that says where a
/// figure is from.
fn names_source(text: &str, sources: &[Vec<String>]) -> bool {
    let words: Vec<String> = text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    sources.iter().filter(|name| !name.is_empty()).any(|name| words.windows(name.len()).any(|w| w == name.as_slice()))
}

/// A source's name as the words a slide would say it in: "season-report.pdf" is "season report".
fn source_words(name: &str) -> Vec<String> {
    let stem = match name.rsplit_once('.') {
        Some((stem, ext)) if (1..=5).contains(&ext.len()) && ext.chars().all(char::is_alphanumeric) => stem,
        _ => name,
    };
    stem.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// A slide of a deck as its talk speaks it (lift-up 10-25 ATTR-2): its headline, the words it
/// shows, and its notes, as health reads them; and the beat that tells it, by its place in the
/// spine (section, beat), where one does.
pub(crate) struct Told<'d> {
    pub slide: SlideForHealth,
    pub beat: Option<(usize, usize, &'d Beat)>,
}

/// Each slide of `deck` as its talk speaks it.
pub(crate) fn told(deck: &Deck) -> Result<Vec<Told<'_>>, OpsError> {
    let slides = slides(deck);
    let (read, ..) = readings(deck, &slides)?;
    let told = slides.iter().filter(|s| !s.states.is_empty()).map(|s| s.beat);
    Ok(read.into_iter().zip(told).map(|(slide, beat)| Told { slide, beat }).collect())
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

/// A plan `asked` for a slide whose words read `words`, to two places, rounded toward the words:
/// a plan held to the most its words carry stays within it once written (agent dogfood 8: 0.7776
/// written as 0.78 read 0.2524 from its words, past the gap, and the fix's plan was found flat).
fn held(asked: f64, words: Option<f64>) -> f64 {
    let hundredths = asked * 100.0;
    // A plan already on a hundredth but for the float's last bits (0.54 − 0.25 is 29.000000000000004
    // hundredths) is that hundredth, not the next one over (W2's review of #1186).
    if (hundredths - hundredths.round()).abs() < 1e-6 {
        return hundredths.round() / 100.0;
    }
    let rounded = match words {
        Some(w) if asked > w => hundredths.floor(),
        Some(w) if asked < w => hundredths.ceil(),
        _ => hundredths.round(),
    };
    rounded / 100.0
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

/// A verb that agrees with `n` of something: "1 slide bends", "2 slides bend" (dogfood pass 9).
fn agree<'a>(n: usize, one: &'a str, more: &'a str) -> &'a str {
    if n == 1 { one } else { more }
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
/// aloud, whichever comes to most. A `duration` that is still the estimate a made deck wrote from
/// its words (`media.narrato.estimated`) counts, but times no beat: the talk runs at least so long.
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
/// index of the first slide it tells; and whether any beat's time was given, by a `duration` that
/// is not the estimate a made deck wrote or by its holds, so a talk timed by its words alone reads
/// as a floor (the calibration against a reader). None where a beat has neither a `duration` nor
/// a hold on every state, or a slide neither a beat nor holds, as a deck written by hand with no
/// spine.
fn talk(deck: &Deck, slides: &[Slide]) -> Option<(f64, Vec<Beaten>, bool)> {
    let state = |id: &str| deck.states.iter().find(|s| s.id == id);
    let mut told: Vec<((usize, usize), usize, f64, Timed)> = Vec::new();
    let mut seconds = 0.0;
    let mut timed = false;
    for (k, slide) in slides.iter().enumerate() {
        match slide.beat {
            Some((i, j, _)) if told.iter().any(|(at, ..)| *at == (i, j)) => {}
            Some((i, j, beat)) => {
                let holds: Vec<Option<f64>> = beat.states.iter().map(|s| state(s).and_then(|s| s.hold)).collect();
                let held = holds.iter().flatten().sum::<f64>() / 1000.0;
                let estimated = (beat.media.as_ref())
                    .and_then(|m| m.pointer("/narrato/estimated"))
                    .and_then(Value::as_f64)
                    .zip(beat.duration)
                    .is_some_and(|(e, d)| e == d);
                let given = match beat.duration {
                    Some(d) => d,
                    None if holds.iter().all(Option::is_some) => 0.0,
                    None => return None,
                };
                timed |= (beat.duration.is_some() && !estimated) || (!holds.is_empty() && held > 0.0);
                // Its notes as the presenter reads them, each once: each state's own, else its
                // slide's earlier step's, else the beat's (SPEC §2.6).
                let mut read: Vec<&str> = Vec::new();
                for note in beat.states.iter().filter_map(|s| scaena_core::spine::notes(deck, s)) {
                    if !read.contains(&note) {
                        read.push(note);
                    }
                }
                let noted = read.into_iter().map(spoken).sum::<f64>();
                let (own, by) = [(held, Timed::Held), (noted, Timed::Noted)]
                    .into_iter()
                    .fold((given, Timed::Given), |most, (t, by)| if t > most.0 { (t, by) } else { most });
                told.push(((i, j), k, own, by));
                seconds += own;
            }
            None => {
                let holds: Option<Vec<f64>> = slide.states.iter().map(|&s| deck.states[s].hold).collect();
                seconds += holds?.iter().sum::<f64>() / 1000.0;
                timed = true;
            }
        }
    }
    (!told.is_empty()).then(|| (seconds, told.into_iter().map(|(_, k, s, by)| (k, s, by)).collect(), timed))
}

/// How long the talk runs, in seconds, as [`talk`] times it: none where it cannot be timed.
pub(crate) fn talk_seconds(deck: &Deck) -> Option<f64> {
    talk(deck, &slides(deck)).map(|(seconds, ..)| seconds)
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

/// Two connectives a speaker opens a slide's notes with in `language`, which carry nothing across
/// alone: German's "Also" and "Aber", French's "Donc" and "Mais", Spanish's "Entonces" and "Pero"
/// (health read of a Spanish deck), Portuguese's "Então" and "Mas" and Italian's "Quindi" and "Ma"
/// (W1's dogfood 11), else English's "So" and "But" (health's English-only sweep, item 2).
fn connectives(language: Option<&str>) -> (&'static str, &'static str) {
    let tag = language.map(|l| l.trim().to_lowercase()).unwrap_or_default();
    match tag.split(['-', '_', ' ']).next().unwrap_or_default() {
        "de" | "deu" | "ger" | "german" | "deutsch" => ("Also", "Aber"),
        "fr" | "fra" | "fre" | "french" | "français" | "francais" => ("Donc", "Mais"),
        "es" | "spa" | "spanish" | "español" | "espanol" => ("Entonces", "Pero"),
        "pt" | "por" | "portuguese" | "português" | "portugues" => ("Então", "Mas"),
        "it" | "ita" | "italian" | "italiano" => ("Quindi", "Ma"),
        _ => ("So", "But"),
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

/// The layouts that frame a talk rather than make one of its points: covers, dividers, and agendas.
pub(crate) const FRAMING: [&str; 10] = [
    "title",
    "photo-title",
    "title-center",
    "title-split",
    "paper-title",
    "section",
    "section-photo",
    "section-number",
    "agenda",
    "agenda-photo",
];

/// The irregular pasts a headline states its point in that [`VERBS`] does not spell out, a short
/// closed list (the calibration against a reader): "We rebuilt 52 miles, not 40" states a point,
/// and read without its verb it was found a topic. Each is a past no noun shares.
const PASTS: [&str; 19] = [
    "caught", "fought", "lent", "outran", "overcame", "quit", "rebuilt", "redrew", "regrew", "rewrote", "shot", "shut",
    "sped", "stole", "swept", "threw", "tore", "undercut", "upheld",
];

/// Whether `w` is one of [`VERBS`] or [`PASTS`].
fn spelled(w: &str) -> bool {
    VERBS.split_ascii_whitespace().any(|v| v == w) || PASTS.contains(&w)
}

/// The verbs a headline that states its point carries, each form spelled out, base, -s, past,
/// and -ing, with the contractions that hold one (lift-up 10-25b FUNC-4), apart by spaces, as
/// [`labels`] reads it. A word here that is also a noun ("plan", "report") keeps a headline
/// from being found, which errs toward saying nothing.
const VERBS: &str = "\
    add added adding adds allow allowed allowing allows am appear appeared appearing appears are aren't ask asked \
    asking asks be beat beaten beating beats became become becomes becoming been began begin beginning \
    begins begun being believe believed believes believing boost boosted boosting boosts bought break breaking \
    breaks bring bringing brings broke broken brought build building builds built buy buying buys call called \
    calling calls came can can't cannot change changed changes changing check checked checking checks choose \
    chooses choosing chose chosen climb climbed climbing climbs close closed closes closing come comes coming \
    compare compared compares comparing continue continued continues continuing cost costing costs could couldn't \
    count counted counting counts create created creates creating cross crossed crosses crossing cut cuts cutting \
    decide decided decides deciding decline declined declines declining deliver delivered delivering delivers \
    depend depended depending depends did didn't dip dipped dipping dips do does doesn't doing don't done double \
    doubled doubles doubling drag dragged dragging drags draw drawing drawn draws drew drive driven drives \
    driving drop dropped dropping drops drove earn earned earning earns end ended ending ends exceed exceeded \
    exceeding exceeds expect expected expecting expects explain explained explaining explains face faced faces \
    facing fail failed failing fails fall fallen falling falls feel feeling feels fell felt find finding finds \
    fix fixed fixes fixing flew flies flown fly flying follow followed following follows forget forgets \
    forgetting forgot forgotten found gain gained gaining gains gave get gets getting give given gives giving go \
    goes going gone got gotten grew grow growing grown grows had hadn't halve halved halves halving happen \
    happened happening happens has hasn't have haven't having held help helped helping helps here's hid hidden \
    hide hides hiding hit hits hitting hold holding holds i'm improve improved improves improving include \
    included includes including increase increased increases increasing is isn't it's jump jumped jumping jumps \
    keep keeping keeps kept knew know knowing known knows lag lagged lagging lags last lasted lasting lasts \
    launch launched launches launching lead leading leads learn learned learning learns leave leaves \
    leaving led left let let's lets letting lift lifted lifting lifts like liked likes liking live lived lives \
    living look looked looking looks lose loses losing lost love loved loves loving lower lowered lowering \
    lowers made make makes making matter mattered mattering matters may mean meaning means meant meet meeting \
    meets met might miss missed misses missing move moved moves moving must mustn't need needed needing needs \
    offer offered offering offers open opened opening opens outgrew outgrow outgrowing outgrown \
    outgrows outpace outpaced outpaces outpacing outperform outperformed outperforming outperforms outsell \
    outselling outsells outsold overtake overtaken overtakes overtaking overtook own owned owning owns paid \
    pass passed passes passing pay paying pays peak peaked peaking peaks plan planned planning plans play played \
    playing plays plunge plunged plunges plunging point pointed pointing points prove proved proves proving pull \
    pulled pulling pulls push pushed pushes pushing put puts putting raise raised raises raising ran reach \
    reached reaches reaching rebound rebounded rebounding rebounds recover recovered recovering recovers reduce \
    reduced reduces reducing remain remained remaining remains remember remembered remembering remembers replace \
    replaced replaces replacing report reported reporting reports require required requires requiring return \
    returned returning returns rise risen rises rising rose run running runs said sank sat save saved saves \
    saving saw say saying says see seeing seem seemed seeming seems seen sees sell selling sells send sending \
    sends sent serve served serves serving set sets setting shall shift shifted shifting shifts ship shipped \
    shipping ships should shouldn't show showed showing shown shows shrank shrink shrinking shrinks \
    shrunk sink sinking sinks sit sits sitting slid slide slides sliding slip slipped slipping slips slow \
    slowed slowing slows slump slumped slumping slumps soar soared soaring soars sold solve solved solves solving \
    speak speaking speaks spend spending spends spent spike spiked spikes spiking spoke spoken spread spreading \
    spreads stall stalled stalling stalls stand standing stands start started starting starts stay stayed staying \
    stays stick sticking sticks stood stop stopped stopping stops strike strikes striking struck stuck suggest \
    suggested suggesting suggests sunk support supported supporting supports surge surged surges surging take \
    taken takes taking talk talked talking talks taught teach teaches teaching tell telling tells tend tended \
    tending tends that's there's they'd they'll they're they've think thinking thinks thought told took top \
    topped topping tops trail trailed trailing trails tried tries triple tripled triples tripling trust trusted \
    trusting trusts try trying turn turned turning turns understand understanding understands understood use used \
    uses using wait waited waiting waits want wanted wanting wants was wasn't watch watched watches watching we'd \
    we'll we're we've went were weren't what's will win winning wins won won't work worked working works worried \
    worries worry worrying would wouldn't write writes writing written wrote you'll you're you've";

/// Verbs an ask opens on, as an imperative ("Vote Tuesday to fund the pilot"), or that follow
/// "to" as an infinitive: read as a verb where [`VERBS`] does not list them (W2's outlined
/// calibration). A noun that is also one ("Pilot results") opens two words and is still a topic.
const IMPERATIVES: &str = "act adopt agree allocate approve ask authorize back book build buy call cancel choose \
    close commit cut decide delay deliver double drop enroll expand extend fix fund give hire invest join keep \
    launch lead let partner pass pause pick pilot pledge prioritize protect raise reallocate reject renew \
    reserve restore schedule secure sign sponsor stop switch test try vote";

/// Whether `headline` names a topic rather than stating a point (lift-up 10-25b FUNC-4): one to
/// six words, no question, none of [`VERBS`], and no word of five letters or more in "-ed".
/// "Q3 by product", "Results", and "The Ask" name one; "Revenue grew every quarter" states one
/// (W2's outlined calibration: a word or two was a topic a reader counted, and missed).
/// A clause opened by how, what, why, where, when, or who, with no question mark, names a topic
/// though a verb is in it ("How It Works", "What families tell us"). An imperative of three words
/// or more ("Vote Tuesday to fund the pilot"), or an infinitive after "to", states a point.
fn labels(headline: &str) -> bool {
    let words: Vec<String> = (headline.split_whitespace())
        .map(|w| w.replace('\u{2019}', "'").trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let asks = |w: &str| IMPERATIVES.split_ascii_whitespace().any(|v| v == w);
    let bidden = words.len() >= 3 && words.first().is_some_and(|w| asks(w));
    let infinitive = words.windows(2).any(|p| p[0] == "to" && asks(&p[1]));
    if headline.trim().ends_with('?') || bidden || infinitive {
        return false;
    }
    let clause = (2..=6).contains(&words.len())
        && words.first().is_some_and(|w| matches!(w.as_str(), "how" | "what" | "why" | "where" | "when" | "who"));
    // A word a determiner opens is a noun, a verb's form or not ("The Ask", "Our plan works" still
    // stating its point on "works").
    let determined = |w: &str| {
        matches!(w, "the" | "a" | "an" | "our" | "your" | "their" | "its" | "this" | "these" | "those" | "my")
    };
    let nouns: Vec<&str> = (headline.split_whitespace().enumerate())
        .filter(|&(i, _)| i == 0 || !words.get(i - 1).is_some_and(|w| determined(w)))
        .map(|(_, w)| w)
        .collect();
    clause || (names_topic(&nouns.join(" "), 1) && (1..=6).contains(&words.len()))
}

/// Whether `headline`, of `fewest` words to six, names a topic rather than stating a point, as
/// [`labels`] reads one: a word or two ("Measurement") counts where the look rewrites a title
/// that stated its point (W1's dogfood 8).
pub(crate) fn names_topic(headline: &str, fewest: usize) -> bool {
    let headline = headline.trim();
    if headline.ends_with('?') {
        return false;
    }
    let words: Vec<String> = (headline.split_whitespace())
        .map(|w| w.replace('\u{2019}', "'").trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let of = |w: &str| VERBS.split_ascii_whitespace().any(|v| v == w);
    // A word of five letters or more in "-ed" is a verb's past or participle, as a point is stated
    // in ("Margins expanded"); "-ing" is not, a gerund naming many a topic ("Pricing by region").
    // A verb's "-s" within two words after "and" or "or" is a plural, the end of a list of
    // things ("Weekend hours and online holds", W1's dogfood 6): a predicate there stands on a
    // verb before it, which is found.
    let listed = |i: usize| words[i.saturating_sub(2)..i].iter().any(|w| matches!(w.as_str(), "and" | "or"));
    let verb = |(i, w): (usize, &String)| {
        let plural = w.len() > 3 && w.ends_with('s') && of(&w[..w.len() - 1]) && listed(i);
        (w.chars().count() >= 5 && w.ends_with("ed")) || PASTS.contains(&w.as_str()) || (of(w) && !plural)
    };
    (fewest..=6).contains(&words.len()) && !words.iter().enumerate().any(verb)
}

/// The shipped themes' layouts that show a measure: a figure's, or a chart's (the calibration
/// against a reader). A run of them in one part of the talk is one move, each slide another figure
/// under the same frame, though their words share none.
const MEASURES: [&str; 9] = [
    "metric-trend",
    "bar-chart",
    "line-chart",
    "price-chart",
    "donut-chart",
    "chart-bullets",
    "two-charts",
    "pictograph",
    "funnel",
];

/// The shipped themes' layouts whose headline is a figure's, or names the figures under it.
const FIGURES: [&str; 11] = [
    "stat",
    "big-number",
    "two-numbers",
    "numbers",
    "number-grid",
    "four-numbers",
    "scorecard",
    "number-photo",
    "kpi-dashboard",
    "figure-numbers",
    "status-report",
];

/// The shipped themes' layouts whose headline names what they hold, a term and its meaning, the
/// voices quoted, the questions asked, the exercise set: a topic by design.
const NAMED: [&str; 7] = ["definition", "glossary", "testimonials", "faq", "logo-wall", "buyer-persona", "quiz"];

/// Whether `word`, small and trimmed of its marks, reads as a verb, as [`labels`] reads one: one
/// of [`VERBS`], a contraction of one ("isn't", "it's"), or a past form of five letters or more
/// in "-ed".
fn verb(word: &str) -> bool {
    let w = word.replace('\u{2019}', "'");
    let w = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_lowercase();
    let contracted = ["n't", "'re", "'ve", "'ll", "'m", "'d", "'s"].iter().any(|c| w.ends_with(c));
    contracted || (w.chars().count() >= 5 && w.ends_with("ed")) || spelled(&w)
}

/// The line under a slide's headline that says its claim, where the headline names a topic
/// (W1's dogfood 3): a headline [`labels`] reads as a topic, or a shorter one of a word or two
/// with no verb, no figure, and no question ("Some history"); and under it a sentence of five
/// words or more with a verb, not a question, a voice quoted, or a card's parts health joins with
/// a dash. A figure's slide, and one whose headline names what it holds by design, has none.
fn buried(deck: &Deck, slide: &Slide, s: &SlideForHealth, title: &str) -> Option<(String, bool)> {
    let layout = slide.states.iter().find_map(|&k| deck.states[k].layout.as_deref()).unwrap_or_default();
    if FIGURES.contains(&layout) || NAMED.contains(&layout) || layout.contains("quote") || layout.starts_with("team") {
        return None;
    }
    let title = title.trim();
    let words: Vec<&str> = title.split_whitespace().collect();
    let short = (1..=2).contains(&words.len())
        && !title.contains('?')
        && !title.chars().any(|c| c.is_ascii_digit())
        && !words.iter().any(|w| verb(w));
    if !short && !labels(title) {
        return None;
    }
    let claims = |line: &str| {
        let words: Vec<&str> = line.split_whitespace().collect();
        let quoted = line.starts_with(['"', '\u{201c}', '\u{2018}', '\'', '\u{ab}', '\u{201e}']);
        // A card's parts, its label then a colon ("People: who was paged, and when"), as one a
        // dash joins.
        let carded = line.find(':').is_some_and(|at| line[..at].split_whitespace().count() <= 3);
        !carded
            && !line.contains(" \u{2014} ")
            && words.len() >= 5
            && !line.contains('?')
            && !quoted
            && words.iter().any(|w| verb(w))
    };
    // Where the beat keeps a claim, the heading shown is in the body; it is not under itself. A
    // line a list sets is one of its items, which the headline it becomes is to leave (W1's
    // dogfood 17: said again, it stood twice).
    (s.body.lines())
        .map(|l| (l.trim().trim_start_matches('\u{2022}').trim(), l.trim().starts_with('\u{2022}')))
        .find(|(l, _)| *l != title && claims(l))
        .map(|(l, listed)| (l.to_string(), listed))
}

/// The layouts a talk closes in (ADR-0032 item 6).
pub(crate) const CLOSING: [&str; 4] = ["closing", "call-to-action", "takeaways", "poster"];

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

/// The plan layouts a talk closes in, as Narrato names them (`media.narrato.layout`): a slide
/// planned in one closes the talk whatever theme layout its items were set in.
const CLOSING_PLANS: [&str; 9] =
    ["closing", "call to action", "next steps", "q&a", "q & a", "qa", "questions", "takeaways", "poster"];

/// Whether the slide's beat planned it in a layout that closes the talk ([`CLOSING_PLANS`]):
/// health read of a Spanish deck, whose Closing of two items was set as bullets.
fn planned_close(slide: &Slide) -> bool {
    let Some((_, _, beat)) = slide.beat else { return false };
    let planned = beat.media.as_ref().and_then(|m| m.pointer("/narrato/layout")).and_then(Value::as_str);
    // Written as Narrato names them ("Next Steps") or as a theme's layouts are ("next-steps").
    planned.is_some_and(|l| CLOSING_PLANS.contains(&l.trim().to_lowercase().replace('-', " ").as_str()))
}

/// The first states of the deck's closing slides: the last run of slides in a closing layout,
/// next steps, or questions, or planned in one ([`planned_close`]), or headed with thanks or
/// questions alone (ADR-0032 item 6).
fn closing(deck: &Deck, slides: &[Slide], read: &[SlideForHealth]) -> Vec<String> {
    let closes = |i: usize| {
        if planned_close(&slides[i]) {
            return true;
        }
        let layout = slides[i].states.iter().find_map(|&k| deck.states[k].layout.as_deref());
        let title = read[i].title.trim().to_lowercase();
        // Next steps and the floor opened belong to a close too, before or after its last word
        // (dogfood pass 9). A headline closes only where it thanks the room or opens the floor in
        // three words or fewer: an FAQ headed "Questions every support lead asks us first"
        // answers them in the talk (the calibration against a reader).
        layout.is_some_and(|l| CLOSING.contains(&l) || matches!(l, "next-steps" | "qa"))
            || (THANKS.iter().any(|t| title.starts_with(t)) && title.split_whitespace().count() <= 3)
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

/// The words coherence reads a slide by, `scaena_narrato::health::coherence::keywords`, split at
/// what is not a letter or a digit in any script rather than at what is not ASCII: the same words
/// in English, and "kündigung", "größer" whole in German (W1's dogfood 7).
fn written(text: &str) -> std::collections::BTreeSet<String> {
    let lower = text.to_lowercase();
    scaena_core::sort::set(
        lower.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() > 3).map(str::to_string),
    )
}

/// Whether two neighbouring slides share no word as coherence reads them, each having some:
/// where either says nothing past a short headline, the pair reads as following, as Vzestup's
/// overlap gives it ½.
fn apart(a: &SlideForHealth, b: &SlideForHealth) -> bool {
    let words = |s: &SlideForHealth| singular(written(&format!("{} {}", s.title, s.body)));
    let (wa, wb) = (words(a), words(b));
    !wa.is_empty() && !wb.is_empty() && wa.is_disjoint(&wb)
}

/// `words` with a plural's "-s" taken off a word of five letters or more that does not end in
/// "-ss", so a word and its plural are one word where a pair is read for a turn: the library
/// keynote's "twelve laundromats" and "Laundromat model" (its health read against a reader).
/// Coherence's figure reads the words as written.
fn singular(words: std::collections::BTreeSet<String>) -> std::collections::BTreeSet<String> {
    scaena_core::sort::set(words.into_iter().map(|w| match w.strip_suffix('s') {
        Some(stem) if stem.chars().count() >= 4 && !stem.ends_with('s') => stem.to_string(),
        _ => w,
    }))
}

/// Whether `next`'s notes open by carrying `before` across (narrative dogfood H4): a word of
/// `before`'s headline and body that `next`'s do not use, as coherence reads words, in the
/// opening of `next`'s notes. A word both slides use, a product's name, carries nothing over;
/// so does its plural.
fn bridges(before: &SlideForHealth, next: &SlideForHealth) -> bool {
    let words = |s: &SlideForHealth| singular(written(&format!("{} {}", s.title, s.body)));
    let carried: Vec<String> = words(before).difference(&words(next)).cloned().collect();
    let opened = singular(written(opening(&next.notes)));
    carried.iter().any(|w| opened.contains(w))
}

/// The host an address goes to, as a person would name it: an email's domain, or a web address's
/// host without `www.`, in lower case.
fn host(href: &str) -> Option<String> {
    let rest = match href.strip_prefix("mailto:") {
        Some(email) => email.rsplit_once('@')?.1,
        None => href.strip_prefix("https://").or_else(|| href.strip_prefix("http://"))?,
    };
    let host = rest.split(['/', '?', '#', ':']).next()?.to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    (!host.is_empty()).then_some(host)
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
    let (read, most, set, coded) = readings(deck, &slides)?;
    let n = read.len();
    let m = health::metrics(&read, None);
    // What heads each slide as the reader hears it (SPEC §3.12): its heading's words, a figure's
    // before its meaning, else its claim where no text heads it. A slide's `title` is its beat's
    // claim where the beat keeps one, which the headline shown may no longer say, so a check of
    // the headline's words reads this.
    let snaps = resolve_states(deck).map_err(|e| OpsError::new(e.to_string()))?;
    // A big number's slide is headed by its figure before its meaning, the figure its point, so
    // it names no topic however few its words.
    let mut shown_heads = Vec::with_capacity(read.len());
    let (heading, led): (Vec<String>, Vec<bool>) = (read.iter().enumerate())
        .map(|(i, s)| {
            let last = slides.get(i).and_then(|sl| sl.states.last()).map(|&l| &snaps[l]);
            let shown = last.and_then(|snap| scaena_core::reading::headline(deck, snap));
            let plain = last.and_then(|snap| {
                (snap.nodes.iter())
                    .filter(|(id, _)| deck.nodes.get(id.as_str()).is_some_and(|n| n.node_type == NodeType::Text))
                    .find(|(_, p)| {
                        p.get("semantic").and_then(Value::as_str) == Some("claim")
                            || matches!(p.get("role").and_then(Value::as_str), Some("display" | "headline" | "title"))
                    })
                    .map(|(_, p)| words(p).split_whitespace().collect::<Vec<_>>().join(" "))
            });
            let led = shown.is_some() && shown != plain;
            let heard = shown.unwrap_or_else(|| s.title.trim().to_string());
            // The headline as the slide shows it, a figure beside it not joined to it, which a
            // finding quotes (W1's dogfood 17: "25.8: Canopy cover, 2016–2025").
            shown_heads.push(plain.filter(|p| !p.is_empty()).unwrap_or_else(|| heard.clone()));
            (heard, led)
        })
        .unzip();
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
    // A beat's whole name, as the data keeps it, and as a sentence says it.
    let beat_name = |k: usize| beats.get(k).cloned().unwrap_or_else(|| format!("beat {}", k + 1));
    let beat_said = |k: usize| headed(&beat_name(k)).to_string();
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
    // A slide that frames the talk, a cover, a divider, an agenda, or its close, makes no point of
    // its own to state.
    let framing = |i: usize| {
        let layout = slides[i].states.iter().find_map(|&k| deck.states[k].layout.as_deref());
        layout.is_some_and(|l| FRAMING.contains(&l) || CLOSING.contains(&l)) || closing.contains(&read[i].id)
    };
    let bends: Vec<_> = bends.into_iter().filter(|d| !(d.delta < 0.0 && closing.contains(&d.slide_id))).collect();

    // Pacing against the brief's range, where it asked one, and the overall again with it.
    let mut pacing = pacing(n, asked, &m);
    // The time its beats take, beside the count (lift-up FUNC-1): Vzestup's score is the count's.
    let talk = talk(deck, &slides);
    // The time the talk is given: the minutes its brief asks, where it asks them (R1's MCP
    // dogfood: a 2-minute talk read as filling a slot of up to 15), within a fifth short of them
    // and a tenth past; else the slot its slides' range is given.
    let asked_minutes = (narrative.and_then(|n| n.pointer("/brief/minutes")).and_then(Value::as_f64))
        .filter(|m| m.is_finite() && *m >= 1.0)
        .map(|m| m.round() as u32);
    // Each with how the line beside the count says it.
    // The time given as the finding says it, as its ask says it, its bounds, and as the line
    // beside the count says it.
    // Bounds in minutes, and whether the talk's minutes are rounded before they are held to them,
    // as the slides' whole-minute slot was.
    type Given = (String, String, (f64, Option<f64>), String, bool);
    let given: Option<Given> = match (asked_minutes, asked) {
        (Some(m), _) => {
            let asks = counted(m as usize, "minute");
            let named = format!("the {asks} its brief asks");
            let m = f64::from(m);
            Some((
                named.clone(),
                named,
                (m * 4.0 / 5.0, Some(m * 11.0 / 10.0)),
                format!("where its brief asks {asks}"),
                false,
            ))
        }
        (None, Some((lo, hi))) => {
            let bounds = scaena_narrato::brief::minutes_for_slides(lo);
            let (count, named) = (range(lo, hi), slot(bounds));
            Some((
                format!("{named} for {count} slides"),
                named.clone(),
                (f64::from(bounds.0), bounds.1.map(f64::from)),
                format!("where {count} slides is {named}"),
                true,
            ))
        }
        (None, None) => None,
    };
    if let Some((seconds, _, timed)) = &talk {
        let against = given.as_ref().map(|(_, _, _, against, _)| format!(", {against}")).unwrap_or_default();
        pacing.says = match timed {
            true => format!("{} {} of talk{against}.", pacing.says, capitalized(&minutes(*seconds))),
            // Timed by its words alone, the talk runs at least that long: no beat says how long.
            // Where every slide has notes, how long they run as written is said beside it, as
            // information (R1's agent dogfood 2): notes may be a script or a presenter's cues, so
            // they find the talk neither short nor long.
            false => {
                let shown: Vec<&SlideForHealth> = read.iter().filter(|s| s.eligible()).collect();
                let noted = !shown.is_empty() && shown.iter().all(|s| !s.notes.trim().is_empty());
                let notes = shown.iter().map(|s| spoken(&s.notes)).sum::<f64>();
                format!(
                    "{} At least {} of talk from its words{against}; no beat is timed.{}",
                    pacing.says,
                    counted(((seconds / 60.0).round() as usize).max(1), "minute"),
                    if noted { format!(" Its notes as written run {}.", minutes(notes)) } else { String::new() }
                )
            }
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
    // The slides balance counts, those not made to make one point (ADR-0032 item 3): where none
    // lists a point, "0 points each" says nothing of the deck, so say which they were (W1's
    // dogfood 5).
    let pointed: Vec<usize> = (0..read.len()).filter(|&i| read[i].eligible() && !read[i].one_idea).collect();
    if pointed.iter().all(|&i| bullets(&read[i]) == 0) {
        short.push(match pointed.is_empty() {
            true => "every slide makes one point by design, so none lists points to count".to_string(),
            false => {
                let (named, _) = slides_named(&pointed);
                format!("no slide balance counts makes a list of points: {named}. The rest make one point by design")
            }
        });
    } else {
        // Over the slides it counts, which it says where it counts fewer than the deck shows: a
        // cut of two points over five slides moves the average by 0.4, as W1's dogfood 7 found.
        let shown = read.iter().filter(|s| s.eligible()).count();
        let over = match pointed.len() < shown {
            true => format!(", over {} of the {shown} slides, those made to list points", pointed.len()),
            false => String::new(),
        };
        let average = balance.avg_bullets_per_slide;
        let points = points(average);
        if average < 2.0 {
            short.push(format!("slides make {average} {points} each{over}, where two to five read as full"));
        }
        if average > 5.0 {
            short.push(format!("slides make {average} {points} each{over}, more than a room takes in"));
        }
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
    // Where the talk turns, each pair of neighbouring slides by its own words alone (W1's dogfood
    // 5 and 6): Vzestup reads a turn against the deck's mean overlap and its spread, so an edit
    // to the close moved them and a turn appeared at slide 3. Read by words, a pair turns where
    // its slides share no word the overlap counts; the cover's into the first slide is the talk
    // opening, no turn. A turn at a beat's edge is soft. Embeddings, where every slide has one,
    // read as Vzestup reads them. Coherence's figure is Vzestup's either way.
    let turns: Vec<(String, bool)> = match m.coherence.method {
        health::coherence::Method::Embedding => (m.coherence.breaks.iter())
            .map(|b| (b.slide_id.clone(), b.severity == health::coherence::Severity::Soft))
            .collect(),
        // A deck whose words no space sets apart is not read for its turns, as it is not for its
        // claims (health read of a Japanese deck): each headline reads as one word, so every pair
        // of slides shared none and turned, and the ask named a whole headline as the word to carry.
        health::coherence::Method::Keyword if spaceless(language) => Vec::new(),
        health::coherence::Method::Keyword => {
            let shown: Vec<usize> = (0..read.len()).filter(|&i| read[i].eligible()).collect();
            // Each pair by its own words, read in any script (W1's dogfood 7): Vzestup's split at
            // anything not ASCII reads "Kündigung" as "ndigung" and loses "für" and "größer", so a
            // German pair turned on words it never wrote. Coherence's figure stays Vzestup's.
            (shown.windows(2))
                .filter(|pair| pair[0] > 0 && apart(&read[pair[0]], &read[pair[1]]))
                .map(|pair| {
                    let beat = |i: usize| read[i].narrative_meta.as_ref().and_then(|n| n.beat_index);
                    let edge = matches!((beat(pair[0]), beat(pair[1])), (Some(a), Some(b)) if a != b);
                    (read[pair[1]].id.clone(), edge)
                })
                .collect()
        }
    };
    let bridged: Vec<&String> = (turns.iter().map(|(id, _)| id))
        .filter(|id| read.iter().position(|s| &s.id == *id).is_some_and(|i| i > 0 && bridges(&read[i - 1], &read[i])))
        .collect();
    // A turn inside a part of the talk between two slides that each show a measure, a figure's or a
    // chart's, is the part's next figure, not a lost room (the calibration against a reader):
    // qbr's metrics and the climate talk's data slides under the frame their part gives them.
    let measures = |i: usize| {
        (slides.get(i).and_then(|s| s.states.first()))
            .and_then(|&k| deck.states.get(k)?.layout.as_deref())
            .is_some_and(|l| FIGURES.contains(&l) || MEASURES.contains(&l))
    };
    let figured: Vec<&String> = (turns.iter())
        .filter(|(_, edge)| !edge)
        .map(|(id, _)| id)
        .filter(|id| read.iter().position(|s| &s.id == *id).is_some_and(|i| i > 0 && measures(i - 1) && measures(i)))
        .collect();
    let soft = turns.iter().filter(|(_, edge)| *edge).count();
    let hard = turns.len() - soft - figured.len();
    let paired = figured.len();
    // By words alone, neighbouring slides share few even where one follows from the other: no
    // number, only where the talk turns.
    let coherence = Measure {
        score: match m.coherence.method {
            health::coherence::Method::Embedding => Some(m.coherence.score as u32),
            health::coherence::Method::Keyword => None,
        },
        says: match (hard, soft, paired) {
            _ if m.coherence.method == health::coherence::Method::Keyword && spaceless(language) => format!(
                "Not read by words: health reads words a space sets apart, and this deck is in {}.",
                language.and_then(language_named).unwrap_or_else(|| "a script set without spaces".to_string())
            ),
            (0, 0, 0) => "No slide breaks from the one before it, read by their words.".into(),
            // Each count said only where it has one, and a turn at a beat's edge as the finding
            // says it, a new part beginning (the coordinator's read of B1 and fleet).
            _ => format!(
                "Read by their words: {}{}.",
                {
                    let said: Vec<String> = [
                        (hard, "where the talk turns sharply"),
                        (soft, "where a new part of the talk begins"),
                        (paired, "where one figure follows another"),
                    ]
                    .into_iter()
                    .filter(|&(n, _)| n > 0)
                    .enumerate()
                    .map(|(k, (n, what))| match k {
                        0 => format!("{} {what}", counted(n, "slide")),
                        _ => format!("{n} {what}"),
                    })
                    .collect();
                    said.join(", ")
                },
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
    // Where no slide declares its intensity, fidelity reads each as a talk usually runs where it
    // stands, its words unread (W2's editor dogfood): the line says so, so it does not read as
    // the slides' own against intensity's "no slide declares" (its figure is #850 FUNC-3's, Jay's).
    let undeclared = !(read.iter().filter(|s| s.eligible()))
        .any(|s| s.narrative_meta.as_ref().and_then(|n| n.expected_intensity).is_some_and(f64::is_finite));
    let fidelity = Measure {
        score: Some(fidelity_score as u32),
        says: match (bends.len(), undeclared) {
            (0, false) => format!("The intensity the slides declare follows {curve_of}."),
            (k, false) => {
                format!("{} {} the intensity away from {curve_of}.", counted(k, "slide"), agree(k, "bends", "bend"))
            }
            (0, true) => {
                format!("No slide declares its intensity: read as a talk usually runs, the slides follow {curve_of}.")
            }
            (k, true) => format!(
                "No slide declares its intensity: read as a talk usually runs, {} {} away from {curve_of}.",
                counted(k, "slide"),
                agree(k, "falls", "fall")
            ),
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
                0 => format!(
                    "{} {} far from the {what} {}.",
                    counted(gaps, "slide"),
                    agree(gaps, "reads", "read"),
                    agree(gaps, "it declares", "they declare")
                ),
                _ => format!(
                    "{} {} far from the {what} {}; on {} the lexicons read none.",
                    counted(gaps, "slide"),
                    agree(gaps, "reads", "read"),
                    agree(gaps, "it declares", "they declare"),
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
                part: meta.and_then(|m| m.beat_index).filter(|&k| k < beats.len()),
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
    // The findings whose field depends on what they ask, by state: an empty one-idea slide asked
    // for its notes alone, an intensity gap whose plan is what strays, and a talk short of its
    // slot, asked for its notes deepened.
    let (mut notes_alone, mut plan_strays, mut notes_deepened): (Vec<String>, Vec<String>, Vec<String>) =
        Default::default();
    let mut find = |id: &str, severity: Severity, kind: Kind, says: String, ask: String, fix: Option<Fix>| {
        if let Some(&i) = place.get(id) {
            findings.push(Finding {
                state: id.to_string(),
                slide: i + 1,
                severity,
                kind,
                says,
                ask,
                fix,
                figure: None,
                path: None,
                field: None,
                into: None,
            });
        }
    };
    // Where in the spine a slide's narrative is declared.
    let at = |id: &str, key: &str| -> Option<String> {
        let (i, j, _) = slides[*place.get(id)?].beat?;
        Some(format!("/spine/sections/{i}/beats/{j}/media/narrato/{key}"))
    };
    // The deck's last slide closes the talk, whatever its layout: an ask set as a list (two items
    // in a Closing make one, R1's agent dogfood 2) states what the room is to do, in a verb a
    // topic's test reads as a noun ("Fund the midday sailing from 2026"), so no headline there is
    // asked for its point, as the cover's and the close's are not.
    let last = |i: usize| i + 1 == read.len();
    // A slide whose headline names what it holds by design: a term, the voices quoted, the
    // questions asked, the exercise set (lecture's "Conditional probability", training's "Quick
    // quiz").
    // A slide planned as one is too, whatever theme layout its items were set in (training's quiz,
    // three questions set as cards).
    let named = |i: usize| {
        let planned = slides[i].beat.and_then(|(_, _, b)| b.media.as_ref()?.pointer("/narrato/layout")?.as_str());
        let plan = planned.map(|l| l.trim().to_lowercase().replace(' ', "-"));
        let laid = slides[i].states.iter().find_map(|&k| deck.states[k].layout.as_deref());
        laid.is_some_and(|l| NAMED.contains(&l)) || plan.is_some_and(|l| NAMED.contains(&l.as_str()))
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
        } else if let Some((line, listed)) =
            (english && i > 0 && !framing(i) && !last(i)).then(|| buried(deck, &slides[i], s, &heading[i])).flatten()
        {
            // A headline that names a topic over the line that says the claim (W1's dogfood 3):
            // a made slide's beat claim is its headline, so "Some history" passes as one, its
            // claim in a bullet. The line is the headline to say, never written for the writer.
            let title = shown_heads[i].trim();
            find(
                &s.id,
                Severity::Warning,
                Kind::Label,
                format!(
                    "Slide {k}'s headline names its topic, \u{201C}{title}\u{201D}; its claim is under it: \u{201C}{line}\u{201D}."
                ),
                if listed {
                    format!(
                        "Make slide {k}'s claim its headline: move \u{201C}{line}\u{201D} up from its list, in place of \u{201C}{title}\u{201D}, or say what the slide proves there."
                    )
                } else {
                    format!(
                        "Make slide {k}'s claim its headline: say \u{201C}{line}\u{201D}, or what the slide proves, in place of \u{201C}{title}\u{201D}."
                    )
                },
                None,
            );
        } else if english && i > 0 && !framing(i) && !last(i) && !led[i] && !named(i) && labels(&heading[i]) {
            // A content slide whose headline names its topic: offered its point, never rewritten.
            find(
                &s.id,
                Severity::Info,
                Kind::Label,
                format!(
                    "Slide {k}'s headline names its topic, \u{201C}{}\u{201D}, where it could state its point.",
                    heading[i].trim()
                ),
                format!("Restate slide {k}'s headline as the point it makes: what should the room believe after it?"),
                None,
            );
        }
        // A slide that draws a chart, a picture, or a table shows something past its headline, and
        // what it lacks is its notes alone (W1 dogfood 16: a chart dropped on a blank slide was
        // said to say nothing past a headline it did not have).
        if s.body.trim().is_empty() && s.notes.trim().is_empty() && !s.drawn {
            // A one-idea layout says its point in its headline by design (W1's dogfood 6): what
            // it lacks is the notes, never more on the slide.
            let (says, ask) = if s.one_idea {
                notes_alone.push(s.id.clone());
                (
                    format!("Slide {k} makes its one point in its headline, and has no notes."),
                    format!("Add notes to slide {k} that say what to tell the room."),
                )
            } else {
                (
                    format!("Slide {k} says nothing past its headline, and has no notes."),
                    format!("Add to slide {k} what bears out its headline, or notes that say what to tell the room."),
                )
            };
            find(&s.id, Severity::Warning, Kind::Empty, says, ask, None);
        }
    }
    // A turn out of an agenda is the agenda's own to make (dogfood pass 10): it names the parts
    // to come, and the slide after it opens the first, sharing few words with a list of them. So
    // is a turn out of a divider, which names the part its next slide opens (the calibration
    // against a reader); a turn into a divider is found as any is.
    let agenda = |i: usize| {
        (slides.get(i).and_then(|s| s.states.first()))
            .and_then(|&k| deck.states.get(k)?.layout.as_deref())
            .is_some_and(|l| l == "agenda" || l.starts_with("agenda-") || l == "section" || l.starts_with("section-"))
    };
    // The beat a slide opens, by the one name the settled outline gives it, else its own. In a
    // deck in another language, a name that is an arc's own is English, no name for a part of
    // the talk: the model's own name for the beat stands in, else none (W1's dogfood 10:
    // "Opens Existing Approaches" in a French deck whose beat was "Approches existantes").
    let arcs =
        |n: &str| scaena_narrato::arcs::arcs().iter().any(|a| a.beat_names().iter().any(|b| b.eq_ignore_ascii_case(n)));
    let beat_named = |i: usize| -> Option<String> {
        let meta = read[i].narrative_meta.as_ref()?;
        let settled = (meta.beat_index)
            .and_then(|b| narrative.and_then(|n| n.pointer(&format!("/outline/beats/{b}/name"))))
            .and_then(Value::as_str);
        [settled, meta.beat_name.as_deref()]
            .into_iter()
            .flatten()
            .map(headed)
            .find(|n| !n.is_empty() && (english || !arcs(n)))
            .map(str::to_string)
    };
    // A connective in the deck's own language, as its speaker would open with one.
    let (so, but) = connectives(language);
    for (id, edge) in turns.iter().filter(|(id, _)| !bridged.contains(&id)) {
        let Some(&i) = place.get(id.as_str()) else { continue };
        if i > 0 && agenda(i - 1) {
            continue;
        }
        let (k, before) = (i + 1, i);
        // The words of the slide before that a bridge might carry over, named so the writer
        // need not guess what "a word of slide b's" is.
        let such = carried(&read[i - 1], &read[i], joining(language));
        if figured.contains(&id) {
            // One figure after another in a part of the talk: information, the room's frame the
            // part's, with the ask a turn makes.
            find(
                id,
                Severity::Info,
                Kind::Turn,
                format!("Slide {k} shows another figure with no word from slide {before}: say what links the two."),
                format!(
                    "Open slide {k}'s notes with the line that carries the talk from slide {before}, in a word of slide {before}'s{such}: a connective alone (\u{201c}{so}\u{201d}, \u{201c}{but}\u{201d}) carries nothing across."
                ),
                None,
            );
        } else if *edge {
            // A turn at a beat's edge opens a new part of the talk, no loss (W1's second read of
            // fleet): information, so a sharp turn inside a beat comes first, with the same ask.
            let part = beat_named(i).unwrap_or_else(|| "a new part of the talk".to_string());
            find(
                id,
                Severity::Info,
                Kind::Turn,
                format!(
                    "Slide {k} opens {part} with nothing from slide {before}: say the line that carries the room across."
                ),
                format!(
                    "Open slide {k}'s notes with the line that carries the room from slide {before} into {part}, in a word of slide {before}'s{such}: a connective alone (\u{201c}{so}\u{201d}, \u{201c}{but}\u{201d}) carries nothing across."
                ),
                None,
            );
        } else {
            find(
                id,
                Severity::Warning,
                Kind::Turn,
                format!("Slide {k} shares little with slide {before}: the turn between them may lose the room."),
                format!(
                    "Open slide {k}'s notes with the line that carries the talk from slide {before}, in a word of slide {before}'s{such}: a connective alone (\u{201c}{so}\u{201d}, \u{201c}{but}\u{201d}) carries nothing across."
                ),
                None,
            );
        }
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
                "Plan slides {}\u{2013}{} calmer by their Intensity, so the talk builds as its arc does{later}.",
                first + 1,
                last + 1
            ),
            false => format!(
                "Plan slides {}\u{2013}{} more intense by their Intensity, so the talk carries what their places call for{later}.",
                first + 1,
                last + 1
            ),
        };
        find(&read[first].id, Severity::Info, Kind::Arc, says, ask, None);
    }
    // A long tail (the calibration against a reader): three slides or more, and a fifth of the
    // deck or more, between the last slide planned within 0.1 of the talk's peak and its close
    // (the deck's last run of closing slides, an ask among them), with no divider among them to
    // open a new part. The thresholds were set on the calibration's thirteen decks, so it is
    // information; it asks for a cut or a move, not a plan, so it names no field.
    let laid = |i: usize| slides.get(i).and_then(|s| s.states.iter().find_map(|&k| deck.states[k].layout.as_deref()));
    let mut close = read.len();
    // The deck's closing run, as the calm close reads it, and an ask before it.
    let closes = |i: usize| closing.contains(&read[i].id) || laid(i) == Some("the-ask");
    while close > 1 && closes(close - 1) {
        close -= 1;
    }
    let top = (1..close).filter_map(planned_at).fold(None, |most: Option<f64>, v| Some(most.map_or(v, |m| m.max(v))));
    if let Some(top) = top {
        let peak = (1..close).filter(|&i| planned_at(i).is_some_and(|v| v >= top - 0.1 - 1e-9)).max();
        if let Some(peak) = peak {
            let tail = peak + 1..close;
            let divided = tail.clone().any(|i| laid(i).is_some_and(|l| l.starts_with("section")));
            if tail.len() >= 3 && tail.len() * 5 >= read.len() && !divided {
                let (first, last) = (tail.start + 1, tail.end);
                let ending = if close < read.len() {
                    format!("its close at slide {}", close + 1)
                } else {
                    "its end".to_string()
                };
                find(
                    &read[tail.start].id,
                    Severity::Info,
                    Kind::Arc,
                    format!(
                        "Slides {first}\u{2013}{last} come after the talk's peak at slide {} and before {ending}: the talk may run down there.",
                        peak + 1
                    ),
                    format!(
                        "Cut slides {first}\u{2013}{last}, fold what they must say into the close, or move them before slide {}, so the talk ends near its peak.",
                        peak + 1
                    ),
                    None,
                );
            }
        }
    }
    // The beat a slide was written for, by the settled outline's name, else the arc's.
    let named_beat = |id: &str| -> Option<String> {
        let k = read[*place.get(id)?].narrative_meta.as_ref()?.beat_index?;
        beats.get(k).map(|b| headed(b).to_string()).filter(|b| !b.is_empty())
    };
    // Fidelity as the score reads it, against the arc's curve where the deck names an arc, else
    // Vzestup's canonical curve; and whether planning slide `i` at `value` raises it as the other
    // slides stand. A plan can lower it, the warp aligning the slides afresh (B1's slide 12, read
    // on the tent at 0.73, planned at 0.70), so a fix is offered only where it raises it (W1's
    // dogfood 17; no figure moves, only which fixes are offered).
    let canonical = arc::canonical_curve(read.iter().filter(|s| s.eligible()).count());
    let fidelity_of = |slides: &[SlideForHealth]| {
        let against = if arc_name.is_some() { &curve } else { &canonical };
        arc::fidelity(slides, against, &arc::FidelityOptions::default()).fidelity
    };
    let now = fidelity_of(&read);
    let raises = |i: usize, value: f64| {
        let mut planned = read.clone();
        planned[i].narrative_meta.get_or_insert_with(NarrativeMeta::default).expected_intensity = Some(value);
        fidelity_of(&planned) > now + 1e-9
    };
    // The slides whose own bend is a finding of its own, apart from a run of them.
    let mut alone: Vec<String> = Vec::new();
    for d in bends.iter().filter(|d| !folded.contains(&d.slide_id.as_str())) {
        let Some(&i) = place.get(d.slide_id.as_str()) else { continue };
        alone.push(d.slide_id.clone());
        let k = i + 1;
        let planned = read[i].narrative_meta.as_ref().and_then(|n| n.expected_intensity).filter(|v| v.is_finite());
        let (hotter, way, moved) = match d.delta > 0.0 {
            true => ("hotter", "calmer", "nearer the climax"),
            false => ("cooler", "more intense", "away from the climax"),
        };
        let beat = named_beat(&d.slide_id).map(|name| format!(", in its beat, {name}")).unwrap_or_default();
        let (says, ask, fix) = match planned {
            // What fidelity read is the plan, held to the scale: its place calls for that less the bend.
            Some(planned) => {
                let planned = planned.clamp(0.0, 1.0);
                let calls = (planned - d.delta).clamp(0.0, 1.0);
                let (was, called) = (level(planned), level(calls));
                let than = match d.delta > 0.0 {
                    true => "more intense than",
                    false => "calmer than",
                };
                // A plan the words cannot carry trades the bend for a gap between them (R1's agent
                // dogfood 2): the plan asked is the one nearest the place's call within the gap the
                // words allow, and where that still bends, the move comes first and no plan is asked.
                let words = slides_read.get(i).and_then(|r| r.read);
                let asked = match words {
                    Some(w) if (w - calls).abs() > INTENSITY_GAP_THRESHOLD => {
                        w + (calls - w).clamp(-INTENSITY_GAP_THRESHOLD, INTENSITY_GAP_THRESHOLD)
                    }
                    _ => calls,
                };
                let reach = (asked - calls).abs() <= DEVIANT_INTENSITY_DELTA + 1e-9;
                let says = format!("Slide {k} was planned {hotter} than its place in the talk.");
                // The plan asked, written where the slide's plan is, for the person to take, and
                // said as it is written (W2's review of #1186).
                let value = held(asked, words);
                let raised = reach && raises(i, value);
                let ask = match (words, reach) {
                    _ if reach && !raised && value != calls && raises(i, calls) => format!(
                        "Slide {k} is planned at {was}, {than} its place in the story calls for ({called}), and the plan its words carry, {}, would set the talk further from its curve as the other slides stand: give it words that carry {called}, or move it {moved}{beat}.",
                        level(value)
                    ),
                    _ if reach && !raised => format!(
                        "Slide {k} is planned at {was}, {than} its place in the story calls for ({called}), but planned alone, even at {called}, it would set the talk further from its curve as the other slides stand: move it {moved}{beat}, or plan the slides around it with it."
                    ),
                    (Some(w), false) => format!(
                        "Slide {k} is planned at {was}, {than} its place in the story calls for ({called}), and its words read {}, further from its place than a plan they carry can come: move it {moved}{beat}.",
                        level(w)
                    ),
                    (Some(w), true) if asked != calls => format!(
                        "Slide {k} is planned at {was}, {than} its place in the story calls for ({called}), and its words read {}: plan it at {} by its Intensity{beat}, as near its place as its words carry, or move it {moved}.",
                        level(w),
                        level(value)
                    ),
                    _ => format!(
                        "Slide {k} is planned at {was}, {than} its place in the story calls for ({called}): plan it {way} by its Intensity{beat}, or move it {moved}."
                    ),
                };
                let fix = raised.then(|| at(&d.slide_id, "intensity")).flatten().map(|path| Fix {
                    says: format!("Plan it at {}", level(value)),
                    patch: vec![json!({ "op": "replace", "path": path, "value": value })],
                });
                (says, ask, fix)
            }
            // A slide that declares none is read as a talk usually runs where it stands: asked for
            // the plan its place calls for, the curve's there, or the nearest its words carry, as
            // a planned slide's is, written where its beat keeps a plan (agent dogfood 6).
            None => {
                let calls = curve.get(i).copied().unwrap_or(0.5).clamp(0.0, 1.0);
                let asked = match slides_read.get(i).and_then(|r| r.read) {
                    Some(w) if (w - calls).abs() > INTENSITY_GAP_THRESHOLD => {
                        w + (calls - w).clamp(-INTENSITY_GAP_THRESHOLD, INTENSITY_GAP_THRESHOLD)
                    }
                    _ => calls,
                };
                let value = held(asked, slides_read.get(i).and_then(|r| r.read));
                let raised = raises(i, value);
                let fix = slides[i].beat.filter(|_| raised).map(|(si, bj, b)| {
                    let base = format!("/spine/sections/{si}/beats/{bj}");
                    let op = match b.media.as_ref() {
                        Some(media) if media.get("narrato").is_some_and(Value::is_object) => {
                            json!({ "op": "add", "path": format!("{base}/media/narrato/intensity"), "value": value })
                        }
                        Some(media) if media.is_object() => {
                            json!({ "op": "add", "path": format!("{base}/media/narrato"), "value": { "intensity": value } })
                        }
                        _ => json!({ "op": "add", "path": format!("{base}/media"),
                                     "value": { "narrato": { "intensity": value } } }),
                    };
                    Fix { says: format!("Plan it at {}", level(value)), patch: vec![op] }
                });
                (
                    format!(
                        "Slide {k} declares no intensity, where its place in the talk calls for a {way} slide than most."
                    ),
                    match raised {
                        true => format!(
                            "Plan slide {k}'s intensity for its place in the story: {} by its Intensity, where the talk's curve stands at {}{beat}.",
                            level(value),
                            level(calls)
                        ),
                        false if value == calls || !raises(i, calls) => format!(
                            "Slide {k}'s place in the story calls for {}, but planned alone, even at {}, it would set the talk further from its curve while the slides around it are read as a talk usually runs: move it {moved}{beat}, or plan them with it.",
                            level(calls),
                            level(calls)
                        ),
                        false => format!(
                            "Slide {k}'s place in the story calls for {}, and the plan its words carry, {}, would set the talk further from its curve as the other slides stand: give it words that carry {}, or move it {moved}{beat}.",
                            level(calls),
                            level(value),
                            level(calls)
                        ),
                    },
                    fix,
                )
            }
        };
        find(&d.slide_id, Severity::Info, Kind::Arc, says, ask, fix);
    }
    // A calm close is no bend (ADR-0032 item 6), and neither are its words a gap (W1's dogfood
    // 6): "Thank you / Questions?" is three words, its "you" and its question mark weighing a
    // third each, so it reads at Peak. The deck's last run of closing slides is not asked to
    // move its words to its plan; intensity's figure is Vzestup's.
    // A slide whose words already carry what its place calls for, its plan straying, is asked
    // for its plan once (W2's editor dogfood 6): the bend that asks the same edit folds into its
    // warning, which says the plan as the bend did and where it is written.
    let mut strays: Vec<String> = Vec::new();
    for g in heard_intensity.iter().filter(|g| !closing.contains(&g.slide_id)) {
        let Some(&i) = place.get(g.slide_id.as_str()) else { continue };
        let k = i + 1;
        let (declared, detected) = (level(g.declared), level(g.detected));
        let delta = bent.get(g.slide_id.as_str()).copied();
        let stray = delta.filter(|delta| (g.detected - (g.declared - delta)).abs() <= INTENSITY_GAP_THRESHOLD);
        let says = match (stray, g.gap > 0.0) {
            (Some(delta), _) => format!(
                "Slide {k} was planned {} than its place in the talk, where its words already stand.",
                if delta > 0.0 { "hotter" } else { "cooler" }
            ),
            (None, true) => format!("Slide {k} reads flatter than it was written to be."),
            (None, false) => format!("Slide {k} reads hotter than it was written to be."),
        };
        if stray.is_some() {
            strays.push(g.slide_id.clone());
        }
        // The words the reading was made of, as a tone ask names its own (R1's MCP dogfood).
        let heard = health::intensity::heard(&read[i].text(), language).unwrap_or_default();
        let named = format!("{}{}", intensed(&heard), whereabouts(&heard, &heading[i], &read[i]));
        // In a person's words; the figures are for the assistant, which reads the deck's. Where
        // the plan bends from the arc, the words are to carry what the place calls for, not the
        // plan, and words that already do leave the plan to change (ADR-0032, item 4).
        let calls = bent.get(g.slide_id.as_str()).map(|delta| g.declared - delta);
        if calls.is_some_and(|calls| (g.detected - calls).abs() <= INTENSITY_GAP_THRESHOLD) {
            plan_strays.push(g.slide_id.clone());
        }
        let ask = match calls {
            Some(calls) if (g.detected - calls).abs() <= INTENSITY_GAP_THRESHOLD => format!(
                "Slide {k}'s words read {detected}, as its place in the story calls for ({}): it is its plan ({declared}) that strays, so set its Intensity for its place{}, not the words, or move it {}.{named}",
                level(calls),
                named_beat(&g.slide_id).map(|name| format!(", in its beat, {name}")).unwrap_or_default(),
                if g.declared > calls { "nearer the climax" } else { "away from the climax" }
            ),
            Some(calls) if g.detected < calls => format!(
                "Make slide {k}'s words carry the intensity its place in the story calls for ({}): sharper verbs, a figure that lands.{named}",
                level(calls)
            ),
            Some(calls) => format!(
                "Calm slide {k}'s words to the intensity its place in the story calls for ({}); they read {detected}.{named}",
                level(calls)
            ),
            None if g.gap > 0.0 => format!(
                "Make slide {k}'s words carry the intensity it declares ({declared}): sharper verbs, a figure that lands.{named}"
            ),
            None => format!(
                "Calm slide {k}'s words to the intensity it declares ({declared}); they read {detected}.{named}"
            ),
        };
        // No fix: the plan's intensity is the arc's, which the words are to carry, not follow.
        find(&g.slide_id, Severity::Warning, Kind::Intensity, says, ask, None);
    }
    for g in heard_tone {
        let Some(&i) = place.get(g.slide_id.as_str()) else { continue };
        let k = i + 1;
        let (declared, detected) = (signed(g.declared), signed(g.detected));
        // The words the reading was made of (W1's dogfood 3): a writer warming a slide moves
        // its tone only with words the lexicon reads, so the ask names them.
        let heard = health::valence::heard(&read[i].text(), language).unwrap_or_default();
        let named = format!("{}{}", toned(&heard), whereabouts(&heard, &heading[i], &read[i]));
        let (says, ask) = match g.gap > 0.0 {
            true => (
                format!("Slide {k} reads graver than it was written to be."),
                format!(
                    "Warm slide {k}'s words toward the tone it declares ({declared}); they read {detected}.{named}"
                ),
            ),
            false => (
                format!("Slide {k} reads more hopeful than it was written to be."),
                format!("Ground slide {k}'s words in the tone it declares ({declared}); they read {detected}.{named}"),
            ),
        };
        // No fix: the plan's tone is the arc's, which the words are to carry, not follow.
        find(&g.slide_id, Severity::Warning, Kind::Tone, says, ask, None);
    }
    for &(i, was_beat, now_beat) in &drifted {
        let id = read[i].id.as_str();
        let (was, now) = (beat_said(was_beat), beat_said(now_beat));
        let k = i + 1;
        let fix = at(id, "beatIndex").zip(at(id, "beatName")).map(|(index, name)| Fix {
            says: format!("Make it part of {now}"),
            patch: vec![
                json!({ "op": "replace", "path": index, "value": now_beat }),
                json!({ "op": "replace", "path": name, "value": beat_name(now_beat) }),
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
    // Words the deck's title holds, or that the slides of every part say (its product's name,
    // "Larder"), tell no part's claim from another's: a part whose slides share only them with its
    // claim carries none of its promise (W2's outlined calibration: larder's Solution part).
    let mut common = content_words(deck.meta.as_ref().and_then(|m| m.title.as_deref()).unwrap_or_default());
    let parts: Vec<std::collections::BTreeSet<String>> = (0..outlined.len())
        .map(of_beat)
        .filter(|of| !of.is_empty())
        .map(|of| {
            let mut words = std::collections::BTreeSet::new();
            for i in of {
                words.append(&mut content_words(&format!("{} {}", read[i].title, read[i].body)));
            }
            words
        })
        .collect();
    if let [first, rest @ ..] = parts.as_slice()
        && !rest.is_empty()
    {
        let mut everywhere = first.clone();
        everywhere.retain(|w| rest.iter().all(|p| p.contains(w)));
        common.append(&mut everywhere);
    }
    let shares = |sentence: &str, of: &[usize]| -> Option<bool> {
        let mut said = content_words(sentence);
        said.retain(|w| !common.contains(w));
        if said.is_empty() {
            return None;
        }
        Some(of.iter().any(|&i| !content_words(&format!("{} {}", read[i].title, read[i].body)).is_disjoint(&said)))
    };
    // The big idea's telling words, less those the deck's title holds or every part says, where
    // no content slide before the close, `of`, says two of them together, said as the finding says
    // it; nothing where one does, or where the idea tells nothing past the deck's name. A slide that builds toward the idea says two of its words at once; one in
    // passing builds nothing (W2's outlined calibration: support's slides say “agents”, “tickets”,
    // and a “third floor”, one a slide, and never the waits the bot halved).
    let unbuilt = |idea: &str, of: &[usize]| -> Option<&'static str> {
        let mut telling = telling_words(idea);
        telling.retain(|w| !common.contains(w));
        let before: Vec<usize> =
            (0..of.iter().copied().min().unwrap_or_default()).filter(|&i| read[i].eligible() && !framing(i)).collect();
        if telling.is_empty() || before.is_empty() {
            return None;
        }
        let (together, said) = match telling.len() > 3 {
            true => (2, "none says two of its words together"),
            false => (1, "none says a word of it"),
        };
        let built = before.iter().any(|&i| {
            let mut words = content_words(&format!("{} {}", read[i].title, read[i].body));
            words.retain(|w| telling.contains(w));
            words.len() >= together
        });
        (!built).then_some(said)
    };
    let spanned = |of: &[usize]| match (of.first(), of.last()) {
        (Some(a), Some(b)) if a != b => format!("slides {}\u{2013}{}", a + 1, b + 1),
        _ => format!("slide {}", of[0] + 1),
    };
    // A deck whose words no space sets apart is not read for its claims, and says so once, as
    // intensity and tone say it (dogfood pass 11): its sentences would each read as one word.
    let unspaced = spaceless(language);
    if unspaced
        && let Some(of) = (0..outlined.len()).map(of_beat).find(|of| !of.is_empty())
        && outlined.iter().any(|(_, claim)| !claim.is_empty())
    {
        let name = language.and_then(language_named).unwrap_or_else(|| "a script set without spaces".to_string());
        find(
            &read[of[0]].id,
            Severity::Info,
            Kind::Claim,
            format!(
                "The slides are not read for the claims the outline settled: health reads words a space sets apart, and this deck is in {name}."
            ),
            "Read each part of the talk against the claim the outline settled for it: say whether its slides carry it, and what to change if not.".to_string(),
            None,
        );
    }
    // The close as the big idea reads it, and whether it lands it, worked out before the claims
    // are: the big idea's ask and the claim's ask of the close each name the other, landed or not,
    // so a close rewritten for one keeps the other (W1's dogfood 17: each fix raised the other,
    // four times, as a close headed by its part's claim missed the idea and one rewritten to the
    // idea missed the claim).
    let mut close: Option<(Vec<usize>, bool)> = None;
    let mut close_unread: Option<(String, Vec<usize>)> = None;
    if let Some(idea) = &big_idea {
        // The close is the deck's last run of closing slides, as the calm close reads it, with the
        // slide just before it where that slide is in the run's beat, where a talk lands its idea
        // before the thanks (its thesis, its ask, its steps); else its last slide. A beat that ends
        // the deck may begin well before its close, and a word or two of the idea a slide says in
        // passing lands nothing (W1's dogfood 3); a slide of an earlier beat is that beat's, not
        // the close's (dogfood 4); and a divider opening the closing beat lands no idea of its
        // own (the climate talk's "What We Can Do", once its Next Steps closed by their plan).
        let last = read.len().checked_sub(1);
        let beat = |i: usize| read[i].narrative_meta.as_ref().and_then(|m| m.beat_index);
        let mut run: Vec<usize> = (0..read.len()).filter(|&i| closing.contains(&read[i].id)).collect();
        if let Some(&first) = run.first()
            && first > 0
            && beat(first - 1) == beat(first)
            && !framing(first - 1)
        {
            run.insert(0, first - 1);
        }
        let closes = (!run.is_empty()).then_some(run).or_else(|| last.map(|l| vec![l]));
        let lands = |of: &[usize]| -> Option<bool> {
            let telling = telling_words(idea);
            if telling.is_empty() {
                return None;
            }
            let texts: Vec<&str> = of.iter().flat_map(|&i| [read[i].title.as_str(), read[i].body.as_str()]).collect();
            let said = content_words(&texts.join(" "));
            // A close that says the idea's own words in its order quotes it, and the words any talk
            // uses count where they are quoted ("where people already are"), never in passing.
            let quoted = quoted_words(idea, &texts);
            let told = telling.intersection(&said).count() + quoted.difference(&telling).count();
            Some(told * LANDED.1 >= telling.len() * LANDED.0 || told >= LANDED_WORDS)
        };
        // The telling words are English's, as the lexicons are: a close in another language is
        // not read for the big idea, and says so, rather than passing or failing on words split
        // where its letters leave ASCII ("schließt") and joining words counted as telling.
        let named = language.filter(|_| !english).map(|l| language_named(l).unwrap_or_else(|| l.to_string()));
        match (named, closes) {
            (Some(name), Some(of)) => close_unread = Some((name, of)),
            (None, Some(of)) => close = lands(&of).map(|landed| (of, landed)),
            _ => {}
        }
    }
    // The claims settled for the parts of the talk the close stands in, carried or not: said in
    // the big idea's ask, as the idea is in theirs.
    let close_claims: Vec<String> = match &close {
        Some((of, _)) if !unspaced => (outlined.iter().enumerate())
            .filter(|(k, (_, claim))| !claim.is_empty() && of_beat(*k).iter().any(|i| of.contains(i)))
            .map(|(_, (_, claim))| unstopped(claim).to_string())
            .collect(),
        _ => Vec::new(),
    };
    for (k, (name, claim)) in outlined.iter().enumerate() {
        let of = of_beat(k);
        if unspaced || claim.is_empty() || of.is_empty() || shares(claim, &of) != Some(false) {
            continue;
        }
        // The beat is named once, in what the finding says, as a part of the talk, so a beat
        // named as a heading ("Call to Action") never reads as a title over its claim (W1's
        // dogfood 6); the ask says "this part".
        // In another language than English the outline's name is the arc's, in English ("Call to
        // Action"), so the part is named by the section its slides stand in, titled in the
        // deck's language by its divider, else by its place (W1's dogfood 7).
        let titled = || {
            let (section, _, _) = slides[of[0]].beat?;
            let title = deck.spine.as_ref()?.sections.get(section)?.title.as_deref()?.trim();
            (!title.is_empty()).then(|| title.to_string())
        };
        let name = match english {
            true => Some(name.clone()).filter(|n| !n.is_empty()),
            false => titled(),
        };
        let part = match name {
            None => format!("part {} of the talk", k + 1),
            Some(name) => format!("the {} part of the talk", headed(&name)),
        };
        let claim = unstopped(claim);
        let slides = spanned(&of);
        let (does, carry) = if of.len() > 1 { ("say", "they carry") } else { ("says", "it carries") };
        let mut said = slides.clone();
        said[..1].make_ascii_uppercase();
        // Where its slides are the close, the big idea is asked of them too, landed or not, so
        // carrying the claim keeps the idea.
        let both = close.as_ref().filter(|(close, _)| of.iter().any(|i| close.contains(i)));
        let ask = match (both, &big_idea) {
            (Some(_), Some(idea)) => {
                format!(
                    "Check the claim: the outline settled that this part of the talk leaves the audience believing \u{201c}{claim}\u{201d}, and the close is to land the big idea too, \u{201c}{}\u{201d}. Rewrite {slides} so {carry} both, or tell me what to settle instead.",
                    unstopped(idea)
                )
            }
            _ => format!(
                "Check the claim: the outline settled that this part of the talk leaves the audience believing \u{201c}{claim}\u{201d}. Rewrite {slides} so {carry} it, or tell me what to settle instead."
            ),
        };
        find(
            &read[of[0]].id,
            Severity::Warning,
            Kind::Claim,
            format!("{said} {does} nothing of the claim settled for {part}, \u{201c}{claim}\u{201d}."),
            ask,
            None,
        );
    }
    if let Some(idea) = &big_idea {
        let quoted = unstopped(idea);
        if let Some((name, of)) = close_unread {
            let slides = spanned(&of);
            find(
                &read[of[0]].id,
                Severity::Info,
                Kind::BigIdea,
                format!(
                    "The close, {slides}, isn't checked for the big idea in {name}: health reads it in English decks only."
                ),
                format!(
                    "The big idea is \u{201c}{quoted}\u{201d}. Read the closing {slides} against it: say whether they land it, and what to change if not."
                ),
                None,
            );
        } else if let Some((of, false)) = close {
            let slides = spanned(&of);
            let also = match close_claims.as_slice() {
                [] => String::new(),
                [claim] => {
                    format!(", while it carries the claim the outline settled for its part, \u{201c}{claim}\u{201d}")
                }
                claims => format!(
                    ", while it carries the claims the outline settled for its parts, {}",
                    claims.iter().map(|c| format!("\u{201c}{c}\u{201d}")).collect::<Vec<_>>().join(" and ")
                ),
            };
            find(
                &read[of[0]].id,
                Severity::Warning,
                Kind::BigIdea,
                format!("The close, {slides}, never lands the big idea: \u{201c}{quoted}\u{201d}"),
                format!(
                    "The big idea is \u{201c}{quoted}\u{201d}. Rewrite the closing {slides} so the talk ends by landing it{also}."
                ),
                None,
            );
        } else if let Some((of, true)) = &close
            && let Some(none) = unbuilt(idea, of)
        {
            // A close that lands the idea where no slide before it builds toward it asserts what
            // the talk never showed (W2's outlined calibration: support's close says the bot
            // halved the waits, and no slide before it shows a wait or the bot). An info: the big
            // idea is the outline's, and no measure reads it.
            let slides = spanned(of);
            find(
                &read[of[0]].id,
                Severity::Info,
                Kind::BigIdea,
                format!(
                    "The close, {slides}, lands the big idea, but no slide before it builds toward it: {none}, so the talk asserts it without showing it."
                ),
                format!(
                    "The big idea is \u{201c}{quoted}\u{201d}. Show it in the body of the talk, on a slide or two before the close, or change the close to land what the talk shows."
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
    // Where the slides balance counts list fewer points than its floor on average, the content
    // slides that list fewest, the three that hold it down, are named on the first of them, so
    // balance's line has a slide to act on (R1's MCP dogfood). Balance and its floor are
    // Vzestup's (ADR-0032); the finding moves no figure.
    let pointed: Vec<usize> = (0..read.len()).filter(|&i| read[i].eligible() && !read[i].one_idea).collect();
    let listed: usize = pointed.iter().map(|&i| bullets(&read[i])).sum();
    let average = m.content_balance.avg_bullets_per_slide;
    if listed > 0 && average < FLOOR_POINTS {
        // A slide that draws a chart, a picture, or a table shows its point (ADR-0032), and one
        // that frames the talk, a divider or an agenda, makes none by design: neither is asked
        // for points (R1's agent dogfood 2), though balance counts them as Vzestup's does.
        // A slide's cards are its points as its list's items are (R1's dogfood 6), though balance
        // counts the bullets alone, as Vzestup's does.
        let points = |i: usize| bullets(&read[i]).max(set[i]);
        let mut thin: Vec<usize> = (pointed.iter().copied())
            .filter(|&i| i > 0 && !closing.contains(&read[i].id) && points(i) < FLOOR_POINTS as usize)
            .filter(|&i| !read[i].drawn && !framing(i) && coded[i] == 0)
            .collect();
        scaena_core::sort::by_key(&mut thin, |&i| (points(i), i));
        thin.truncate(3);
        scaena_core::sort::by_key(&mut thin, |&i| i);
        if let Some(&first) = thin.first() {
            let (named, many) = slides_named(&thin);
            find(
                &read[first].id,
                Severity::Warning,
                Kind::Thin,
                {
                    let mut said = format!(
                        "{named} {} the fewest points: the slides that list points make {average} each, under the two that read as full.",
                        if many { "list" } else { "lists" }
                    );
                    said[..1].make_ascii_uppercase();
                    said
                },
                match many {
                    true => format!(
                        "Give {named} a point more each, what bears out each headline, or set them in a layout made for one point."
                    ),
                    false => format!(
                        "Give {named} a point more, what bears out its headline, or set it in a layout made for one point."
                    ),
                },
                None,
            );
        }
    }

    // A headline that counts what its slide lists ("Three traps that make reviews useless") is
    // held to the count, in an English deck (the calibration against a reader): a slide that
    // promises three and lists two loses the room on the third. No patch: which is right, the
    // count or the list, is the writer's.
    for (i, s) in read.iter().enumerate() {
        if i == 0 || framing(i) || s.one_idea || s.drawn || !s.eligible() {
            continue;
        }
        let (Some(said), listed) = (counts_in(&s.title, language), bullets(s)) else { continue };
        if listed == 0 || listed == said || shows_figure(s, said) {
            continue;
        }
        let k = i + 1;
        let word = |n: usize| match n {
            1..=12 => {
                ["one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve"]
                    [n - 1]
                    .to_string()
            }
            n => n.to_string(),
        };
        let (said, listed) = (word(said), word(listed));
        find(
            &s.id,
            Severity::Warning,
            Kind::Story,
            format!("Slide {k}'s headline counts {said}, and the slide lists {listed}."),
            format!(
                "Make slide {k}'s headline and its list agree: list the {said} it promises, or count the {listed} it lists."
            ),
            None,
        );
    }
    let mut unnoted = Vec::new();
    let mut aloud = Vec::new();
    for (i, s) in read.iter().enumerate() {
        let k = i + 1;
        let points = most.get(i).copied().unwrap_or(0);
        let words = s.body.split_whitespace().count().saturating_sub(coded.get(i).copied().unwrap_or(0));
        if points > DENSE_POINTS {
            // Where tightening this slide would take the points the deck's slides make on
            // average under balance's floor, the ask says the trade and how to keep it: balance
            // and its floor are Vzestup's (ADR-0032), so the ask, not the score, is Scaena's to
            // set (W1's dogfood 4).
            let ask = match thin_after_cut(&read, i, &closing, &coded) {
                Some(thin) => format!(
                    "Tighten slide {k} to the {DENSE_POINTS} points that matter most, and keep what you cut in its notes to say aloud. Cutting alone lowers the deck's balance, since the deck's other slides that list points list few: give {} a point more as you tighten it.",
                    slide_list(&thin),
                ),
                None => format!("Tighten slide {k} to the {DENSE_POINTS} points that matter most, or split it in two."),
            };
            find(
                &s.id,
                Severity::Warning,
                Kind::Dense,
                format!("Slide {k} makes {points} points in one place, more than a room takes in."),
                ask,
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
        if (!s.body.trim().is_empty() || s.drawn) && s.notes.trim().is_empty() {
            unnoted.push(i);
        }
        if reads_aloud(s) {
            aloud.push(i);
        }
    }
    // A slide that says again what a slide before it said, in other words than lint's W425
    // reads as the same claim (W1's dogfood 4): their headlines share two content words or more,
    // and half of the shorter's, or their words, headline and body, are half the same. The slide
    // just before it, as before, and else the nearest earlier slide that makes a point, so a
    // slide that repeats one two back, with another between, is found too (agent dogfood 7:
    // slide 6 said slide 4's accessorials again, slide 5 between); a cover, a divider, an
    // agenda, and the close frame the talk, and say its words again by design; and a slide with
    // a figure of its own the earlier one has not tells a change, not the same point ("0.8 hours"
    // after "6.5 hours" four slides back: the fleet pitch's before and after).
    let told = |was: &SlideForHealth, now: &SlideForHealth| {
        let before = scaena_narrato::figures(&format!("{} {}", was.title, was.body));
        scaena_narrato::figures(&format!("{} {}", now.title, now.body)).iter().all(|f| before.contains(f))
    };
    // Each slide found saying again what an earlier one said, with that one: what its finding
    // offers to fold it into.
    let mut repeats: Vec<(String, String)> = Vec::new();
    for i in 1..read.len() {
        let earlier = (0..i - 1).rev().filter(|&j| !framing(i) && !framing(j) && told(&read[j], &read[i]));
        let Some((at, shared)) =
            std::iter::once(i - 1).chain(earlier).find_map(|j| repeated(&read[j], &read[i]).map(|w| (j, w)))
        else {
            continue;
        };
        let (was, now) = (&read[at], &read[i]);
        let (k, j) = (i + 1, at + 1);
        let said: Vec<String> = shared.iter().map(|w| format!("\u{201c}{w}\u{201d}")).collect();
        let said = match said.split_last() {
            Some((last, [])) => last.clone(),
            Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
            None => continue,
        };
        find(
            &now.id,
            Severity::Warning,
            Kind::Story,
            format!(
                "Slide {k} says again what slide {j} said: its headline shares {said} with \u{201c}{}\u{201d}.",
                unstopped(was.title.trim())
            ),
            // A slide further back folds as the one just before it does (R1's #1161), its words
            // joining that slide's place, the slides between left as they were; or it is cut.
            if at + 1 == i {
                format!("Fold slide {k} into slide {j}, or give slide {k} a point of its own that moves the talk on.")
            } else {
                format!(
                    concat!(
                        "Fold slide {k} into slide {j}, which says it already, or cut it, ",
                        "or give slide {k} a point of its own that moves the talk on."
                    ),
                    k = k,
                    j = j
                )
            },
            None,
        );
        repeats.push((now.id.clone(), was.id.clone()));
    }
    // The slides with nothing for the presenter to say, as one finding at the first of them: a
    // deck written by hand has no notes on any, and a row for each would bury the rest.
    // The slides whose notes say what the slide shows, as one finding at the first of them: the
    // room reads it there, and a presenter who reads it aloud again tells them nothing (W1's
    // dogfood 4). A warning, so it opens in the Story: a reader finds it the clearest tell of a
    // weak deck (the calibration against a reader, docs/examples/health-calibration.md).
    if let Some(&first) = aloud.first() {
        let (named, many) = slides_named(&aloud);
        let each = if many { "each" } else { "it" };
        find(
            &read[first].id,
            Severity::Warning,
            Kind::Notes,
            format!("The notes for {named} read the slide aloud: nearly every word of them is on {each}."),
            format!(
                "Rewrite the notes for {named} to tell the room what {each} does not show: why it matters, the story or example behind it, and the line that leads on."
            ),
            None,
        );
    }
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
            // The slides that add least: those whose words a slide before them, one that makes a
            // point, said most of, then those that turn least from the one before, the first and
            // the last kept, and a divider, an agenda, or the close, which frame the talk, and
            // its ask, which it is given for, never named. A slide's likeness to its neighbour
            // alone named a coherent talk's best-joined slides, its hook among them, and missed
            // one that said again a slide two back (agent dogfood 7).
            let pairs = &m.coherence.pair_similarities;
            let mut cut: Vec<usize> = (1..n.saturating_sub(1)).filter(|&i| !framing(i) && !closes(i)).collect();
            scaena_core::sort::by_key(&mut cut, |&i| {
                let said = (0..i).filter(|&j| !framing(j)).map(|j| already(&read[j], &read[i])).fold(0.0, f64::max);
                let turned = pairs.get(i - 1).copied().unwrap_or(0.0);
                (std::cmp::Reverse((said * 1e6) as i64), std::cmp::Reverse((turned * 1e6) as i64))
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
    // An address a made deck links that neither its brief nor its sources name (lift-up 20261025
    // SEC-4). A deck no narrative made is the person's own, its links theirs.
    if let Some(narrative) = narrative {
        let named = (narrative.get("brief").map(Value::to_string).into_iter())
            .chain(narrative.get("sources").map(Value::to_string))
            .collect::<String>()
            .to_lowercase();
        for link in crate::narrative::links(deck) {
            let Some(host) = host(&link.href) else { continue };
            let Some(slide) = read.get(link.slide - 1) else { continue };
            if named.contains(&host) {
                continue;
            }
            let k = link.slide;
            let shown = link.href.trim_start_matches("mailto:").trim_start_matches("https://");
            find(
                &slide.id,
                Severity::Warning,
                Kind::Link,
                format!("Slide {k} links {shown}, which neither the brief nor the sources name."),
                format!(
                    "Check where slide {k}'s link to {shown} goes, and keep it only if it is the address you mean."
                ),
                None,
            );
        }
    }
    // A figure a slide writes in a deck made from its sources, where the slide names none of them
    // (W1's dogfood 4): its beat cites no source, no address, and no data the deck keeps, so the
    // figure was the model's word alone, or a source's the slide never says.
    let sources: Vec<Vec<String>> = (narrative.and_then(|n| n.get("sources")).and_then(Value::as_array))
        .map(|all| {
            (all.iter())
                .flat_map(|s| ["name", "title"].map(|k| s.get(k).and_then(Value::as_str).map(source_words)))
                .flatten()
                .collect()
        })
        .unwrap_or_default();
    // The deck's sources as a person names them, by name, else title, else address: what a finding
    // says the slide cites none of, never that the figure came from them (R1's agent dogfood 9).
    let named: Vec<String> = (narrative.and_then(|n| n.get("sources")).and_then(Value::as_array))
        .map(|all| {
            (all.iter())
                .filter_map(|s| {
                    ["name", "title", "url"].iter().find_map(|k| s.get(*k).and_then(Value::as_str).map(str::trim))
                })
                .filter(|n| !n.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let uncited = match named.as_slice() {
        [] => "cites none of the deck's sources for it".to_string(),
        [one] => format!("does not cite the deck's source, {one}, for it"),
        [a, b] => format!("cites neither of the deck's sources, {a} and {b}, for it"),
        [rest @ .., last] if rest.len() < 4 => {
            format!("cites none of the deck's sources, {}, and {last}, for it", rest.join(", "))
        }
        [first, second, third, more @ ..] => {
            format!("cites none of the deck's sources, {first}, {second}, {third}, and {} more, for it", more.len())
        }
    };
    let mut unsourced: Vec<(String, String)> = Vec::new();
    if !sources.is_empty() {
        // The figures a slide that cites a source writes, by value and unit: one an earlier slide
        // sourced is sourced where it is said again (W1's dogfood 5).
        let mut known: Vec<(String, String)> = Vec::new();
        // Each figure found unsourced, by value and unit, with the slides that write it, the first
        // the one it is found on: a figure several slides write is one finding that names them all
        // (W2's health read: Valdemar's 2,3 M€ on slides 5 and 8), which citing the first answers,
        // the rest then saying again what it sourced.
        let mut found: Vec<((String, String), String, Vec<usize>)> = Vec::new();
        for (i, s) in read.iter().enumerate() {
            let shown = format!("{}\n{}", s.title, s.body);
            let cited = slides.get(i).and_then(|sl| sl.beat).is_some_and(|(.., b)| !b.evidence.is_empty())
                || names_source(&shown, &sources);
            let written = figures(&shown);
            if cited {
                known.extend(written.into_iter().map(|f| (f.value, f.unit)));
                continue;
            }
            // A slide's first figure no cited slide wrote: one an earlier slide was found for adds
            // this slide to its finding, else it is found here.
            let Some(f) = written.into_iter().find(|f| !known.contains(&(f.value.clone(), f.unit.clone()))) else {
                continue;
            };
            let key = (f.value.clone(), f.unit.clone());
            match found.iter_mut().find(|(k, ..)| *k == key) {
                Some((.., on)) => on.push(i),
                None => found.push((key, f.shown, vec![i])),
            }
        }
        for (_, figure, on) in found {
            let (first, k) = (on[0], on[0] + 1);
            unsourced.push((read[first].id.clone(), figure.clone()));
            let (says, ask) = match on.len() {
                1 => (
                    format!("Slide {k} writes {figure}, and {uncited}."),
                    format!(
                        "Say on slide {k} where {figure} comes from, naming the source, or cite the source it is from."
                    ),
                ),
                _ => {
                    let (again, many) = slides_named(&on[1..]);
                    let does = if many { "do" } else { "does" };
                    (
                        format!("Slide {k} writes {figure}, as {again} {does}, and {uncited}."),
                        format!(
                            "Say on slide {k} where {figure} comes from, naming the source, or cite the source it is from: that sources it on {again} too."
                        ),
                    )
                }
            };
            find(&read[first].id, Severity::Warning, Kind::Source, says, ask, None);
        }
    }
    // A talk that runs short of the slot its slides are given, or past it, found on its shortest
    // beat or its longest, with the beats that run so named (lift-up FUNC-1).
    if let (Some((seconds, beats, timed)), Some((given, asked_for, (lo, hi), _, rounded))) = (&talk, &given) {
        let said = if *rounded { (seconds / 60.0).round() } else { seconds / 60.0 };
        // A talk no beat times runs at least as long as its words take: never found short (the
        // calibration against a reader), whatever its brief asks; found long, as words past the
        // slot are.
        let under = *timed && said < *lo;
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
            let (at, says, ask) = if under {
                (
                    short,
                    format!(
                        "The talk runs {}, short of {given}. Its shortest beat, slide {}'s, runs {}; its longest, slide {}'s, {}.",
                        minutes(*seconds),
                        short + 1,
                        span(shortest),
                        long + 1,
                        span(longest)
                    ),
                    format!(
                        "Deepen the notes of {named}, the shortest {}, until the talk fills {asked_for}: what to tell the room beyond what {} shows.",
                        if many { "beats" } else { "beat" },
                        if many { "each" } else { "it" }
                    ),
                )
            } else {
                (
                    long,
                    format!(
                        "The talk runs {}, past {given}. Its longest beat, slide {}'s, runs {} {}; its shortest, slide {}'s, {}.",
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
                        "Bring the talk within {asked_for}, the longest {} first: tighten or cut {named}.",
                        if many { "beats" } else { "beat" }
                    ),
                )
            };
            if under {
                notes_deepened.push(read[at].id.clone());
            }
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
    // USE-4): what the person may try again, by hand or with the assistant, or dismiss. Its fix
    // takes its record away, held to the record as read, so a fix gone stale once another took a
    // record before it is refused, not another record taken; on a slide with several, the
    // highest first, so each taken in the order given finds its own.
    let refused = narrative.and_then(|n| n.get("refused")).and_then(Value::as_array).into_iter().flatten();
    for (i, r) in refused.enumerate().collect::<Vec<_>>().into_iter().rev() {
        let text = |key: &str| r.get(key).and_then(Value::as_str).unwrap_or_default();
        let Some(k) = r.get("slide").and_then(Value::as_u64).map(|k| k as usize) else { continue };
        let Some(slide) = k.checked_sub(1).and_then(|i| slides.get(i)) else { continue };
        let (what, why) = (text("what"), text("why"));
        let path = format!("/meta/narrative/refused/{i}");
        let fix = Fix {
            says: "Dismiss it".into(),
            patch: vec![json!({ "op": "test", "path": path, "value": r }), json!({ "op": "remove", "path": path })],
        };
        // One let go as it was read, for its figures or its claim, lint never judged (W1 dogfood 9).
        let (says, ask) = match r.get("by").and_then(Value::as_str) {
            None => (
                format!(
                    "Slide {k} was kept as made: the model asked for {what}, and lint found it worse for that. {why}"
                ),
                format!(
                    "The model asked slide {k} for {what}. Make that change without what lint found it caused: {why}"
                ),
            ),
            Some(_) => (
                format!("Slide {k} was kept as made: the model asked for {what}, and {why}."),
                format!(
                    "The model asked slide {k} for {what}, which was let go: {why}. Tighten it so it still says that."
                ),
            ),
        };
        find(&deck.states[slide.states[0]].id, Severity::Info, Kind::Look, says, ask, Some(fix));
    }
    // A bend whose slide's warning asks the same edit is that warning's (W2's editor dogfood 6).
    // A run of bends names several slides' plans, and stays.
    // A long tail asks a cut or a move, not a plan: it is neither folded nor given a path.
    let tail = |f: &Finding| f.kind == Kind::Arc && f.says.contains("after the talk's peak");
    let once = |f: &Finding| f.kind == Kind::Arc && !tail(f) && strays.contains(&f.state) && alone.contains(&f.state);
    findings.retain(|f| !once(f));
    // Each `arc` finding, and each warning that asks a slide's plan in its place, says where the
    // slide's plan is written, for an agent.
    for f in findings
        .iter_mut()
        .filter(|f| (f.kind == Kind::Arc && !tail(f)) || (f.kind == Kind::Intensity && strays.contains(&f.state)))
    {
        f.path = at(&f.state, "intensity");
    }
    // Each finding that asks for one field says which (W2's editor dogfood 6).
    for f in findings.iter_mut() {
        let of = |ids: &[String]| ids.contains(&f.state);
        f.field = match f.kind {
            Kind::Title | Kind::Label => Some(Field::Headline),
            Kind::Turn | Kind::Notes => Some(Field::Notes),
            Kind::Empty if of(&notes_alone) => Some(Field::Notes),
            Kind::Pacing if of(&notes_deepened) => Some(Field::Notes),
            Kind::Arc if f.says.contains("after the talk's peak") => None,
            Kind::Arc => Some(Field::Intensity),
            Kind::Intensity if of(&plan_strays) => Some(Field::Intensity),
            Kind::Claim if f.severity == Severity::Warning => Some(Field::Claim),
            _ => None,
        };
    }
    // Each finding of a slide said again names the earlier slide it may be folded into.
    for f in findings.iter_mut().filter(|f| f.kind == Kind::Story && f.says.contains(" says again what slide ")) {
        f.into = repeats.iter().find(|(id, _)| *id == f.state).map(|(_, was)| was.clone());
    }
    // Each `source` finding says the figure it found no source for.
    for f in findings.iter_mut().filter(|f| f.kind == Kind::Source) {
        f.figure = unsourced.iter().find(|(id, _)| *id == f.state).map(|(_, figure)| figure.clone());
    }
    // Each finding names the step it was read at, which an editor shows (W2's editor dogfood): a
    // slide's notes are read across its steps, the first's first, and its plan is its beat's, so a
    // finding about either, or one that the slide does not stand, names its first step; one about
    // what the slide shows names its last, where it is built whole and its words were read.
    for f in &mut findings {
        if !matches!(
            f.kind,
            Kind::Turn | Kind::Notes | Kind::Empty | Kind::Arc | Kind::Drift | Kind::Pacing | Kind::Layout
        ) && let Some(&last) = slides.get(f.slide - 1).and_then(|s| s.states.last())
        {
            f.state = deck.states[last].id.clone();
        }
    }
    // A slide that does not stand first, then in the order the slides stand, and on each slide in
    // the order health found them.
    scaena_core::sort::by_key(&mut findings, |f| (f.kind != Kind::Layout, f.slide));
    // A slide that does not stand caps the score at the clean slides' share, less ten for each
    // that does not (ADR-0032 item 1, Jay's call, 2026-10-25).
    let broken = findings.iter().filter(|f| f.kind == Kind::Layout).count();
    let cap = (broken > 0)
        .then(|| ((100.0 * (n - broken) as f64 / n as f64).round() as u32).saturating_sub(10 * broken as u32));
    // The cap said where it holds the score down, not where the score sits under it already.
    let capped = cap.filter(|&c| c < score).map(|cap| Capped {
        slides: findings.iter().filter(|f| f.kind == Kind::Layout).map(|f| f.slide).collect(),
        cap,
        score,
    });
    let score = cap.map_or(score, |c| score.min(c));

    // The figures the slides state, each value and unit once.
    let mut stated: Vec<(String, String)> = Vec::new();
    for r in &read {
        for f in figures(&format!("{}\n{}", r.title, r.body)) {
            if !stated.contains(&(f.value.clone(), f.unit.clone())) {
                stated.push((f.value, f.unit));
            }
        }
    }
    let compare = (stated.len() >= COMPARED).then(comparing);
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
        capped,
        compare,
    })
}

/// The words of `text` Scaena's own checks read it by, the claim's and the big idea's: lowercased,
/// split at whatever is not a letter or a figure of any script, each more than three characters
/// long with a letter in it, so "coûtera" and "émissions" stay whole and a year alone is no word,
/// and none of a script that sets no spaces between its words ([`unspaced`]) (dogfood pass 11). Vzestup's coherence splits its own way, ASCII alone
/// (`scaena_narrato::health::coherence::keywords`), its figures Vzestup's (ADR-0032).
fn content_words(text: &str) -> std::collections::BTreeSet<String> {
    (text.to_lowercase().split(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.chars().count() > 3 && w.chars().any(char::is_alphabetic) && !w.chars().any(unspaced))
        .map(str::to_string)
        .collect()
}

/// The headline words `now` shares with the slide before it, `was`, where it says that slide's
/// point again (W1's dogfood 4): two or more, half of the shorter headline's at least, unless each
/// headline says a word of its own over bodies that share under half their words, or the two
/// slides' words half the same; a headline the same word for word is lint's W425, and none here.
/// A deck's slides as Narrato writes them share a third of their headlines at most, and a fifth of
/// their words.
fn repeated(was: &SlideForHealth, now: &SlideForHealth) -> Option<Vec<String>> {
    let (a, b) = (said(&was.title), said(&now.title));
    if a.is_empty() || a == b {
        return None;
    }
    let shared: Vec<String> = a.intersection(&b).cloned().collect();
    // Headlines a writer set parallel on purpose ("The town built its own grid", then "… its own
    // market") each say a word of their own, and their slides go on to say different things: a
    // claim said twice says its new words over the same body. The bodies share half of the
    // shorter's words, or the headlines are no repeat; headlines that share a figure say one
    // claim, whatever their bodies (W1's dogfood 5: "52 miles, not 40", then "the 40-mile goal").
    let (ba, bb) = (said(&was.body), said(&now.body));
    let own = !a.is_subset(&b) && !b.is_subset(&a);
    let figured = shared.iter().any(|w| w.chars().all(|c| c.is_ascii_digit()));
    let parallel = own && !figured && ba.intersection(&bb).count() * 2 < ba.len().min(bb.len()).max(1);
    let headlines = shared.len() >= 2 && shared.len() * 2 >= a.len().min(b.len()) && !parallel;
    let whole = |s: &SlideForHealth| said(&format!("{} {}", s.title, s.body));
    let (wa, wb) = (whole(was), whole(now));
    let words = wa.intersection(&wb).count() * 2 >= wa.union(&wb).count() && !shared.is_empty();
    // Each shared word as the headline writes it ("hours", not "hour").
    let written: Vec<String> = (now.title.to_lowercase().split(|c: char| !(c.is_alphanumeric() || c == ',')))
        .map(|w| w.trim_matches(',').to_string())
        .collect();
    let shown = |w: &String| {
        (written.iter())
            .find(|x| said(x).contains(w) && x.chars().any(char::is_alphanumeric))
            .cloned()
            .unwrap_or_else(|| w.clone())
    };
    (headlines || words).then(|| shared.iter().map(shown).collect())
}

/// The share of the words `now` says, its headline's and its body's, that `was` said already: none
/// where it says fewer than three, which tell too little to judge (agent dogfood 7).
fn already(was: &SlideForHealth, now: &SlideForHealth) -> f64 {
    let whole = |s: &SlideForHealth| said(&format!("{} {}", s.title, s.body));
    let (before, words) = (whole(was), whole(now));
    if words.len() < 3 {
        return 0.0;
    }
    words.intersection(&before).count() as f64 / words.len() as f64
}

/// The words `text` says, as a repeat is read (W1's dogfood 5): its content words, a hyphen
/// splitting a word in two ("40-mile"), each plural folded into its singular ("miles" is
/// "mile"), and its figures, ten and over and not a year, by their digits ("52", "184000").
fn said(text: &str) -> std::collections::BTreeSet<String> {
    let mut words =
        scaena_core::sort::set(content_words(text).into_iter().map(|w| {
            if w.len() > 4 && w.ends_with('s') && !w.ends_with("ss") { w[..w.len() - 1].to_string() } else { w }
        }));
    for w in text.split(|c: char| !(c.is_ascii_digit() || c == ',' || c == '.')) {
        let digits = w.trim_matches(|c| c == ',' || c == '.').replace(',', "");
        let Ok(n) = digits.parse::<f64>() else { continue };
        if n >= 10.0 && !(digits.len() == 4 && (1900.0..=2100.0).contains(&n)) {
            words.insert(digits);
        }
    }
    words
}

/// The share of a slide's notes' words that are its own (`READ_ALOUD`) past which its notes read
/// it aloud, and the fewest words of notes that are read so: a deck's notes as Narrato writes them
/// say a fifth to a half of theirs, and a note of a word or two says too little to judge.
const READ_ALOUD: (usize, usize) = (3, 4);
const READ_ALOUD_WORDS: usize = 3;

/// Whether `slide`'s notes say what it shows: three in four of their content words or more are
/// its headline's and body's (W1's dogfood 4).
fn reads_aloud(slide: &SlideForHealth) -> bool {
    let notes = content_words(&slide.notes);
    let shown = content_words(&format!("{} {}", slide.title, slide.body));
    let own = notes.intersection(&shown).count();
    if notes.len() >= READ_ALOUD_WORDS && own * READ_ALOUD.1 >= notes.len() * READ_ALOUD.0 {
        return true;
    }
    // Notes that are a line of the slide again, the headline or one of its lines, read it aloud
    // at any length (W1's dogfood 5: "- We rebuilt 52 miles, not 40." under that headline).
    let lines: Vec<_> = std::iter::once(slide.title.as_str()).chain(slide.body.lines()).map(line_said).collect();
    let said: Vec<_> = slide.notes.lines().map(line_said).filter(|l| !l.is_empty()).collect();
    !said.is_empty() && said.iter().all(|l| lines.contains(l))
}

/// What a line says, as notes that read it aloud are matched to it: its words, small, and its
/// figures by value and unit, the words that name a figure's unit folded into the figure, so
/// "184,000 dollars" says what "$184,000" says and "- We rebuilt 52 miles." what its headline does.
fn line_said(line: &str) -> std::collections::BTreeSet<String> {
    let written = figures(line);
    let units: Vec<&str> = written.iter().map(|f| f.unit.as_str()).collect();
    let mut said = scaena_core::sort::set(written.iter().map(|f| format!("{}|{}", f.value, f.unit)));
    for w in line.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()) {
        if !units.contains(&unit_of(w).as_str()) {
            said.insert(w.to_lowercase());
        }
    }
    said
}

/// `sentence` as a finding quotes it in the middle of one of its own: without its closing stop, so
/// the sentence after the quote reads "…already are”. Rewrite", never "…already are.”. Rewrite"
/// (W1's dogfood 3 and 4). A question or an exclamation keeps its mark, which says how it reads.
/// A full stop of a script set without spaces goes too, "…用意します”. Read", never
/// "…用意します。”. Read" (health read of a Japanese deck).
fn unstopped(sentence: &str) -> &str {
    sentence.trim_end().trim_end_matches(['.', '\u{3002}', '\u{ff61}']).trim_end()
}

/// A beat's name as a sentence says it (W1's dogfood 8): a name written as a heading and its
/// gloss, "Solution: Introduce your answer", is said by its heading, "Solution", as the browser's
/// Tighten takes it; the whole name stays in the data. A name with nothing on one side of its
/// colon is said whole, trimmed.
fn headed(name: &str) -> &str {
    let name = name.trim();
    match name.split_once(':') {
        Some((head, gloss)) if !head.trim().is_empty() && !gloss.trim().is_empty() => head.trim(),
        _ => name,
    }
}

/// The most words a tone or an intensity ask names on either side.
const TONED: usize = 4;

/// The words a tone was read from, as a sentence a tone ask ends with: those read as grave, the
/// gravest first, and those read as hopeful, each at most [`TONED`] and named once, and which side
/// reads none ("Health reads “missed” and “outage” as grave, and no word as hopeful."). Nothing
/// where no word was read.
fn toned(heard: &[(String, f64)]) -> String {
    named(heard, ("grave", "hopeful"), false)
}

/// The words an intensity was read from, as [`toned`] names a tone's (R1's MCP dogfood): those read
/// as heated first, then those read as calm, a `!` or a `?` named as the mark it is ("Health reads
/// “you” and its question mark as heated, and no word as calm.").
fn intensed(heard: &[(String, f64)]) -> String {
    named(heard, ("calm", "heated"), true)
}

/// `heard`'s words as a sentence: those weighed below zero, called `sides.0`, and those above,
/// `sides.1`, the side above first where `above_first`, each weightiest first, at most [`TONED`],
/// and named once; nothing where no word was read.
fn named(heard: &[(String, f64)], sides: (&str, &str), above_first: bool) -> String {
    let side = |below: bool| -> Vec<String> { picked(heard, below).iter().map(|w| quoted(w)).collect() };
    let list = |ws: &[String]| match ws {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    let (below, above) = (side(true), side(false));
    let ((first, a), (second, b)) = match above_first {
        true => ((above, sides.1), (below, sides.0)),
        false => ((below, sides.0), (above, sides.1)),
    };
    match (first.is_empty(), second.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!(" Health reads {} as {a}, and no word as {b}.", list(&first)),
        (true, false) => format!(" Health reads {} as {b}, and no word as {a}.", list(&second)),
        (false, false) => format!(" Health reads {} as {a}, and {} as {b}.", list(&first), list(&second)),
    }
}

/// The words of `heard` a finding names on one side, those weighed below zero or above it: each
/// once, the weightiest first, the sort stable so a tie keeps the order they stand in, at most
/// [`TONED`].
fn picked(heard: &[(String, f64)], below: bool) -> Vec<String> {
    let mut words: Vec<&(String, f64)> = heard.iter().filter(|(_, w)| (*w < 0.0) == below && *w != 0.0).collect();
    scaena_core::sort::by(&mut words, |a, b| b.1.abs().total_cmp(&a.1.abs()));
    let mut out: Vec<String> = Vec::new();
    for (w, _) in words {
        if !out.contains(w) && out.len() < TONED {
            out.push(w.clone());
        }
    }
    out
}

/// A word a finding names, as a sentence says it: quoted, or a mark named as the mark it is.
fn quoted(w: &str) -> String {
    match w {
        "!" => "its exclamation mark".to_string(),
        "?" => "its question mark".to_string(),
        _ => format!("\u{201c}{w}\u{201d}"),
    }
}

/// Where the words a tone or an intensity finding names stand (W2's editor dogfood 6), as a
/// sentence its ask ends with: in its headline as it shows, elsewhere on the slide, in its notes,
/// or only in the claim its beat keeps for it, which the slide no longer says and the editors'
/// Update sets to its words; nothing where no word is named. Its reading is the slide's title,
/// body, and notes, and its title is the beat's claim where the beat keeps one, so a word may be
/// read that the slide does not show.
fn whereabouts(heard: &[(String, f64)], headline: &str, s: &SlideForHealth) -> String {
    let tokens = |text: &str| -> String {
        let lowered = text.to_lowercase();
        let words: Vec<&str> = (lowered.split(|c: char| !matches!(c, 'a'..='z' | '0'..='9' | '%' | '$' | '&' | '-')))
            .filter(|t| !t.is_empty() && *t != "-")
            .collect();
        format!(" {} ", words.join(" "))
    };
    let has = |text: &str, w: &str| match w {
        "!" | "?" => text.contains(w),
        _ => tokens(text).contains(&format!(" {w} ")),
    };
    let mut words = picked(heard, false);
    words.extend(picked(heard, true));
    let mut places: Vec<(&str, Vec<String>)> = Vec::new();
    for w in &words {
        let place = if has(headline, w) {
            "in its headline"
        } else if has(&s.body, w) {
            "elsewhere on the slide"
        } else if has(&s.notes, w) {
            "in its notes"
        } else {
            "only in its story's claim, which the slide no longer says (Update it to the slide's words)"
        };
        match places.iter_mut().find(|(p, _)| *p == place) {
            Some((_, ws)) => {
                if !ws.contains(&quoted(w)) {
                    ws.push(quoted(w));
                }
            }
            None => places.push((place, vec![quoted(w)])),
        }
    }
    // As the slide is read: its headline, the rest of it, its notes, and the claim behind it.
    let rank = |p: &str| match p {
        "in its headline" => 0,
        "elsewhere on the slide" => 1,
        "in its notes" => 2,
        _ => 3,
    };
    scaena_core::sort::by_key(&mut places, |(p, _)| rank(p));
    let said: Vec<String> = (places.iter())
        .map(|(place, ws)| {
            let is = if ws.len() > 1 { "are" } else { "is" };
            let list = match ws.as_slice() {
                [one] => one.clone(),
                [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
                [] => String::new(),
            };
            format!("{list} {is} {place}")
        })
        .collect();
    match said.is_empty() {
        true => String::new(),
        false => {
            format!(" {}.", capitalized(&said.join("; ")))
        }
    }
}

/// The words a line that carries the room across a turn might take from the slide before it,
/// `was`, into `next` (R1's MCP dogfood): those that would answer the turn as [`bridges`] reads
/// one, a word `was` uses and `next` does not, taken from its headline in the order they stand,
/// else from its body, each a word as written whole, no figure and none of the commonest words,
/// at most three, as a clause the
/// turn's ask names them in; nothing where it has none.
fn carried(was: &SlideForHealth, next: &SlideForHealth, common: &[&str]) -> String {
    let words = |s: &SlideForHealth| written(&format!("{} {}", s.title, s.body));
    let (ours, theirs) = (words(was), words(next));
    let ordered = |text: &str| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        // A word as written whole, so a contraction's half ("didn") is not named, though it reads
        // as one; a word in any script is, as the bridge reads it ("größe").
        let lowered = text.to_lowercase();
        let whole = lowered.split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '\u{2019}');
        for w in whole.filter(|w| w.chars().all(char::is_alphanumeric)) {
            let fits = ours.contains(w) && !theirs.contains(w) && !common.contains(&w);
            if fits && w.chars().any(char::is_alphabetic) && !out.iter().any(|o| o == w) && out.len() < 3 {
                out.push(w.to_string());
            }
        }
        out
    };
    let mut found = ordered(&was.title);
    if found.is_empty() {
        found = ordered(&was.body);
    }
    let quoted: Vec<String> = found.iter().map(|w| format!("\u{201c}{w}\u{201d}")).collect();
    match quoted.as_slice() {
        [] => String::new(),
        [one] => format!(", such as {one}"),
        [rest @ .., last] => format!(", such as {} or {last}", rest.join(", ")),
    }
}

/// A letter of a script that sets no space between its words, Han, kana, Hangul, or Thai: a
/// run of them is a phrase or a sentence, not a word, so a check that reads words reads none.
fn unspaced(c: char) -> bool {
    matches!(u32::from(c),
        0x0E00..=0x0E7F | 0x1100..=0x11FF | 0x3040..=0x30FF | 0x3130..=0x318F | 0x31F0..=0x31FF
        | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF66..=0xFF9F
        | 0x20000..=0x2FFFF)
}

/// Whether `language` is one whose words no space sets apart, as health reads them: Chinese,
/// Japanese, or Korean, by its tag or its English name. Its sentences read as one word each, so
/// a check that reads words reads nothing there.
fn spaceless(language: Option<&str>) -> bool {
    let Some(name) = language.map(str::trim).filter(|l| !l.is_empty()) else { return false };
    let name = name.to_lowercase();
    let head = name.split(['-', '_', ' ']).next().unwrap_or_default();
    matches!(head, "ja" | "jpn" | "japanese" | "zh" | "zho" | "chi" | "chinese" | "ko" | "kor" | "korean")
}

/// The fewest points the slides that make several make on average before balance marks a deck
/// down: Vzestup's floor, `metrics`' "Add more bullets to thin slides" (ADR-0032).
const FLOOR_POINTS: f64 = 2.0;

/// What a headline counts in `language`, where it counts the things its slide lists: in
/// English, [`counts`]; in Spanish, [`cuenta`] ("Tres barrios"); in Japanese, [`kazoe`] ("三つ",
/// "3つ"; W2's health read); in another language, nothing, which reads none.
fn counts_in(headline: &str, language: Option<&str>) -> Option<usize> {
    let tag = language.map(|l| l.trim().to_lowercase()).unwrap_or_default();
    match tag.split(['-', '_', ' ']).next().unwrap_or_default() {
        "es" | "spa" | "spanish" | "español" | "espanol" => cuenta(headline),
        "ja" | "jpn" | "japanese" | "日本語" => kazoe(headline),
        _ if in_english(language) => counts(headline),
        _ => None,
    }
}

/// What a Spanish headline counts, as [`counts`] reads an English one: a number from two to
/// twelve, in words or figures, before a word that names them ("Tres barrios concentran la
/// mitad"), never a measure ("tres años", "40 kilómetros") or a figure's own words ("dos de cada
/// tres").
fn cuenta(headline: &str) -> Option<usize> {
    const WORDS: [&str; 11] =
        ["dos", "tres", "cuatro", "cinco", "seis", "siete", "ocho", "nueve", "diez", "once", "doce"];
    const MEASURES: [&str; 26] = [
        "de",
        "por",
        "veces",
        "segundos",
        "minutos",
        "horas",
        "días",
        "dias",
        "semanas",
        "meses",
        "años",
        "anos",
        "trimestres",
        "kilómetros",
        "kilometros",
        "metros",
        "litros",
        "personas",
        "puntos",
        "euros",
        "dólares",
        "dolares",
        "mil",
        "millones",
        "y",
        "o",
    ];
    let words: Vec<String> = (headline.split_whitespace())
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let number = |w: &str| w == "uno" || w == "una" || WORDS.contains(&w) || w.parse::<f64>().is_ok();
    let mut found = None;
    for (i, w) in words.iter().enumerate() {
        let n = WORDS
            .iter()
            .position(|n| n == w)
            .map(|k| k + 2)
            .or_else(|| w.parse::<usize>().ok().filter(|n| (2..=12).contains(n)));
        let Some(n) = n else { continue };
        // A ratio's second number ("dos de cada tres", "uno de cada doce") counts nothing listed.
        let back = |k: usize| i.checked_sub(k).map(|j| words[j].as_str());
        if back(1) == Some("cada") && back(2) == Some("de") && back(3).is_some_and(number) {
            continue;
        }
        let next = words.get(i + 1).map(String::as_str);
        if next.is_none_or(|next| MEASURES.contains(&next) || next.chars().all(|c| c.is_ascii_digit())) {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(n);
    }
    found
}

/// What a Japanese headline counts: a number from two to twelve, in kanji or figures, half or
/// full width, before a counter of things listed (つ, 点, 項目, 個, 件, 案), "理由は三つあります" or
/// "3つのポイント", never a part of a larger figure ("13つ") or a measure ("3億円", "12月").
fn kazoe(headline: &str) -> Option<usize> {
    const COUNTERS: [&str; 6] = ["つ", "点", "項目", "個", "件", "案"];
    let chars: Vec<char> = headline.chars().collect();
    let digit = |c: char| c.to_digit(10).or_else(|| ('０'..='９').contains(&c).then(|| c as u32 - '０' as u32));
    let kanji = |c: char| "一二三四五六七八九十".find(c).map(|i| i as u32 / 3 + 1);
    let numeral = |c: char| digit(c).is_some() || kanji(c).is_some() || "百千万億兆〇".contains(c);
    let mut found = None;
    let mut i = 0;
    while i < chars.len() {
        if !numeral(chars[i]) || (i > 0 && numeral(chars[i - 1])) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && numeral(chars[i]) {
            i += 1;
        }
        let run = &chars[start..i];
        let n = if run.iter().all(|&c| digit(c).is_some()) {
            run.iter().try_fold(0u32, |n, &c| Some(n * 10 + digit(c)?))
        } else {
            match run {
                ['十'] => Some(10),
                [one] => kanji(*one).filter(|&n| n < 10),
                ['十', one] => kanji(*one).filter(|&n| n < 3).map(|n| 10 + n),
                _ => None,
            }
        };
        let rest: String = chars[i..].iter().collect();
        let counter = COUNTERS.iter().any(|c| rest.starts_with(c));
        let Some(n) = n.map(|n| n as usize).filter(|n| (2..=12).contains(n) && counter) else { continue };
        if found.is_some() {
            return None;
        }
        found = Some(n);
    }
    found
}

/// What a headline counts, where it counts the things its slide lists: a number from two to
/// twelve, in words or figures, before a word that names them ("Three traps", "in five parts"),
/// never a measure ("six hours", "40 miles") or a figure's own words ("one in seven").
fn counts(headline: &str) -> Option<usize> {
    const WORDS: [&str; 11] =
        ["two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve"];
    const MEASURES: [&str; 20] = [
        "in", "of", "times", "percent", "seconds", "minutes", "hours", "days", "weeks", "months", "years", "quarters",
        "miles", "people", "points", "dollars", "x", "to", "and", "or",
    ];
    let words: Vec<String> = (headline.split_whitespace())
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let mut found = None;
    for (i, w) in words.iter().enumerate() {
        let n = WORDS
            .iter()
            .position(|n| n == w)
            .map(|k| k + 2)
            .or_else(|| w.parse::<usize>().ok().filter(|n| (2..=12).contains(n)));
        let Some(n) = n else { continue };
        // A ratio's second number ("one in twelve", "two of three") counts nothing listed.
        let number = |w: &str| w == "one" || WORDS.contains(&w) || w.parse::<f64>().is_ok();
        if i >= 2 && matches!(words[i - 1].as_str(), "in" | "of" | "out") && number(&words[i - 2]) {
            continue;
        }
        let next = words.get(i + 1).map(String::as_str);
        if next.is_none_or(|next| MEASURES.contains(&next) || next.chars().all(|c| c.is_ascii_digit())) {
            continue;
        }
        // Two counts in one headline say nothing plain about the list.
        if found.is_some() {
            return None;
        }
        found = Some(n);
    }
    found
}

/// Whether one of the items `s` lists is led by `n`, as a figure or its word: the headline's
/// count is then a figure the slide shows ("$1.2M opens two neighborhoods" over a card of "2 new
/// neighborhoods"), not how many it lists (agent dogfood 6).
fn shows_figure(s: &SlideForHealth, n: usize) -> bool {
    const WORDS: [&str; 12] =
        ["one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve"];
    let word = n.checked_sub(1).and_then(|k| WORDS.get(k));
    s.body.lines().filter_map(|l| l.trim().strip_prefix('\u{2022}')).any(|item| {
        let first = item.split(|c: char| c.is_whitespace() || c == ',').find(|w| !w.is_empty()).unwrap_or_default();
        first == n.to_string() || word.is_some_and(|w| first.eq_ignore_ascii_case(w))
    })
}

/// The bullet lines balance counts on a slide: those its body sets as a list's items.
fn bullets(s: &SlideForHealth) -> usize {
    s.body.split('\n').map(str::trim).filter(|l| l.starts_with('\u{2022}') || l.starts_with('-')).count()
}

/// Where cutting slide `i` to [`DENSE_POINTS`] would leave the points the deck's slides make on
/// average under the floor, as balance counts them, whether they stood at it or under it already
/// (under it, balance marks each point fewer down, W1's dogfood 6) (the slides read that are
/// not one idea by design), the other such slides a point more on keeps it, by place: the content
/// slides among them, never the cover or the close, which make no points by design, though
/// balance counts them. None where the cut keeps the average at the floor or above, or no
/// content slide but `i` counts.
fn thin_after_cut(read: &[SlideForHealth], i: usize, closing: &[String], coded: &[usize]) -> Option<Vec<usize>> {
    let counted: Vec<usize> = (0..read.len()).filter(|&j| read[j].eligible() && !read[j].one_idea).collect();
    if !counted.contains(&i) {
        return None;
    }
    let total: usize = counted.iter().map(|&j| bullets(&read[j])).sum();
    let cut = bullets(&read[i]).saturating_sub(DENSE_POINTS);
    let average = |total: usize| total as f64 / counted.len() as f64;
    if cut == 0 || average(total - cut) >= FLOOR_POINTS {
        return None;
    }
    let thin: Vec<usize> = (counted.into_iter())
        .filter(|&j| j != i && j > 0 && !closing.contains(&read[j].id) && coded.get(j).is_none_or(|&c| c == 0))
        .collect();
    (!thin.is_empty()).then_some(thin)
}

/// The noun balance's average counts in, agreeing with the figure as it reads, as [`agree`] has a
/// count agree: "1 point", else "1.8 points" (Nordlicht, R1's agent dogfood 3).
fn points(average: f64) -> &'static str {
    agree(if average.to_string() == "1" { 1 } else { 2 }, "point", "points")
}

/// Slides by place as a sentence names them: "slide 4", "slides 1 and 10", "slides 1, 4 and 10".
fn slide_list(at: &[usize]) -> String {
    let named: Vec<String> = at.iter().map(|i| (i + 1).to_string()).collect();
    match named.as_slice() {
        [one] => format!("slide {one}"),
        [rest @ .., last] => format!("slides {} and {last}", rest.join(", ")),
        [] => String::new(),
    }
}

/// The words of a run of [`QUOTED_RUN`] or more of `idea`'s words, in its order, that one of
/// `texts` says, those of more than three letters: the idea quoted, where a word any talk uses
/// says it as the telling ones do (the library keynote's "Put 4% of the budget where people
/// already are", for a big idea that ends "when it goes where people already are").
fn quoted_words(idea: &str, texts: &[&str]) -> std::collections::BTreeSet<String> {
    let words = |text: &str| -> Vec<String> {
        (text.to_lowercase().split(|c: char| !c.is_alphanumeric()))
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    };
    let idea = words(idea);
    let mut quoted = std::collections::BTreeSet::new();
    for said in texts.iter().map(|t| words(t)) {
        for start in 0..said.len() {
            for from in 0..idea.len() {
                let run = said[start..].iter().zip(&idea[from..]).take_while(|(a, b)| a == b).count();
                if run >= QUOTED_RUN {
                    said[start..start + run].iter().for_each(|w| quoted.extend(content_words(w)));
                }
            }
        }
    }
    quoted
}

/// The words of a big idea a close must say in a row, in the idea's order, to quote it: four, so
/// that a phrase any sentence makes ("one of the") quotes nothing.
const QUOTED_RUN: usize = 4;

/// The share of the big idea's telling words a close must say to land it, as a fraction: half,
/// or [`LANDED_WORDS`] of a long one's (W1's dogfood 3). Two of a long idea's words a close says
/// in passing ("people", "already") are not it said.
const LANDED: (usize, usize) = (1, 2);

/// The telling words of a big idea a close that says this many of them lands, however long it is.
const LANDED_WORDS: usize = 3;

/// Words of more than three letters a sentence holds that say nothing of what it is about, in
/// English: those that join its parts, and those any talk uses.
const COMMON: [&str; 105] = [
    "about", "after", "again", "against", "also", "already", "always", "among", "another", "anyone", "around", "back",
    "because", "been", "before", "being", "best", "better", "between", "both", "come", "comes", "could", "does",
    "doing", "done", "down", "during", "each", "even", "ever", "every", "find", "first", "from", "gets", "give",
    "going", "good", "great", "have", "having", "here", "into", "just", "keep", "know", "last", "less", "like", "long",
    "made", "make", "makes", "many", "more", "most", "much", "must", "need", "needs", "never", "next", "only", "other",
    "over", "people", "really", "same", "should", "some", "start", "still", "stop", "such", "take", "than", "that",
    "their", "them", "then", "there", "these", "they", "thing", "things", "this", "those", "through", "time", "very",
    "want", "well", "were", "what", "when", "where", "which", "while", "will", "with", "would", "year", "years",
    "your",
];

/// The words of more than three letters that join a sentence's parts in `language`, which a turn's
/// ask never names as one to carry across: [`GERMAN`] in a German deck (R1's agent dogfood 3:
/// "sich", "zwei", and "voll" were named), [`FRENCH`] in a French one (health's English-only
/// sweep, item 3), [`SPANISH`] in a Spanish one (health read of a Spanish deck: "tres" was
/// named), [`PORTUGUESE`] and [`ITALIAN`] in theirs (W1's dogfood 11: "para" was named), else
/// [`COMMON`], English's, as before.
fn joining(language: Option<&str>) -> &'static [&'static str] {
    let tag = language.map(|l| l.trim().to_lowercase()).unwrap_or_default();
    let head = tag.split(['-', '_', ' ']).next().unwrap_or_default();
    match head {
        "de" | "deu" | "ger" | "german" | "deutsch" => &GERMAN,
        "fr" | "fra" | "fre" | "french" | "français" | "francais" => &FRENCH,
        "es" | "spa" | "spanish" | "español" | "espanol" => &SPANISH,
        "pt" | "por" | "portuguese" | "português" | "portugues" => &PORTUGUESE,
        "it" | "ita" | "italian" | "italiano" => &ITALIAN,
        _ => &COMMON,
    }
}

/// Words of more than three letters a German sentence holds that say nothing of what it is about:
/// those that join its parts, the counts to twelve, and the words that only weigh another.
const GERMAN: [&str; 119] = [
    "aber", "acht", "allem", "allen", "aller", "alles", "also", "anders", "auch", "beim", "bereits", "bevor", "bitte",
    "dabei", "dafür", "daher", "damit", "dann", "darauf", "darum", "dass", "davon", "dazu", "denen", "denn", "deren",
    "dessen", "diese", "diesem", "diesen", "dieser", "dieses", "doch", "dort", "drei", "durch", "eine", "einem",
    "einen", "einer", "eines", "einmal", "etwa", "etwas", "euch", "fünf", "ganz", "gegen", "haben", "hatte", "hatten",
    "heute", "hier", "ihnen", "ihre", "ihrem", "ihren", "ihrer", "immer", "jede", "jedem", "jeden", "jeder", "jedes",
    "jetzt", "kann", "kein", "keine", "können", "mehr", "muss", "müssen", "nach", "neben", "neun", "nicht", "noch",
    "oder", "ohne", "schon", "sechs", "sehr", "sein", "seine", "seit", "sich", "sieben", "sind", "soll", "sollen",
    "sondern", "sowie", "über", "unser", "unsere", "unter", "viel", "viele", "vier", "voll", "waren", "weil", "weiter",
    "welche", "wenig", "weniger", "wenn", "werden", "wieder", "will", "wird", "wollen", "wurde", "wurden", "zehn",
    "zwei", "zwischen", "zwölf", "alle",
];

/// Words of more than three letters a French sentence holds that say nothing of what it is about:
/// those that join its parts, the counts to twelve, and the words that only weigh another.
const FRENCH: [&str; 108] = [
    "ainsi", "alors", "après", "assez", "aucun", "aucune", "aussi", "autre", "autres", "avant", "avec", "avez",
    "avoir", "avons", "beaucoup", "bien", "ceci", "cela", "celle", "celles", "celui", "certains", "cette", "ceux",
    "chaque", "chez", "cinq", "comme", "comment", "contre", "dans", "depuis", "déjà", "deux", "doit", "doivent",
    "donc", "dont", "douze", "elle", "elles", "encore", "enfin", "entre", "était", "étaient", "être", "êtes", "fait",
    "faire", "huit", "jamais", "leur", "leurs", "lors", "lorsque", "mais", "même", "mêmes", "moins", "neuf", "notre",
    "nous", "onze", "parce", "pendant", "peut", "peuvent", "plus", "pour", "pourquoi", "puis", "quand", "quatre",
    "quel", "quelle", "quelles", "quels", "quelque", "quelques", "quoi", "sans", "selon", "sept", "sera", "seront",
    "sinon", "sommes", "sont", "sous", "souvent", "surtout", "tandis", "tant", "toujours", "tous", "tout", "toute",
    "toutes", "très", "trois", "trop", "vers", "voici", "voilà", "votre", "vous", "vraiment",
];

/// Words of more than three letters a Spanish sentence holds that say nothing of what it is about:
/// those that join its parts, the counts to twelve, and the words that only weigh another.
const SPANISH: [&str; 109] = [
    "además", "ahora", "algo", "alguna", "algunas", "alguno", "algunos", "ante", "antes", "aquí", "aunque", "bien",
    "cada", "casi", "cinco", "como", "cómo", "contra", "cual", "cuál", "cuando", "cuándo", "cuatro", "desde",
    "después", "diez", "doce", "donde", "dónde", "durante", "ella", "ellas", "ellos", "entre", "esta", "está",
    "estaba", "estado", "estamos", "están", "estar", "estas", "este", "esto", "estos", "fueron", "hace", "hacer",
    "hacia", "hasta", "menos", "mientras", "misma", "mismo", "mucho", "muchas", "muchos", "nada", "ninguna", "ninguno",
    "nosotros", "nuestra", "nuestro", "nuestras", "nuestros", "nueve", "ocho", "once", "otra", "otras", "otro",
    "otros", "para", "pero", "poco", "poder", "porque", "puede", "pueden", "pues", "quién", "quienes", "sean", "según",
    "seis", "sido", "siempre", "siete", "sino", "sobre", "solo", "sólo", "somos", "también", "tanto", "tener", "tiene",
    "tienen", "todas", "todo", "todos", "tras", "tres", "unas", "unos", "usted", "ustedes", "vamos", "veces",
];

/// Words of more than three letters a Portuguese sentence holds that say nothing of what it is
/// about: those that join its parts, the counts to twelve, and the words that only weigh another
/// (W1's dogfood 11: "para" was named beside "receita" and "subiu").
const PORTUGUESE: [&str; 139] = [
    "agora", "ainda", "algo", "algum", "alguma", "algumas", "alguns", "além", "ante", "antes", "apenas", "aquela",
    "aquelas", "aquele", "aqueles", "aqui", "aquilo", "assim", "cada", "cinco", "como", "contra", "cuja", "cujo",
    "dela", "delas", "dele", "deles", "depois", "desde", "dessa", "desse", "desta", "deste", "dois", "doze", "duas",
    "durante", "elas", "eles", "embora", "enquanto", "entre", "então", "essa", "essas", "esse", "esses", "esta",
    "estamos", "estar", "estas", "estava", "este", "estes", "está", "estão", "fazer", "foram", "isso", "isto", "mais",
    "menos", "mesma", "mesmas", "mesmo", "mesmos", "muita", "muitas", "muito", "muitos", "nada", "nela", "nele",
    "nenhum", "nenhuma", "nessa", "nesse", "nesta", "neste", "nossa", "nossas", "nosso", "nossos", "nove", "numa",
    "oito", "onde", "onze", "outra", "outras", "outro", "outros", "para", "pela", "pelas", "pelo", "pelos", "pode",
    "podem", "poder", "pois", "porque", "porém", "pouco", "quais", "qual", "quando", "quanto", "quase", "quatro",
    "quem", "segundo", "seis", "seja", "sejam", "sempre", "sete", "seus", "sido", "sobre", "somos", "suas", "também",
    "tanta", "tanto", "temos", "tinha", "toda", "todas", "todo", "todos", "três", "tudo", "umas", "vamos", "vezes",
    "você", "vocês",
];

/// Words of more than three letters an Italian sentence holds that say nothing of what it is about:
/// those that join its parts, the counts to twelve, and the words that only weigh another.
const ITALIAN: [&str; 115] = [
    "adesso",
    "affinché",
    "agli",
    "alla",
    "alle",
    "allo",
    "allora",
    "altra",
    "altre",
    "altri",
    "altro",
    "anche",
    "ancora",
    "anzi",
    "avere",
    "benché",
    "bene",
    "cinque",
    "cioè",
    "come",
    "contro",
    "cosa",
    "dagli",
    "dalla",
    "dalle",
    "dallo",
    "degli",
    "della",
    "delle",
    "dello",
    "dentro",
    "dieci",
    "dodici",
    "dopo",
    "dove",
    "dunque",
    "durante",
    "ecco",
    "essa",
    "essere",
    "esso",
    "fino",
    "forse",
    "fuori",
    "invece",
    "loro",
    "meno",
    "mentre",
    "molta",
    "molte",
    "molti",
    "molto",
    "negli",
    "nella",
    "nelle",
    "nello",
    "niente",
    "nostra",
    "nostre",
    "nostri",
    "nostro",
    "nove",
    "nulla",
    "oggi",
    "ogni",
    "oltre",
    "otto",
    "perché",
    "però",
    "poco",
    "poiché",
    "potere",
    "presso",
    "prima",
    "proprio",
    "quale",
    "quali",
    "quando",
    "quanto",
    "quasi",
    "quattro",
    "quella",
    "quelle",
    "quelli",
    "quello",
    "questa",
    "queste",
    "questi",
    "questo",
    "sebbene",
    "sempre",
    "senza",
    "sette",
    "siamo",
    "sono",
    "sopra",
    "sotto",
    "stati",
    "stato",
    "stessa",
    "stesso",
    "sugli",
    "sulla",
    "sulle",
    "sullo",
    "tante",
    "tanti",
    "tanto",
    "tutta",
    "tutte",
    "tutti",
    "tutto",
    "undici",
    "verso",
    "volte",
];

/// The words of more than three letters a big idea is about: `content_words` less [`COMMON`].
fn telling_words(text: &str) -> std::collections::BTreeSet<String> {
    content_words(text).into_iter().filter(|w| !COMMON.contains(&w.as_str())).collect()
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

    /// One of W2's outlined calibration decks (`docs/examples/health-calibration-outlined`), made
    /// from its slides and its outline as `scaena generate --slides --from-outline` makes it, its
    /// slides first changed by `change`.
    fn outlined_deck(name: &str, change: impl FnOnce(&mut Vec<answer::Slide>)) -> Value {
        let dir = root().join("docs/examples/health-calibration-outlined");
        let text = std::fs::read_to_string(dir.join(format!("{name}.slides.json"))).unwrap();
        let mut written = answer::slides(&text).unwrap();
        change(&mut written.slides);
        let outline: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{name}.outline.json"))).unwrap()).unwrap();
        let narrative = Narrative {
            arc: outline["arc"].as_str().unwrap().into(),
            arc_reasoning: String::new(),
            executive_summary: written.executive_summary,
            slides: written.slides,
            truncated: false,
            icons: Default::default(),
            sources: Vec::new(),
            outline: Some(serde_json::from_value(outline).unwrap()),
            brief: Brief { topic: name.into(), length: "5-10 slides".into(), ..Brief::default() },
        };
        let dusk = std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap();
        let theme: Theme = serde_json::from_str(&dusk).unwrap();
        deck(&narrative, &theme, &Pictures::new()).unwrap().deck
    }

    /// A close that lands the big idea where no slide before it builds toward it, says two of its
    /// words together, is an info on the close, and none once a slide shows it (W2's outlined
    /// calibration: support's close says the bot halved the waits, and its slides between report
    /// the move, the hiring, and the on-call rota, a word of the idea at most on each).
    #[test]
    fn a_close_that_asserts_an_idea_no_slide_built_is_told_so() {
        let unbuilt = |h: &Health| -> Vec<(Severity, usize, String, String)> {
            (h.findings.iter())
                .filter(|f| f.kind == Kind::BigIdea)
                .map(|f| (f.severity, f.slide, f.says.clone(), f.ask.clone()))
                .collect()
        };
        let support = of(&outlined_deck("support", |_| {}));
        let found = unbuilt(&support);
        assert_eq!(found.len(), 1, "{found:#?}");
        let (severity, slide, says, ask) = &found[0];
        assert_eq!((*severity, *slide), (Severity::Info, 9), "{found:#?}");
        assert_eq!(
            says,
            "The close, slides 9\u{2013}10, lands the big idea, but no slide before it builds toward it: none says two of its words together, so the talk asserts it without showing it."
        );
        assert!(ask.starts_with("The big idea is \u{201c}Our waits halved because the triage bot"), "{ask}");
        assert!(ask.ends_with("Show it in the body of the talk, on a slide or two before the close, or change the close to land what the talk shows."), "{ask}");
        // A slide that shows the bot at work builds toward it.
        let shown = of(&outlined_deck("support", |slides| {
            slides[4].key_bullets.push("The triage bot answers a third of tickets on its own".into());
        }));
        assert_eq!(unbuilt(&shown), [], "{:#?}", shown.findings);
        // The other five: two closes that miss the idea are warned of as before, and the three
        // that land it build toward it.
        for name in ["lanes", "sourdough", "vans", "larder", "warehouse"] {
            let h = of(&outlined_deck(name, |_| {}));
            assert!(unbuilt(&h).iter().all(|(s, ..)| *s == Severity::Warning), "{name}: {:#?}", unbuilt(&h));
        }
    }

    /// A slide that draws a chart and says nothing in words is asked for its notes, never said to
    /// say nothing past its headline (W1 dogfood 16: a chart dropped on a blank slide was).
    #[test]
    fn a_slide_that_draws_a_chart_is_asked_for_notes_alone() {
        let mut deck: Value = serde_json::from_str(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        for st in deck["states"].as_array_mut().unwrap() {
            // The revenue slide shows its chart alone: no headline, no note, no notes.
            if st["id"] == "revenue" || st["id"] == "mix" {
                st.as_object_mut().unwrap().remove("notes");
                st["props"].as_object_mut().unwrap().remove("note");
                st["props"].as_object_mut().unwrap().remove("title");
            }
            if st["id"] == "revenue" {
                st["remove"] = json!(["subtitle", "bg", "title"]);
            }
            if st["id"] == "close" {
                st["props"]["title"] = json!({});
            }
        }
        for section in deck["spine"]["sections"].as_array_mut().unwrap() {
            for beat in section["beats"].as_array_mut().unwrap() {
                beat.as_object_mut().unwrap().remove("notes");
            }
        }
        let h = of(&deck);
        assert!(h.findings.iter().all(|f| !(f.kind == Kind::Empty && f.slide == 2)), "{:#?}", h.findings);
        let noted = h.findings.iter().find(|f| f.kind == Kind::Notes && f.says.ends_with("no notes for you."));
        assert!(noted.is_some_and(|f| f.says.contains("2")), "{:#?}", h.findings);
    }

    /// A slide built by hand as a grid of cards makes a point a card (R1's dogfood 6): ridgeline's
    /// recap and its plan, three cards each, are never named thin once another slide lists points,
    /// while its cost, two cards, still is. A card's label, figure, and words are one card.
    #[test]
    fn a_grid_of_cards_built_by_hand_makes_a_point_a_card() {
        let path = root().join("docs/examples/ridgeline.deck.json");
        let mut deck: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        // A slide that lists points, so balance has points to count.
        deck["nodes"]["lesson-sub"]["text"] = "Steep streets need help\nE-bikes give it".into();
        deck["nodes"]["lesson-sub"]["list"] = json!([{ "kind": "bullet" }, { "kind": "bullet" }]);
        let h = of(&deck);
        let thin = h.findings.iter().find(|f| f.kind == Kind::Thin).expect("a thin finding");
        assert!(thin.says.starts_with("Slide 8 lists the fewest points"), "{}", thin.says);
        for slide in ["10", "11"] {
            assert!(!thin.says.contains(&format!("Slide {slide}")), "{}", thin.says);
            assert!(!thin.says.contains(&format!(" {slide} ")), "{}", thin.says);
        }
        // The cards are counted for the finding alone: balance reads the bullets, as Vzestup's does.
        let (read, _, set, _) =
            readings(&Deck::from_value(&deck).unwrap(), &slides(&Deck::from_value(&deck).unwrap())).unwrap();
        let at = |id: &str| read.iter().position(|s| s.id == id).unwrap();
        assert_eq!((set[at("recap")], set[at("plan")], set[at("cost")]), (3, 3, 0));
        assert_eq!(bullets(&read[at("recap")]), 0, "{}", read[at("recap")].body);
    }

    /// A deck whose slides state three or more figures offers the ask that compares them, as a model
    /// reads them (`docs/examples/health-calibration.md`, change 5): keynote's 31 outage hours a
    /// year against 286 to 2 is a reader's to weigh. A deck that states fewer offers none.
    #[test]
    fn a_deck_of_figures_offers_to_compare_them() {
        let h = of(&dogfood("keynote", "", |_| {}));
        let ask = h.compare.expect("keynote states its outage hours, its cost, and more");
        assert_eq!(ask, comparing());
        assert!(ask.question.contains("Change nothing until I say which is right"), "{}", ask.question);
        let plain = |slides: &mut Vec<answer::Slide>| {
            let words = |t: &str| t.chars().filter(|c| !c.is_ascii_digit()).collect::<String>();
            for s in slides.iter_mut() {
                s.slide_title = words(&s.slide_title);
                s.subtitle = words(&s.subtitle);
                s.key_bullets = s.key_bullets.iter().map(|b| words(b)).collect();
                s.chart_data = None;
            }
        };
        assert_eq!(
            of(&dogfood("keynote", "", plain)).compare,
            None,
            "a deck that states no figures is asked nothing of them"
        );
    }

    /// A finding names the step health read it at, which an editor shows (W2's editor dogfood):
    /// B1's slide 4 builds its 97 words over three steps, the first a headline alone, so its
    /// finding names the last; slide 12's turn is answered in its notes, which its first opens.
    #[test]
    fn a_finding_names_the_step_it_was_read_at() {
        let text = std::fs::read_to_string(root().join("tests/bench/b1.scaena/deck.json")).unwrap();
        let h = of(&serde_json::from_str(&text).unwrap());
        let on = |kind: Kind, slide: usize| {
            h.findings.iter().find(|f| f.kind == kind && f.slide == slide).map(|f| f.state.as_str())
        };
        assert_eq!(on(Kind::Dense, 4), Some("belief-2-line"), "{:#?}", h.findings);
        assert_eq!(on(Kind::Turn, 12), Some("belief-10"));
        assert_eq!(h.slides[3].state, "belief-2", "a slide is still shown by its first state");
    }

    /// A made deck that links an address neither its brief nor its sources name is warned of it,
    /// on the slide that shows it; one its brief or its sources name is not, nor a deck no
    /// narrative made (lift-up 20261025 SEC-4).
    #[test]
    fn a_link_neither_the_brief_nor_the_sources_name_is_a_warning() {
        let mut deck = climate();
        let states = deck["states"].as_array().unwrap();
        let k = states.len();
        let last = states[k - 1]["props"].as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        let id = last.iter().find(|id| deck["nodes"][id.as_str()]["text"].is_string()).unwrap().clone();
        let node = deck["nodes"][id.as_str()].as_object_mut().unwrap();
        node.remove("text");
        node.insert(
            "runs".into(),
            json!([
                { "text": "Read " },
                { "text": "portalderpower.org/playbook", "link": { "href": "https://portalderpower.org/playbook" } },
                { "text": " or write to " },
                { "text": "team@Tidewater.io", "link": { "href": "mailto:team@Tidewater.io" } },
            ]),
        );
        let said = |deck: &Value| -> Vec<(usize, String)> {
            (of(deck).findings.into_iter()).filter(|f| f.kind == Kind::Link).map(|f| (f.slide, f.says)).collect()
        };
        let n = of(&deck).slides.len();
        assert_eq!(
            said(&deck),
            [
                (
                    n,
                    format!(
                        "Slide {n} links portalderpower.org/playbook, which neither the brief nor the sources name."
                    )
                ),
                (n, format!("Slide {n} links team@Tidewater.io, which neither the brief nor the sources name.")),
            ]
        );
        // Named in the brief, or among the sources, an address is the person's own.
        deck["meta"]["narrative"]["brief"]["topic"] = json!("Climate, with the playbook at www.PortalDerPower.org");
        deck["meta"]["narrative"]["sources"] = json!([{ "name": "notes.md", "kind": "markdown", "url": "https://tidewater.io/notes", "bytes": 1, "truncated": false }]);
        assert_eq!(said(&deck), []);
        deck["meta"]["narrative"]["brief"]["topic"] = json!("Climate");
        deck["meta"].as_object_mut().unwrap().remove("narrative");
        assert_eq!(said(&deck), []);
        assert_eq!(host("https://www.Example.org:8443/a?b"), Some("example.org".into()));
        assert_eq!(host("mailto:a@b.io"), Some("b.io".into()));
        assert_eq!(host("ftp://x"), None);
    }

    /// Every beat of `deck` given `seconds`.
    fn timed(deck: &mut Value, seconds: f64) {
        for section in deck["spine"]["sections"].as_array_mut().unwrap() {
            for beat in section["beats"].as_array_mut().unwrap() {
                beat["duration"] = json!(seconds);
            }
        }
    }

    /// The minutes a brief asks are the talk's slot where it asks them (R1's MCP dogfood): a
    /// 2-minute talk read against a slot of up to 15 minutes for its slides found nothing, the
    /// slot's floor none; against the 15 minutes asked it is short, within a fifth short of them
    /// and a tenth past it is not. Pacing's score is Vzestup's count.
    #[test]
    fn a_talk_is_paced_by_the_minutes_its_brief_asks() {
        let mut deck = climate();
        let n = of(&deck).slides.len();
        let timing =
            |h: &Health| h.findings.iter().filter(|f| f.says.starts_with("The talk runs")).cloned().collect::<Vec<_>>();
        // About two minutes in all.
        timed(&mut deck, 120.0 / n as f64);
        for state in deck["states"].as_array_mut().unwrap() {
            state.as_object_mut().unwrap().remove("notes");
        }
        for section in deck["spine"]["sections"].as_array_mut().unwrap() {
            for beat in section["beats"].as_array_mut().unwrap() {
                beat.as_object_mut().unwrap().remove("notes");
            }
        }
        deck["meta"]["narrative"]["brief"]["length"] = json!("5-10 slides");
        let slots = of(&deck);
        assert!(timing(&slots).is_empty(), "the slot of up to 15 minutes has no floor: {:?}", slots.findings);
        deck["meta"]["narrative"]["brief"]["minutes"] = json!(15);
        let h = of(&deck);
        let found = timing(&h);
        assert_eq!(found.len(), 1, "{:?}", h.findings);
        assert!(
            found[0].says.starts_with("The talk runs about 2 minutes, short of the 15 minutes its brief asks."),
            "{}",
            found[0].says
        );
        assert!(found[0].ask.contains("until the talk fills the 15 minutes its brief asks"), "{}", found[0].ask);
        assert!(h.pacing.says.ends_with(", where its brief asks 15 minutes."), "{}", h.pacing.says);
        assert_eq!((h.score, h.pacing.score), (slots.score, slots.pacing.score), "the score is Vzestup's");
        // Within a fifth short of the minutes asked: nothing found.
        deck["meta"]["narrative"]["brief"]["minutes"] = json!(2);
        assert!(timing(&of(&deck)).is_empty());

        // At each edge of a 15-minute brief, the talk's minutes as they run: 12.0 is not short and
        // 11.9 is; 16.5 is not long and 16.6 is (the coordinator's bounds, ⅘ and 1.1×).
        deck["meta"]["narrative"]["brief"]["minutes"] = json!(15);
        let beats: Vec<String> = (deck["spine"]["sections"].as_array().unwrap().iter().enumerate())
            .flat_map(|(i, s)| {
                (0..s["beats"].as_array().unwrap().len()).map(move |j| format!("/spine/sections/{i}/beats/{j}"))
            })
            .collect();
        let mut lasting = |total: f64| {
            for beat in &beats {
                deck.pointer_mut(beat).unwrap()["duration"] = json!(1.0);
            }
            deck.pointer_mut(beats.last().unwrap()).unwrap()["duration"] = json!(total - (beats.len() - 1) as f64);
            timing(&of(&deck)).into_iter().map(|f| f.says).collect::<Vec<_>>()
        };
        assert!(lasting(12.0 * 60.0).is_empty(), "12.0 minutes is not short");
        let short = lasting(11.9 * 60.0);
        assert!(short.len() == 1 && short[0].contains("short of the 15 minutes its brief asks"), "{short:?}");
        assert!(lasting(16.5 * 60.0).is_empty(), "16.5 minutes is not long");
        let long = lasting(16.6 * 60.0);
        assert!(long.len() == 1 && long[0].contains("past the 15 minutes its brief asks"), "{long:?}");
    }

    /// The plan a bend asks in words is the one its fix writes (W2's review of #1186): rounded
    /// toward the words, a person who types the plan the ask says writes what the fix would.
    #[test]
    fn a_bends_ask_says_the_plan_its_fix_writes() {
        let base = dogfood("lecture", "", |_| {});
        let states: Vec<String> = of(&base).slides.iter().map(|s| s.state.clone()).collect();
        let mut checked = 0;
        for state in &states {
            for plan in [0.05, 0.15, 0.3, 0.45, 0.6, 0.75, 0.9, 0.98] {
                let mut deck = base.clone();
                planned(&mut deck, state, plan);
                for f in of(&deck).findings.into_iter().filter(|f| f.kind == Kind::Arc && f.state == *state) {
                    let (Some(at), Some(fix)) = (f.ask.find(": plan it at "), f.fix.as_ref()) else { continue };
                    let said = &f.ask[at + ": plan it at ".len()..][..4];
                    assert_eq!(format!("Plan it at {said}"), fix.says, "{}", f.ask);
                    checked += 1;
                }
            }
        }
        assert!(checked > 0, "a bend that asks a plan its words carry");
    }

    /// A plan written to two places is rounded toward the words it is held to, so a plan held to
    /// the most they carry is found within it (agent dogfood 8: 0.7776 written as 0.78, 0.2524
    /// from words that read 0.5276, asked the slide's words to carry it).
    #[test]
    fn a_plan_held_to_its_words_stays_within_them_once_written() {
        assert_eq!(held(0.5276 + INTENSITY_GAP_THRESHOLD, Some(0.5276)), 0.77);
        assert_eq!(held(0.4724 - INTENSITY_GAP_THRESHOLD, Some(0.4724)), 0.23);
        assert_eq!((held(0.5, None), held(0.914, Some(0.67)), held(0.6, Some(0.6))), (0.5, 0.91, 0.6));
        // On a hundredth but for the float's last bits, it is that hundredth (W2's review of #1186).
        assert_eq!((held(0.54 - INTENSITY_GAP_THRESHOLD, Some(0.54)), held(0.04 + 0.25, Some(0.04))), (0.29, 0.29));
        for k in 0..=1000 {
            let w = f64::from(k) / 1000.0;
            for asked in [w + INTENSITY_GAP_THRESHOLD, w - INTENSITY_GAP_THRESHOLD] {
                let asked = asked.clamp(0.0, 1.0);
                assert!((held(asked, Some(w)) - w).abs() <= INTENSITY_GAP_THRESHOLD + 1e-9, "{asked} for {w}");
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

    /// What the model's look let go is a finding each, which its fix dismisses (lift-up USE-4):
    /// the record taken away, held to it as read, the highest first; the deck then says nothing
    /// of it, and a fix gone stale is refused.
    #[test]
    fn what_the_look_let_go_is_dismissed_by_its_fix() {
        let mut deck = climate();
        let refused = json!([
            { "slide": 2, "what": "a shorter title", "why": "Its text runs past its place." },
            { "slide": 3, "what": "another layout", "why": "Two texts overlap." }
        ]);
        deck.pointer_mut("/meta/narrative").unwrap()["refused"] = refused.clone();
        let looks =
            |deck: &Value| -> Vec<Finding> { of(deck).findings.into_iter().filter(|f| f.kind == Kind::Look).collect() };
        let found = looks(&deck);
        assert_eq!(found.iter().map(|f| f.slide).collect::<Vec<_>>(), [2, 3]);
        let fix = |f: &Finding| f.fix.clone().expect("a fix").patch;
        assert_eq!(found[0].fix.as_ref().unwrap().says, "Dismiss it");
        // One let go for its figures as the look was read is no slide lint found worse (W1
        // dogfood 9).
        let mut figured = deck.clone();
        figured.pointer_mut("/meta/narrative").unwrap()["refused"] = json!([{
            "slide": 2, "what": "the title \u{201c}Sunday visits\u{201d}",
            "why": "it drops 2,475, which the slide wrote", "by": "figures"
        }]);
        let said = &looks(&figured)[0].says;
        assert_eq!(
            said,
            "Slide 2 was kept as made: the model asked for the title \u{201c}Sunday visits\u{201d}, and it drops 2,475, which the slide wrote."
        );
        assert_eq!(
            fix(&found[1]),
            [
                json!({ "op": "test", "path": "/meta/narrative/refused/1", "value": refused[1] }),
                json!({ "op": "remove", "path": "/meta/narrative/refused/1" })
            ]
        );
        // Each taken in turn, as the slides give them: the other still finds its own record.
        let apply = |deck: &mut Value, patch: &[Value]| -> bool {
            let mut was = deck.clone();
            for op in patch {
                let path = op["path"].as_str().unwrap();
                match op["op"].as_str().unwrap() {
                    "test" if was.pointer(path) != Some(&op["value"]) => return false,
                    "remove" => {
                        let (parent, at) = path.rsplit_once('/').unwrap();
                        was.pointer_mut(parent).unwrap().as_array_mut().unwrap().remove(at.parse().unwrap());
                    }
                    _ => {}
                }
            }
            *deck = was;
            true
        };
        let stale = fix(&found[1]);
        assert!(apply(&mut deck, &fix(&found[0])), "the first slide's dismissed");
        assert_eq!(looks(&deck).iter().map(|f| f.slide).collect::<Vec<_>>(), [3]);
        let now = fix(&looks(&deck)[0]);
        assert_eq!(now[1]["path"], "/meta/narrative/refused/0", "read again, the record left is the first");
        assert!(apply(&mut deck, &now));
        assert!(looks(&deck).is_empty());
        assert_eq!(deck["meta"]["narrative"]["refused"], json!([]));
        assert!(!apply(&mut deck, &stale), "a fix gone stale is refused");
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
        // Its first turn its words do not make: none is found out of its divider, so slide 5's.
        let turn = before.findings.iter().find(|f| f.kind == Kind::Turn).expect("a turn its words do not make").clone();
        let k = turn.slide;
        assert_eq!(
            turn.ask,
            format!(
                "Open slide {k}'s notes with the line that carries the talk from slide {b}, in a word of slide {b}'s, such as \u{201c}hottest\u{201d} or \u{201c}record\u{201d}: a connective alone (\u{201c}So\u{201d}, \u{201c}But\u{201d}) carries nothing across.",
                b = k - 1
            )
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

        // The word the ask names answers the turn (R1's MCP dogfood).
        let h = noted(&mut deck, "The hottest year is where we go next.".to_string());
        assert!(!found(&h), "the word named bridges: {:#?}", h.findings);
        let h = noted(&mut deck, format!("From {carried} we turn to what comes next.\nThen the rest."));
        assert!(!found(&h), "{:#?}", h.findings);
        assert!(h.coherence.says.contains("; the notes bridge "), "{}", h.coherence.says);
        assert_eq!(h.coherence.score, before.coherence.score, "no figure moves");
        // A figure's point ends no sentence: the opening runs on past "3.5" to the word carried.
        let h = noted(&mut deck, format!("Retention rose 3.5% since {carried}. Then the rest."));
        assert!(!found(&h), "{:#?}", h.findings);
        assert_eq!(opening("Rose 3.5% since then. Later."), "Rose 3.5% since then.");
        assert_eq!(opening("\n  - Up 2.1x!\nMore"), "- Up 2.1x!");
        let h = noted(&mut deck, format!("Plainly onward. Later, {carried}."));
        assert!(found(&h), "a word past the notes' opening carries nothing over");
        // A connective alone carries nothing over, as the ask says (W1's dogfood 5).
        for connective in ["So, onward.", "But that's not all.", "That's why it matters."] {
            assert!(found(&noted(&mut deck, connective.to_string())), "{connective}");
        }
        if let Some(shared) = shared {
            let h = noted(&mut deck, format!("{shared} again, and onward."));
            assert!(found(&h), "a word both slides use carries nothing over");
        }
    }

    /// The notes health reads are those a presenter reads (editor dogfood 3): a made deck keeps
    /// each slide's on its beat, which the editor's Notes field shows and `set_state` writes, so a
    /// bridge typed into the field opens what the presenter reads and answers the turn; the state
    /// keeps no notes of its own. A state's own notes are read in place of its beat's, as the
    /// presenter reads them.
    #[test]
    fn a_bridge_written_in_the_notes_field_answers_the_turn() {
        struct NoFiles;
        impl scaena_core::validate::BundleFiles for NoFiles {
            fn exists(&self, _: &str) -> bool {
                true
            }
            fn read_text(&self, _: &str) -> Option<String> {
                None
            }
        }
        let deck = climate();
        let before = of(&deck);
        let turn = before.findings.iter().find(|f| f.kind == Kind::Turn).expect("a turn its words do not make").clone();
        let k = turn.slide;
        let d = Deck::from_value(&deck).unwrap();
        let read = readings(&d, &slides(&d)).unwrap().0;
        let words = |s: &SlideForHealth| health::coherence::keywords(&format!("{} {}", s.title, s.body));
        let carried = words(&read[k - 2]).difference(&words(&read[k - 1])).next().cloned().expect("a word before");
        // What the Notes field shows: the beat's, which the presenter reads.
        let shown = scaena_core::spine::notes(&d, &turn.state).unwrap().to_string();
        assert!(!shown.is_empty() && d.states.iter().find(|s| s.id == turn.state).unwrap().notes.is_none());
        let bridged = format!("From {carried} we turn to what comes next.\n{shown}");
        let op = json!({ "op": "set_state", "id": turn.state, "prop": "notes", "value": bridged });
        let made = scaena_core::patch::compile(&deck, &[op], &NoFiles).unwrap().doc;
        let m = Deck::from_value(&made).unwrap();
        assert_eq!(scaena_core::spine::notes(&m, &turn.state), Some(bridged.as_str()), "the presenter reads it");
        assert!(m.states.iter().find(|s| s.id == turn.state).unwrap().notes.is_none(), "written on the beat");
        let h = of(&made);
        assert!(!h.findings.iter().any(|f| f.kind == Kind::Turn && f.state == turn.state), "{:#?}", h.findings);

        // A state's own notes are read in place of its beat's, as the presenter reads them.
        let mut own = deck.clone();
        let at = own["states"].as_array().unwrap().iter().position(|s| s["id"] == turn.state.as_str()).unwrap();
        own["states"][at]["notes"] = json!(format!("From {carried} onward."));
        let beat = beat_of(&own, &turn.state);
        own.pointer_mut(&beat).unwrap()["notes"] = json!("We turn to what comes next.");
        let h = of(&own);
        assert!(!h.findings.iter().any(|f| f.kind == Kind::Turn && f.state == turn.state), "{:#?}", h.findings);
    }

    /// A turn out of an agenda is not found (dogfood pass 10): the agenda names the parts to
    /// come, and the slide after it opens the first, sharing few words with a list of them. The
    /// same turn out of any other slide is.
    #[test]
    fn a_turn_out_of_an_agenda_is_not_found() {
        let mut deck = climate();
        let before = of(&deck);
        let turn = before.findings.iter().find(|f| f.kind == Kind::Turn).expect("a turn its words do not make").clone();
        let d = Deck::from_value(&deck).unwrap();
        let first = slides(&d)[turn.slide - 2].states[0];
        let found = |h: &Health| h.findings.iter().any(|f| f.kind == Kind::Turn && f.state == turn.state);
        assert!(found(&before));
        deck["states"][first]["layout"] = json!("agenda");
        let h = of(&deck);
        assert!(!found(&h), "{:#?}", h.findings);
        deck["states"][first]["layout"] = json!("agenda-photo");
        assert!(!found(&of(&deck)));
        deck["states"][first]["layout"] = json!("bullets");
        assert!(found(&of(&deck)), "a turn out of a slide that lists no parts to come");
    }

    /// A turn out of a divider is not found, as an agenda's is not (the calibration against a
    /// reader): the divider names the part its next slide opens. On the climate talk, slides 4,
    /// 13, and 17 open their parts after a divider; a turn into a divider is found as before.
    #[test]
    fn a_turn_out_of_a_divider_is_not_found() {
        let deck = climate();
        let h = of(&deck);
        let d = Deck::from_value(&deck).unwrap();
        let sl = slides(&d);
        let layout = |k: usize| d.states[sl[k - 1].states[0]].layout.clone().unwrap_or_default();
        let turned = |k: usize| h.findings.iter().any(|f| f.kind == Kind::Turn && f.slide == k);
        for k in [4, 13, 17] {
            assert_eq!(layout(k - 1), "section", "slide {} is a divider", k - 1);
            assert!(!turned(k), "slide {k} opens the part its divider names: {:#?}", h.findings);
        }
        // Into a divider: slide 21's turn into slide 22, as before.
        assert_eq!(layout(22), "section");
        assert!(turned(22), "{:#?}", h.findings);
        // The same turn out of a slide that names no part is found.
        let mut plain = deck.clone();
        plain["states"][sl[3 - 1].states[0]]["layout"] = json!("bullets");
        assert!(of(&plain).findings.iter().any(|f| f.kind == Kind::Turn && f.slide == 4));
        assert_eq!(h.coherence.score, None, "no figure moves");
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

    /// The deck's last slide closes the talk whatever its layout (R1's agent dogfood 2): the Cove
    /// Island ask, written as a Closing with two items and so set as a list, headed by a topic. It
    /// is not asked for its point; the same headline on a slide before the close is.
    #[test]
    fn the_last_slide_is_not_asked_for_its_point() {
        let ask = |slide: &mut answer::Slide| {
            slide.slide_title = "The midday sailing from 2026".into();
            slide.layout_suggestion = "Closing".into();
            slide.key_bullets = vec![
                "Approve $1.1M a year for a fourth daily sailing".into(),
                "Review ridership and on-time performance after twelve months".into(),
            ];
        };
        let labelled = |h: &Health, k: usize| h.findings.iter().any(|f| f.kind == Kind::Label && f.slide == k);
        let closed = dogfood("fleet", "", |slides| ask(slides.last_mut().unwrap()));
        let h = of(&closed);
        let n = h.slides.len();
        assert!(!labelled(&h, n), "{:#?}", h.findings.iter().filter(|f| f.slide == n).collect::<Vec<_>>());
        // Before the close, the same headline is a topic's.
        let inside = dogfood("fleet", "", |slides| ask(&mut slides[6]));
        assert!(
            labelled(&of(&inside), 7),
            "{:#?}",
            of(&inside).findings.iter().filter(|f| f.slide == 7).collect::<Vec<_>>()
        );
    }

    /// A headline is judged by the words the slide shows (W2's editor dogfood, Edit its headline):
    /// fleet's slide 6 keeps its beat's claim, "Routes in minutes, not hours", while its headline
    /// shows "0.8 hours of planning a day by week five", a figure before its meaning, so it names
    /// no topic. A headline typed over a claim that states a point is read as it now shows. No
    /// figure moves.
    #[test]
    fn a_headline_is_judged_by_the_words_the_slide_shows() {
        let label = |h: &Health, k: usize| h.findings.iter().find(|f| f.kind == Kind::Label && f.slide == k).cloned();
        let fleet = dogfood("fleet", "", |_| {});
        let before = of(&fleet);
        assert!(label(&before, 6).is_none(), "{:#?}", label(&before, 6));
        // Grant's slide 8: its claim states a point, and its headline, typed over, names a topic.
        let mut deck = dogfood("grant", "", |_| {});
        fn retype(v: &mut Value, from: &str, to: &str) -> bool {
            match v {
                Value::String(s) if s == from => {
                    *s = to.to_string();
                    true
                }
                Value::Array(a) => a.iter_mut().any(|v| retype(v, from, to)),
                Value::Object(o) => o.values_mut().any(|v| retype(v, from, to)),
                _ => false,
            }
        }
        let was = of(&deck);
        assert!(label(&was, 8).is_none(), "{:#?}", label(&was, 8));
        assert!(
            retype(&mut deck["nodes"], "Food and travel ran 11% over plan", "Pricing by region"),
            "the headline's text"
        );
        let typed = of(&deck);
        let f = label(&typed, 8).expect("the headline shown names its topic");
        assert!(f.says.contains("\u{201C}Pricing by region\u{201D}"), "{}", f.says);
        assert_eq!(typed.score, was.score, "no figure moves");
    }

    /// A content slide's headline that names its topic is offered its point (lift-up 10-25b
    /// FUNC-4): an info finding with no fix, in an English deck, never on the cover, a divider,
    /// or the close, and never on a question. The cases measured on the dogfood decks and the
    /// examples hold the heuristic to precision: a false "your headline is a topic" costs more
    /// than a label missed.
    /// A headline that names what its slide holds by design is no topic to restate: a term, the
    /// voices quoted, the exercise set, laid out or planned as one, now that a word or two is
    /// judged (W2's outlined calibration). A card's label and a colon ("People: who was paged")
    /// is no claim under a topic. Training's list headline is offered its point, an info.
    #[test]
    fn a_topic_by_design_is_not_asked_for_its_point() {
        let label = |h: &Health, k: usize| h.findings.iter().find(|f| f.kind == Kind::Label && f.slide == k).cloned();
        let training = of(&dogfood("training", "", |_| {}));
        // Its Definition ("Blameless postmortem") and its Quiz, set as cards ("Quick quiz").
        assert!(label(&training, 3).is_none() && label(&training, 10).is_none(), "{:#?}", training.findings);
        let five = label(&training, 5).expect("the list's headline names its topic");
        assert_eq!(five.severity, Severity::Info, "a card's label is no claim: {five:#?}");
        // Lecture's Definition, "Conditional probability".
        let lecture = of(&dogfood("lecture", "", |_| {}));
        assert!(label(&lecture, 4).is_none(), "{:#?}", label(&lecture, 4));
        // Appeal's testimonials, "What families tell us", and its scorecard, which is a topic.
        let appeal = of(&dogfood("appeal", "", |_| {}));
        assert!(label(&appeal, 9).is_none() && label(&appeal, 7).is_some(), "{:#?}", appeal.findings);
    }

    #[test]
    fn a_headline_that_names_its_topic_is_offered_its_point() {
        // Labels, as the charts example and the deepdive deck wrote them.
        assert!(labels("Q3 by product"));
        assert!(labels("Faster checkout, cheaper database, more orders"));
        // Points, each with its verb: the appeal's, the training's imperative, the charts'.
        assert!(!labels("Families wait longer every year"));
        assert!(!labels("Before you publish, check these"));
        assert!(!labels("Revenue grew every quarter"));
        assert!(!labels("It\u{2019}s a record quarter"));
        // A question and a long line are not judged.
        assert!(!labels("Why did churn rise this year?"));
        assert!(!labels("A wet start to the week, then sun"));
        // A word or two names a topic, and so does a clause opened by how or what, its verb
        // inside it (W2's outlined calibration: a reader counted "Results", "The Ask", and
        // "How It Works", and health found none of them).
        for topic in ["Results", "The Ask", "How It Works", "What families tell us", "Why now", "Our ask"] {
            assert!(labels(topic), "{topic}");
        }
        // An ask opened by its imperative, or a verb after "to", states a point, though the
        // verb is none [`VERBS`] lists; a noun that is also one, in two words, is a topic.
        for stated in
            ["Vote Tuesday to fund the pilot", "Fund the midday sailing from 2026", "A plan to expand by June"]
        {
            assert!(!labels(stated), "{stated}");
        }
        assert!(labels("Pilot results"));
        assert!(!labels("Our plan works"));
        // Points stated in a past the list need not spell out, or an irregular one it does.
        for stated in [
            "Margins expanded in every region",
            "Gap widened every year",
            "Prices lowered across regions",
            "Rivals outsold us twice",
            "Share slid in Q3",
            "Churn collapsed after onboarding",
        ] {
            assert!(!labels(stated), "{stated}");
        }
        // A gerund names a topic.
        assert!(labels("Pricing by region"));
        // An irregular past states a point (the calibration against a reader): the grant's.
        assert!(!labels("We rebuilt 52 miles, not 40"));
        assert!(!labels("6,820 volunteer hours rebuilt it"));
        assert!(!labels("Crews swept every trail by June"));
        // A verb's "-s" ending a list of things is a plural (W1's dogfood 6); a verb before it
        // still states a point.
        assert!(labels("Weekend hours and online holds"));
        assert!(labels("Late fees or holds"));
        assert!(!labels("Weekend hours grow and holds"));
        assert!(!labels("The library holds weekend hours"));

        let deck = climate();
        let h = of(&deck);
        let layout = |id: &str| {
            (deck["states"].as_array().unwrap().iter())
                .find(|s| s["id"] == id)
                .and_then(|s| s["layout"].as_str())
                .map(str::to_string)
        };
        let labelled = |h: &Health| h.findings.iter().filter(|f| f.kind == Kind::Label).cloned().collect::<Vec<_>>();
        // The climate deck as Narrato wrote it: the headlines it names a topic by, and none else.
        let base = labelled(&h);
        let named: Vec<&str> = base.iter().map(|f| h.slides[f.slide - 1].title.as_str()).collect();
        assert_eq!(
            named,
            [
                "Who Emits: History Versus Today",
                "Four Years of Records",
                "The Path to Net Zero",
                "Pledges Versus the 1.5°C Path",
                // "Next Steps for Our Team" is planned as Next Steps: part of the close, which
                // makes no point of its own to state (health read of a Spanish deck).
            ],
            "{base:#?}"
        );
        // A content slide headed by its own words: one a figure leads is headed by the figure.
        let d = Deck::from_value(&deck).unwrap();
        let snaps = resolve_states(&d).unwrap();
        let shown = |i: usize| scaena_core::reading::headline(&d, &snaps[*slides(&d)[i].states.last().unwrap()]);
        let content = (1..h.slides.len() - 1)
            .find(|&i| {
                layout(&h.slides[i].state)
                    .is_none_or(|l| !FRAMING.contains(&l.as_str()) && !CLOSING.contains(&l.as_str()))
                    && !base.iter().any(|f| f.slide == i + 1)
                    && shown(i).as_deref() == Some(h.slides[i].title.as_str())
            })
            .expect("a content slide");
        let retitled = |deck: &Value, i: usize, to: &str| -> Value {
            let text = serde_json::to_string(deck).unwrap();
            let was = serde_json::to_string(&h.slides[i].title).unwrap();
            serde_json::from_str(&text.replace(&was, &serde_json::to_string(to).unwrap())).unwrap()
        };

        let titled = retitled(&deck, content, "Emissions by sector");
        let found: Vec<Finding> = labelled(&of(&titled)).into_iter().filter(|f| !base.contains(f)).collect();
        assert_eq!(found.len(), 1, "{found:#?}");
        let k = content + 1;
        assert_eq!((found[0].slide, found[0].severity, found[0].fix.clone()), (k, Severity::Info, None));
        assert_eq!(
            found[0].says,
            format!(
                "Slide {k}'s headline names its topic, \u{201C}Emissions by sector\u{201D}, where it could state its point."
            )
        );
        assert_eq!(
            found[0].ask,
            format!("Restate slide {k}'s headline as the point it makes: what should the room believe after it?")
        );
        // The cover frames the talk, and a deck in another language is not read for it.
        assert_eq!(labelled(&of(&retitled(&deck, 0, "Emissions by sector"))), base);
        let mut german = titled.clone();
        german["meta"]["narrative"]["brief"]["language"] = json!("German");
        assert!(labelled(&of(&german)).is_empty());
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

    /// The close is the slides that end the deck (W2's dogfood): where a model wrote the slides'
    /// beats otherwise than the outline, and the last outlined beat's slides stand before the end,
    /// the big idea's finding names the deck's last run of closing slides, not that beat's.
    #[test]
    fn the_close_is_the_slides_that_end_the_deck() {
        let mut deck = climate();
        let h = of(&deck);
        let n = h.slides.len();
        let written: Vec<usize> = (h.slides.iter())
            .map(|s| {
                let at = format!("{}/media/narrato/beatIndex", beat_of(&deck, &s.state));
                deck.pointer(&at).and_then(Value::as_u64).unwrap() as usize
            })
            .collect();
        let last_beat = h.beats.len() - 1;
        let beats: Vec<Value> =
            (h.beats.iter()).map(|name| json!({ "name": name, "claim": "", "slides": 1 })).collect();
        deck["meta"]["narrative"]["outline"] =
            json!({ "bigIdea": "Zebras orchestrate symphonies underwater", "beats": beats });
        // Each slide of the last beat but the first moved to the first beat: the last beat's slides
        // now stand before the end.
        let of_last: Vec<usize> = (0..n).filter(|&i| written[i] == last_beat).collect();
        assert!(of_last.contains(&(n - 1)), "the climate deck's last beat ends it");
        for &i in &of_last[1..] {
            let at = format!("{}/media/narrato/beatIndex", beat_of(&deck, &h.slides[i].state));
            *deck.pointer_mut(&at).unwrap() = json!(0);
        }
        let found = of(&deck);
        let big = found.findings.iter().find(|f| f.kind == Kind::BigIdea).expect("the big idea is not landed");
        // The deck's last run of closing slides, Questions then Closing, ends at its last slide.
        assert!(
            big.says.starts_with(&format!("The close, slides {}\u{2013}{n}, never lands", big.slide)),
            "{}",
            big.says
        );
        assert_ne!(big.slide, of_last[0] + 1, "not the last outlined beat's first slide, which stands before the end");
    }

    /// A headline closes the talk only where it thanks the room or opens the floor in three words
    /// or fewer (the calibration against a reader): launch's FAQ, "Questions every support lead
    /// asks us first", answers them in the talk, so it is no calm close and its cooler plan is
    /// found; headed "Questions?", it closes, and is not.
    #[test]
    fn a_slide_headed_with_questions_closes_only_where_it_opens_the_floor() {
        let cooler = |deck: &Value| {
            of(deck)
                .findings
                .into_iter()
                .any(|f| f.kind == Kind::Arc && f.slide == 13 && f.says.contains("planned cooler"))
        };
        let faq = dogfood("launch", "", |_| {});
        assert!(cooler(&faq), "{:#?}", of(&faq).findings);
        let floor = dogfood("launch", "", |slides| slides[12].slide_title = "Questions?".into());
        assert!(!cooler(&floor), "{:#?}", of(&floor).findings);
    }

    /// A bend asks for a plan its slide's words can carry (R1's agent dogfood 2): planned to the
    /// curve, words that read far from it would be an intensity gap in the bend's place. The plan
    /// asked is the nearest the curve within the gap the words allow, with a fix that writes it;
    /// where that still bends, the move comes first and no plan is asked. No figure moves.
    /// A plan is offered only where it raises fidelity as the deck stands (W1's dogfood 17): the
    /// warp aligns the slides afresh with each plan, so a plan nearer its place can set the talk
    /// further from its curve. No figure moves; only which fixes are offered.
    #[test]
    fn a_plan_is_offered_only_where_it_raises_fidelity() {
        let fidelity = |h: &Health| h.fidelity.score.unwrap();
        let arc =
            |h: &Health, slide: usize| h.findings.iter().find(|f| f.kind == Kind::Arc && f.slide == slide).cloned();
        // B1 plans nothing, so each slide is read on the tent: slide 12's plan, 0.70, would stand
        // under the tent's 0.73 at its place. Taking each fix offered in turn, fidelity never falls,
        // and slide 12 is asked for words or its place, with no plan.
        let text = std::fs::read_to_string(root().join("tests/bench/b1.scaena/deck.json")).unwrap();
        let mut deck: Value = serde_json::from_str(&text).unwrap();
        let mut h = of(&deck);
        let twelve = arc(&h, 12).expect("slide 12 is found");
        assert!(twelve.fix.is_none(), "{twelve:#?}");
        assert!(
            twelve.ask.contains(
                "would set the talk further from its curve as the other slides stand: give it words that carry"
            ),
            "{}",
            twelve.ask
        );
        let mut taken = 0;
        while let Some(fix) = h.findings.iter().filter(|f| f.kind == Kind::Arc).find_map(|f| f.fix.clone()) {
            scaena_core::patch::apply(&mut deck, &fix.patch).unwrap();
            let after = of(&deck);
            assert!(
                fidelity(&after) >= fidelity(&h) && after.score >= h.score,
                "{} took fidelity {} to {}",
                fix.says,
                fidelity(&h),
                fidelity(&after)
            );
            (h, taken) = (after, taken + 1);
        }
        assert_eq!((taken, fidelity(&h)), (4, 86));
        // The QBR's slide 8: the plan its words carry, 0.37, would lower fidelity, and so would its
        // place's 0.36, so none is offered and the slides around it are asked for too.
        let deck = dogfood("qbr", "", |_| {});
        let h = of(&deck);
        let eight = arc(&h, 8).expect("slide 8 bends");
        assert!(
            eight.fix.is_none() && eight.ask.contains("but planned alone, even at 0.36, it would set the talk further from its curve as the other slides stand: move it"),
            "{eight:#?}"
        );
        let mut planned = deck.clone();
        scaena_core::patch::apply(
            &mut planned,
            &[json!({ "op": "replace", "path": eight.path.clone().unwrap(), "value": 0.37 })],
        )
        .unwrap();
        assert!(fidelity(&of(&planned)) < fidelity(&h), "the plan withheld lowers fidelity");
        // Slide 7's still raises it, and is offered.
        assert!(arc(&h, 7).and_then(|f| f.fix).is_some_and(|f| f.says == "Plan it at 0.46"));
    }

    #[test]
    fn a_bend_asks_for_a_plan_its_words_can_carry() {
        // Lecture's slide 11 is planned at 0.50 where its place calls for 0.85; its words read 0.50.
        let deck = dogfood("lecture", "", |_| {});
        let h = of(&deck);
        let bend = h.findings.iter().find(|f| f.kind == Kind::Arc && f.slide == 11).expect("slide 11 bends").clone();
        assert!(
            bend.ask.contains("calls for (0.85), and its words read 0.50: plan it at 0.75 by its Intensity")
                && bend.ask.contains("as near its place as its words carry, or move it away from the climax."),
            "{}",
            bend.ask
        );
        let fix = bend.fix.clone().expect("the plan asked, as a fix");
        assert_eq!(fix.says, "Plan it at 0.75");
        assert_eq!(fix.patch, vec![json!({ "op": "replace", "path": bend.path.clone().unwrap(), "value": 0.75 })]);
        // Taken, the slide neither bends nor reads far from its plan, and no figure moved to say so.
        let mut taken = deck.clone();
        scaena_core::patch::apply(&mut taken, &fix.patch).unwrap();
        let after = of(&taken);
        let state = &bend.state;
        assert!(
            !after.findings.iter().any(|f| &f.state == state && matches!(f.kind, Kind::Arc | Kind::Intensity)),
            "{:#?}",
            after.findings.iter().filter(|f| &f.state == state).collect::<Vec<_>>()
        );
        // A plan its words carry where they read near the curve: the curve's call, with its fix.
        let near = h.findings.iter().find(|f| f.kind == Kind::Arc && f.slide == 13).expect("slide 13 bends");
        assert!(near.ask.contains("(0.75): plan it more intense by its Intensity"), "{}", near.ask);
        assert_eq!(near.fix.as_ref().map(|f| f.says.as_str()), Some("Plan it at 0.75"));
        // Words further from the curve than any plan they carry: the move first, and no plan asked.
        // Slide 3's place calls for 0.25; heated words read past 0.75.
        let far = dogfood("lecture", "", |slides| {
            slides[2].key_bullets =
                vec!["Shocking! Urgent! You must act now!".into(), "Crisis! Danger! Never again!".into()];
            slides[2].speaker_notes = "You! Now! Urgent! Shocking! Every single one of you must act!".into();
        });
        let h = of(&far);
        let read = h.slides[2].read.expect("its words read");
        assert!((read - h.slides[2].curve).abs() > 0.5, "{read} against {}", h.slides[2].curve);
        let bend = h.findings.iter().find(|f| f.kind == Kind::Arc && f.slide == 3).expect("slide 3 bends");
        assert!(
            bend.ask.ends_with("further from its place than a plan they carry can come: move it nearer the climax, in its beat, Orientation."),
            "{}",
            bend.ask
        );
        assert_eq!(bend.fix, None);
    }

    /// A talk timed by its words alone says how long its notes run as written, where every slide
    /// has notes (R1's agent dogfood 2), as information: notes may be cues, so no finding.
    #[test]
    fn a_talk_with_notes_on_every_slide_says_how_long_they_run() {
        let h = of(&dogfood("keynote", "", |_| {}));
        assert!(
            h.pacing.says.ends_with("; no beat is timed. Its notes as written run about 4 minutes."),
            "{}",
            h.pacing.says
        );
        assert!(!h.findings.iter().any(|f| f.kind == Kind::Pacing && f.says.contains("short of")));
        let one_bare = of(&dogfood("keynote", "", |slides| slides[3].speaker_notes.clear()));
        assert!(one_bare.pacing.says.ends_with("; no beat is timed."), "{}", one_bare.pacing.says);
    }

    /// A dogfood deck (`tests/fixtures/dogfood`) as its writer wrote it, its slides first changed
    /// by `change`, made in Dusk, and its outline the arc's beats as its slides name them, no
    /// claim settled, and `big_idea`.
    fn dogfood(name: &str, big_idea: &str, change: impl FnOnce(&mut Vec<answer::Slide>)) -> Value {
        grounded_dogfood(name, big_idea, Vec::new(), change)
    }

    /// `dogfood`, the deck made from `sources`, so each slide's `sourceRef` is cited in its beat's
    /// evidence as a made deck cites it.
    fn grounded_dogfood(
        name: &str,
        big_idea: &str,
        sources: Vec<scaena_narrato::Provenance>,
        change: impl FnOnce(&mut Vec<answer::Slide>),
    ) -> Value {
        let path = root().join(format!("tests/fixtures/dogfood/{name}.slides.json"));
        let mut written = answer::slides(&std::fs::read_to_string(path).unwrap()).unwrap();
        change(&mut written.slides);
        let mut names: Vec<String> = Vec::new();
        for s in &written.slides {
            let k = s.beat_index.unwrap_or_default();
            if names.len() <= k {
                names.resize(k + 1, String::new());
            }
            names[k] = s.beat_name.clone().unwrap_or_default();
        }
        let narrative = Narrative {
            arc: "Hero's Journey".into(),
            arc_reasoning: String::new(),
            executive_summary: written.executive_summary,
            slides: written.slides,
            truncated: false,
            icons: Default::default(),
            sources,
            outline: None,
            brief: Brief { topic: name.into(), length: "11-15 slides".into(), ..Brief::default() },
        };
        let dusk = std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap();
        let theme: Theme = serde_json::from_str(&dusk).unwrap();
        let mut deck = deck(&narrative, &theme, &Pictures::new()).unwrap().deck;
        let beats: Vec<Value> = names.iter().map(|n| json!({ "name": n, "claim": "", "slides": 1 })).collect();
        deck["meta"]["narrative"]["outline"] = json!({ "bigIdea": big_idea, "beats": beats });
        deck
    }

    /// A turn's ask in a deck in another language names the part it opens by the model's name for
    /// the beat, never the arc's English one the settled outline carries (W1's dogfood 10: "Opens
    /// Existing Approaches" in a French deck whose beat was "Approches existantes"); an English
    /// deck's asks name the outline's, as before.
    #[test]
    fn a_french_turn_names_its_part_in_french() {
        let arc = scaena_narrato::arcs::arcs().iter().find(|a| a.arc == "Hero's Journey").unwrap();
        let english: Vec<&str> = arc.beat_names();
        let opened = |deck: &Value| -> Vec<String> {
            (of(deck).findings.iter())
                .filter(|f| f.kind == Kind::Turn && f.says.contains(" opens "))
                .map(|f| f.says.clone())
                .collect()
        };
        let mut deck = dogfood("route9", "", |slides| {
            for s in slides.iter_mut() {
                s.beat_name = Some(format!("Partie {}", s.beat_index.unwrap_or_default() + 1));
            }
        });
        let beats: Vec<Value> = english.iter().map(|n| json!({ "name": n, "claim": "", "slides": 1 })).collect();
        deck["meta"]["narrative"]["outline"]["beats"] = json!(beats);
        let said = opened(&deck);
        assert!(!said.is_empty(), "{:#?}", of(&deck).findings);
        assert!(said.iter().all(|s| english.iter().any(|n| s.contains(&format!(" opens {n} ")))), "{said:#?}");
        deck["meta"]["narrative"]["brief"]["language"] = json!("French");
        let said = opened(&deck);
        assert!(!said.is_empty());
        assert!(said.iter().all(|s| s.contains(" opens Partie ")), "{said:#?}");
    }

    /// Where the slides balance counts list under two points on average, the content slides that
    /// list fewest are named, so balance's line has a slide to act on (R1's MCP dogfood): fleet
    /// with slide 4 thinned to one bar. A deck at the floor finds none.
    #[test]
    fn the_slides_holding_balance_down_are_named() {
        let full = of(&dogfood("fleet", "", |_| {}));
        assert!(!full.findings.iter().any(|f| f.kind == Kind::Thin), "{:#?}", full.findings);
        let thinned = |slides: &mut Vec<answer::Slide>| slides[3].key_bullets.truncate(1);
        let h = of(&dogfood("fleet", "", thinned));
        assert!(h.balance.says.starts_with("Slides make 1.8 points each"), "{}", h.balance.says);
        let thin: Vec<&Finding> = h.findings.iter().filter(|f| f.kind == Kind::Thin).collect();
        assert_eq!(thin.len(), 1, "{:#?}", h.findings);
        assert_eq!(thin[0].severity, Severity::Warning);
        assert!(thin[0].says.starts_with("Slide"), "{}", thin[0].says);
        assert!(thin[0].says.contains('4'), "{}", thin[0].says);
        assert!(
            thin[0].says.ends_with("the slides that list points make 1.8 each, under the two that read as full."),
            "{}",
            thin[0].says
        );
        assert!(thin[0].ask.starts_with("Give "), "{}", thin[0].ask);
    }

    /// Code's lines are no points (R1's agent dogfood 11): a slide whose words are code is neither
    /// found dense for its many lines nor thin for its few, as a chart's slide is not. On fleet,
    /// slide 9 is dense and, thinned, slide 4 thin; each of their texts under the headline made
    /// code, neither is found, and the score does not move, since warnings carry none.
    #[test]
    fn code_makes_no_points_a_room_reads() {
        let coded = |mut deck: Value, k: usize| {
            let d = Deck::from_value(&deck).unwrap();
            let sl = slides(&d);
            let snaps = resolve_states(&d).unwrap();
            let last = *sl[k - 1].states.last().unwrap();
            let ids: Vec<String> = (snaps[last].nodes.iter())
                .filter(|(id, p)| {
                    d.nodes.get(id.as_str()).is_some_and(|n| n.node_type == NodeType::Text)
                        && !matches!(p.get("role").and_then(Value::as_str), Some("display" | "headline" | "title"))
                        && !matches!(p.get("semantic").and_then(Value::as_str), Some("claim"))
                })
                .map(|(id, _)| id.clone())
                .collect();
            assert!(!ids.is_empty(), "slide {k} has words under its headline");
            for id in ids {
                deck["nodes"][id.as_str()]["code"] = json!({ "language": "plain" });
            }
            deck
        };
        let dense = of(&dogfood("fleet", "", |_| {}));
        assert!(dense.findings.iter().any(|f| f.kind == Kind::Dense && f.slide == 9), "{:#?}", dense.findings);
        let code = of(&coded(dogfood("fleet", "", |_| {}), 9));
        assert!(!code.findings.iter().any(|f| f.kind == Kind::Dense && f.slide == 9), "{:#?}", code.findings);
        let thinned = |slides: &mut Vec<answer::Slide>| slides[3].key_bullets.truncate(1);
        let thin = of(&dogfood("fleet", "", thinned));
        let named = |h: &Health| {
            (h.findings.iter().filter(|f| f.kind == Kind::Thin)).any(|f| {
                let before = f.says.split(" the fewest").next().unwrap_or_default();
                before.split(|c: char| !c.is_ascii_digit()).any(|n| n == "4")
            })
        };
        assert!(named(&thin), "{:#?}", thin.findings);
        let code = of(&coded(dogfood("fleet", "", thinned), 4));
        assert!(!named(&code), "{:#?}", code.findings);
    }

    /// A slide that draws a chart, a picture, or a table shows its point, and a divider or an agenda
    /// makes none by design, so `thin` names neither (R1's agent dogfood 2): pitch's chart slides
    /// 4 and 11 and its divider, slide 7, and climate's divider, slide 3, were named. Balance's
    /// figure counts them still.
    #[test]
    fn a_chart_or_a_divider_is_not_asked_for_points() {
        for deck in [dogfood("pitch", "", |_| {}), dogfood("route9", "", |_| {}), climate()] {
            let h = of(&deck);
            let d = Deck::from_value(&deck).unwrap();
            let sl = slides(&d);
            let read = readings(&d, &sl).unwrap().0;
            for f in h.findings.iter().filter(|f| f.kind == Kind::Thin) {
                for k in f
                    .says
                    .split(" the fewest")
                    .next()
                    .unwrap()
                    .split(|c: char| !c.is_ascii_digit())
                    .filter_map(|n| n.parse::<usize>().ok())
                {
                    let layout = sl[k - 1].states.iter().find_map(|&j| d.states[j].layout.clone()).unwrap_or_default();
                    assert!(
                        !read[k - 1].drawn && !FRAMING.contains(&layout.as_str()),
                        "slide {k} ({layout}): {}",
                        f.says
                    );
                }
            }
        }
        let pitch = of(&dogfood("pitch", "", |_| {}));
        assert!(!pitch.findings.iter().any(|f| f.kind == Kind::Thin), "{:#?}", pitch.findings);
        assert!(pitch.balance.says.starts_with("Slides make 1.8 points each"), "{}", pitch.balance.says);
    }

    /// A slide that declares no intensity where its place calls for another than most is asked for
    /// the plan its place calls for, by its value, with the fix that writes it where its beat keeps
    /// a plan (agent dogfood 6: "Plan slide 8's intensity for its place" named the path, no value).
    #[test]
    fn a_slide_with_no_plan_is_asked_for_the_one_its_place_calls_for() {
        let mut deck = dogfood("keynote", "", |_| {});
        for section in deck["spine"]["sections"].as_array_mut().unwrap() {
            for beat in section["beats"].as_array_mut().unwrap() {
                if let Some(narrato) = beat.pointer_mut("/media/narrato").and_then(Value::as_object_mut) {
                    narrato.remove("intensity");
                }
            }
        }
        let h = of(&deck);
        let unplanned: Vec<&Finding> =
            (h.findings.iter()).filter(|f| f.says.contains("declares no intensity")).collect();
        assert!(!unplanned.is_empty(), "{:#?}", h.findings);
        // One whose plan would lower fidelity as the deck stands is asked for words, with no plan.
        // The keynote's slide 7, planned alone even at the 0.37 its place calls for, would set the
        // talk further from its curve while the slides around it are read on the tent.
        for f in unplanned.iter().filter(|f| f.fix.is_none()) {
            assert!(f.ask.contains("it would set the talk further from its curve"), "{}", f.ask);
        }
        let seven = unplanned.iter().find(|f| f.slide == 7).expect("slide 7 is unplanned");
        assert!(
            seven.fix.is_none()
                && seven.ask.contains("but planned alone, even at 0.37, it would set the talk further from its curve while the slides around it are read as a talk usually runs: move it"),
            "{}",
            seven.ask
        );
        // B1 plans nothing: slide 11's plan, the slides around it read on the tent, raises
        // fidelity, so it is offered, and taken it leaves the slide planned.
        let text = std::fs::read_to_string(root().join("tests/bench/b1.scaena/deck.json")).unwrap();
        let deck: Value = serde_json::from_str(&text).unwrap();
        let h = of(&deck);
        let f = (h.findings.iter())
            .find(|f| f.says.contains("declares no intensity") && f.fix.is_some())
            .expect("an unplanned slide offered its plan");
        assert_eq!(f.slide, 11);
        let curve = h.slides[f.slide - 1].curve;
        assert!(f.ask.contains(&format!("where the talk's curve stands at {curve:.2}")), "{}", f.ask);
        let fix = f.fix.as_ref().expect("the plan its place calls for, as a fix");
        assert!(fix.says.starts_with("Plan it at "), "{}", fix.says);
        let mut planned = deck.clone();
        scaena_core::patch::apply(&mut planned, &fix.patch).unwrap();
        let after = of(&planned);
        assert!(
            !after.findings.iter().any(|g| g.slide == f.slide && g.says.contains("declares no intensity")),
            "{:#?}",
            after.findings
        );
        // A beat with no plan at all, as a deck made by hand keeps one, is given one.
        let mut bare = deck.clone();
        for section in bare["spine"]["sections"].as_array_mut().unwrap() {
            for beat in section["beats"].as_array_mut().unwrap() {
                beat.as_object_mut().unwrap().remove("media");
            }
        }
        let h = of(&bare);
        let f = (h.findings.iter())
            .find(|f| f.says.contains("declares no intensity") && f.fix.is_some())
            .expect("an unplanned slide");
        let fix = f.fix.as_ref().expect("a fix");
        assert!(fix.patch[0]["path"].as_str().unwrap().ends_with("/media"), "{:?}", fix.patch);
        scaena_core::patch::apply(&mut bare, &fix.patch).unwrap();
    }

    /// The `brief` op takes the goals and lengths Narrato takes, which core keeps beside its patch
    /// ops (agent dogfood 6), and health reads the brief it writes.
    #[test]
    fn the_brief_op_takes_what_narrato_takes() {
        assert_eq!(scaena_core::patch::GOALS, scaena_narrato::brief::GOALS);
        struct NoFiles;
        impl scaena_core::validate::BundleFiles for NoFiles {
            fn exists(&self, _: &str) -> bool {
                true
            }
            fn read_text(&self, _: &str) -> Option<String> {
                None
            }
        }
        let deck = climate();
        let brief = [json!({ "op": "brief", "length": "26-40 slides" })];
        let made = scaena_core::patch::compile(&deck, &brief, &NoFiles).unwrap().doc;
        assert!(of(&made).pacing.says.contains("26\u{2013}40"), "{}", of(&made).pacing.says);
    }

    /// A headline that counts what its slide lists is held to the count (the calibration against a
    /// reader): the training deck's "Three traps that make reviews useless" lists two. A measure,
    /// a figure's words, a count kept, and a deck in another language are not found.
    #[test]
    fn a_headline_that_counts_its_list_is_held_to_it() {
        let h = of(&dogfood("training", "", |_| {}));
        let counted: Vec<&Finding> =
            h.findings.iter().filter(|f| f.kind == Kind::Story && f.says.contains("headline counts")).collect();
        assert_eq!(counted.len(), 1, "{:#?}", h.findings);
        assert_eq!(counted[0].slide, 8);
        assert_eq!(counted[0].says, "Slide 8's headline counts three, and the slide lists two.");
        assert_eq!(
            counted[0].ask,
            "Make slide 8's headline and its list agree: list the three it promises, or count the two it lists."
        );
        assert!(counted[0].fix.is_none());
        // What a headline counts, and what it does not.
        assert_eq!(counts("Three traps that make reviews useless"), Some(3));
        assert_eq!(counts("Write it in five parts, in this order"), Some(5));
        assert_eq!(counts("4 steps took it to production"), Some(4));
        for none in [
            "Your dispatchers spend six hours a day planning routes",
            "One in twelve freight invoices is wrong",
            "Storms washed out 40 miles of trail",
            "Two mistakes and three fixes",
            "We changed one loop",
        ] {
            assert_eq!(counts(none), None, "{none}");
        }
        // In Spanish and in Japanese, by their own numbers and counters (W2's health read).
        assert_eq!(counts_in("Tres barrios concentran la mitad de las fugas", Some("Spanish")), Some(3));
        assert_eq!(counts_in("El plan avanza en tres fases", Some("es")), Some(3));
        assert_eq!(counts_in("利用が増えた理由は三つあります", Some("Japanese")), Some(3));
        assert_eq!(counts_in("3つのポイント", Some("ja")), Some(3));
        assert_eq!(counts_in("５つの提案", Some("ja-JP")), Some(5));
        assert_eq!(counts_in("十二項目の改善", Some("ja")), Some(12));
        for none in [
            "Con 2,3 M€ en tres años reducimos a la mitad el agua",
            "Dos de cada tres hogares ahorran",
            "Pedimos al pleno aprobar 2,3 M€",
        ] {
            assert_eq!(counts_in(none, Some("Spanish")), None, "{none}");
        }
        for none in ["分館の新設には3億円かかります", "採決は12月10日", "利用者は12万人", "13つの案"]
        {
            assert_eq!(counts_in(none, Some("Japanese")), None, "{none}");
        }
        assert_eq!(counts_in("Drei Fehler, die Reviews nutzlos machen", Some("German")), None);
        // The list put right: nothing found.
        let kept = |slides: &mut Vec<answer::Slide>| slides[7].key_bullets.push("Timelines with no timestamps".into());
        let h = of(&dogfood("training", "", kept));
        assert!(!h.findings.iter().any(|f| f.says.contains("headline counts")), "{:#?}", h.findings);
        // In another language, not read.
        let mut deck = dogfood("training", "", |_| {});
        deck["meta"]["narrative"]["brief"]["language"] = json!("German");
        assert!(!of(&deck).findings.iter().any(|f| f.says.contains("headline counts")));
        // In Japanese, its own count is held to the list (W2's health read: 理由は三つ over seven).
        let japanese = |slides: &mut Vec<answer::Slide>| slides[7].slide_title = "レビューを無駄にする三つの罠".into();
        let mut deck = dogfood("training", "", japanese);
        deck["meta"]["narrative"]["brief"]["language"] = json!("Japanese");
        let h = of(&deck);
        let counted: Vec<&Finding> = h.findings.iter().filter(|f| f.says.contains("headline counts")).collect();
        assert_eq!(counted.len(), 1, "{:#?}", h.findings);
        assert_eq!(counted[0].says, "Slide 8's headline counts three, and the slide lists two.");
        // A count the slide shows as one of its figures is no count of its list (agent dogfood 6):
        // "$1.2M opens two neighborhoods" over three cards, the first "2 new neighborhoods".
        let figured = |slides: &mut Vec<answer::Slide>| {
            slides[7].slide_title = "$1.2M opens two neighborhoods in 2027".into();
            slides[7].layout_suggestion = "Three Numbers".into();
            slides[7].key_bullets = vec![
                "2 new neighborhoods, chosen by demand".into(),
                "$1.2M to build, less than the pilot cost".into(),
                "$1.20 a trip to run, against $2.90 by bus".into(),
            ];
        };
        let h = of(&dogfood("training", "", figured));
        assert!(!h.findings.iter().any(|f| f.says.contains("headline counts")), "{:#?}", h.findings);
        let card = |body: &str| SlideForHealth { body: body.into(), ..SlideForHealth::default() };
        assert!(shows_figure(&card("\u{2022} 2 \u{2014} New neighborhoods\n\u{2022} $1.2M \u{2014} to build"), 2));
        assert!(shows_figure(&card("\u{2022} Two, chosen by demand"), 2));
        assert!(!shows_figure(&card("\u{2022} Timelines with no timestamps\n\u{2022} 20 minutes"), 2));
    }

    /// A turn's pair is read by its words in any script (W1's dogfood 7): Vzestup's split at
    /// anything not ASCII loses "Größe" and "Müll" whole, so two German slides that share them
    /// read as sharing nothing and turned. Read whole, they share both; an English pair reads as
    /// Vzestup reads it, and the bridge reads the notes alike.
    #[test]
    fn a_turn_reads_its_pair_in_any_script() {
        let slide = |title: &str, body: &str, notes: &str| SlideForHealth {
            title: title.into(),
            body: body.into(),
            notes: notes.into(),
            ..SlideForHealth::default()
        };
        let a = slide("Die Größe des Mülls", "• Müll wächst jährlich", "");
        let b = slide("Weniger Müll, gleiche Größe", "• Sammelstellen neu verteilt", "");
        assert_eq!(health::coherence::keyword_overlap(&a, &b), 0.0, "Vzestup's split reads them apart");
        assert!(!apart(&a, &b), "read whole, they share Größe and Müll");
        // In English, the words are Vzestup's own.
        for text in ["Revenue grew 12% in 2025, it's said", "The co-op's freezers failed — day two"] {
            assert_eq!(written(text), health::coherence::keywords(text), "{text}");
        }
        // The bridge reads the notes' opening in any script.
        let c = slide("Neue Standorte", "• Sammelstellen in jedem Viertel", "Mehr Müll heißt neue Wege.");
        let before = slide("Die Größe des Mülls", "• Müll wächst jährlich", "");
        assert!(bridges(&before, &c), "Müll carries the room across");
        // The turn's ask names the words the bridge reads, whole in any script (R1's MCP dogfood).
        let after = slide("Neue Standorte", "• Sammelstellen in jedem Viertel", "");
        assert_eq!(carried(&before, &after, &COMMON), ", such as \u{201c}größe\u{201d} or \u{201c}mülls\u{201d}");

        // On the German board deck, two slides of one beat that share only Größe and Müll: no turn.
        let paired = |slides: &mut Vec<answer::Slide>| {
            for (k, (title, bullets)) in [
                ("Die Größe des Mülls", vec!["Müll wächst jährlich", "Sammeln kostet mehr"]),
                ("Weniger Müll, gleiche Größe", vec!["Sammelstellen neu verteilt", "Touren gekürzt"]),
            ]
            .into_iter()
            .enumerate()
            {
                let s = &mut slides[8 + k];
                s.slide_title = title.into();
                s.key_bullets = bullets.into_iter().map(String::from).collect();
                s.layout_suggestion = "Bullets".into();
                s.subtitle.clear();
                s.speaker_notes = "Weiter.".into();
            }
        };
        let deck = dogfood("board-de", "", paired);
        let d = Deck::from_value(&deck).unwrap();
        let read = readings(&d, &slides(&d)).unwrap().0;
        assert_eq!(health::coherence::keyword_overlap(&read[8], &read[9]), 0.0, "Vzestup's split reads them apart");
        let h = of(&deck);
        assert!(!h.findings.iter().any(|f| f.kind == Kind::Turn && f.slide == 10), "{:#?}", h.findings);
    }

    /// A Spanish deck's turn asks name Spanish connectives (health read of a Spanish deck: its four
    /// turn asks said "So" and "But"), "Entonces" and "Pero", as a German deck's name German ones.
    #[test]
    fn a_spanish_turn_names_spanish_connectives() {
        assert_eq!(connectives(Some("Spanish")), ("Entonces", "Pero"));
        assert_eq!(connectives(Some("es-MX")), ("Entonces", "Pero"));
        assert_eq!(connectives(Some("español")), ("Entonces", "Pero"));
        let turned = |slides: &mut Vec<answer::Slide>| {
            for (k, title) in ["Tres riesgos pesan sobre la red", "Calendario de obras 2027"].into_iter().enumerate() {
                let s = &mut slides[7 + k];
                s.slide_title = title.into();
                s.key_bullets = vec![];
                s.layout_suggestion = "Statement".into();
                s.subtitle.clear();
                s.speaker_notes = String::new();
            }
        };
        let mut deck = dogfood("board-de", "", turned);
        deck["meta"]["narrative"]["brief"]["language"] = json!("Spanish");
        let h = of(&deck);
        let ask = (h.findings.iter())
            .find(|f| f.kind == Kind::Turn && f.slide == 9)
            .map(|f| f.ask.clone())
            .unwrap_or_else(|| panic!("slide 9 turns: {:#?}", h.findings));
        assert!(
            ask.ends_with(
                "a connective alone (\u{201c}Entonces\u{201d}, \u{201c}Pero\u{201d}) carries nothing across."
            ),
            "{ask}"
        );
    }

    /// A turn's ask names no German joining word to carry across in a German deck (R1's agent
    /// dogfood 3: "sich", "zwei", and "voll" were named), the words the slide is about in their
    /// place; an English deck's words to leave out are English's, as before.
    #[test]
    fn a_german_turn_names_no_joining_word_to_carry() {
        let slide = |title: &str| SlideForHealth { title: title.into(), ..SlideForHealth::default() };
        let (was, next) = (slide("So zahlt sich die Investition aus"), slide("Risiken und wie wir sie begrenzen"));
        assert_eq!(
            carried(&was, &next, joining(Some("German"))),
            ", such as \u{201c}zahlt\u{201d} or \u{201c}investition\u{201d}"
        );
        assert_eq!(carried(&was, &next, joining(Some("de-DE"))), carried(&was, &next, &GERMAN));
        assert!(carried(&was, &next, joining(None)).contains("\u{201c}sich\u{201d}"), "English's list keeps it");
        let (was, next) = (slide("Their pilot proved the routes"), slide("Kiosks open soon"));
        assert_eq!(
            carried(&was, &next, joining(Some("en"))),
            ", such as \u{201c}pilot\u{201d}, \u{201c}proved\u{201d} or \u{201c}routes\u{201d}"
        );

        // On the German board deck, slide 9 turns from slide 8, which says little but its joining
        // words beside two that tell.
        let turned = |slides: &mut Vec<answer::Slide>| {
            for (k, title) in ["Zwei Risiken zeigen sich jetzt voll", "Fahrplan bis Ende 2027"].into_iter().enumerate()
            {
                let s = &mut slides[7 + k];
                s.slide_title = title.into();
                s.key_bullets = vec![];
                s.layout_suggestion = "Statement".into();
                s.subtitle.clear();
                s.speaker_notes = String::new();
            }
        };
        let mut deck = dogfood("board-de", "", turned);
        deck["meta"]["narrative"]["brief"]["language"] = json!("German");
        let h = of(&deck);
        let ask = (h.findings.iter())
            .find(|f| f.kind == Kind::Turn && f.slide == 9)
            .map(|f| f.ask.clone())
            .unwrap_or_else(|| panic!("slide 9 turns: {:#?}", h.findings));
        assert!(ask.contains("such as \u{201c}risiken\u{201d} or \u{201c}zeigen\u{201d}:"), "{ask}");
    }

    /// A turn's ask gives its connectives in the deck's language (health's English-only sweep,
    /// item 2): a German deck's speaker opens with "Also" or "Aber", a French one's with "Donc" or
    /// "Mais"; an English deck's ask is as before, "So" and "But".
    #[test]
    fn a_turn_names_connectives_in_the_decks_language() {
        assert_eq!(connectives(Some("German")), ("Also", "Aber"));
        assert_eq!(connectives(Some("fr-FR")), ("Donc", "Mais"));
        assert_eq!(connectives(None), ("So", "But"));
        assert_eq!(connectives(Some("Japanese")), ("So", "But"), "another language's are English's, as before");
        let turned = |slides: &mut Vec<answer::Slide>| {
            for (k, title) in ["Zwei Risiken zeigen sich deutlich", "Fahrplan bis Ende 2027"].into_iter().enumerate() {
                let s = &mut slides[7 + k];
                s.slide_title = title.into();
                s.key_bullets = vec![];
                s.layout_suggestion = "Statement".into();
                s.subtitle.clear();
                s.speaker_notes = String::new();
            }
        };
        let mut deck = dogfood("board-de", "", turned);
        deck["meta"]["narrative"]["brief"]["language"] = json!("German");
        let h = of(&deck);
        let ask = (h.findings.iter())
            .find(|f| f.kind == Kind::Turn && f.slide == 9)
            .map(|f| f.ask.clone())
            .unwrap_or_else(|| panic!("slide 9 turns: {:#?}", h.findings));
        assert!(
            ask.ends_with("a connective alone (\u{201c}Also\u{201d}, \u{201c}Aber\u{201d}) carries nothing across."),
            "{ask}"
        );
    }

    /// A turn's ask names no French joining word to carry across in a French deck (health's
    /// English-only sweep, item 3), as a German deck's names no German one: "ainsi", "notre", and
    /// "toutes" give way to the words the slide is about.
    #[test]
    fn a_french_turn_names_no_joining_word_to_carry() {
        let slide = |title: &str| SlideForHealth { title: title.into(), ..SlideForHealth::default() };
        let (was, next) =
            (slide("Ainsi notre réseau dessert toutes les communes"), slide("Les risques et comment les limiter"));
        assert_eq!(
            carried(&was, &next, joining(Some("French"))),
            ", such as \u{201c}réseau\u{201d}, \u{201c}dessert\u{201d} or \u{201c}communes\u{201d}"
        );
        assert_eq!(carried(&was, &next, joining(Some("fr-FR"))), carried(&was, &next, &FRENCH));
        assert_eq!(carried(&was, &next, joining(Some("français"))), carried(&was, &next, &FRENCH));
        assert!(carried(&was, &next, joining(None)).contains("\u{201c}ainsi\u{201d}"), "English's list keeps it");
        // W1's dogfood 10, a French deck: its turn asks offered "sans", "pour", "plus", and
        // "chaque" as the words to carry across.
        let (was, next) =
            (slide("Chaque pêcheur gagne plus sans payer pour le quai"), slide("Le calendrier des travaux"));
        assert_eq!(
            carried(&was, &next, joining(Some("French"))),
            ", such as \u{201c}pêcheur\u{201d}, \u{201c}gagne\u{201d} or \u{201c}payer\u{201d}"
        );
        assert!(carried(&was, &next, joining(None)).contains("\u{201c}chaque\u{201d}"), "English's list keeps it");

        // On the board deck told in French, slide 9 turns from slide 8, which says little but its
        // joining words beside two that tell.
        let turned = |slides: &mut Vec<answer::Slide>| {
            for (k, title) in
                ["Deux risques pèsent encore", "Calendrier jusqu\u{2019}à fin 2027"].into_iter().enumerate()
            {
                let s = &mut slides[7 + k];
                s.slide_title = title.into();
                s.key_bullets = vec![];
                s.layout_suggestion = "Statement".into();
                s.subtitle.clear();
                s.speaker_notes = String::new();
            }
        };
        let mut deck = dogfood("board-de", "", turned);
        deck["meta"]["narrative"]["brief"]["language"] = json!("French");
        let h = of(&deck);
        let ask = (h.findings.iter())
            .find(|f| f.kind == Kind::Turn && f.slide == 9)
            .map(|f| f.ask.clone())
            .unwrap_or_else(|| panic!("slide 9 turns: {:#?}", h.findings));
        assert!(ask.contains("such as \u{201c}risques\u{201d} or \u{201c}pèsent\u{201d}:"), "{ask}");
    }

    /// A Portuguese or Italian deck's turn asks name its own connectives and no joining word of its
    /// language to carry across (W1's dogfood 11: a Brazilian deck's ask said "So" and "But" and
    /// named "para" beside "receita" and "subiu").
    #[test]
    fn a_portuguese_turn_names_its_connectives_and_no_joining_word() {
        assert_eq!(connectives(Some("Portuguese")), ("Ent\u{e3}o", "Mas"));
        assert_eq!(connectives(Some("pt-BR")), ("Ent\u{e3}o", "Mas"));
        assert_eq!(connectives(Some("portugu\u{ea}s")), ("Ent\u{e3}o", "Mas"));
        assert_eq!(connectives(Some("it")), ("Quindi", "Ma"));
        assert_eq!(connectives(Some("Italian")), ("Quindi", "Ma"));
        let slide = |title: &str| SlideForHealth { title: title.into(), ..SlideForHealth::default() };
        let (was, next) =
            (slide("A receita subiu de R$ 1,2 mi para R$ 2,1 mi em seis meses"), slide("O que puxou o crescimento"));
        let said = carried(&was, &next, joining(Some("pt-BR")));
        assert!(!said.contains("\u{201c}para\u{201d}") && !said.contains("\u{201c}seis\u{201d}"), "{said}");
        assert!(said.contains("\u{201c}receita\u{201d}") && said.contains("\u{201c}subiu\u{201d}"), "{said}");
        assert!(carried(&was, &next, joining(None)).contains("\u{201c}para\u{201d}"), "English's list keeps it");
        let (was, next) = (slide("Il fatturato cresce ancora nelle nostre sedi"), slide("Tre rischi da gestire"));
        let said = carried(&was, &next, joining(Some("Italian")));
        assert!(!said.contains("\u{201c}ancora\u{201d}") && !said.contains("\u{201c}nelle\u{201d}"), "{said}");
        assert!(said.contains("\u{201c}fatturato\u{201d}"), "{said}");
    }

    /// A turn's ask names no Spanish joining word to carry across in a Spanish deck (health read of
    /// a Spanish deck: "tres" was named beside "plan" and "avanza"), as a French deck's names no
    /// French one.
    #[test]
    fn a_spanish_turn_names_no_joining_word_to_carry() {
        let slide = |title: &str| SlideForHealth { title: title.into(), ..SlideForHealth::default() };
        let (was, next) =
            (slide("El plan avanza en tres fases"), slide("Sin inversión, las pérdidas volverán a subir"));
        assert_eq!(
            carried(&was, &next, joining(Some("Spanish"))),
            ", such as \u{201c}plan\u{201d}, \u{201c}avanza\u{201d} or \u{201c}fases\u{201d}"
        );
        let (was, next) = (slide("Cada barrio tiene tres fugas"), slide("Reparar cuesta menos que perder agua"));
        assert_eq!(
            carried(&was, &next, joining(Some("es-ES"))),
            ", such as \u{201c}barrio\u{201d} or \u{201c}fugas\u{201d}"
        );
        assert_eq!(carried(&was, &next, joining(Some("español"))), carried(&was, &next, &SPANISH));
        assert!(carried(&was, &next, joining(None)).contains("\u{201c}cada\u{201d}"), "English's list keeps it");

        // On the board deck told in Spanish, slide 9 turns from slide 8, which says little but its
        // joining words beside two that tell.
        let turned = |slides: &mut Vec<answer::Slide>| {
            for (k, title) in ["Tres riesgos pesan ahora", "Calendario hasta 2027"].into_iter().enumerate() {
                let s = &mut slides[7 + k];
                s.slide_title = title.into();
                s.key_bullets = vec![];
                s.layout_suggestion = "Statement".into();
                s.subtitle.clear();
                s.speaker_notes = String::new();
            }
        };
        let mut deck = dogfood("board-de", "", turned);
        deck["meta"]["narrative"]["brief"]["language"] = json!("Spanish");
        let h = of(&deck);
        let ask = (h.findings.iter())
            .find(|f| f.kind == Kind::Turn && f.slide == 9)
            .map(|f| f.ask.clone())
            .unwrap_or_else(|| panic!("slide 9 turns: {:#?}", h.findings));
        assert!(ask.contains("such as \u{201c}riesgos\u{201d} or \u{201c}pesan\u{201d}:"), "{ask}");
    }

    /// Balance's line says how many slides its average is over, where it counts fewer than the
    /// deck shows (W1's dogfood 7): two points cut over five slides move it 0.4, which over nine
    /// would read as an error in the arithmetic. The figure is Vzestup's.
    #[test]
    fn balance_says_the_slides_its_average_is_over() {
        let thinned = |slides: &mut Vec<answer::Slide>| slides[3].key_bullets.truncate(1);
        let h = of(&dogfood("fleet", "", thinned));
        assert_eq!(
            h.balance.says,
            "Slides make 1.8 points each, over 4 of the 10 slides, those made to list points, where two to five read as full."
        );
        // A deck whose every slide balance counts says nothing of it.
        let every = |slides: &mut Vec<answer::Slide>| {
            for s in slides.iter_mut() {
                s.layout_suggestion = "Bullets".into();
                s.key_bullets.truncate(1);
            }
        };
        let all = of(&dogfood("fleet", "", every));
        assert!(!all.balance.says.contains(", over "), "{}", all.balance.says);
        // An average that reads as one is one point, as a reader says it (Nordlicht, R1's agent
        // dogfood 3, read "Slides make 1 points each"); any other is points.
        assert_eq!(points(1.0), "point");
        for other in [0.0, 0.8, 1.5, 2.0, 5.5] {
            assert_eq!(points(other), "points", "{other}");
        }
    }

    /// A dense slide's ask names what cutting costs where balance would pay for it (W1's dogfood
    /// 4): on the fleet pitch, the slides balance counts list 2 points on average, at its floor, so
    /// cutting slide 9's six bars to five lowers the score. The ask says so, and names slide 4 to
    /// give a point more; following it, the cut and the point, keeps the score. Balance's floor
    /// is Vzestup's, unchanged.
    #[test]
    fn a_dense_ask_names_what_a_cut_costs_and_how_to_keep_it() {
        let h = of(&dogfood("fleet", "", |_| {}));
        let dense = h.findings.iter().find(|f| f.kind == Kind::Dense).expect("slide 9 makes six points");
        assert_eq!(dense.slide, 9, "{dense:?}");
        assert!(
            dense
                .ask
                .starts_with("Tighten slide 9 to the 5 points that matter most, and keep what you cut in its notes"),
            "{}",
            dense.ask
        );
        assert!(
            dense.ask.contains("Cutting alone lowers the deck's balance")
                && dense.ask.ends_with("give slide 4 a point more as you tighten it."),
            "{}",
            dense.ask
        );
        // Cut alone, as the ask before said to: the score falls.
        let cut = |slides: &mut Vec<answer::Slide>| slides[8].key_bullets.truncate(5);
        let tightened = of(&dogfood("fleet", "", cut));
        assert!(tightened.score < h.score, "{} then {}", h.score, tightened.score);
        // Cut, and a point more on slide 4, as the ask says now: the score holds.
        let followed = |slides: &mut Vec<answer::Slide>| {
            cut(slides);
            slides[3].key_bullets.push("Dispatch software: priced for a hundred trucks".into());
        };
        let kept = of(&dogfood("fleet", "", followed));
        assert!(kept.score >= h.score, "{} then {}", h.score, kept.score);
        assert!(!kept.findings.iter().any(|f| f.kind == Kind::Dense), "{:?}", kept.findings);
    }

    /// The cost is said where the deck stands under balance's floor already (W1's dogfood 6):
    /// there balance marks each point fewer down, so slide 9's cut from six bars to five lowers
    /// the score again, and the ask says so as it does at the floor.
    #[test]
    fn a_dense_ask_names_the_cost_under_the_floor_too() {
        let thinned = |slides: &mut Vec<answer::Slide>| slides[3].key_bullets.truncate(1);
        let h = of(&dogfood("fleet", "", thinned));
        let dense = h.findings.iter().find(|f| f.kind == Kind::Dense).expect("slide 9 makes six points");
        assert_eq!(dense.slide, 9, "{dense:?}");
        assert!(dense.ask.contains("Cutting alone lowers the deck's balance"), "{}", dense.ask);
        // Under the floor before the cut, and the cut alone lowers the score further.
        assert!(h.balance.says.starts_with("Slides make 1.8 points each"), "{}", h.balance.says);
        let cut = |slides: &mut Vec<answer::Slide>| {
            thinned(slides);
            slides[8].key_bullets.truncate(5);
        };
        let tightened = of(&dogfood("fleet", "", cut));
        assert!(tightened.score < h.score, "{} then {}", h.score, tightened.score);
    }

    /// A word and its plural are one word where a pair is read for a turn (the library keynote's
    /// health read against a reader: "twelve laundromats" and then "Laundromat model"). The
    /// keynote's slides 4 and 5, in one beat, given those words alone in common, do not turn;
    /// given none, they do.
    #[test]
    fn a_word_and_its_plural_do_not_turn() {
        let paired = |next: &'static str| {
            move |slides: &mut Vec<answer::Slide>| {
                slides[3].slide_title = "Twelve laundromats signed up first".into();
                slides[3].key_bullets = vec![];
                slides[4].slide_title = next.into();
                slides[4].key_bullets = vec!["Partner early".into(), "Train staff".into(), "Fund steadily".into()];
                slides[4].speaker_notes = String::new();
            }
        };
        let turned = |next: &'static str| {
            of(&dogfood("keynote", "", paired(next))).findings.iter().any(|f| f.kind == Kind::Turn && f.slide == 5)
        };
        assert!(!turned("The laundromat model spread"));
        assert!(turned("The kiosk model spread"));
        assert_eq!(
            singular(written("Twelve laundromats, the business, its bus")),
            written("twelve laundromat business its bus")
        );
    }

    /// A close that says the big idea's own words in its order lands it, the words any talk uses
    /// among them (the library keynote's "Put 4% of the budget where people already are", under a
    /// big idea that ends "when it goes where people already are"); the same words said in passing
    /// land nothing (W1's dogfood 3), nor do the keynote's "twelve people".
    #[test]
    fn a_close_that_quotes_the_big_idea_lands_it() {
        let idea = "The library reaches every reader when it goes where people already are";
        let closed = |title: &'static str| {
            move |slides: &mut Vec<answer::Slide>| slides.last_mut().unwrap().slide_title = title.into()
        };
        // Landed, as the warning reads it: the keynote's talk is of a fishing town's power, so no
        // slide before its close builds toward a library's idea, and the info says so.
        let landed =
            |h: &Health| !h.findings.iter().any(|f| f.kind == Kind::BigIdea && f.severity == Severity::Warning);
        assert!(!landed(&of(&dogfood("keynote", idea, |_| {}))), "the keynote's close says only \"people\"");
        assert!(landed(&of(&dogfood("keynote", idea, closed("Go where people already are")))));
        assert!(!landed(&of(&dogfood("keynote", idea, closed("People already know us")))), "said in passing");
        assert!(quoted_words(idea, &["Build where people already are"]).contains("already"));
        assert!(quoted_words(idea, &["where people already"]).is_empty(), "three words in a row quote nothing");
    }

    /// A tone ask names the words the tone was read from (W1's dogfood 3): the keynote's eighth
    /// slide, hopeful by plan, reads graver for its outages; its headline warmed with "love" and
    /// "missed" reads graver still, since the lexicon reads "missed" as grave and not "love".
    /// The ask says so, and the reading is the same as before it named them.
    #[test]
    fn a_tone_ask_names_the_words_the_tone_was_read_from() {
        let warmed = |slides: &mut Vec<answer::Slide>| {
            slides[7].slide_title = "We love the harbor we almost missed".into();
        };
        let h = of(&dogfood("keynote", "", warmed));
        let tone = h.findings.iter().find(|f| f.kind == Kind::Tone && f.slide == 8).expect("slide 8 reads graver");
        assert!(tone.ask.starts_with("Warm slide 8's words toward the tone it declares"), "{}", tone.ask);
        assert!(tone.ask.contains(" Health reads \u{201c}outages\u{201d}"), "{}", tone.ask);
        assert!(tone.ask.contains("\u{201c}missed\u{201d}") && tone.ask.contains(" as grave"), "{}", tone.ask);
        assert!(!tone.ask.contains("\u{201c}love\u{201d}"), "the lexicon does not read \"love\": {}", tone.ask);
        // Named or not, the reading is the lexicon's: slide 8's tone is what valence reads.
        let read = h.slides[7].tone_read.unwrap();
        assert!(tone.ask.contains(&signed(read)), "{} reads {read}", tone.ask);
    }

    /// A close lands the big idea by what it is about, not by a word it says in passing (W1's
    /// dogfood 3): the keynote's close weakened to "Thank you" and "Questions?" is found never
    /// landing it, though the slide before it, and the rest of the last beat's, share words with
    /// it; a thesis just before the close lands it, as does a close that says it itself, and a
    /// thesis with the steps between it and the close does not.
    #[test]
    fn a_close_lands_the_big_idea_by_what_it_is_about() {
        let idea = "Twelve neighbors who stop waiting for permission can power a whole town";
        let thanked = |slides: &mut Vec<answer::Slide>| {
            let last = slides.last_mut().unwrap();
            last.slide_title = "Thank you".into();
            last.subtitle = "Questions?".into();
        };
        let weak = of(&dogfood("keynote", idea, thanked));
        let n = weak.slides.len();
        let big = weak.findings.iter().find(|f| f.kind == Kind::BigIdea).expect("a close of thanks lands nothing");
        // An idea that ends in a stop is quoted without it, so no sentence ends twice.
        let stopped = of(&dogfood("keynote", &format!("{idea}."), thanked));
        let ended = stopped.findings.iter().find(|f| f.kind == Kind::BigIdea).unwrap();
        assert!(ended.says.ends_with(&format!("\u{201c}{idea}\u{201d}")), "{}", ended.says);
        assert!(ended.ask.starts_with(&format!("The big idea is \u{201c}{idea}\u{201d}. Rewrite")), "{}", ended.ask);
        assert_eq!(stopped.big_idea.as_deref(), Some(format!("{idea}.").as_str()), "the idea itself is as settled");
        let close = format!("The close, slides {}\u{2013}{n}, never lands", n - 1);
        assert!(big.says.starts_with(&close), "{}", big.says);
        // The last beat's slide before the close shares a word with it, which the rule before
        // took for the idea said.
        let steps = &weak.slides[n - 2];
        assert!(!content_words(&steps.title).is_disjoint(&content_words(idea)), "{}", steps.title);

        // The thesis that stands just before the close, in its beat, lands it, the steps between
        // them taken out; left in the beat before, it is that beat's (dogfood 4).
        let fine = "Resilience is built by neighbors, not delivered to them";
        let found = |h: &Health| {
            h.findings.iter().filter(|f| f.kind == Kind::BigIdea && f.severity == Severity::Warning).count()
        };
        let stepless = |slides: &mut Vec<answer::Slide>| {
            let at = slides.len() - 2;
            slides.remove(at);
        };
        let closed = |slides: &mut Vec<answer::Slide>| {
            stepless(slides);
            let at = slides.len() - 2;
            slides[at].beat_index = slides[at + 1].beat_index;
            slides[at].beat_name = slides[at + 1].beat_name.clone();
        };
        assert_eq!(found(&of(&dogfood("keynote", fine, closed))), 0, "the thesis before the close lands it");
        assert_eq!(found(&of(&dogfood("keynote", fine, stepless))), 1, "a thesis of the beat before is not the close");
        assert_eq!(found(&of(&dogfood("keynote", fine, |_| {}))), 1, "steps between them, the thesis is not the close");
        let said = |slides: &mut Vec<answer::Slide>| slides.last_mut().unwrap().slide_title = idea.into();
        assert_eq!(found(&of(&dogfood("keynote", idea, said))), 0, "a close that says it lands it");
    }

    /// A slide that says again what the slide before it said is asked to fold in or move on (W1's
    /// dogfood 4): the fleet pitch's "Planning takes minutes instead of hours" after "Routes in
    /// minutes, not hours". No other dogfood deck has one, and a headline the same word for word is
    /// lint's W425, not found again.
    #[test]
    fn a_slide_that_says_the_one_before_again_is_found() {
        let again = |h: &Health| -> Vec<Finding> {
            (h.findings.iter()).filter(|f| f.kind == Kind::Story && f.says.contains("says again")).cloned().collect()
        };
        let found = again(&of(&dogfood("fleet", "", |_| {})));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].slide, 7);
        assert_eq!(
            found[0].says,
            "Slide 7 says again what slide 6 said: its headline shares \u{201c}hours\u{201d} and \u{201c}minutes\u{201d} with \u{201c}Routes in minutes, not hours\u{201d}."
        );
        assert_eq!(
            found[0].ask,
            "Fold slide 7 into slide 6, or give slide 7 a point of its own that moves the talk on."
        );
        for name in ["deepdive", "board-de", "appeal", "training", "pitch", "qbr", "lecture", "keynote", "launch"] {
            assert!(again(&of(&dogfood(name, "", |_| {}))).is_empty(), "{name}");
        }
        // A claim said again in other words and the same figure (W1's dogfood 5): plurals fold
        // and a hyphen splits, so "the 40-mile goal" says "52 miles, not 40" again.
        let paraphrased = |slides: &mut Vec<answer::Slide>| {
            slides[2].slide_title = "We rebuilt 52 miles, not 40".into();
            slides[3].slide_title = "Crews beat the 40-mile goal by 12 miles".into();
        };
        let found = again(&of(&dogfood("fleet", "", paraphrased)));
        assert!(found.iter().any(|f| f.slide == 4), "{found:?}");
        // Headlines set parallel, each a word of its own, over slides that say different things.
        let parallel = |slides: &mut Vec<answer::Slide>| {
            slides[5].slide_title = "The town built its own grid".into();
            slides[6].slide_title = "The town built its own market".into();
        };
        assert!(again(&of(&dogfood("keynote", "", parallel))).is_empty(), "anaphora is no repeat");
        let moved = |slides: &mut Vec<answer::Slide>| {
            slides[6].slide_title = "Dispatchers spend the saved time on exceptions".into()
        };
        assert!(again(&of(&dogfood("fleet", "", moved))).is_empty());
    }

    /// A slide that says again what a slide two or more back said, another between them, is found
    /// as one just after it is, and the slides a deck past its length is asked to cut are those an
    /// earlier slide said most of, not those most like their neighbour (agent dogfood 7): the
    /// pitch with its accessorials said again on slide 6, slide 5 between, and three slides that
    /// tell How It Works again a step at a time named its hook, its big number, and its chart.
    #[test]
    fn a_slide_that_says_again_what_one_further_back_said_is_found_and_named_to_cut() {
        let faulted = |slides: &mut Vec<answer::Slide>| {
            let six = &mut slides[5];
            six.slide_title = "Accessorials are where most of the overbilling hides".into();
            six.layout_suggestion = "Bullets".into();
            six.key_bullets = vec![
                "Accessorial charges are 41% of every overbilled dollar".into(),
                "Liftgates, detention, and residential fees rarely match the contract".into(),
                "They are billed after the fact, so nobody checks them".into(),
            ];
            let mut told = Vec::new();
            for (title, items) in [
                (
                    "Manifest reads every invoice format carriers send",
                    ["EDI, email, and carrier portals", "Scanned PDFs included", "Parsed line by line"],
                ),
                (
                    "Manifest matches every charge against the contract",
                    [
                        "Each charge against the contract rate",
                        "Each charge against the shipment",
                        "Each charge against proof of delivery",
                    ],
                ),
                (
                    "Manifest disputes what doesn't match before AP pays",
                    [
                        "Disputes go to the carrier with evidence",
                        "AP only pays verified amounts",
                        "Carriers usually reissue the invoice",
                    ],
                ),
            ] {
                let mut s = slides[7].clone();
                s.slide_title = title.into();
                s.layout_suggestion = "Bullets".into();
                s.key_bullets = items.iter().map(|&w| w.into()).collect();
                s.subtitle = String::new();
                told.push(s);
            }
            slides.splice(9..9, told);
        };
        let h = of(&dogfood("pitch", "", faulted));
        let again: Vec<&Finding> = (h.findings.iter()).filter(|f| f.says.contains("says again")).collect();
        let cut_or_own = concat!(
            "Fold slide 6 into slide 4, which says it already, or cut it, ",
            "or give slide 6 a point of its own that moves the talk on."
        );
        let six = again.iter().find(|f| f.slide == 6 && f.says.starts_with("Slide 6 says again what slide 4 said"));
        let six = six.unwrap_or_else(|| panic!("{again:#?}"));
        assert_eq!(six.ask, cut_or_own);
        // It names slide 4, which it may be folded into (R1's #1161).
        let deck = Deck::from_value(&dogfood("pitch", "", faulted)).unwrap();
        let four = deck.slides()[3][0];
        assert_eq!(six.into.as_deref(), Some(deck.states[four].id.as_str()));
        // Folded into slide 4, as its row offers: slide 6's words join slide 4's, slide 5 is as
        // it was, and slide 6 is gone (R1's #1161). Its points written short, so they fit there.
        let short = |slides: &mut Vec<answer::Slide>| {
            faulted(slides);
            slides[5].key_bullets = vec!["41% of overbilled dollars".into(), "Rarely checked".into()];
        };
        let deck = Deck::from_value(&dogfood("pitch", "", short)).unwrap();
        let short_h = health(&deck).unwrap();
        let six = (short_h.findings.iter())
            .find(|f| f.slide == 6 && f.says.starts_with("Slide 6 says again what slide 4 said"));
        let six = six.unwrap_or_else(|| panic!("{:#?}", short_h.findings));
        let dir = std::env::temp_dir().join(format!("scaena-health-fold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in ["themes", "fonts"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
            for f in std::fs::read_dir(root().join("docs/examples").join(sub)).unwrap() {
                let f = f.unwrap().path();
                std::fs::copy(&f, dir.join(sub).join(f.file_name().unwrap())).unwrap();
            }
        }
        let mut made = deck.to_value().unwrap();
        made["theme"] = json!("themes/dusk.theme.json");
        let ridgeline: Value =
            serde_json::from_str(&std::fs::read_to_string(root().join("docs/examples/ridgeline.deck.json")).unwrap())
                .unwrap();
        if made["fonts"].as_array().is_none_or(Vec::is_empty) {
            made["fonts"] = ridgeline["fonts"].clone();
        }
        std::fs::write(dir.join("pitch.deck.json"), made.to_string()).unwrap();
        let b = crate::open(&dir.join("pitch.deck.json")).unwrap();
        let (gone, five) = (deck.states[deck.slides()[5][0]].id.clone(), deck.slides()[4].clone());
        let folded = crate::states::folding(&b, &gone, six.into.as_deref()).unwrap();
        assert_eq!(folded.refused, None, "{folded:?}");
        assert_eq!(folded.said, "Folded slide 6 into slide 4");
        let files = crate::lint::View::of(&b);
        let doc = scaena_core::patch::compile(&deck.to_value().unwrap(), &folded.patch, &files).unwrap().doc;
        let after = Deck::from_value(&doc).unwrap();
        assert_eq!(after.slides().len(), deck.slides().len() - 1);
        assert!(after.state_index(&gone).is_none(), "slide 6 goes");
        let (was, now) = (scaena_core::resolve_states(&deck).unwrap(), scaena_core::resolve_states(&after).unwrap());
        let words = |snaps: &[scaena_core::Snapshot], id: &str| {
            let snap = snaps.iter().find(|s| s.state_id == id).unwrap();
            let mut all: Vec<String> = snap.nodes.values().map(scaena_core::reading::words).collect();
            all.sort();
            all
        };
        for &k in &five {
            let id = &deck.states[k].id;
            assert_eq!(words(&was, id), words(&now, id), "slide 5 is as it was");
        }
        let four_last = &deck.states[*deck.slides()[3].last().unwrap()].id;
        let joined = words(&now, four_last).join("\n");
        assert!(joined.contains("Accessorials are where most of the overbilling hides"), "{joined}");
        // By the patch op, as `deck_patch` takes it: the same fold, said as folding into slide 4.
        let ops = json!([{ "op": "fold_slide", "slide": gone, "into": six.into }]);
        let dry = crate::patch::reported(&b, &ops, true, false).unwrap();
        let fold = dry.folded.as_ref().expect("what it folds where");
        assert!(fold.says.starts_with("Folds slide 6 into slide 4: its words join"), "{}", fold.says);
        let (_, write) = crate::patch::patching(&b, &ops, None).unwrap();
        let by_hand = scaena_core::patch::compile(&b.deck.to_value().unwrap(), &folded.patch, &files).unwrap().doc;
        assert_eq!(write.unwrap().deck.to_json().unwrap(), Deck::from_value(&by_hand).unwrap().to_json().unwrap());
        // Into a slide after it, or into itself, it is refused, saying why.
        let later = deck.states[deck.slides()[7][0]].id.clone();
        let refused = crate::states::folding(&b, &gone, Some(&later)).unwrap();
        assert_eq!(
            refused.refused.as_deref(),
            Some("Slide 6 folds only into a slide before it, and slide 8 is after it.")
        );
        let itself = crate::states::folding(&b, &gone, Some(&gone)).unwrap();
        assert_eq!(itself.refused.as_deref(), Some("Slide 6 cannot be folded into itself."));
        assert!(again.iter().any(|f| f.says.contains("what slide 8 said")), "{again:#?}");
        // Three past the 11–15 slides asked: the three that tell How It Works again, not the hook.
        let cut = (h.findings.iter()).find(|f| f.kind == Kind::Pacing && f.says.contains("add least")).expect("a cut");
        assert!(cut.says.ends_with(": slides 10, 11 and 12 add least to the slides before them."), "{}", cut.says);
    }

    /// Notes that read the slide aloud are coached, as one finding at the first (W1's dogfood 4):
    /// the fleet pitch's notes are each its headline again, the cover's aside, and its slides 2 to
    /// 10 are named; the keynote's, which tell what its slides do not show, are not.
    #[test]
    fn notes_that_read_the_slide_aloud_are_coached() {
        let aloud = |h: &Health| -> Vec<Finding> {
            (h.findings.iter()).filter(|f| f.kind == Kind::Notes && f.says.contains("aloud")).cloned().collect()
        };
        let fleet = of(&dogfood("fleet", "", |_| {}));
        let found = aloud(&fleet);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].slide, 2);
        // A warning, so it opens in the Story, never folded among the infos.
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(
            found[0].says,
            "The notes for slides 2, 3, 4, 5, 6, 7, 8, 9 and 10 read the slide aloud: nearly every word of them is on each."
        );
        assert!(found[0].ask.starts_with("Rewrite the notes for slides 2, 3, 4"), "{}", found[0].ask);
        assert!(aloud(&of(&dogfood("keynote", "", |_| {}))).is_empty());
        // Notes that are the headline word for word read it aloud at any length (W1's dogfood 5),
        // a figure's unit said in words or in its sign alike.
        let echoed = |slides: &mut Vec<answer::Slide>| {
            for s in slides.iter_mut() {
                s.speaker_notes = "Thanks.".into();
            }
            slides[2].slide_title = "We rebuilt 52 miles, not 40".into();
            slides[2].speaker_notes = "- We rebuilt 52 miles, not 40.".into();
            slides[7].slide_title = "The award was $184,000".into();
            slides[7].speaker_notes = "The award was 184,000 dollars.".into();
        };
        let found = aloud(&of(&dogfood("fleet", "", echoed)));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].says.starts_with("The notes for slides 3 and 8 read the slide aloud"), "{}", found[0].says);
        // Notes that tell the room more than the slide shows are not found.
        let told = |slides: &mut Vec<answer::Slide>| {
            for s in slides.iter_mut().skip(1) {
                s.speaker_notes = "Pause here, and ask who has stayed late rebuilding a route by hand.".into();
            }
        };
        assert!(aloud(&of(&dogfood("fleet", "", told))).is_empty());
    }

    /// A figure a slide writes in a deck made from its sources, where the slide names none, is
    /// asked where it comes from (W1's dogfood 4): the fleet pitch's "38%" fuel cut cites nothing,
    /// though the deck was made from the pilot's sheet. The slides that cite it, the cover's date,
    /// and a deck made from no sources are not found; cited, slide 8 is not either.
    #[test]
    fn a_figure_no_source_backs_in_a_sourced_deck_is_asked_where_it_comes_from() {
        let pilot = || {
            vec![scaena_narrato::Provenance {
                name: "pilot.csv".into(),
                kind: "csv".into(),
                url: None,
                bytes: 1,
                truncated: false,
            }]
        };
        let unsourced = |deck: &Value| -> Vec<(usize, String, String)> {
            (of(deck).findings.into_iter())
                .filter(|f| f.kind == Kind::Source)
                .map(|f| (f.slide, f.says, f.ask))
                .collect()
        };
        // Made from the pilot's sheet, each slide that names it cited in its beat (`source:NAME`).
        let made = grounded_dogfood("fleet", "", pilot(), |_| {});
        let cited = |k: usize| {
            (made["spine"]["sections"].as_array().unwrap().iter())
                .flat_map(|s| s["beats"].as_array().unwrap().iter())
                .nth(k)
                .is_some_and(|b| b["evidence"].as_array().is_some_and(|e| !e.is_empty()))
        };
        assert!(cited(1) && !cited(7), "slide 2 names the sheet, slide 8 nothing");
        let found = unsourced(&made);
        assert_eq!(found.len(), 1, "{found:?}");
        let (slide, says, ask) = &found[0];
        assert_eq!(*slide, 8);
        assert_eq!(says, "Slide 8 writes 38%, and does not cite the deck's source, pilot.csv, for it.");
        assert_eq!(ask, "Say on slide 8 where 38% comes from, naming the source, or cite the source it is from.");
        let figured = of(&made).findings.into_iter().find(|f| f.kind == Kind::Source).and_then(|f| f.figure);
        assert_eq!(figured.as_deref(), Some("38%"), "the finding says its figure");
        // Its `sourceRef` given, slide 8 is cited and found sourced.
        let named = |slides: &mut Vec<answer::Slide>| slides[7].source_ref = Some("pilot.csv".into());
        assert!(unsourced(&grounded_dogfood("fleet", "", pilot(), named)).is_empty());
        // A chart's figures are its data's, which its beat cites as `@data`, `sourceRef` or none.
        let charted = |slides: &mut Vec<answer::Slide>| {
            let weeks: Vec<String> = (1..=6).map(|w| format!("Week {w}")).collect();
            slides[8].source_ref = None;
            slides[8].chart_data = Some(answer::ChartData {
                categories: weeks,
                series: vec![answer::Series {
                    name: Some("Late deliveries".into()),
                    values: [38.0, 35.0, 22.0, 18.0, 15.0, 14.0].map(Some).to_vec(),
                }],
                unit: None,
                source_id: None,
            });
        };
        let charts = grounded_dogfood("fleet", "", pilot(), charted);
        assert!(!unsourced(&charts).iter().any(|(k, ..)| *k == 9), "{:?}", unsourced(&charts));
        assert!(unsourced(&dogfood("fleet", "", |_| {})).is_empty(), "a deck made from no sources");
        // Doing what the ask says (W1's dogfood 5): slide 8's own words name the pilot it is from.
        let said = |slides: &mut Vec<answer::Slide>| {
            slides[7].key_bullets = vec!["less fuel spend, from the pilot".into()];
        };
        assert!(unsourced(&grounded_dogfood("fleet", "", pilot(), said)).is_empty(), "the slide names its source");
        // A figure an earlier slide sourced, the same value and unit, is sourced where it is said
        // again: slide 2 cites the sheet for its 6.5 hours.
        let again = |slides: &mut Vec<answer::Slide>| {
            slides[7].slide_title = "Dispatchers win back 6.5 hours a day".into();
            slides[7].key_bullets = vec![];
        };
        assert!(unsourced(&grounded_dogfood("fleet", "", pilot(), again)).is_empty(), "6.5 hours, as slide 2 cites it");
        // A figure several slides write that none sources is one finding, on the first, naming
        // them all (W2's health read: Valdemar's 2,3 M€ on slides 5 and 8).
        let twice = |slides: &mut Vec<answer::Slide>| {
            slides[6].key_bullets = vec!["Planning time fell from hours to minutes, and fuel spend 38%".into()];
        };
        let found = unsourced(&grounded_dogfood("fleet", "", pilot(), twice));
        assert_eq!(found.len(), 1, "{found:?}");
        let (slide, says, ask) = &found[0];
        assert_eq!(*slide, 7);
        assert_eq!(
            says,
            "Slide 7 writes 38%, as slide 8 does, and does not cite the deck's source, pilot.csv, for it."
        );
        assert_eq!(
            ask,
            "Say on slide 7 where 38% comes from, naming the source, or cite the source it is from: that sources it on slide 8 too."
        );
        // Slide 7 cited, slide 8 is sourced with it.
        let both = |slides: &mut Vec<answer::Slide>| {
            twice(slides);
            slides[6].source_ref = Some("pilot.csv".into());
        };
        assert!(unsourced(&grounded_dogfood("fleet", "", pilot(), both)).is_empty());
        // Made from more than one source, the finding names each, and claims none of them holds it.
        let mut two = pilot();
        two.push(scaena_narrato::Provenance {
            name: "season.pdf".into(),
            kind: "pdf".into(),
            url: None,
            bytes: 1,
            truncated: false,
        });
        let found = unsourced(&grounded_dogfood("fleet", "", two, |_| {}));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            found[0].1,
            "Slide 8 writes 38%, and cites neither of the deck's sources, pilot.csv and season.pdf, for it."
        );
        let shown = |text: &str| figures(text).into_iter().map(|f| (f.shown, f.value, f.unit)).collect::<Vec<_>>();
        assert!(shown("Q3 by product, p99 latency, Week 1, 3 steps, November 2026").is_empty());
        assert_eq!(shown("Costs fell to $1.4M (2.5x less)")[0], ("$1.4M".into(), "1.4".into(), "$".into()));
        // "40-mile" is the figure 40 with its unit, as "40 miles" is; dollars are "$".
        assert_eq!(shown("the 40-mile goal")[0], ("40 miles".into(), "40".into(), "mile".into()));
        assert_eq!(shown("40 miles")[0], ("40".into(), "40".into(), "mile".into()));
        assert_eq!(&shown("184,000 dollars")[0].1, &shown("$184,000")[0].1);
        assert_eq!(&shown("184,000 dollars")[0].2, &shown("$184,000")[0].2);
        // A German or French account's thousands of euros are one unit however written.
        for written in ["Pilotkosten: 240 TEUR", "240 k\u{20ac} im Jahr", "240k\u{20ac}", "240 KEUR"] {
            assert_eq!(shown(written)[0].1, "240", "{written}");
            assert_eq!(shown(written)[0].2, "k\u{20ac}", "{written}");
        }
        assert!(names_source("on page 11 of the season report", &[source_words("season-report.pdf")]));
        assert!(!names_source("the season was long", &[source_words("season-report.pdf")]));
    }

    /// A date's day is no figure to source, as each language writes a date (W2's health read:
    /// Valdemar's "Votación: 15 de diciembre" read as the figure 15); a count beside the words of
    /// one still is.
    #[test]
    fn a_dates_day_is_no_figure_to_source() {
        let shown = |text: &str| figures(text).into_iter().map(|f| f.shown).collect::<Vec<_>>();
        for date in [
            "Votación: 15 de diciembre",
            "Votação: 15 de dezembro",
            "Vote le 15 décembre",
            "Abstimmung am 15. Dezember",
            "Voto il 15 dicembre",
            "Vote on December 15, then 15 December, then Dec. 15th",
            "採決：12月15日",
            "表决：12月15日",
        ] {
            assert!(shown(date).is_empty(), "{date}: {:?}", shown(date));
        }
        // A count, or a figure beside a date, is read as before.
        assert_eq!(shown("15 de los 30 barrios"), ["15", "30"]);
        assert_eq!(shown("Votación: 15 de diciembre, por 2,3 M€"), ["2,3 M€"]);
        assert_eq!(shown("December saw 15 outages"), ["15"], "a month named before a count, not a day");
        assert_eq!(shown("A 40 de descuento"), ["40"]);
    }

    /// A time of day is no figure to source (the ferry deck, read against a reader): "a fourth
    /// sailing at 1:10", between "the full 11:40" and "the 3:30", states a timetable. Fleet's
    /// slide 8, made from its pilot and given those words in place of its figure, is asked
    /// for no source; given a figure beside them, it is asked for that one.
    #[test]
    fn a_time_of_day_is_no_figure_to_source() {
        let pilot = || {
            vec![scaena_narrato::Provenance {
                name: "pilot.csv".into(),
                kind: "csv".into(),
                url: None,
                bytes: 1,
                truncated: false,
            }]
        };
        let shown = |text: &str| figures(text).into_iter().map(|f| f.shown).collect::<Vec<_>>();
        assert!(shown("A fourth sailing at 1:10, between the full 11:40 and the 3:30 (1:10 pm daily)").is_empty());
        assert_eq!(shown("a 1:5 split, 24:00, 9:75"), ["1:5", "24:00", "9:75"], "not hours and minutes");
        let timed = |figure: &'static str| {
            move |slides: &mut Vec<answer::Slide>| {
                slides[7].slide_title = "A fourth sailing at 1:10".into();
                slides[7].layout_suggestion = "Bullet List".into();
                slides[7].key_bullets = vec![format!("Between the full 11:40 and the 3:30{figure}")];
            }
        };
        let unsourced = |deck: &Value| -> Vec<String> {
            (of(deck).findings.into_iter()).filter(|f| f.kind == Kind::Source && f.slide == 8).map(|f| f.says).collect()
        };
        let plain = unsourced(&grounded_dogfood("fleet", "", pilot(), timed("")));
        assert!(plain.is_empty(), "{plain:?}");
        let priced = unsourced(&grounded_dogfood("fleet", "", pilot(), timed(", for $1.1M a year")));
        assert!(priced.iter().any(|s| s.contains("writes $1.1M")), "{priced:?}");
    }

    /// A figure in a script set without spaces is read inside its phrase, as the builder reads it
    /// (health read of a Japanese deck): "3億円" in "建設費：3億円" with its magnitude and currency,
    /// and a percent set right after one with it; and none a date's or a time's counter follows,
    /// as a year and a clock are none: "2026年の歩み" was found unsourced on the cover, the slide's
    /// "3億円" never.
    #[test]
    fn a_figure_is_read_inside_a_phrase_a_script_sets_without_spaces() {
        let named = |text: &str| {
            figures(text).into_iter().map(|f| (f.shown, f.value, f.unit)).collect::<Vec<(String, String, String)>>()
        };
        let one = |shown: &str, value: &str, unit: &str| vec![(shown.to_string(), value.to_string(), unit.to_string())];
        assert_eq!(named("建設費：3億円"), one("3億円", "3", "億円"));
        assert_eq!(named("年間運営費：4,500万円"), one("4,500万円", "4500", "万円"));
        assert_eq!(named("国の補助金：運営費の50%"), one("50%", "50", "%"));
        assert_eq!(named("前年より18%増"), one("18%", "18", "%"));
        assert_eq!(named("12万人が図書館を利用しました"), one("12万", "12", "万"));
        for none in ["みどり市立図書館 2026年の歩み", "採決：12月10日", "夜9時までの開館"] {
            assert_eq!(named(none), [], "{none}");
        }
        // A word in a spaced script is read as before.
        assert_eq!(named("40 miles of trail, 38% more"), [one("40", "40", "mile"), one("38%", "38", "%")].concat());
    }

    /// A quoted sentence's full stop comes off before the finding's own sentence goes on, in a
    /// script set without spaces as in English (health read of a Japanese deck: the big idea was
    /// quoted "…用意します。”. Read the closing slide 7").
    #[test]
    fn a_quoted_sentence_loses_its_full_stop_in_any_script() {
        assert_eq!(
            unstopped("駅北の分館で、増え続ける利用者に席を用意します。"),
            "駅北の分館で、増え続ける利用者に席を用意します"
        );
        assert_eq!(unstopped("席を用意します｡ "), "席を用意します");
        assert_eq!(unstopped("Fund the branch library."), "Fund the branch library");
        assert_eq!(unstopped("本当にできますか？"), "本当にできますか？", "a question keeps its mark");
    }

    /// A figure a German or French slide sets apart from its percent, currency, or scale is named
    /// with them (R1's agent dogfood 3: Nordlicht's "40 % mehr Anfragen" was "writes 40"); an
    /// English figure is named as before, a word after it ("40 miles") left out.
    #[test]
    fn a_figure_is_named_with_the_marks_set_apart_after_it() {
        let named = |text: &str| figures(text).into_iter().map(|f| (f.shown, f.unit)).collect::<Vec<_>>();
        let one = |shown: &str, unit: &str| vec![(shown.to_string(), unit.to_string())];
        assert_eq!(named("betreut heute 40 % mehr Anfragen"), one("40 %", "%"));
        assert_eq!(named("Umsatz von 48,6 Mio. € im Quartal"), one("48,6 Mio. €", "mio"));
        assert_eq!(named("une hausse de 18 %, puis la suite"), one("18 %", "%"));
        // Spanish's magnitudes and currencies are named whole too (R2's Valdemar).
        assert_eq!(named("Inversi\u{f3}n de 2,3 M\u{20ac} en 2026"), one("2,3 M\u{20ac}", "m\u{20ac}"));
        assert_eq!(named("unos 1.234,5 millones de litros")[0].0, "1.234,5 millones");
        // A magnitude and its currency as one word, as the builder reads it, whole, thousands of
        // euros one unit however written (W1's #1080).
        assert_eq!(named("Kosten: 240 TEUR für zwei Stellen"), one("240 TEUR", "k\u{20ac}"));
        assert_eq!(named("Kosten: 240 k\u{20ac} für zwei Stellen"), one("240 k\u{20ac}", "k\u{20ac}"));
        assert_eq!(named("40 miles of trail, 38% more"), [one("40", "mile"), one("38%", "%")].concat());
        // On the German board deck, made from a source, slide 8's unsourced figure is named whole.
        let board = || {
            vec![scaena_narrato::Provenance {
                name: "bericht.pdf".into(),
                kind: "pdf".into(),
                url: None,
                bytes: 1,
                truncated: false,
            }]
        };
        let said = |slides: &mut Vec<answer::Slide>| {
            slides[7].slide_title = "Der Vertrieb ist voll ausgelastet".into();
            slides[7].key_bullets = vec!["Jede Stelle betreut 40 % mehr Anfragen".into()];
            slides[7].layout_suggestion = "Bullet List".into();
        };
        let h = of(&grounded_dogfood("board-de", "", board(), said));
        let source = h.findings.iter().find(|f| f.kind == Kind::Source && f.slide == 8).expect("slide 8's figure");
        let says = source.says.replace(['\u{a0}', '\u{202f}'], " ");
        assert!(says == "Slide 8 writes 40 %, and does not cite the deck's source, bericht.pdf, for it.", "{says}");
    }

    /// The close takes the slide before its run only where that slide is in the run's beat
    /// (W1's dogfood 4): the fleet pitch's proof chart, beat 4, stands before its closing, beat 5,
    /// and is not read as the close; moved into the closing beat, it is.
    #[test]
    fn the_close_takes_the_slide_before_it_only_in_its_beat() {
        let idea = "Ridgefleet pays for itself in a quarter by planning Northline's routes overnight.";
        let says = |h: &Health| h.findings.iter().find(|f| f.kind == Kind::BigIdea).expect("a bare close").says.clone();
        let alone = says(&of(&dogfood("fleet", idea, |_| {})));
        assert!(alone.starts_with("The close, slide 10, never lands"), "{alone}");
        let joined = |slides: &mut Vec<answer::Slide>| {
            slides[8].beat_index = slides[9].beat_index;
            slides[8].beat_name = slides[9].beat_name.clone();
        };
        let both = says(&of(&dogfood("fleet", idea, joined)));
        assert!(both.starts_with("The close, slides 9\u{2013}10, never lands"), "{both}");
    }

    /// A turn is its own pair's (W1's dogfood 5 and 6): read by words, a pair of slides turns where
    /// they share no word the overlap counts, so rewriting the close finds no new turn at the
    /// start, as reading against the deck's mean and spread did. The cover's step into the first
    /// slide is no turn, and coherence's figure is Vzestup's.
    #[test]
    fn a_turn_is_its_own_pairs() {
        let turned = |h: &Health| -> Vec<usize> {
            (h.findings.iter()).filter(|f| f.kind == Kind::Turn).map(|f| f.slide).collect()
        };
        // The pitch's close rewritten to say what the slide before it says: read against the
        // deck's mean and spread, its turns at slides 5, 6, and 12 went, and one came at 14.
        let before = of(&dogfood("pitch", "", |_| {}));
        let closed = |slides: &mut Vec<answer::Slide>| {
            let n = slides.len();
            let prev = slides[n - 2].clone();
            let last = &mut slides[n - 1];
            last.slide_title = prev.slide_title.clone();
            last.key_bullets = prev.key_bullets.clone();
        };
        let after = of(&dogfood("pitch", "", closed));
        let far = |t: Vec<usize>| t.into_iter().filter(|&k| k < 14).collect::<Vec<_>>();
        assert_eq!(far(turned(&before)), [5, 6, 12]);
        assert_eq!(far(turned(&after)), far(turned(&before)), "an edit to the close turns nothing before it");
        assert_eq!(after.coherence.score, before.coherence.score, "coherence's figure is Vzestup's");
        // Every pair that shares no word turns, past the cover, and no other does.
        let deck: Deck = serde_json::from_value(dogfood("fleet", "", |_| {})).unwrap();
        let read = readings(&deck, &slides(&deck)).unwrap().0;
        let sims = scaena_narrato::health::metrics(&read, None).coherence.pair_similarities;
        let bare: Vec<usize> = (1..sims.len()).filter(|&j| sims[j] == 0.0).map(|j| j + 2).collect();
        let found = turned(&of(&dogfood("fleet", "", |_| {})));
        assert!(!bare.is_empty() && bare.iter().all(|k| found.contains(k)), "{bare:?} {found:?}");
        assert!(!found.contains(&2), "the cover's step into the talk is no turn");
        // A turn at a beat's edge opens a new part, information named by its beat (W1's second
        // read of fleet); the sharp turn inside a beat is the warning, so it comes first.
        let fleet = of(&dogfood("fleet", "", |_| {}));
        let by = |severity: Severity| -> Vec<usize> {
            (fleet.findings.iter())
                .filter(|f| f.kind == Kind::Turn && f.severity == severity)
                .map(|f| f.slide)
                .collect()
        };
        assert_eq!(by(Severity::Warning), [9]);
        assert_eq!(by(Severity::Info), [3, 4, 5, 8, 10]);
        let opened = fleet.findings.iter().find(|f| f.kind == Kind::Turn && f.slide == 4).unwrap();
        assert_eq!(
            opened.says,
            "Slide 4 opens Existing Approaches with nothing from slide 3: say the line that carries the room across."
        );
        assert_eq!(
            opened.ask,
            "Open slide 4's notes with the line that carries the room from slide 3 into Existing Approaches, in a word of slide 3's, such as \u{201c}late\u{201d}, \u{201c}deliveries\u{201d} or \u{201c}cost\u{201d}: a connective alone (\u{201c}So\u{201d}, \u{201c}But\u{201d}) carries nothing across."
        );
        assert_eq!(fleet.coherence.score, None, "no figure moves");
        assert_eq!(
            fleet.coherence.says,
            "Read by their words: 1 slide where the talk turns sharply, 5 where a new part of the talk begins."
        );
        // A count of none is not said.
        let keynote = of(&dogfood("keynote", "", |_| {}));
        assert!(!keynote.coherence.says.contains("turns sharply"), "{}", keynote.coherence.says);
    }

    /// A headline that names a topic over the line that says its claim is a warning of its own
    /// kind, `label` (W1's dogfood 3): the keynote's third slide weakened to "Some history", its
    /// claim in a bullet, is asked for that line as its headline, with no patch; the keynote as
    /// written, and a topic with no claim under it, are not.
    #[test]
    fn a_topic_headline_over_its_claim_is_found() {
        let claim = "Port Alder had lost power four times in ten years before anyone acted";
        let buried = |slides: &mut Vec<answer::Slide>| {
            let third = &mut slides[2];
            third.slide_title = "Some history".into();
            third.layout_suggestion = "Bullets".into();
            third.key_bullets = vec![claim.into(), "Each time, the town waited on the utility".into()];
        };
        let topics = |h: &Health| -> Vec<Finding> {
            (h.findings.iter()).filter(|f| f.kind == Kind::Label && f.severity == Severity::Warning).cloned().collect()
        };
        let weak = of(&dogfood("keynote", "", buried));
        let found = topics(&weak);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].slide, 3);
        assert!(
            found[0]
                .says
                .starts_with("Slide 3's headline names its topic, \u{201C}Some history\u{201D}; its claim is under it"),
            "{}",
            found[0].says
        );
        // From the slide's list, it is moved up, not said again over itself (W1's dogfood 17).
        assert_eq!(
            found[0].ask,
            format!(
                "Make slide 3's claim its headline: move \u{201C}{claim}\u{201D} up from its list, in place of \u{201C}Some history\u{201D}, or say what the slide proves there."
            )
        );
        assert!(found[0].fix.is_none(), "the slide's words are the writer's to change (ADR-0032)");
        assert!(topics(&of(&dogfood("keynote", "", |_| {}))).is_empty(), "each slide is headed by its claim");
        // A topic with no line that says a claim under it is asked nothing of the kind.
        let bare = |slides: &mut Vec<answer::Slide>| {
            buried(slides);
            slides[2].key_bullets = vec!["Four outages".into(), "Ten years".into()];
        };
        assert!(topics(&of(&dogfood("keynote", "", bare))).is_empty());
        // One finding a slide: the label the plain topic headline gets is the warning here.
        assert_eq!(weak.findings.iter().filter(|f| f.kind == Kind::Label && f.slide == 3).count(), 1);
        // A line under it that no list sets is said as the headline, as before.
        let subtitled = |slides: &mut Vec<answer::Slide>| {
            buried(slides);
            slides[2].key_bullets.clear();
            slides[2].subtitle = claim.into();
        };
        let said = topics(&of(&dogfood("keynote", "", subtitled)));
        let said = said.first().unwrap_or_else(|| panic!("a subtitle's claim is found"));
        assert!(said.ask.contains(&format!("say \u{201C}{claim}\u{201D}, or what the slide proves")), "{}", said.ask);
    }

    /// Where no slide balance counts lists a point, balance says which slides it counted rather
    /// than "0 points each" (W1's dogfood 5): the grant renewal's content slides are big numbers
    /// and statements, which make one point by design, and its cover, chart, and close list none.
    /// The score is as before.
    #[test]
    fn balance_says_which_slides_it_counted_where_none_lists_points() {
        let h = of(&dogfood("grant", "", |_| {}));
        assert!(
            h.balance.says.starts_with(
                "No slide balance counts makes a list of points: slides 1, 7 and 9. The rest make one point by design"
            ),
            "{}",
            h.balance.says
        );
        // A deck whose counted slides list points says how many, as before.
        let fleet = of(&dogfood("fleet", "", |_| {}));
        assert!(!fleet.balance.says.contains("makes a list of points"), "{}", fleet.balance.says);
    }

    /// A one-idea slide with no notes is asked for its notes alone (W1's dogfood 6): a statement
    /// makes its point in its headline by design, so health never asks to add to the slide.
    #[test]
    fn a_one_idea_slide_is_asked_for_notes_alone() {
        let bare = |slides: &mut Vec<answer::Slide>| {
            let s = slides.iter_mut().find(|s| s.layout_suggestion == "Statement").unwrap();
            s.subtitle.clear();
            s.speaker_notes.clear();
        };
        let deck = dogfood("keynote", "", bare);
        let h = of(&deck);
        let empty: Vec<&Finding> = h.findings.iter().filter(|f| f.kind == Kind::Empty).collect();
        assert_eq!(empty.len(), 1, "{:#?}", h.findings);
        let k = empty[0].slide;
        assert_eq!(empty[0].says, format!("Slide {k} makes its one point in its headline, and has no notes."));
        assert_eq!(empty[0].ask, format!("Add notes to slide {k} that say what to tell the room."));
    }

    /// The close's words are not asked to carry its plan's intensity (W1's dogfood 6): the
    /// keynote's close weakened to "Thank you / Questions?" and planned calm reads far hotter by
    /// its few words, and is asked nothing of it; intensity's figure is as before.
    #[test]
    fn a_bare_thanks_is_not_asked_to_calm() {
        let thanked = |slides: &mut Vec<answer::Slide>| {
            let last = slides.last_mut().unwrap();
            last.slide_title = "Thank you".into();
            last.key_bullets = vec!["Questions?".into()];
            last.speaker_notes = "Thank you. Questions?".into();
            // Planned calm, as a thanks is.
            last.expected_intensity = Some(0.4);
        };
        let h = of(&dogfood("keynote", "", thanked));
        let n = h.slides.len();
        let read = h.slides[n - 1].read.expect("the lexicon reads the thanks");
        assert!(read - 0.4 > INTENSITY_GAP_THRESHOLD, "the thanks reads {read}, far from its plan");
        assert!(!h.findings.iter().any(|f| f.kind == Kind::Intensity && f.slide == n), "{:#?}", h.findings);
    }

    /// In a deck in another language, a claim's part is named in the deck's words, by the section
    /// its slides stand in, never by the arc's English name (W1's dogfood 7).
    #[test]
    fn a_claim_names_its_part_in_the_decks_language() {
        let mut deck = dogfood("board-de", "", |_| {});
        deck["meta"]["narrative"]["brief"]["language"] = json!("German");
        let beats = deck["meta"]["narrative"]["outline"]["beats"].as_array().unwrap().len();
        let last = beats - 1;
        // The outline's name is the arc's, in English, as W1's deck had it.
        deck["meta"]["narrative"]["outline"]["beats"][last]["name"] = json!("Call to Action");
        deck["meta"]["narrative"]["outline"]["beats"][last]["claim"] = json!("Pinguine jonglieren leise mit Marmelade");
        let h = of(&deck);
        let claim = h.findings.iter().find(|f| f.kind == Kind::Claim).expect("a claim its slides do not carry");
        let sections = deck["spine"]["sections"].as_array().unwrap();
        let title = sections.last().unwrap()["title"].as_str().expect("the section's title").to_string();
        assert_ne!(title, "Call to Action");
        assert!(
            claim.says.contains(&format!("settled for the {title} part of the talk"))
                && !claim.says.contains("Call to Action"),
            "{}",
            claim.says
        );
        // In English, the outline's name, as before.
        deck["meta"]["narrative"]["brief"]["language"] = json!("English");
        let english = of(&deck);
        let claim = english.findings.iter().find(|f| f.kind == Kind::Claim).unwrap();
        assert!(claim.says.contains("the Call to Action part of the talk"), "{}", claim.says);
    }

    /// A close in another language is not read for the big idea (the coordinator's check on W1's
    /// dogfood 3): the German board deck's close says its idea, "drei Beschlüsse", which English's
    /// telling words cannot see, so health says it reads the close only in English, an info, and
    /// warns of nothing.
    #[test]
    fn a_close_in_another_language_is_not_read_for_the_big_idea() {
        let mut deck = dogfood("board-de", "Der Bericht schließt mit drei Beschlüssen", |_| {});
        deck["meta"]["narrative"]["brief"]["language"] = json!("German");
        let h = of(&deck);
        let big: Vec<&Finding> = h.findings.iter().filter(|f| f.kind == Kind::BigIdea).collect();
        assert_eq!(big.len(), 1, "{big:?}");
        assert_eq!(big[0].severity, Severity::Info);
        // Said plainly, in the deck's language's name, as what it cannot do (W1's dogfood 7).
        assert!(
            big[0].says.ends_with("isn't checked for the big idea in German: health reads it in English decks only."),
            "{}",
            big[0].says
        );
        // A close that never says the idea is told the same, and warned of nothing: a loose match
        // of German words is never a finding (Jay's call, 2026-10-25: health says nothing it did
        // not read).
        deck["meta"]["narrative"]["outline"]["bigIdea"] = json!("Pinguine jonglieren leise mit Marmelade");
        let h = of(&deck);
        let big: Vec<&Finding> = h.findings.iter().filter(|f| f.kind == Kind::BigIdea).collect();
        assert_eq!(big.len(), 1, "{big:?}");
        assert_eq!(big[0].severity, Severity::Info);
    }

    /// Scaena's own checks read words in any script (dogfood pass 11): a French claim keeps its
    /// accented words whole, "coûtera" no "tera" and "émissions" no "missions", and a figure alone
    /// is no word. A deck in Japanese, Chinese, or Korean is not read for its claims, and says so
    /// once, an info; its claims, each one long run of letters, would otherwise never be found.
    #[test]
    fn claims_are_read_by_words_in_any_script_and_not_where_no_space_sets_them_apart() {
        let words = content_words("Ne rien faire coûtera plus cher qu'investir 86 M€ ; −40 % d'émissions en 2030");
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        assert_eq!(words, ["cher", "coûtera", "faire", "investir", "plus", "rien", "émissions"]);
        assert!(content_words("倉庫の自動化と拠点の統合で、2027年度に戻す").is_empty());
        assert!(content_words("Schließt").contains("schließt"));
        assert!((spaceless(Some("ja")), spaceless(Some("zh-Hant")), spaceless(Some("Korean"))) == (true, true, true));
        assert!(!spaceless(Some("fr")) && !spaceless(Some("English")) && !spaceless(None));

        let mut deck = climate();
        let h = of(&deck);
        let beats: Vec<Value> =
            (h.beats.iter()).map(|name| json!({ "name": name, "claim": "倉庫を自動化する" })).collect();
        deck["meta"]["narrative"]["outline"] = json!({ "beats": beats });
        deck["meta"]["narrative"]["brief"]["language"] = json!("Japanese");
        let claims: Vec<Finding> = of(&deck).findings.into_iter().filter(|f| f.kind == Kind::Claim).collect();
        assert_eq!(claims.len(), 1, "{claims:?}");
        assert_eq!(claims[0].severity, Severity::Info);
        assert!(claims[0].says.ends_with("and this deck is in Japanese."), "{}", claims[0].says);
    }

    /// A deck in Japanese, Chinese, or Korean is not read for its turns (health read of a Japanese
    /// deck): each headline, one long run of letters, read as one word, so every pair of slides
    /// shared none and turned, and the ask named a whole headline as the word to carry across. The
    /// coherence line says why, as intensity and tone say theirs; its figure, none by words, stays.
    #[test]
    fn turns_are_not_read_where_no_space_sets_words_apart() {
        let japanese = |slides: &mut Vec<answer::Slide>| {
            let titles = ["売上は前年より伸びました", "本館はもう満席です", "利用が増えた理由は三つあります"];
            for (k, s) in slides.iter_mut().enumerate() {
                s.slide_title = format!("{}（{}）", titles[k % titles.len()], k + 1);
                s.key_bullets = vec![];
                s.subtitle.clear();
                s.speaker_notes = String::new();
            }
        };
        let mut deck = dogfood("board-de", "", japanese);
        let turned = |h: &Health| h.findings.iter().filter(|f| f.kind == Kind::Turn).count();
        deck["meta"]["narrative"]["brief"]["language"] = json!("German");
        let read = of(&deck);
        assert!(turned(&read) > 0, "read as words, every pair turns: {:#?}", read.findings);
        deck["meta"]["narrative"]["brief"]["language"] = json!("Japanese");
        let h = of(&deck);
        assert_eq!(turned(&h), 0, "{:#?}", h.findings);
        assert_eq!(
            h.coherence.says,
            "Not read by words: health reads words a space sets apart, and this deck is in Japanese."
        );
        assert_eq!((h.coherence.score, h.score), (read.coherence.score, read.score));
    }

    /// The settled outline read (lift-up 10-24b FUNC-1): a deck whose beats each say their claim
    /// and whose close says its big idea is found carrying both, and the big idea is given for the
    /// editors to show; a claim no slide of its beat says, and a big idea the close never says,
    /// are each found on the beat's first slide, with what to ask.
    /// A part's slides that share with its claim only a word the deck's title holds, or one
    /// every part's slides say, carry none of its promise (W2's outlined calibration: larder's
    /// Solution part shared the product's name with its claim, and was missed).
    #[test]
    fn a_claim_carried_only_by_a_shared_name_is_not_carried() {
        let mut deck = climate();
        let h = of(&deck);
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
        deck["meta"]["narrative"]["outline"] = json!({ "bigIdea": h.slides[h.slides.len() - 1].title, "beats": beats });
        // A word beat 1's headline says, made the deck's title, beside words no slide says.
        let name = content_words(&h.slides[first_of(1)].title).into_iter().next().expect("a word of beat 1's headline");
        deck["meta"]["title"] = json!(format!("The {name} talk"));
        deck["meta"]["narrative"]["outline"]["beats"][1]["claim"] = json!(format!("{name} penguins juggle marmalade"));
        let found = of(&deck);
        assert!(
            found.findings.iter().any(|f| f.kind == Kind::Claim && f.says.contains("penguins juggle marmalade")),
            "{name}: {:#?}",
            found.findings.iter().filter(|f| f.kind == Kind::Claim).collect::<Vec<_>>()
        );
        // A claim of the title's words alone says nothing either way.
        deck["meta"]["narrative"]["outline"]["beats"][1]["claim"] = json!(name);
        assert!(!of(&deck).findings.iter().any(|f| f.kind == Kind::Claim), "a claim of shared words alone");
    }

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
        // The close is the deck's last run of closing slides: the big idea its last slide's headline.
        let close = h.slides.len() - 1;
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
        deck["meta"]["narrative"]["outline"]["beats"][1]["claim"] = json!("Penguins juggle marmalade quietly.");
        deck["meta"]["narrative"]["outline"]["bigIdea"] = json!("Zebras orchestrate symphonies underwater");
        let missed = of(&deck);
        assert_eq!(
            outlined(&missed),
            // The close's finding on its first slide: the next steps before Questions and Closing.
            [(Kind::Claim, first_of(1) + 1), (Kind::BigIdea, close - 1)],
            "{:?}",
            missed.findings
        );
        let claim = missed.findings.iter().find(|f| f.kind == Kind::Claim).unwrap();
        // The beat named once, as a part of the talk, never as a heading over its claim.
        assert!(
            claim.says.ends_with(&format!(
                "settled for the {second} part of the talk, \u{201c}Penguins juggle marmalade quietly\u{201d}."
            )),
            "{}",
            claim.says
        );
        assert!(claim.ask.starts_with("Check the claim: the outline settled that this part"), "{}", claim.ask);
        assert!(!claim.ask.contains(&second), "{}", claim.ask);
        // A claim settled with its stop is quoted without it, so no sentence ends twice.
        assert!(claim.ask.contains("\u{201c}Penguins juggle marmalade quietly\u{201d}. Rewrite"), "{}", claim.ask);
        let big = missed.findings.iter().find(|f| f.kind == Kind::BigIdea).unwrap();
        assert!(big.says.starts_with("The close, slide") && big.ask.starts_with("The big idea is"), "{big:?}");
        // A claim settled for another part than the close is asked of its own slides alone.
        assert!(!claim.ask.contains("Zebras") && !big.ask.contains("Penguins"), "{}\n{}", claim.ask, big.ask);
        // The close is asked for the big idea and its part's claim each with the other, whether it
        // carries the other or not, so a close rewritten for one keeps the other (W1's dogfood 17:
        // a close headed by its part's claim missed the idea, and one rewritten to the idea missed
        // the claim, four times).
        let closing_beat = written[close];
        let idea = h.slides[close].title.trim().trim_end_matches('.').to_string();
        // The claim carried, the idea missed: the big idea's ask keeps the claim.
        let kept = h.slides[first_of(closing_beat)].title.trim().trim_end_matches('.').to_string();
        assert!(
            big.ask.ends_with(&format!(
                "so the talk ends by landing it, while it carries the claim the outline settled for its part, \u{201c}{kept}\u{201d}."
            )),
            "{}",
            big.ask
        );
        // The idea landed, the claim missed: the claim's ask keeps the idea.
        let mut landed = deck.clone();
        landed["meta"]["narrative"]["outline"]["bigIdea"] = json!(h.slides[close].title);
        landed["meta"]["narrative"]["outline"]["beats"][closing_beat]["claim"] =
            json!("Fund the plan in the 2027 budget.");
        let landed = of(&landed);
        assert!(!landed.findings.iter().any(|f| f.kind == Kind::BigIdea), "{:#?}", landed.findings);
        let claim = (landed.findings.iter())
            .find(|f| f.kind == Kind::Claim && f.says.contains("Fund the plan"))
            .unwrap_or_else(|| panic!("{:#?}", landed.findings));
        assert!(
            claim.ask.contains(&format!(
                "\u{201c}Fund the plan in the 2027 budget\u{201d}, and the close is to land the big idea too, \u{201c}{idea}\u{201d}. Rewrite"
            )) && claim.ask.contains(" both, or tell me what to settle instead."),
            "{}",
            claim.ask
        );
        // Both missed: each is asked with the other.
        let mut both = deck.clone();
        both["meta"]["narrative"]["outline"]["beats"][closing_beat]["claim"] =
            json!("Fund the plan in the 2027 budget.");
        let twice = of(&both);
        let claim = (twice.findings.iter())
            .find(|f| f.kind == Kind::Claim && f.says.contains("Fund the plan"))
            .unwrap_or_else(|| panic!("{:#?}", twice.findings));
        let big = twice.findings.iter().find(|f| f.kind == Kind::BigIdea).unwrap();
        assert!(
            claim.ask.contains(
                "and the close is to land the big idea too, \u{201c}Zebras orchestrate symphonies underwater\u{201d}"
            ) && claim.ask.contains(" both, or tell me"),
            "{}",
            claim.ask
        );
        assert!(
            big.ask.ends_with(
                "while it carries the claim the outline settled for its part, \u{201c}Fund the plan in the 2027 budget\u{201d}."
            ),
            "{}",
            big.ask
        );
        // What each finds is as it was: the same findings, on the same slides.
        let found = |h: &Health| -> Vec<(Kind, usize, String)> {
            (h.findings.iter())
                .filter(|f| matches!(f.kind, Kind::Claim | Kind::BigIdea))
                .map(|f| (f.kind, f.slide, f.says.clone()))
                .collect()
        };
        assert_eq!(found(&twice).len(), 3, "{:#?}", found(&twice));
        // A big idea in another script has no words to read by: nothing is found of it.
        deck["meta"]["narrative"]["outline"]["bigIdea"] = json!("気候変動は止められる");
        assert!(!of(&deck).findings.iter().any(|f| f.kind == Kind::BigIdea));
    }

    /// A part of the talk that moves from one figure to the next is one move, though the slides
    /// share no word (the calibration against a reader): on the QBR, the turns between its metric
    /// slides, a number grid, a line, a bar, and a funnel, are information, with the turn's ask,
    /// and the coherence line counts them apart; a turn into a slide that shows no measure stays a
    /// warning. No figure moves.
    #[test]
    fn a_run_of_figures_in_one_part_is_one_move() {
        let h = of(&dogfood("qbr", "", |_| {}));
        let turn = |k: usize| h.findings.iter().find(|f| f.kind == Kind::Turn && f.slide == k).expect("a turn");
        for k in [4, 6, 7] {
            let f = turn(k);
            assert_eq!(f.severity, Severity::Info, "{f:#?}");
            assert_eq!(
                f.says,
                format!("Slide {k} shows another figure with no word from slide {}: say what links the two.", k - 1)
            );
            assert!(
                f.ask.starts_with(&format!("Open slide {k}'s notes with the line that carries the talk")),
                "{}",
                f.ask
            );
        }
        assert_eq!(turn(9).severity, Severity::Warning, "a turn into the scenarios is the room's to follow");
        assert!(h.coherence.says.contains("3 where one figure follows another"), "{}", h.coherence.says);
        assert_eq!(h.coherence.score, None, "no figure moves");
    }

    /// A talk timed by its words alone, every beat's `duration` still the estimate a made deck
    /// wrote, is a floor (the calibration against a reader): said as at least so long, never found
    /// short of its slot, whatever its brief asks, and still found long past it. A beat a person
    /// times makes the talk timed, held to its slot as before.
    #[test]
    fn a_talk_timed_by_its_words_alone_is_a_floor() {
        let short = |h: &Health| h.findings.iter().any(|f| f.kind == Kind::Pacing && f.says.contains("short of"));
        let deck = dogfood("fleet", "", |_| {});
        let h = of(&deck);
        assert!(
            h.pacing.says.contains("At least ") && h.pacing.says.contains("; no beat is timed."),
            "{}",
            h.pacing.says
        );
        assert!(!short(&h), "{:#?}", h.findings);
        // A brief that asks minutes is a plan for the talk, not for its beats: still no floor found short.
        let mut asked = deck.clone();
        asked["meta"]["narrative"]["brief"]["minutes"] = json!(30);
        let h = of(&asked);
        assert!(!short(&h), "{:#?}", h.findings);
        assert!(h.pacing.says.contains("where its brief asks 30 minutes; no beat is timed."), "{}", h.pacing.says);
        // Words past the slot are found long all the same.
        let mut long = asked.clone();
        let at = beat_of(&long, long["states"][1]["id"].as_str().unwrap());
        long.pointer_mut(&at).unwrap()["notes"] = json!("word ".repeat(140 * 40));
        let h = of(&long);
        assert!(h.findings.iter().any(|f| f.kind == Kind::Pacing && f.says.contains(", past ")), "{:#?}", h.findings);
        // One beat a person times, its duration no longer the estimate: the talk is timed, and
        // short of its slot as before.
        let mut timed = asked.clone();
        timed.pointer_mut(&at).unwrap()["duration"] = json!(60);
        let h = of(&timed);
        assert!(h.pacing.says.contains(" of talk, where its brief asks 30 minutes."), "{}", h.pacing.says);
        assert!(short(&h), "{:#?}", h.findings);
        // A deck saved before the estimate was kept reads its durations as given, as it did.
        let mut older = asked;
        for section in older["spine"]["sections"].as_array_mut().unwrap() {
            for beat in section["beats"].as_array_mut().unwrap() {
                beat["media"]["narrato"].as_object_mut().unwrap().remove("estimated");
            }
        }
        assert!(short(&of(&older)));
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
            // The arc's beat it was written for, the band the graph draws it in (lift-up ATTR-4).
            let part = beat["media"]["narrato"]["beatIndex"].as_u64().map(|k| k as usize);
            assert_eq!(read.part, part);
            assert!(part.is_some_and(|k| k < h.beats.len()), "{part:?} of {:?}", h.beats);
        }
        // Asked for 16 to 25 slides, it has 25: paced as asked, its beats a short talk for them.
        assert_eq!(
            h.pacing,
            Measure {
                score: Some(100),
                says: "25 slides, as the brief asked (16\u{2013}25). At least 8 minutes of talk from its words, where 16\u{2013}25 slides is a 31\u{2013}45 minute slot; no beat is timed. Its notes as written run about 1 minute.".into()
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

    /// A beat named as a heading and its gloss is said by its heading wherever health names it
    /// (W1's dogfood 8), "Existing Approaches: what teams use today" as "Existing Approaches",
    /// the whole name kept in the data; a name with nothing on one side of its colon is said whole.
    #[test]
    fn a_beat_is_said_by_its_heading() {
        assert_eq!(headed("Solution: Introduce your answer"), "Solution");
        assert_eq!(headed("  Impact: Quantify cost of inaction "), "Impact");
        assert_eq!(headed("Call to Action"), "Call to Action");
        assert_eq!(headed("Note:"), "Note:");
        assert_eq!(headed(": a gloss alone"), ": a gloss alone");

        let mut deck = dogfood("fleet", "", |_| {});
        let beats = deck.pointer_mut("/meta/narrative/outline/beats").and_then(Value::as_array_mut).unwrap();
        let named = beats.iter_mut().find(|b| b["name"] == "Existing Approaches").expect("fleet's beat");
        named["name"] = json!("Existing Approaches: what teams use today");
        let h = of(&deck);
        let opened = h.findings.iter().find(|f| f.kind == Kind::Turn && f.slide == 4).expect("the turn into it");
        assert!(opened.says.starts_with("Slide 4 opens Existing Approaches with nothing"), "{}", opened.says);
        assert!(opened.ask.contains(" into Existing Approaches, "), "{}", opened.ask);
        assert!(!h.findings.iter().any(|f| f.says.contains("what teams use") || f.ask.contains("what teams use")));
        assert!(
            h.beats.iter().any(|b| b == "Existing Approaches: what teams use today"),
            "the whole name stays in the data: {:?}",
            h.beats
        );
    }

    /// Each finding that asks for one field of its slide says which (W2's editor dogfood 6), as its
    /// ask asks: a headline, notes, an intensity planned, or a part's claim; none where it asks for
    /// the slide's words at large, a cut, a move, or a source. On the dogfood decks and the climate
    /// talk, each kind keeps to its field, and every field is said somewhere.
    #[test]
    fn a_finding_names_the_field_it_asks_to_edit() {
        let mut decks: Vec<Value> = ["appeal", "fleet", "grant", "keynote", "lecture", "pitch", "qbr", "training"]
            .iter()
            .map(|name| dogfood(name, "", |_| {}))
            .collect();
        decks.push(climate());
        // A one-idea slide with nothing to say: its notes alone.
        decks.push(dogfood("keynote", "", |slides| {
            for s in slides.iter_mut() {
                s.speaker_notes.clear();
            }
        }));
        let mut seen: Vec<Field> = Vec::new();
        for deck in &decks {
            for f in of(deck).findings {
                let want = match f.kind {
                    Kind::Title | Kind::Label => Some(Field::Headline),
                    Kind::Turn | Kind::Notes => Some(Field::Notes),
                    Kind::Empty => f.ask.starts_with("Add notes").then_some(Field::Notes),
                    Kind::Pacing => f.ask.starts_with("Deepen the notes").then_some(Field::Notes),
                    Kind::Arc => (!f.says.contains("after the talk's peak")).then_some(Field::Intensity),
                    Kind::Intensity => f.ask.contains("set its Intensity").then_some(Field::Intensity),
                    Kind::Claim => (f.severity == Severity::Warning).then_some(Field::Claim),
                    _ => None,
                };
                assert_eq!(f.field, want, "{f:#?}");
                if let Some(field) = f.field
                    && !seen.contains(&field)
                {
                    seen.push(field);
                }
                if f.field == Some(Field::Intensity) {
                    assert!(f.kind != Kind::Arc || f.path.is_some(), "an arc's plan is where its path says: {f:#?}");
                }
            }
        }
        for field in [Field::Headline, Field::Notes, Field::Intensity] {
            assert!(seen.contains(&field), "{field:?} said nowhere: {seen:?}");
        }
        assert_eq!(serde_json::to_value(Field::Headline).unwrap(), json!("headline"));
    }

    /// A long tail (the calibration against a reader): slides that run on after the talk's peak
    /// and before its close are information on the first of them, each slide named, asking for a
    /// cut or a move and naming no field. A divider among them opens a new part of the talk, so
    /// climate's Response, after its peak, is no tail.
    #[test]
    fn a_talk_that_runs_on_after_its_peak_is_asked_to_end_near_it() {
        let tail = |deck: &Value| {
            of(deck)
                .findings
                .into_iter()
                .filter(|f| f.kind == Kind::Arc && f.says.contains("after the talk's peak"))
                .collect::<Vec<_>>()
        };
        let launch = tail(&dogfood("launch", "", |_| {}));
        assert_eq!(launch.len(), 1, "{launch:#?}");
        let f = &launch[0];
        assert_eq!(f.severity, Severity::Info);
        assert_eq!(f.slide, 11);
        assert!(
            f.says.starts_with(
                "Slides 11\u{2013}13 come after the talk's peak at slide 10 and before its close at slide 14"
            ),
            "{}",
            f.says
        );
        assert!(
            f.ask.starts_with(
                "Cut slides 11\u{2013}13, fold what they must say into the close, or move them before slide 10"
            ),
            "{}",
            f.ask
        );
        assert_eq!((f.field, f.path.as_ref()), (None, None), "{f:#?}");
        // The ask before the thanks is the close's.
        let appeal = tail(&dogfood("appeal", "", |_| {}));
        assert_eq!(appeal.len(), 1, "{appeal:#?}");
        assert!(
            appeal[0].says.starts_with(
                "Slides 7\u{2013}9 come after the talk's peak at slide 6 and before its close at slide 10"
            ),
            "{}",
            appeal[0].says
        );
        // Climate runs seven slides on after its peak, but a divider opens them as a new part.
        assert!(tail(&climate()).is_empty());
        // So do the decks a reader finds no tail in.
        for name in ["keynote", "pitch", "qbr", "lecture", "grant", "training", "deepdive", "fleet", "board-de"] {
            assert!(tail(&dogfood(name, "", |_| {})).is_empty(), "{name}");
        }
        // Planned to build to its close, launch has no tail.
        let built = tail(&dogfood("launch", "", |slides| {
            for s in slides.iter_mut().skip(10).take(3) {
                s.expected_intensity = Some(0.85);
            }
        }));
        assert!(built.is_empty(), "{built:#?}");
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
        // It names the words its reading was made of (R1's MCP dogfood), as a tone ask does.
        let d = Deck::from_value(&deck).unwrap();
        let read = readings(&d, &slides(&d)).unwrap().0;
        // The heading the slide shows, as health reads it (SPEC §3.12).
        let snaps = resolve_states(&d).unwrap();
        let shown = scaena_core::reading::headline(&d, &snaps[*slides(&d)[i].states.last().unwrap()]);
        let headline = shown.unwrap_or_else(|| read[i].title.clone());
        let heard = health::intensity::heard(&read[i].text(), None).unwrap_or_default();
        let named = intensed(&heard);
        assert!(!named.is_empty(), "words that read an intensity: {heard:?}");
        // And where they stand, after them (W2's editor dogfood 6).
        let placed = whereabouts(&heard, &headline, &read[i]);
        assert!(!placed.is_empty());
        assert!(finding.ask.ends_with(&format!("{named}{placed}")), "{}", finding.ask);
        // The plan is the arc's: the words are to carry it, so no fix declares what they read
        // in its place.
        assert_eq!(finding.fix, None);
    }

    /// A tone or intensity ask says where the words it names stand (W2's editor dogfood 6): its
    /// reading is the slide's title, body, and notes, and its title is the beat's claim, so a word
    /// left only in the claim after the headline was retyped, or only in the notes, is said to be
    /// there, the claim's with its Update.
    #[test]
    fn a_reading_says_where_its_words_stand() {
        let slide = SlideForHealth {
            title: "Deliveries missed and nobody knew".into(),
            body: "\u{2022} Late trucks every week".into(),
            notes: "We were behind on every route.".into(),
            ..SlideForHealth::default()
        };
        let heard = vec![
            ("missed".to_string(), -1.0),
            ("late".to_string(), -0.8),
            ("behind".to_string(), -0.6),
            ("knew".to_string(), 0.2),
        ];
        assert_eq!(
            whereabouts(&heard, "Deliveries and nobody knew", &slide),
            " \u{201c}knew\u{201d} is in its headline; \u{201c}late\u{201d} is elsewhere on the slide; \u{201c}behind\u{201d} is in its notes; \u{201c}missed\u{201d} is only in its story's claim, which the slide no longer says (Update it to the slide's words)."
        );
        assert_eq!(whereabouts(&[], "Anything", &slide), "");
        let marked = vec![("?".to_string(), 1.0)];
        assert_eq!(whereabouts(&marked, "Ready?", &slide), " Its question mark is in its headline.");
    }

    /// An intensity ask names its words as a tone ask does, heated first, a mark as the mark it
    /// is, each once and at most four a side (R1's MCP dogfood).
    #[test]
    fn an_intensity_ask_names_what_it_heard() {
        let w = |pairs: &[(&str, f64)]| pairs.iter().map(|(w, n)| (w.to_string(), *n)).collect::<Vec<_>>();
        assert_eq!(
            intensed(&w(&[("you", 1.2), ("?", 1.0), ("you", 1.2), ("summary", -0.5)])),
            " Health reads \u{201c}you\u{201d} and its question mark as heated, and \u{201c}summary\u{201d} as calm."
        );
        assert_eq!(
            intensed(&w(&[("record", 2.0)])),
            " Health reads \u{201c}record\u{201d} as heated, and no word as calm."
        );
        assert_eq!(
            intensed(&w(&[("overview", -0.5)])),
            " Health reads \u{201c}overview\u{201d} as calm, and no word as heated."
        );
        assert_eq!(intensed(&[]), "");
        assert_eq!(
            toned(&w(&[("missed", -1.0), ("win", 0.5)])),
            " Health reads \u{201c}missed\u{201d} as grave, and \u{201c}win\u{201d} as hopeful.",
            "a tone ask reads as before"
        );
    }

    /// A close is known by its plan as well as its layout (health read of a Spanish deck): a slide
    /// its beat planned as a Closing, a Call to Action, Next Steps, or Q&A closes the talk though
    /// its items were set in another layout, so a calm plan there is no bend. Planned as anything
    /// else, the same slide bends. Findings only: fidelity counts every slide, so no figure moves.
    #[test]
    fn a_close_is_known_by_its_plan_whatever_layout_it_was_set_in() {
        let mut deck = climate();
        let before = of(&deck);
        let last = before.slides.last().unwrap().state.clone();
        let title = before.slides.last().unwrap().title.to_lowercase();
        assert!(!THANKS.iter().any(|t| title.starts_with(t)), "the close is not known by its headline: {title}");
        let at = deck["states"].as_array().unwrap().iter().position(|s| s["id"] == last.as_str()).unwrap();
        deck["states"][at]["layout"] = json!("bullets");
        let beat = beat_of(&deck, &last);
        *deck.pointer_mut(&format!("{beat}/media/narrato/intensity")).unwrap() = json!(0.0);
        let cooler =
            |h: &Health| h.findings.iter().any(|f| f.state == last && f.kind == Kind::Arc && f.says.contains("cooler"));
        deck.pointer_mut(&format!("{beat}/media/narrato")).unwrap()["layout"] = json!("Bullets");
        let open = of(&deck);
        assert!(cooler(&open), "planned as a list, a calm last slide bends: {:#?}", open.findings);
        for plan in ["Closing", "Call to Action", "Next Steps", "Q&A"] {
            deck.pointer_mut(&format!("{beat}/media/narrato")).unwrap()["layout"] = json!(plan);
            let closed = of(&deck);
            assert!(!cooler(&closed), "planned as {plan}, it closes the talk: {:#?}", closed.findings);
            assert_eq!(
                (closed.fidelity.score, closed.score, closed.balance.score),
                (open.fidelity.score, open.score, open.balance.score),
                "no figure moves"
            );
        }
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
        // The same slide in a layout that does not close, planned as none, and headed as no
        // thanks, bends.
        deck["states"][at]["layout"] = json!("statement");
        let beat = beat_of(&deck, &last);
        deck.pointer_mut(&format!("{beat}/media/narrato")).unwrap()["layout"] = json!("Statement");
        let open = of(&deck);
        let title = open.slides.last().unwrap().title.to_lowercase();
        if !THANKS.iter().any(|t| title.starts_with(t)) {
            assert!(cooler(&open, &last), "the same plan away from a close bends: {:#?}", open.findings);
        }
        assert_eq!(closed.fidelity.score, open.fidelity.score, "no figure moves");
        // Next steps before the Closing, planned at rest, are part of the close; the floor opened
        // too (dogfood pass 9). A slide before them that closes nothing still bends.
        let thanks = |i: usize| THANKS.iter().any(|t| before.slides[i].title.to_lowercase().starts_with(t));
        let n = before.slides.len();
        let k = (2..n - 1).rev().find(|&k| !thanks(k) && !thanks(k - 1)).unwrap();
        let [steps, ask] = [k, k - 1].map(|i| before.slides[i].state.clone());
        let layouts = |deck: &mut Value, step: &str| {
            for (i, slide) in before.slides.iter().enumerate().skip(k - 1) {
                let layout = match i {
                    i if i == k - 1 => "statement",
                    i if i == k => step,
                    _ => "closing",
                };
                let at =
                    (deck["states"].as_array().unwrap().iter()).position(|s| s["id"] == slide.state.as_str()).unwrap();
                deck["states"][at]["layout"] = json!(layout);
                declare(deck, &slide.state, "intensity", 0.0);
                // Planned as it is laid out, so the slide before the close is planned as none.
                let beat = beat_of(deck, &slide.state);
                deck.pointer_mut(&format!("{beat}/media/narrato")).unwrap()["layout"] = json!(layout);
            }
        };
        for step in ["next-steps", "qa"] {
            let mut deck = climate();
            layouts(&mut deck, step);
            let h = of(&deck);
            assert!(!cooler(&h, &steps), "{step} before the close is no bend: {:#?}", h.findings);
            assert!(cooler(&h, &ask), "a slide before the close that closes nothing bends: {:#?}", h.findings);
        }
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
            bend.ask.contains(&format!("is planned at {plan:.2}"))
                && (bend.ask.contains(&format!("plan it {way}"))
                    || bend.ask.contains("as near its place as its words carry")),
            "{}",
            bend.ask
        );
        // It asks the plan or the place, never the words (ADR-0032 item 4): what they read is why.
        assert!(!bend.ask.contains("its words and") && !bend.ask.to_lowercase().contains("rewrite"), "{}", bend.ask);
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
            // A plan that strays, its words as its place calls for, is the warning's alone, its bend
            // folded into it (W2's editor dogfood 6).
            let folded = f.ask.contains("it is its plan");
            assert!(bends || folded || f.ask.contains("intensity it declares"), "{}", f.ask);
        }
        // Only what the findings say changed: every figure is as before but the plan's.
        assert_eq!((h.pacing.clone(), h.balance.clone()), (before.pacing, before.balance));

        // The ask names the control that plans it (editor dogfood 3), which the inspector offers
        // with nothing selected: the beat's plan, by JSON Patch where it lives. Planned as its place
        // calls for, it bends no more.
        assert!(bend.ask.contains("by its Intensity"), "{}", bend.ask);
        let d = Deck::from_value(&deck).unwrap();
        let dusk = std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap();
        let theme: Theme = serde_json::from_str(&dusk).unwrap();
        let offered = scaena_core::choices::state_choices(&d, &theme, &state).unwrap().plan.expect("its plan, offered");
        assert_eq!(offered.intensity, Some(plan));
        // An agent over MCP is told where the plan is written, and the beat by its name (R1's
        // MCP dogfood): the inspector's place for it, and the plan asked as a fix that writes it
        // there (R1's agent dogfood 2).
        let path = bend.path.clone().expect("where its plan is written");
        assert_eq!(path, format!("{}/intensity", offered.at));
        let fix = bend.fix.clone().expect("the plan asked, as a fix");
        assert_eq!((fix.patch.len(), &fix.patch[0]["op"], &fix.patch[0]["path"]), (1, &json!("replace"), &json!(path)));
        assert!(bend.ask.contains("by its Intensity, in its beat, "), "{}", bend.ask);
        let place = h.slides.iter().find(|s| s.state == state).unwrap().curve;
        let op = json!({ "op": "add", "path": path, "value": place });
        let mut replanned = deck.clone();
        scaena_core::patch::apply(&mut replanned, &[op]).unwrap();
        let after = of(&replanned);
        assert!(!after.findings.iter().any(|f| f.state == state && f.kind == Kind::Arc), "{:#?}", after.findings);
    }

    /// A slide whose words already carry what its place calls for, planned far from it, is asked
    /// for its plan once (W2's editor dogfood 6): the bend that asks the same edit folds into the
    /// warning, which says the plan as the bend did, where it is written, its beat, and the move
    /// the bend offered. No figure moves.
    #[test]
    fn a_plan_that_strays_is_asked_for_once() {
        let climate = climate();
        let before = of(&climate);
        // A slide whose words read about what its place calls for, planned at the far end.
        let (deck, h, state) = (before.slides.iter().enumerate().skip(1))
            .filter(|(_, s)| s.read.is_some_and(|r| (r - s.curve).abs() < 0.1))
            .find_map(|(_, s)| {
                let mut deck = climate.clone();
                planned(&mut deck, &s.state, if s.curve < 0.5 { 1.0 } else { 0.0 });
                let h = of(&deck);
                let strays = h
                    .findings
                    .iter()
                    .any(|f| f.state == s.state && f.kind == Kind::Intensity && f.ask.contains("it is its plan"));
                strays.then(|| (deck, h, s.state.clone()))
            })
            .expect("a slide whose words read as its place calls for, its plan straying");
        let on: Vec<&Finding> = h.findings.iter().filter(|f| f.state == state).collect();
        assert!(!on.iter().any(|f| f.kind == Kind::Arc), "one row for one edit: {on:#?}");
        let gap = on.iter().find(|f| f.kind == Kind::Intensity).unwrap();
        assert!(
            gap.says.contains("was planned ") && gap.says.ends_with("where its words already stand."),
            "{}",
            gap.says
        );
        assert!(gap.ask.contains("set its Intensity for its place, in its beat, "), "{}", gap.ask);
        assert!(gap.ask.contains(", or move it "), "{}", gap.ask);
        let d = Deck::from_value(&deck).unwrap();
        let dusk = std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap();
        let theme: Theme = serde_json::from_str(&dusk).unwrap();
        let offered = scaena_core::choices::state_choices(&d, &theme, &state).unwrap().plan.expect("its plan, offered");
        assert_eq!(gap.path.as_deref(), Some(format!("{}/intensity", offered.at).as_str()));
        assert_eq!((h.fidelity.score, h.intensity.score), (h.fidelity.score, h.intensity.score));
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

    /// What a measure says of its slides agrees with how many it counts: "1 slide bends the
    /// intensity", "2 slides bend it"; "1 slide reads far from the tone it declares" (dogfood
    /// pass 9).
    #[test]
    fn a_measure_says_its_slides_as_many_as_it_counts() {
        let climate = climate();
        let before = of(&climate);
        let mut decks = vec![before.clone()];
        // Each slide that reads something planned at the far end of it, alone: one bend at a time.
        for (i, plan) in heard(&before).into_iter().take(4) {
            let mut deck = climate.clone();
            planned(&mut deck, &before.slides[i].state, plan);
            decks.push(of(&deck));
        }
        let (mut one, mut more) = (0, 0);
        for h in &decks {
            for says in [&h.fidelity.says, &h.intensity.says, &h.tone.says] {
                let Some(n) = says.split(' ').next().and_then(|w| w.parse::<usize>().ok()) else { continue };
                if n == 1 {
                    one += 1;
                    assert!(says.starts_with("1 slide ") && !says.starts_with("1 slides"), "{says}");
                    assert!(!says.contains("slide bend ") && !says.contains("slide read far"), "{says}");
                    assert!(!says.contains("they declare"), "{says}");
                } else {
                    more += 1;
                    assert!(!says.contains("slides bends") && !says.contains("slides reads"), "{says}");
                }
            }
        }
        assert!(
            one > 0,
            "a measure counting one slide is said: {:#?}",
            decks.iter().map(|h| &h.fidelity.says).collect::<Vec<_>>()
        );
        assert!(more > 0);
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
        // A cap over the score holds nothing down, and says nothing.
        assert_eq!(health_in(&d, &one).unwrap().capped, None);
        let two = [one[0].clone(), Linted::new("E100", Severity::Error, "text overflows").state(id(9))];
        let held = health_in(&d, &two).unwrap();
        assert_eq!(held.score, 72);
        // Held down, it says by which slides, to what, and from what (W1's dogfood 5).
        let broken: Vec<usize> = held.findings.iter().filter(|f| f.kind == Kind::Layout).map(|f| f.slide).collect();
        assert_eq!(held.capped, Some(Capped { slides: broken, cap: 72, score: 84 }));
        let warned = [Linted::new("W210", Severity::Warning, "too many words").state(id(6))];
        assert_eq!(health_in(&d, &warned).unwrap().score, 84);
        assert_eq!(health_in(&d, &warned).unwrap().capped, None);
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
        // Fidelity reads the slides as a talk usually runs, and says so, never as their words
        // (W2's editor dogfood); its figure is as before.
        assert!(
            h.fidelity.says.starts_with("No slide declares its intensity: read as a talk usually runs, "),
            "{}",
            h.fidelity.says
        );
        assert!(h.fidelity.score.is_some());
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
    /// A topic headline over its claim is quoted as the slide shows it (W1's dogfood 17): a line
    /// chart's callout figure stands beside its headline, and the reader hears the two together,
    /// "25.8%: Canopy cover, 2016–2025", but the words to replace are the headline's alone.
    #[test]
    fn a_topic_headline_beside_its_figure_is_quoted_as_shown() {
        let chart = |slides: &mut Vec<answer::Slide>| {
            let third = &mut slides[2];
            third.slide_title = "Canopy cover, 2016–2025".into();
            third.layout_suggestion = "Line Chart".into();
            third.key_bullets = vec!["Canopy cover fell every year since 2016".into(), "The city lost 4 points".into()];
            third.chart_data = Some(answer::ChartData {
                categories: (2016..=2025).map(|y| y.to_string()).collect(),
                series: vec![answer::Series {
                    name: Some("Canopy".into()),
                    values: [29.8, 29.4, 29.0, 28.5, 28.1, 27.6, 27.2, 26.7, 26.3, 25.8].map(Some).to_vec(),
                }],
                unit: Some("%".into()),
                source_id: None,
            });
        };
        let deck = dogfood("keynote", "", chart);
        let d = Deck::from_value(&deck).unwrap();
        let snaps = resolve_states(&d).unwrap();
        let heard = scaena_core::reading::headline(&d, &snaps[*d.slides()[2].last().unwrap()]);
        assert_eq!(
            heard.as_deref(),
            Some("25.8%: Canopy cover, 2016\u{2013}2025"),
            "the reader hears the figure first"
        );
        let h = of(&deck);
        let label = (h.findings.iter()).find(|f| f.kind == Kind::Label && f.slide == 3).expect("its claim is under it");
        let shown = "\u{201C}Canopy cover, 2016\u{2013}2025\u{201D}";
        assert!(label.says.contains(shown) && label.ask.ends_with(&format!("in place of {shown}.")), "{label:?}");
        assert!(!label.says.contains("25.8") && !label.ask.contains("25.8"), "{label:?}");
    }
}
