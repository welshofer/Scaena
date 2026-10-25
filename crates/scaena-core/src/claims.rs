//! A beat's claim kept in step with the slide that says it (lift-up FUNC-2).
//!
//! A person rewrites a headline on the canvas, and the beat goes on claiming the old words: the
//! spine, health, the assistant's facts, the PDF's outline, and the video's chapters all read
//! them. Only an edit knows the claim went stale, since a slide may say its claim in words of its
//! own from the start, so this compares the deck before an edit with the deck after it: a beat
//! whose claim its slide said before, and says no more.

use crate::document::{Deck, Props};
use crate::tracking::Snapshot;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A beat whose claim its slide said before an edit and says no more: what it claimed, what the
/// slide says now, and the patch that sets the claim to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Restated {
    /// The beat, by its id.
    pub beat: String,
    /// The state of the beat's slide whose words `says` are, or, with none, the beat's first.
    pub state: String,
    /// Where the claim is, as a JSON pointer.
    pub path: String,
    /// The claim, as the beat still says it.
    pub claim: String,
    /// What the slide says now: the words of the text that said the claim, in the state it said
    /// it in, else the last that shows it; none where every such text was taken away.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub says: Option<String>,
    /// The patch that sets the claim to what the slide says, where it says something: a `test`
    /// of the old claim, then a `replace`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fix: Vec<Value>,
}

/// Each beat of `after` whose claim its slide said in `before` and says no more, `before_snaps`
/// and `after_snaps` the two decks' states resolved. A claim is said where a text shown with
/// `semantic: claim`, in any state of the beat's slides, holds its words or is held in them (a
/// headline that shortens a claim says it), case, spacing, and a closing stop aside. A beat
/// whose claim the edit changed itself, or that is new, is passed over.
pub fn restated(before: &Deck, before_snaps: &[Snapshot], after: &Deck, after_snaps: &[Snapshot]) -> Vec<Restated> {
    let Some(spine) = &after.spine else { return Vec::new() };
    let was = |id: &str| before.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats).find(|b| b.id == id);
    let mut out = Vec::new();
    for (s, section) in spine.sections.iter().enumerate() {
        for (b, beat) in section.beats.iter().enumerate() {
            let Some(old) = was(&beat.id) else { continue };
            if old.claim != beat.claim || norm(&beat.claim).is_empty() {
                continue;
            }
            let then = shown(before_snaps, &old.states);
            let now = shown(after_snaps, &beat.states);
            if !says(&then, &beat.claim) || says(&now, &beat.claim) {
                continue;
            }
            let path = format!("/spine/sections/{s}/beats/{b}/claim");
            // What the slide says now: a text that said the claim before, in the state it said it
            // in, else in the last state that shows it; none where every such text was taken away.
            let sayers: Vec<(&str, &str)> = (then.iter())
                .flat_map(|(state, said)| said.iter().map(move |(node, w)| (state.as_str(), node.as_str(), w)))
                .filter(|(_, _, w)| holds(w, &beat.claim))
                .map(|(state, node, _)| (state, node))
                .collect();
            let words = |w: &str| w.split_whitespace().collect::<Vec<_>>().join(" ");
            let there = now.iter().find_map(|(state, said)| {
                let (_, w) = said.iter().find(|(node, _)| sayers.contains(&(state.as_str(), node.as_str())))?;
                Some((state.clone(), words(w)))
            });
            let last = there.or_else(|| {
                now.iter().rev().find_map(|(state, said)| {
                    let (_, w) = said.iter().find(|(node, _)| sayers.iter().any(|(_, n)| n == node))?;
                    Some((state.clone(), words(w)))
                })
            });
            let state = last
                .as_ref()
                .map(|(state, _)| state.clone())
                .or_else(|| beat.states.first().cloned())
                .unwrap_or_default();
            let says = last.map(|(_, words)| words);
            let fix = match &says {
                Some(words) => vec![
                    json!({ "op": "test", "path": path, "value": beat.claim }),
                    json!({ "op": "replace", "path": path, "value": words }),
                ],
                None => Vec::new(),
            };
            out.push(Restated { beat: beat.id.clone(), state, path, claim: beat.claim.clone(), says, fix });
        }
    }
    out
}

/// Each state of the slides `states` are on, in the deck's order, with the claims it shows, each
/// by its node, its headline's first.
fn shown(snaps: &[Snapshot], states: &[String]) -> Vec<(String, Vec<(String, String)>)> {
    let slides: Vec<&str> =
        snaps.iter().filter(|s| states.contains(&s.state_id)).map(|s| s.slide_id.as_str()).collect();
    snaps
        .iter()
        .filter(|s| slides.contains(&s.slide_id.as_str()))
        .map(|s| {
            let mut claims: Vec<(&String, &Props)> =
                s.nodes.iter().filter(|(_, p)| p.get("semantic").and_then(Value::as_str) == Some("claim")).collect();
            crate::sort::by_key(&mut claims, |(_, p)| !heading(p));
            let words = (claims.into_iter())
                .map(|(node, p)| (node.clone(), words(p)))
                .filter(|(_, w)| !w.trim().is_empty())
                .collect();
            (s.state_id.clone(), words)
        })
        .collect()
}

