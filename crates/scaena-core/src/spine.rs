//! The spine projection (SPEC §10, PLAN 1.22): the deck's narrative as the pipelines that
//! are not the deck read it. The infographic, the motion piece, and the podcast read this
//! and call `scaena render` and `export`; none of them reads `deck.json`.
//!
//! - The spine as the deck holds it: sections, beats, claims, evidence, notes, durations,
//!   and media hints (SPEC §3.11).
//! - Every state in cue-list order: its slide, its notes, and, given the global timeline,
//!   where it plays (SPEC §2.4).
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Its dwell at rest, ms, as the deck gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<f64>,
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

/// `deck`'s spine projection, placed on `timeline` when it is given: with no renders.
pub fn projection(deck: &Deck, timeline: Option<&Timeline>) -> SpineProjection {
    let order: IndexMap<&str, usize> = deck.states.iter().enumerate().map(|(i, s)| (s.id.as_str(), i)).collect();
    let slot = |id: &str| timeline.and_then(|t| t.slot(id));
    let states = deck
        .states
        .iter()
        .map(|s| SpineState {
            id: s.id.clone(),
            slide: deck.slide_of(s).to_string(),
            notes: s.notes.clone(),
            hold: s.hold,
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
    fn a_beat_shows_its_last_state_and_plays_from_its_first_to_its_last() {
        let deck = deck();
        let timeline =
            Timeline::new([("a".into(), 500.0, 2000.0), ("a2".into(), 300.0, 1000.0), ("b".into(), 0.0, 0.0)]);
        let p = projection(&deck, Some(&timeline));
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
        let light = serde_json::to_value(projection(&deck, None)).unwrap();
        assert!(light["states"][0].get("start").is_none() && light["beats"]["built"].get("start").is_none());
        assert_eq!(light["beats"]["built"], json!({ "section": "s", "state": "a2" }));
    }
}
