//! One slide written into an open deck as the generator builds them (lift-up FUNC-1): a slide
//! Narrato wrote, or a person's model wrote for one place, built by the narrative's own slide
//! builder in the deck's theme and brief, and put in as one patch.

use super::{Builder, Held, Narrative, Pictures, Placed, Taken, alike, assemble, clear, language_tag};
use crate::lint::{View, Write, lint_in};
use crate::patch::{Patched, patching};
use crate::{Bundle, Context, OpsError};
use scaena_core::Deck;
use scaena_core::Finding;
use scaena_core::model::theme::Theme;
use scaena_narrato::{Brief, Icon, Slide};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Where a slide goes in an open deck.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Place {
    /// After every slide.
    End,
    /// After the slide that holds this state.
    After(String),
    /// In place of the slide that holds this state, with all its steps.
    Replace(String),
}

/// A slide built for an open deck: the patch that puts it there, all or none; its states, first
/// the slide's own; the layout it took; and its beat's id.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Written {
    pub patch: Vec<Value>,
    pub states: Vec<String>,
    pub layout: String,
    pub beat: String,
}

/// `slide` built for the deck `doc`, in `theme`, as the deck's generator builds a slide: its
/// layout chosen for its items, each place filled, its figures, builds, and icons (`icons`, by
/// the word each item names, as [`super::icons_found`] finds them), in the brief the deck keeps
/// (`meta.narrative.brief`, else none). The patch:
///
/// - adds its nodes and data under ids the deck does not take, and its states after the slide
///   `place` names, or at the end;
/// - gives it a beat after that slide's, in its section, its claim the slide's title and its
///   notes the slide's;
/// - for [`Place::Replace`], puts it in the old slide's beat, which keeps its id, and takes the
///   old slide away with `remove_slide`, its nodes no other slide shows going with it. A picture
///   the old slide showed is shown again, in a layout that holds one.
pub fn slide_into(
    doc: &Value,
    theme: &Theme,
    slide: Slide,
    place: &Place,
    icons: BTreeMap<String, Icon>,
) -> Result<Written, OpsError> {
    let deck = Deck::from_json(&doc.to_string()).map_err(|e| OpsError::new(format!("the deck: {e}")))?;
    let slides = deck.slides();
    let id_of = |k: usize| deck.states[k].id.as_str();
    let holding = |state: &str| slides.iter().position(|s| s.iter().any(|k| id_of(*k) == state));
    let (index, at, replaced) = match place {
        Place::End => (slides.len(), deck.states.len(), None),
        Place::After(state) => {
            let i = holding(state).ok_or_else(|| OpsError::new(format!("no state `{state}` in the deck")))?;
            (i + 1, slides[i].last().map_or(0, |k| k + 1), None)
        }
        Place::Replace(state) => {
            let i = holding(state).ok_or_else(|| OpsError::new(format!("no state `{state}` in the deck")))?;
            (i, slides[i].last().map_or(0, |k| k + 1), Some(id_of(slides[i][0]).to_string()))
        }
    };
    let total = slides.len() + usize::from(replaced.is_none());
    // The picture the old slide showed, shown again: an image its first state shows with a file.
    let mut pictures = Pictures::new();
    if let Some(old) = &replaced {
        let state = deck.states.iter().find(|s| &s.id == old);
        let shown = state.map(|s| s.props.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
        let picture = shown.iter().find_map(|id| {
            let node = &doc["nodes"][id];
            let src = node["src"].as_str().filter(|_| node["type"] == "image" && node.get("fetched").is_none())?;
            let alt = node["alt"].as_str().unwrap_or_default();
            Some(Placed { src: src.to_string(), alt: alt.to_string() })
        });
        if let Some(picture) = picture {
            pictures.insert(0, picture);
        }
    }

    // The deck as built so far: every id it takes, so what is built takes none of them.
    let mut seed = Builder {
        nodes: doc["nodes"].as_object().cloned().unwrap_or_default(),
        states: doc["states"].as_array().cloned().unwrap_or_default(),
        data: doc["data"].as_object().cloned().unwrap_or_default(),
        ..Builder::default()
    };
    for section in doc.pointer("/spine/sections").and_then(Value::as_array).into_iter().flatten() {
        if let Some(id) = section["id"].as_str() {
            seed.unique(id, Taken::Section);
        }
        for beat in section["beats"].as_array().into_iter().flatten() {
            if let Some(id) = beat["id"].as_str() {
                seed.unique(id, Taken::Beat);
            }
        }
    }
    let (nodes_before, states_before) = (seed.nodes.clone(), seed.states.len());
    let data_before = seed.data.clone();

    let narrative = Narrative {
        arc: doc.pointer("/meta/narrative/arc").and_then(Value::as_str).unwrap_or_default().to_string(),
        arc_reasoning: String::new(),
        executive_summary: String::new(),
        slides: vec![slide],
        truncated: false,
        brief: brief_of(doc),
        icons,
        sources: Vec::new(),
        outline: None,
    };
    let (made, built) = assemble(&narrative, theme, &pictures, &BTreeMap::new(), seed, index, total)?;
    let layout = made.layouts.first().cloned().unwrap_or_default();
    let new_states: Vec<Value> = built.states[states_before..].to_vec();
    let states: Vec<String> = new_states.iter().filter_map(|s| s["id"].as_str().map(str::to_string)).collect();
    let mut beat = made
        .deck
        .pointer("/spine/sections/0/beats/0")
        .cloned()
        .ok_or_else(|| OpsError::new("the slide was built with no beat"))?;

    let mut patch = Vec::new();
    for (id, node) in built.nodes.iter().filter(|(id, _)| !nodes_before.contains_key(*id)) {
        patch.push(json!({ "op": "add", "path": format!("/nodes/{}", esc(id)), "value": node }));
    }
    let new_data: Vec<(&String, &Value)> = built.data.iter().filter(|(id, _)| !data_before.contains_key(*id)).collect();
    if !new_data.is_empty() && doc.get("data").is_none() {
        patch.push(json!({ "op": "add", "path": "/data", "value": {} }));
    }
    for (id, data) in new_data {
        patch.push(json!({ "op": "add", "path": format!("/data/{}", esc(id)), "value": data }));
    }
    for (k, state) in new_states.iter().enumerate() {
        patch.push(json!({ "op": "add", "path": format!("/states/{}", at + k), "value": state }));
    }

    // Its beat: the old slide's, where it takes one's place; else one after the beat of the slide
    // it follows, or the last, in that beat's section.
    let sections = doc.pointer("/spine/sections").and_then(Value::as_array).cloned().unwrap_or_default();
    let beat_holding = |state: &str| {
        sections.iter().enumerate().find_map(|(s, section)| {
            section["beats"].as_array().and_then(|beats| {
                beats
                    .iter()
                    .position(|b| b["states"].as_array().is_some_and(|l| l.iter().any(|x| x == state)))
                    .map(|b| (s, b))
            })
        })
    };
    let beat_id = match place {
        Place::Replace(_) => {
            let old = replaced.as_deref().expect("set for a replace");
            match beat_holding(old) {
                Some((s, b)) => {
                    let was = &sections[s]["beats"][b];
                    let id = was["id"].as_str().unwrap_or_default().to_string();
                    beat["id"] = json!(id);
                    // The old slide's states leave the beat with `remove_slide`; any other the
                    // beat holds stays.
                    let others: Vec<Value> = (was["states"].as_array().into_iter().flatten())
                        .filter(|x| x.as_str().is_some_and(|x| !slides_of(&deck, old).contains(&x)))
                        .cloned()
                        .collect();
                    let mut listed = beat["states"].as_array().cloned().unwrap_or_default();
                    listed.extend(others);
                    beat["states"] = json!(listed);
                    patch.push(
                        json!({ "op": "replace", "path": format!("/spine/sections/{s}/beats/{b}"), "value": beat }),
                    );
                    id
                }
                None => add_beat(&mut patch, &sections, None, beat, made.deck.pointer("/spine/sections/0").cloned()),
            }
        }
        Place::After(state) => {
            let last = slides[index - 1].last().map(|k| id_of(*k)).unwrap_or(state.as_str());
            let after = beat_holding(last).or_else(|| beat_holding(state));
            add_beat(&mut patch, &sections, after, beat, made.deck.pointer("/spine/sections/0").cloned())
        }
        Place::End => {
            let last = sections.iter().enumerate().rev().find_map(|(s, section)| {
                section["beats"].as_array().filter(|b| !b.is_empty()).map(|b| (s, b.len() - 1))
            });
            add_beat(&mut patch, &sections, last, beat, made.deck.pointer("/spine/sections/0").cloned())
        }
    };
    // Its nodes stay on its slide, as a slide's own do (PLAN 3.24): a deck whose next slide
    // starts from the state before it, rather than on its own, would show them there.
    let mut with = doc.clone();
    if scaena_core::patch::apply(&mut with, &patch).is_ok()
        && let Ok(with) = Deck::from_json(&with.to_string())
        && let Some(first) = states.first()
    {
        let new: Vec<(String, scaena_core::document::Node)> = (built.nodes.iter())
            .filter(|(id, _)| !nodes_before.contains_key(*id))
            .filter_map(|(id, node)| Some((id.clone(), serde_json::from_value(node.clone()).ok()?)))
            .collect();
        for hide in crate::inspect::leaving(&with, first, &new) {
            patch.push(serde_json::to_value(hide).context("a patch")?);
        }
    }
    if let Some(old) = replaced {
        patch.push(json!({ "op": "remove_slide", "slide": old }));
    }
    Ok(Written { patch, states, layout, beat: beat_id })
}

/// What [`slide_write`] did, or would do: the patch as a patch reports it; the slide's states,
/// first its own; the layout it took; its beat; each fix lint offered on it that was taken, as
/// the generator takes them; and what lint still finds on it that a fix does not mend, a text
/// past its place first.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SlideWritten {
    #[serde(flatten)]
    pub patched: Patched,
    pub slide: Vec<String>,
    pub layout: String,
    pub beat: String,
    pub fixed: Vec<Finding>,
    pub left: Vec<Finding>,
}