/// Whether any claim `shown` holds `claim`'s words, or is held in them.
fn says(shown: &[(String, Vec<(String, String)>)], claim: &str) -> bool {
    shown.iter().flat_map(|(_, said)| said).any(|(_, w)| holds(w, claim))
}

/// Whether `words` hold `claim`'s, or are held in them.
fn holds(words: &str, claim: &str) -> bool {
    let (w, claim) = (norm(words), norm(claim));
    !w.is_empty() && (w.contains(&claim) || claim.contains(&w))
}

fn heading(p: &Props) -> bool {
    matches!(p.get("role").and_then(Value::as_str), Some("headline" | "title" | "display"))
}

/// A text node's words as written: its `text`, or its runs' in turn.
fn words(props: &Props) -> String {
    match (props.get("text"), props.get("runs")) {
        (Some(Value::String(text)), _) => text.clone(),
        (_, Some(Value::Array(runs))) => runs.iter().filter_map(|r| r.get("text")?.as_str()).collect(),
        _ => String::new(),
    }
}

/// Words compared as W425 compares claims: case, spacing, and the closing stop aside.
fn norm(s: &str) -> String {
    let words: Vec<String> = s.split_whitespace().map(str::to_lowercase).collect();
    words.join(" ").trim_end_matches(['.', '!', '?']).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracking::resolve_states;

    fn deck(headline: Option<&str>, claim: &str) -> Deck {
        let mut nodes = serde_json::Map::new();
        if let Some(h) = headline {
            nodes.insert(
                "h".into(),
                json!({ "type": "text", "role": "headline", "text": h, "semantic": "claim", "at": { "in": "main" } }),
            );
        }
        nodes.insert("n".into(), json!({ "type": "text", "role": "body", "text": "A note.", "at": { "in": "main" } }));
        let props: serde_json::Map<String, Value> = nodes.keys().map(|k| (k.clone(), json!({}))).collect();
        let doc = json!({
            "scaena": crate::FORMAT_VERSION,
            "canvas": { "width": 1920, "height": 1080 },
            "theme": "theme.json",
            "spine": { "sections": [{ "id": "s", "beats": [{ "id": "flat", "claim": claim, "states": ["a"] }] }] },
            "nodes": nodes,
            "states": [{ "id": "a", "props": props }, { "id": "a-2", "slide": "a" }],
        });
        Deck::from_json(&doc.to_string()).unwrap()
    }

    fn between(before: &Deck, after: &Deck) -> Vec<Restated> {
        restated(before, &resolve_states(before).unwrap(), after, &resolve_states(after).unwrap())
    }

    #[test]
    fn a_headline_edited_away_from_its_claim_is_reported_with_its_fix() {
        let was = deck(Some("Sales were flat."), "Sales were flat.");
        let now = deck(Some("Sales grew 12% in Q3."), "Sales were flat.");
        let found = between(&was, &now);
        assert_eq!(found.len(), 1, "{found:?}");
        let r = &found[0];
        assert_eq!((r.beat.as_str(), r.says.as_deref()), ("flat", Some("Sales grew 12% in Q3.")));
        assert_eq!(r.path, "/spine/sections/0/beats/0/claim");
        assert_eq!(r.fix[1], json!({ "op": "replace", "path": r.path, "value": "Sales grew 12% in Q3." }));
        // A headline that shortens the claim, or one edited that still says it, is no change.
        assert!(between(&was, &deck(Some("Sales were flat"), "Sales were flat in Q3.")).is_empty());
        assert!(between(&was, &deck(Some("In Q3, sales were flat."), "Sales were flat.")).is_empty());
    }

    #[test]
    fn a_paraphrase_made_at_the_start_is_never_reported() {
        let paraphrase = deck(Some("Nothing moved."), "Sales were flat.");
        assert!(between(&paraphrase, &paraphrase).is_empty());
        // Edited, it said no claim before either: nothing went stale.
        assert!(between(&paraphrase, &deck(Some("Still nothing."), "Sales were flat.")).is_empty());
    }

    #[test]
    fn a_headline_taken_away_is_reported_with_no_fix() {
        let was = deck(Some("Sales were flat."), "Sales were flat.");
        let found = between(&was, &deck(None, "Sales were flat."));
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].says.as_deref(), found[0].fix.len()), (None, 0));
    }

    #[test]
    fn a_claim_said_in_a_step_alone_counts_as_said() {
        // The headline shows from the slide's second step on.
        let step = |h: &str| {
            let mut d = deck(Some(h), "Sales were flat.").to_value().unwrap();
            d["states"][0]["props"].as_object_mut().unwrap().remove("h");
            d["states"][1]["props"] = json!({ "h": {} });
            Deck::from_json(&d.to_string()).unwrap()
        };
        let found = between(&step("Sales were flat."), &step("Sales grew."));
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].state.as_str(), found[0].says.as_deref()), ("a-2", Some("Sales grew.")));
        assert!(between(&step("Sales were flat."), &step("Sales were flat.")).is_empty());
    }

    #[test]
    fn a_claim_the_edit_set_itself_is_not_reported() {
        let was = deck(Some("Sales were flat."), "Sales were flat.");
        assert!(between(&was, &deck(Some("Sales grew."), "Sales grew.")).is_empty());
    }
}
