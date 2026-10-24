//! The spine projection (SPEC §10, PLAN 1.22): the deck's narrative as the pipelines that
//! are not the deck read it. The infographic, the motion piece, and the podcast read this
//! and call `scaena render` and `export`; none of them reads `deck.json`.
//!
//! - The spine as the deck holds it: sections, beats, claims, evidence, notes, durations,
//!   and media hints (SPEC §3.11).
//! - Every state in cue-list order: its slide, the notes a presenter reads for it, and,
//!   given the global timeline, where it plays (SPEC §2.4), paced by the beats or by the
//!   holds ([`Pace`]).
//! - Every beat by id, in spine order: the state that shows it, the last of its states in
//!   the deck's order (a build at its fullest, as a PDF page shows a slide); when it plays;
//!   and, once they are drawn, its renders, by path relative to the projection's file.
//!
//! `docs/schema/spine.schema.json` is generated from these types (ADR-0007).

use crate::document::{Deck, Spine};
use crate::timeline::Timeline;
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The spine projection: `export --format spine` writes it; `spine_read` returns it
/// without timing or renders.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(title = "Scaena spine projection")]
#[serde(deny_unknown_fields)]
pub struct SpineProjection {
    /// The format of the deck it was projected from: its `scaena`.
    pub scaena: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The deck's language (BCP 47), for a voice that reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// The deck's canvas, in canvas units: width and height.
    pub canvas: [f64; 2],
    /// The formats the deck lists (SPEC §3.4), as it names them: `16:9`, `9:16`. A beat
    /// has a render in each whose canvas is not the deck's own.
    #[serde(default)]
    pub formats: Vec<String>,
    /// The spine as the deck holds it; absent if it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spine: Option<Spine>,
    /// How the timeline its states and beats are placed on was paced ([`Pace`]). Given the
    /// timeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pace: Option<Pace>,
    /// Every state, in cue-list order.
    pub states: Vec<SpineState>,
    /// Every beat, by id, in spine order.
    #[serde(default)]
    pub beats: IndexMap<String, SpineBeat>,
}

/// A state: its slide, its notes, and where it plays.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpineState {
    pub id: String,
    /// The slide it builds (SPEC §2.2).
    pub slide: String,
    /// What a presenter reads for it (SPEC §2.6, [`notes`]): its own notes, else its beat's.
    /// None where nothing is written on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Its dwell at rest, ms, as the deck gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<f64>,
    /// Its time at rest on the timeline, ms: its `hold`, or under `pace: beats`, where it has
    /// none, its share of its beat's `duration` ([`paced`]). Given the timeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest: Option<f64>,
    /// When its transition starts on the global timeline, ms. Given the timeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    /// Its cue, ms: its transition and motions. Its hold follows. Given the timeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<f64>,
}

/// A beat: the state that shows it, when it plays, and its renders.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpineBeat {
    /// Its section's id.
    pub section: String,
    /// The state that shows it: the last of its states in the deck's order. None if it
    /// names no state of the deck.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// When it starts on the global timeline, ms: where its first state starts. Given the
    /// timeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    /// When it ends, ms: where its last state's hold ends. Given the timeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    /// `state` at rest, small, in the deck's own format: a path relative to the
    /// projection's file. Once drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    /// `state` at rest in each of the deck's other formats, at that format's canvas size:
    /// paths relative to the projection's file, by format. Once drawn.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub formats: IndexMap<String, String>,
}

/// How a projection paces the deck's global timeline (SPEC §2.4, §10).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Pace {
    /// Each state its `hold`; where it has none, its share of its beat's `duration`, less the
    /// cues of the beat's states ([`paced`]): a talk timed as it is given, though a player
    /// waits on the presenter.
    #[default]
    Beats,
    /// Each state its `hold` alone, as a player auto-advances: a state with none comes to
    /// rest and goes on at once.
    Holds,
}

impl Pace {
    /// `beats` or `holds`.
    pub fn name(self) -> &'static str {
        match self {
            Pace::Beats => "beats",
            Pace::Holds => "holds",
        }
    }
}