/// [`slide_write`] with nothing written: the slide built by [`slide_into`] in the bundle's theme,
/// its set of texts held to one size and set clear of each other, and each fix lint offers on its
/// states taken, as a made deck's are, all in one patch checked as a patch is.
pub fn slide_writing(
    b: &Bundle,
    slide: Slide,
    place: &Place,
    icons: BTreeMap<String, Icon>,
) -> Result<(SlideWritten, Option<Write>), OpsError> {
    let theme = crate::theme(b)?;
    let what = match place {
        Place::Replace(_) => format!("Slide written again: {}", slide.slide_title),
        _ => format!("Slide written: {}", slide.slide_title),
    };
    let doc = b.deck.to_value().context("the deck")?;
    let written = slide_into(&doc, &theme, slide, place, icons)?;
    let mut ops = written.patch.clone();
    let (first, write) = patching(b, &Value::Array(ops.clone()), Some(&what))?;
    let Some(write) = write.filter(|_| !first.refused) else {
        let left = first.added.clone();
        let out = SlideWritten {
            patched: first,
            slide: written.states,
            layout: written.layout,
            beat: written.beat,
            fixed: Vec::new(),
            left,
        };
        return Ok((out, None));
    };
    // Its set held to one size and set clear, as the generator holds a made slide's: what that
    // changes of the slide's own nodes joins the patch.
    let view = View { base: &b.files, pending: write.files.clone() };
    let was = write.deck.to_value().context("the deck")?;
    let mut doc = was.clone();
    let mut held = Held::default();
    alike(&mut doc, &view, &mut held)?;
    let ours = |f: &Finding| f.state.as_deref().is_some_and(|s| written.states.iter().any(|w| w == s));
    let found: Vec<Finding> = lint_in(&Deck::from_json(&doc.to_string()).context("the deck")?, &view)?.findings;
    clear(&mut doc, &view, found.into_iter().filter(ours).collect(), &mut held)?;
    let added: Vec<&String> = (doc["nodes"].as_object().into_iter().flat_map(|m| m.keys()))
        .filter(|id| written.patch.iter().any(|op| op["path"] == format!("/nodes/{}", esc(id))))
        .collect();
    for id in added {
        if doc["nodes"][id] != was["nodes"][id] {
            ops.push(json!({ "op": "replace", "path": format!("/nodes/{}", esc(id)), "value": doc["nodes"][id] }));
        }
    }
    // Then each fix lint offers on it, each taken where it applies.
    let deck = Deck::from_json(&doc.to_string()).context("the deck")?;
    let findings = lint_in(&deck, &view)?.findings;
    let mut fixed = Vec::new();
    for f in findings.iter().filter(|f| ours(f) && f.code != "I405") {
        let Some(fix) = &f.fix else { continue };
        if scaena_core::patch::apply(&mut doc, fix).is_ok() {
            ops.extend(fix.iter().cloned());
            fixed.push(f.clone());
        }
    }
    let (patched, write) = patching(b, &Value::Array(ops), Some(&what))?;
    let left = match &write {
        Some(w) => lint_in(&w.deck, &view)?.findings.into_iter().filter(|f| ours(f) && f.fix.is_none()).collect(),
        None => patched.added.clone(),
    };
    let mut left: Vec<Finding> = left;
    scaena_core::sort::by_key(&mut left, |f| f.code != "E100");
    let out = SlideWritten { patched, slide: written.states, layout: written.layout, beat: written.beat, fixed, left };
    Ok((out, write))
}

