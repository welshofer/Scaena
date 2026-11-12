//! A made deck's speaker notes written for its talk (lift-up 10-25 ATTR-2): what each slide is
//! shown as, the words its notes are to run to, and the notes the model wrote set in its beat,
//! the beat's time counted again from them. Notes only: no slide's words or layout change.

use super::{Generated, View, Write, errors, lint_in};
use crate::OpsError;
use scaena_core::Deck;
use scaena_narrato::duration;
use scaena_narrato::notes::{self, Spoken, Timed};
use serde_json::{Value, json};

/// The minutes the deck's talk is given ([`notes::slot`]): those its brief asks
/// (`meta.narrative.brief.minutes`), else the middle of the slot its length gives.
pub fn slot(deck: &Deck) -> f64 {
    let brief = deck.meta.as_ref().and_then(|m| m.extra.get("narrative")).and_then(|n| n.get("brief"));
    let length = brief.and_then(|b| b.get("length")).and_then(Value::as_str).unwrap_or_default();
    let asked = (brief.and_then(|b| b.get("minutes")).and_then(Value::as_f64))
        .filter(|m| m.is_finite() && *m >= 1.0 && *m <= f64::from(u32::MAX))
        .map(|m| m.round() as u32);
    notes::slot(asked, length)
}

/// What a deck's notes are to fill, for an agent that writes them (agent dogfood 3): the talk's
/// `slot`, in minutes; how long the talk `run`s now, as health times it (its beats' durations,
/// holds, and notes said aloud), where it can be timed; and each slide's `budgets`, the words its
/// notes are to run to so the talk fills the slot.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Notes {
    /// The minutes the talk is given: the brief's `minutes`, else the middle of its length's slot.
    pub slot: f64,
    /// The minutes the talk runs now, to a tenth, as `deck_health`'s pacing times it: each beat
    /// the longest of its `duration` (a made deck's estimate from its words), its holds, and its
    /// notes said aloud. Not the notes alone, which `deck_health` says apart ("its notes as
    /// written run about …"): notes short of `budgets` leave the talk at the estimate (agent
    /// dogfood 4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<f64>,
    /// Each slide, by its number from 1, and the words its notes are to run to.
    pub budgets: Vec<Budget>,
}

/// A slide's notes' budget: the slide, by its number from 1, and the words.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Budget {
    pub slide: usize,
    pub words: usize,
}

/// [`Notes`] for `deck`.
pub fn budgeted(deck: &Deck) -> Result<Notes, OpsError> {
    let spoken = speaking(deck)?;
    let run = crate::health::talk_seconds(deck).map(|s| (s / 6.0).round() / 10.0);
    let budgets = spoken.iter().map(|s| Budget { slide: s.slide, words: s.budget }).collect();
    Ok(Notes { slot: slot(deck), run, budgets })
}