/// `timeline` paced by `pace`. Under [`Pace::Beats`], each beat with a `duration` shares it
/// among the states it names that have no `hold` of their own: what is left of it once the
/// beat's cues and the holds its states do have are counted, in equal parts, never below 0.
/// A state two beats name is the first's, as a chapter is; a hold the deck gives, as
/// Rehearse keeps one (PLAN 2.63), always wins. [`Pace::Holds`] is `timeline` as it is.
pub fn paced(deck: &Deck, timeline: &Timeline, pace: Pace) -> Timeline {
    let mut rest: IndexMap<&str, f64> = timeline.slots.iter().map(|s| (s.state.as_str(), s.hold)).collect();
    if pace == Pace::Beats {
        let held: IndexMap<&str, bool> = deck.states.iter().map(|s| (s.id.as_str(), s.hold.is_some())).collect();
        let mut taken: Vec<&str> = Vec::new();
        for beat in deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats) {
            let states: Vec<&str> = (timeline.slots.iter())
                .map(|s| s.state.as_str())
                .filter(|s| beat.states.iter().any(|b| b == s) && !taken.contains(s))
                .collect();
            taken.extend(&states);
            let Some(seconds) = beat.duration.filter(|d| *d > 0.0) else { continue };
            let free: Vec<&str> = states.iter().copied().filter(|s| !held.get(s).copied().unwrap_or(false)).collect();
            if free.is_empty() {
                continue;
            }
            let used: f64 = (states.iter())
                .filter_map(|s| timeline.slot(s))
                .map(|slot| slot.span + if free.contains(&slot.state.as_str()) { 0.0 } else { slot.hold })
                .sum();
            let share = ((seconds * 1000.0 - used) / free.len() as f64).max(0.0);
            for s in free {
                rest.insert(s, share);
            }
        }
    }
    Timeline::new(timeline.slots.iter().map(|s| (s.state.clone(), s.span, rest[s.state.as_str()])))
}

/// The notes a presenter reads for `state` (SPEC §2.6): the state's own, else those of the
/// first beat, in spine order, that names it; none for a state the deck lacks. A generated deck
/// writes its speaker notes on the beats (PLAN G.2), and a state's own, where it has them, say
/// more about that step than the beat's.
pub fn notes<'a>(deck: &'a Deck, state: &str) -> Option<&'a str> {
    let own = deck.states.iter().find(|s| s.id == state)?;
    if let Some(notes) = own.notes.as_deref() {
        return Some(notes);
    }
    let mut beats = deck.spine.iter().flat_map(|s| s.sections.iter()).flat_map(|s| s.beats.iter());
    Some(beats.find(|b| b.states.iter().any(|s| s == state)).and_then(|b| b.notes.as_deref()).unwrap_or(""))
}