/// `slide` written into the bundle's deck where `place` says, as the deck's generator builds a
/// slide (lift-up FUNC-1), unless `dry_run`; refused, and nothing written, as a patch is.
pub fn slide_write(
    b: &Bundle,
    slide: Slide,
    place: &Place,
    icons: BTreeMap<String, Icon>,
    dry_run: bool,
) -> Result<SlideWritten, OpsError> {
    let (mut out, write) = slide_writing(b, slide, place, icons)?;
    match write {
        Some(write) if !dry_run => crate::lint::write(b, write)?,
        _ => out.patched.applied &= !dry_run,
    }
    Ok(out)
}

/// What to ask a model for one slide at `place` in the deck `doc` (lift-up FUNC-1), the person's
/// `ask` with it: the brief and arc the deck keeps, the part of the talk the slide is in (its
/// section's title and, where it is written again, its beat's claim), the headlines either side
/// of it, and, where it is written again, its words as its first state shows them, in order.
pub fn one_asked(doc: &Value, place: &Place, ask: &str) -> Result<scaena_narrato::one::OneSlide, OpsError> {
    let deck = Deck::from_json(&doc.to_string()).map_err(|e| OpsError::new(format!("the deck: {e}")))?;
    let slides = deck.slides();
    let first_of = |i: usize| deck.states[slides[i][0]].id.as_str();
    let holding = |state: &str| slides.iter().position(|s| s.iter().any(|k| deck.states[*k].id == state));
    let (before, after, here) = match place {
        Place::End => (slides.len().checked_sub(1), None, None),
        Place::After(state) => {
            let i = holding(state).ok_or_else(|| OpsError::new(format!("no state `{state}` in the deck")))?;
            (Some(i), Some(i + 1).filter(|n| *n < slides.len()), None)
        }
        Place::Replace(state) => {
            let i = holding(state).ok_or_else(|| OpsError::new(format!("no state `{state}` in the deck")))?;
            (i.checked_sub(1), Some(i + 1).filter(|n| *n < slides.len()), Some(i))
        }
    };
    // A slide's headline: its beat's claim, else its first state's first words.
    let sections = doc.pointer("/spine/sections").and_then(Value::as_array).cloned().unwrap_or_default();
    let beat_of = |state: &str| {
        sections.iter().find_map(|section| {
            let beat = section["beats"]
                .as_array()?
                .iter()
                .find(|b| b["states"].as_array().is_some_and(|l| l.iter().any(|x| x == state)))?;
            Some((
                section["title"].as_str().unwrap_or_default().to_string(),
                beat["claim"].as_str().unwrap_or_default().to_string(),
            ))
        })
    };
    let words = |state: &str| -> Vec<String> {
        let shown = doc["states"].as_array().and_then(|l| l.iter().find(|s| s["id"] == state));
        (shown.and_then(|s| s["props"].as_object()).into_iter().flat_map(|p| p.keys()))
            .filter_map(|id| {
                let node = &doc["nodes"][id];
                let text = match (node["text"].as_str(), node["runs"].as_array()) {
                    (Some(text), _) => text.to_string(),
                    (None, Some(runs)) => runs.iter().filter_map(|r| r["text"].as_str()).collect(),
                    _ => return None,
                };
                Some(text).filter(|t| !t.trim().is_empty())
            })
            .collect()
    };
    let headline = |i: usize| {
        let state = first_of(i);
        beat_of(state)
            .map(|(_, claim)| claim)
            .filter(|c| !c.trim().is_empty())
            .or_else(|| words(state).into_iter().next())
    };
    let anchor = here.or(before);
    let (beat, claim) = match anchor.and_then(|i| beat_of(first_of(i))) {
        Some((section, claim)) => (Some(section).filter(|s| !s.trim().is_empty()), here.map(|_| claim)),
        None => (None, None),
    };
    Ok(scaena_narrato::one::OneSlide {
        brief: brief_of(doc),
        arc: doc.pointer("/meta/narrative/arc").and_then(Value::as_str).unwrap_or_default().to_string(),
        beat,
        claim: claim.filter(|c| !c.trim().is_empty()),
        before: before.and_then(headline),
        after: after.and_then(headline),
        now: here.map(|i| words(first_of(i)).join("\n")),
        ask: ask.to_string(),
    })
}