/// Each slide of `deck` as the model is asked to speak it (lift-up 10-25 ATTR-2): its headline,
/// the words it shows, its draft notes, the headlines either side, and the words its notes are
/// to run to ([`notes::budgets`]): the minutes the brief's slot gives the talk
/// ([`slot`]), shared among the slides as their beats' durations are, each less its
/// change and its own words, so the talk reads as the slot's middle by the measure it is timed by.
pub fn speaking(deck: &Deck) -> Result<Vec<Spoken>, OpsError> {
    let slides = crate::health::told(deck)?;
    let timed: Vec<Timed> = (slides.iter())
        .map(|t| {
            let beat = t.beat.map(|(.., b)| b);
            let narrato = beat.and_then(|b| b.media.as_ref()).and_then(|m| m.get("narrato"));
            let said = |k: &str| narrato.and_then(|n| n.get(k)).and_then(Value::as_str).unwrap_or_default();
            Timed {
                seconds: beat.and_then(|b| b.duration).unwrap_or(15.0),
                shown: notes::counted(&t.slide.title) + notes::counted(&t.slide.body),
                factor: duration::factor(said("layout"), said("visual"), &t.slide.title),
            }
        })
        .collect();
    // The beats the spine plans that no slide tells yet share the slot too, a slide each, so the
    // first slides written to a planned spine are not given the whole talk (agent dogfood 8: a
    // cover written first was budgeted 1,427 words of a 10-minute slot).
    let planned = (deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats))
        .filter(|b| !b.states.iter().any(|id| deck.state_index(id).is_some()))
        .map(|b| Timed {
            seconds: b.duration.unwrap_or(15.0),
            shown: notes::counted(&b.claim),
            factor: duration::factor("", "", &b.claim),
        });
    let mut timed = timed;
    timed.extend(planned);
    let mut budgets = notes::budgets(slot(deck), &timed);
    budgets.truncate(slides.len());
    let titles: Vec<&str> = slides.iter().map(|t| t.slide.title.trim()).collect();
    // The slide before is said whole, its words after its headline, so its notes may open on a
    // figure or a phrase of it this slide does not use (health's turn, H4).
    let before = |b: usize| {
        let shown: Vec<&str> = slides[b].slide.body.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        if shown.is_empty() { titles[b].to_string() } else { format!("{} \u{2014} {}", titles[b], shown.join("; ")) }
    };
    Ok((slides.iter().zip(budgets).enumerate())
        .map(|(i, (t, budget))| Spoken {
            slide: i + 1,
            title: t.slide.title.trim().to_string(),
            words: t.slide.body.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect(),
            claim: String::new(),
            draft: t.slide.notes.trim().to_string(),
            budget,
            before: i.checked_sub(1).map(before).unwrap_or_default(),
            after: titles.get(i + 1).map(|a| a.to_string()).unwrap_or_default(),
        })
        .collect())
}

/// `write` with `written`, each a slide's number from 1 and the notes the model wrote for it,
/// set as its beat's notes, the beat's `duration` counted again as Narrato counts a slide's time
/// from its words ([`duration::respoken`]), and lint run again on it; what `made` says follows.
/// A slide no beat tells takes none.
pub fn noted(made: &mut Generated, mut write: Write, written: Vec<(usize, String)>) -> Result<Write, OpsError> {
    if written.is_empty() {
        return Ok(write);
    }
    let places = crate::health::told(&write.deck)?;
    let mut doc = write.deck.to_value()?;
    let mut taken = Vec::new();
    for (slide, text) in written {
        let Some(Some((i, j, _))) = slide.checked_sub(1).and_then(|k| places.get(k)).map(|t| t.beat) else { continue };
        let beat = &mut doc["spine"]["sections"][i]["beats"][j];
        if !beat.is_object() {
            continue;
        }
        let was = beat["notes"].as_str().unwrap_or_default();
        let narrato = &beat["media"]["narrato"];
        let str_of = |v: &Value| v.as_str().unwrap_or_default().to_string();
        let factor =
            duration::factor(&str_of(&narrato["layout"]), &str_of(&narrato["visual"]), &str_of(&beat["claim"]));
        if let Some(seconds) = beat["duration"].as_f64() {
            beat["duration"] = json!(duration::respoken(seconds, notes::counted(was), notes::counted(&text), factor));
            // Notes written to the slot are its plan, no estimate any more.
            if let Some(narrato) = beat["media"]["narrato"].as_object_mut() {
                narrato.remove("estimated");
            }
        }
        beat["notes"] = json!(text);
        taken.push(slide);
    }
    let nothing = scaena_store::Files::Zip(Default::default());
    let view = View { base: &nothing, pending: write.files.clone() };
    let deck = Deck::from_json(&doc.to_string()).map_err(|e| OpsError::new(format!("the deck with its notes: {e}")))?;
    let found = lint_in(&deck, &view)?.findings;
    made.created.errors = errors(&found);
    made.created.findings = found;
    made.noted.extend(taken);
    write.deck = deck;
    Ok(write)
}