/// `deck`'s spine projection, placed on `timeline` when it is given, paced by `pace` ([`paced`]):
/// with no renders.
pub fn projection(deck: &Deck, timeline: Option<&Timeline>, pace: Pace) -> SpineProjection {
    let paced = timeline.map(|t| paced(deck, t, pace));
    let timeline = paced.as_ref();
    let order: IndexMap<&str, usize> = deck.states.iter().enumerate().map(|(i, s)| (s.id.as_str(), i)).collect();
    let slot = |id: &str| timeline.and_then(|t| t.slot(id));
    let states = deck
        .states
        .iter()
        .map(|s| SpineState {
            id: s.id.clone(),
            slide: deck.slide_of(s).to_string(),
            notes: notes(deck, &s.id).filter(|n| !n.is_empty()).map(String::from),
            hold: s.hold,
            rest: slot(&s.id).map(|slot| slot.hold),
            start: slot(&s.id).map(|slot| slot.start),
            span: slot(&s.id).map(|slot| slot.span),
        })
        .collect();
    let mut beats = IndexMap::new();
    for section in deck.spine.iter().flat_map(|s| &s.sections) {
        for beat in &section.beats {
            let named: Vec<&str> = beat.states.iter().map(String::as_str).filter(|s| order.contains_key(s)).collect();
            let state = named.iter().max_by_key(|s| order[**s]).map(|s| s.to_string());
            let slots: Vec<_> = named.iter().filter_map(|s| slot(s)).collect();
            let start = slots.iter().map(|s| s.start).reduce(f64::min);
            let end = slots.iter().map(|s| s.end()).reduce(f64::max);
            let entry =
                SpineBeat { section: section.id.clone(), state, start, end, thumbnail: None, formats: IndexMap::new() };
            beats.insert(beat.id.clone(), entry);
        }
    }
    let meta = deck.meta.as_ref();
    SpineProjection {
        scaena: deck.scaena.clone(),
        title: meta.and_then(|m| m.title.clone()),
        lang: meta.and_then(|m| m.lang.clone()),
        canvas: [deck.canvas.width, deck.canvas.height],
        formats: deck.formats.clone(),
        spine: deck.spine.clone(),
        pace: timeline.map(|_| pace),
        states,
        beats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn deck() -> Deck {
        serde_json::from_value(json!({
            "scaena": crate::FORMAT_VERSION, "canvas": { "width": 1920, "height": 1080 }, "formats": ["9:16"],
            "meta": { "title": "Q3", "lang": "en-US" }, "nodes": {},
            "states": [{ "id": "a", "hold": 2000 }, { "id": "a2", "slide": "a", "hold": 1000 }, { "id": "b", "notes": "Pause." }],
            "spine": { "sections": [{ "id": "s", "beats": [
                { "id": "built", "claim": "A builds.", "states": ["a2", "a"] },
                { "id": "then", "claim": "Then B.", "states": ["b", "gone"] },
                { "id": "none", "claim": "Nothing shows this." }] }] }
        }))
        .unwrap()
    }

    #[test]
    fn a_presenter_reads_a_states_own_notes_else_its_first_beats() {
        let mut deck = deck();
        // `a` has none of its own: the first beat that names it has none either.
        assert_eq!(notes(&deck, "a"), Some(""));
        deck.spine.as_mut().unwrap().sections[0].beats[0].notes = Some("Build it.".into());
        assert_eq!(notes(&deck, "a"), Some("Build it."));
        assert_eq!(notes(&deck, "a2"), Some("Build it."));
        // Its own win.
        assert_eq!(notes(&deck, "b"), Some("Pause."));
        // A state no beat names, with none of its own, has none; one the deck lacks, no answer.
        deck.states[2].notes = None;
        deck.spine.as_mut().unwrap().sections[0].beats[1].states.clear();
        assert_eq!(notes(&deck, "b"), Some(""));
        assert_eq!(notes(&deck, "gone"), None);
    }

    #[test]
    fn a_beat_shows_its_last_state_and_plays_from_its_first_to_its_last() {
        let deck = deck();
        let timeline =
            Timeline::new([("a".into(), 500.0, 2000.0), ("a2".into(), 300.0, 1000.0), ("b".into(), 0.0, 0.0)]);
        let p = projection(&deck, Some(&timeline), Pace::Beats);
        assert_eq!((p.title.as_deref(), p.lang.as_deref(), p.canvas), (Some("Q3"), Some("en-US"), [1920.0, 1080.0]));
        assert_eq!(p.states[1].slide, "a");
        assert_eq!((p.states[1].start, p.states[1].span), (Some(2500.0), Some(300.0)));
        let built = &p.beats["built"];
        // Named out of order, it shows the later state, and plays from `a` to `a2`'s hold.
        assert_eq!((built.state.as_deref(), built.start, built.end), (Some("a2"), Some(0.0), Some(3800.0)));
        // A state the deck does not have is passed over.
        assert_eq!(p.beats["then"].state.as_deref(), Some("b"));
        assert_eq!(p.beats["none"].state, None);
        assert_eq!(p.beats.keys().collect::<Vec<_>>(), ["built", "then", "none"]);
        // Without the timeline, nothing is timed.
        let light = serde_json::to_value(projection(&deck, None, Pace::Beats)).unwrap();
        assert!(light["states"][0].get("start").is_none() && light["beats"]["built"].get("start").is_none());
        assert_eq!(light["beats"]["built"], json!({ "section": "s", "state": "a2" }));
    }
    #[test]
    fn the_beats_pace_the_states_with_no_hold_and_a_hold_the_deck_gives_wins() {
        let deck: Deck = serde_json::from_value(json!({
            "scaena": crate::FORMAT_VERSION, "canvas": { "width": 1920, "height": 1080 }, "nodes": {},
            "states": [{ "id": "a" }, { "id": "a2", "slide": "a" }, { "id": "b", "hold": 4000 }, { "id": "c" },
                       { "id": "d" }],
            "spine": { "sections": [{ "id": "s", "beats": [
                { "id": "built", "claim": "A builds.", "states": ["a", "a2"], "duration": 10, "notes": "Slowly." },
                { "id": "held", "claim": "B is rehearsed.", "states": ["b"], "duration": 30 },
                { "id": "short", "claim": "D runs over.", "states": ["d", "a"], "duration": 0.1 }] }] }
        }))
        .unwrap();
        let timeline = Timeline::new([
            ("a".into(), 500.0, 0.0),
            ("a2".into(), 300.0, 0.0),
            ("b".into(), 200.0, 4000.0),
            ("c".into(), 100.0, 0.0),
            ("d".into(), 400.0, 0.0),
        ]);
        let rests = |t: &Timeline| t.slots.iter().map(|s| s.hold).collect::<Vec<_>>();
        // `built`'s ten seconds, less its cues, in two: `a` is the first beat's, not `short`'s.
        // `b` keeps the hold Rehearse gave it, `c` no beat names, and `d`'s cue outruns its beat.
        let paced = paced(&deck, &timeline, Pace::Beats);
        assert_eq!(rests(&paced), [4600.0, 4600.0, 4000.0, 0.0, 0.0]);
        assert_eq!(paced.slot("b").unwrap().start, 10_000.0, "the states lie end to end");
        assert_eq!(rests(&super::paced(&deck, &timeline, Pace::Holds)), rests(&timeline));
        // The spine says how it was paced, each state's rest beside its hold, and what a
        // presenter reads for it.
        let p = projection(&deck, Some(&timeline), Pace::Beats);
        assert_eq!(p.pace, Some(Pace::Beats));
        assert_eq!((p.states[0].hold, p.states[0].rest), (None, Some(4600.0)));
        assert_eq!((p.states[2].hold, p.states[2].rest), (Some(4000.0), Some(4000.0)));
        assert_eq!(p.beats["built"].end, Some(10_000.0));
        assert_eq!(p.states[1].notes.as_deref(), Some("Slowly."));
        assert_eq!(p.states[3].notes, None);
        assert_eq!(serde_json::to_value(Pace::Holds).unwrap(), json!("holds"));
    }
}