/// The ops that put `beat` after the beat at `after` (a section's index and a beat's in it), in
/// that section; with none, in the deck's last section, or in `section`, built with the slide,
/// where the deck has no spine. Its id.
fn add_beat(
    patch: &mut Vec<Value>,
    sections: &[Value],
    after: Option<(usize, usize)>,
    beat: Value,
    section: Option<Value>,
) -> String {
    let id = beat["id"].as_str().unwrap_or_default().to_string();
    match after {
        Some((s, b)) => {
            patch.push(json!({ "op": "add", "path": format!("/spine/sections/{s}/beats/{}", b + 1), "value": beat }))
        }
        None if !sections.is_empty() => patch.push(
            json!({ "op": "add", "path": format!("/spine/sections/{}/beats/-", sections.len() - 1), "value": beat }),
        ),
        None => {
            let mut section = section.unwrap_or_else(|| json!({ "id": "section", "title": "Section", "beats": [] }));
            section["beats"] = json!([beat]);
            patch.push(json!({ "op": "add", "path": "/spine", "value": { "sections": [section] } }));
        }
    }
    id
}

/// The ids of the states of the slide that holds `state`.
fn slides_of<'d>(deck: &'d Deck, state: &str) -> Vec<&'d str> {
    deck.slides()
        .into_iter()
        .find(|s| s.iter().any(|k| deck.states[*k].id == state))
        .map(|s| s.into_iter().map(|k| deck.states[k].id.as_str()).collect())
        .unwrap_or_default()
}

/// The brief a deck keeps (`meta.narrative.brief`), its language from `meta.lang`; a deck made
/// by hand keeps none, and is written to its title.
fn brief_of(doc: &Value) -> Brief {
    let mut brief: Brief =
        doc.pointer("/meta/narrative/brief").cloned().and_then(|b| serde_json::from_value(b).ok()).unwrap_or_default();
    if brief.topic.trim().is_empty() {
        brief.topic = doc.pointer("/meta/title").and_then(Value::as_str).unwrap_or_default().to_string();
    }
    if brief.language.is_none() {
        let lang = doc.pointer("/meta/lang").and_then(Value::as_str);
        brief.language =
            lang.and_then(|tag| LANGUAGES.iter().find(|name| language_tag(name) == Some(tag))).map(|n| n.to_string());
    }
    brief
}

/// The languages Narrato writes, by the names [`language_tag`] knows.
const LANGUAGES: &[&str] = &[
    "English",
    "Spanish",
    "French",
    "German",
    "Portuguese",
    "Italian",
    "Dutch",
    "Czech",
    "Polish",
    "Ukrainian",
    "Japanese",
    "Chinese (Simplified)",
    "Korean",
    "Arabic",
    "Hindi",
];

/// `id` as a JSON Pointer's token.
fn esc(id: &str) -> String {
    id.replace('~', "~0").replace('/', "~1")
}
