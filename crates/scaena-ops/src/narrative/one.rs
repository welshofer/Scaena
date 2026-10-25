//! One slide written into an open deck as the generator builds them (lift-up FUNC-1): a slide
//! Narrato wrote, or a person's model wrote for one place, built by the narrative's own slide
//! builder in the deck's theme and brief, and put in as one patch.

use super::{Builder, Held, Narrative, Pictures, Placed, Taken, alike, assemble, clear, language_tag};
use crate::lint::{View, Write, lint_in};
use crate::patch::{Patched, patching};
use crate::{Bundle, Context, OpsError};
use scaena_core::Deck;
use scaena_core::Finding;
use scaena_core::data::{Datum, SourceFiles};
use scaena_core::model::theme::Theme;
use scaena_core::tracking::{Snapshot, resolve_states};
use scaena_narrato::{Brief, ChartData, Icon, Series, Slide};
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
/// the slide's own; the layout it took; its beat's id; and what the change is called.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Written {
    pub patch: Vec<Value>,
    pub states: Vec<String>,
    pub layout: String,
    pub beat: String,
    pub what: String,
}

/// `slide` built for the deck `doc`, in `theme`, as the deck's generator builds a slide: its
/// layout chosen for its items, each place filled, its figures, builds, and icons (`icons`, by
/// the word each item names, as [`super::icons_found`] finds them), in the brief the deck keeps
/// (`meta.narrative.brief`, else none), entering after the slide before it as a made deck's
/// slides enter. `files` are the bundle's, which its data sources read. The patch:
///
/// - adds its nodes and data under ids the deck does not take, and its states after the slide
///   `place` names, or at the end, every state of the deck showing what it showed (a slide after
///   it that tracked from the one before it written again to show it still);
/// - gives it a beat after that slide's beat, in its section, its claim the slide's title and its
///   notes the slide's, where the deck has a spine;
/// - for [`Place::Replace`], puts it in the old slide's beat, which keeps its id, its claim, its
///   evidence, its notes, its time, and what else it holds; gives it the old slide's transition,
///   hold, furniture, and notes, and every link to the old slide; and takes the old slide away
///   with `remove_slide`, its nodes no other slide shows going with it. A picture the old slide
///   showed is shown again, in a layout that holds one, and so is the chart it showed, from its
///   rows, where the slide written draws none of its own.
pub fn slide_into(
    doc: &Value,
    theme: &Theme,
    slide: Slide,
    place: &Place,
    icons: BTreeMap<String, Icon>,
    files: &dyn SourceFiles,
) -> Result<Written, OpsError> {
    let deck = Deck::from_json(&doc.to_string()).map_err(|e| OpsError::new(format!("the deck: {e}")))?;
    let snaps = resolve_states(&deck).map_err(|e| OpsError::new(format!("the deck: {e}")))?;
    let slides = deck.slides();
    let id_of = |k: usize| deck.states[k].id.as_str();
    let holding = |state: &str| slides.iter().position(|s| s.iter().any(|k| id_of(*k) == state));
    let found = |state: &str| holding(state).ok_or_else(|| OpsError::new(format!("no state `{state}` in the deck")));
    // Where it goes among the slides, and among the states; the slide it takes the place of; and
    // the slide it follows.
    let (index, at, old, previous) = match place {
        Place::End => (slides.len(), deck.states.len(), None, slides.len().checked_sub(1)),
        Place::After(state) => {
            let i = found(state)?;
            (i + 1, slides[i].last().map_or(0, |k| k + 1), None, Some(i))
        }
        Place::Replace(state) => {
            let i = found(state)?;
            (i, slides[i].last().map_or(0, |k| k + 1), Some(i), i.checked_sub(1))
        }
    };
    let replaced = old.map(|i| id_of(slides[i][0]).to_string());
    let what = match old {
        Some(_) => format!("Slide written again: {}", slide.slide_title),
        None => format!("Slide written: {}", slide.slide_title),
    };
    let old_states: Vec<&str> = old.map(|i| slides[i].iter().map(|k| id_of(*k)).collect()).unwrap_or_default();
    let total = slides.len() + usize::from(old.is_none());
    // The old slide whole, as its last state shows it.
    let old_shown = old.and_then(|i| slides[i].last()).map(|k| &snaps[*k]);
    let mut slide = slide;
    let mut pictures = Pictures::new();
    if let Some(shown) = old_shown {
        // The picture the old slide showed, shown again: an image it shows with a file.
        let picture = shown.nodes.keys().find_map(|id| {
            let node = &doc["nodes"][id];
            let src = node["src"].as_str().filter(|_| node["type"] == "image" && node.get("fetched").is_none())?;
            let alt = node["alt"].as_str().unwrap_or_default();
            Some(Placed { src: src.to_string(), alt: alt.to_string() })
        });
        if let Some(picture) = picture {
            pictures.insert(0, picture);
        }
        // The chart it showed, drawn again from its rows where the slide written draws none.
        if slide.chart_data.is_none() {
            slide.chart_data = charted(doc, &deck, files, shown).map(|(data, _)| data);
        }
    }

    // The deck as built so far: every id it takes, so what is built takes none of them; and the
    // slide it follows, which it enters after.
    let mut seed = Builder {
        nodes: doc["nodes"].as_object().cloned().unwrap_or_default(),
        states: doc["states"].as_array().cloned().unwrap_or_default(),
        data: doc["data"].as_object().cloned().unwrap_or_default(),
        previous: previous
            .and_then(|p| slides[p].last())
            .map(|k| (snaps[*k].layout.clone().unwrap_or_default(), Slide::default())),
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
    let mut new_states: Vec<Value> = built.states[states_before..].to_vec();
    let states: Vec<String> = new_states.iter().filter_map(|s| s["id"].as_str().map(str::to_string)).collect();
    let mut beat = made
        .deck
        .pointer("/spine/sections/0/beats/0")
        .cloned()
        .ok_or_else(|| OpsError::new("the slide was built with no beat"))?;

    // What the old slide's first state says of how it enters and is presented, its own.
    if let (Some(old), Some(first)) = (old, new_states.first_mut()) {
        let was = &doc["states"][slides[old][0]];
        for key in ["transition", "hold", "furniture", "notes"] {
            if let Some(value) = was.get(key) {
                first[key] = value.clone();
            }
        }
    }

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

    // Its states among the deck's, each of the deck's showing what it showed: the slide after it,
    // tracking from the state before it, is written again to track from the new slide's last.
    let mut with = doc.clone();
    scaena_core::patch::apply(&mut with, &patch).map_err(|e| OpsError::new(format!("the slide: {e}")))?;
    let mut given = deck.states.clone();
    let typed: Vec<scaena_core::document::State> = (new_states.iter())
        .map(|s| serde_json::from_value(s.clone()))
        .collect::<Result<_, _>>()
        .context("the slide's states")?;
    given.splice(at..at, typed);
    let mut kept =
        scaena_core::patch::states_kept(&with, given).map_err(|e| OpsError::new(format!("the slide's place: {e}")))?;
    if let Some(list) = kept.as_array_mut() {
        for state in list.iter_mut() {
            if let Some(k) = states.iter().position(|id| state["id"] == id.as_str()) {
                *state = new_states[k].clone();
            }
        }
    }
    // Every link to the old slide goes to the new one.
    if let Some(first) = states.first().filter(|_| !old_states.is_empty()) {
        relink(&mut kept, &old_states, first);
        for at in ["/nodes", "/overrides"] {
            if let Some(mut part) = with.pointer(at).cloned()
                && relink(&mut part, &old_states, first)
            {
                patch.push(json!({ "op": "replace", "path": at, "value": part }));
            }
        }
    }
    patch.push(json!({ "op": "replace", "path": "/states", "value": kept }));

    // Its beat: the old slide's, where it takes one's place; else one after the beat of the slide
    // it follows, or the last, in that beat's section. None where the deck has no spine.
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
    // The beat of any of a slide's states, the last's first.
    let slide_beat = |i: usize| slides[i].iter().rev().find_map(|k| beat_holding(id_of(*k)));
    let beat_id = if sections.is_empty() {
        String::new()
    } else {
        match old.and_then(slide_beat) {
            Some((s, b)) if old.is_some() => {
                let was = &sections[s]["beats"][b];
                let mut merged = was.clone();
                // The old slide's states leave the beat with `remove_slide`; any other the beat
                // holds stays, after the new slide's.
                let others = (was["states"].as_array().into_iter().flatten())
                    .filter(|x| x.as_str().is_some_and(|x| !old_states.contains(&x)))
                    .cloned();
                let listed: Vec<Value> = states.iter().map(|s| json!(s)).chain(others).collect();
                merged["states"] = json!(listed);
                // What the old beat says stays; what it does not say, the new slide's beat does.
                for (key, value) in beat.as_object().into_iter().flatten() {
                    if key != "id" && merged.get(key).is_none_or(|v| v.is_null() || v == "") {
                        merged[key] = value.clone();
                    }
                }
                patch.push(
                    json!({ "op": "replace", "path": format!("/spine/sections/{s}/beats/{b}"), "value": merged }),
                );
                was["id"].as_str().unwrap_or_default().to_string()
            }
            _ => {
                let after = match place {
                    Place::End => sections.iter().enumerate().rev().find_map(|(s, section)| {
                        section["beats"].as_array().filter(|b| !b.is_empty()).map(|b| (s, b.len() - 1))
                    }),
                    _ => previous.and_then(slide_beat),
                };
                add_beat(&mut patch, &sections, after, std::mem::take(&mut beat))
            }
        }
    };
    // Its nodes stay on its slide, as a slide's own do (PLAN 3.24).
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
    Ok(Written { patch, states, layout, beat: beat_id, what })
}

/// Each run's link in `value` (a deck's nodes, its overrides, or its states) to one of `from`
/// pointed at `to`. Whether one was.
fn relink(value: &mut Value, from: &[&str], to: &str) -> bool {
    match value {
        Value::Object(m) => {
            let mut changed = false;
            if let Some(Value::Object(link)) = m.get_mut("link")
                && link.get("state").and_then(Value::as_str).is_some_and(|s| from.contains(&s))
            {
                link.insert("state".into(), json!(to));
                changed = true;
            }
            for v in m.values_mut() {
                changed |= relink(v, from, to);
            }
            changed
        }
        Value::Array(items) => items.iter_mut().fold(false, |changed, v| relink(v, from, to) | changed),
        _ => false,
    }
}

/// The first chart `shown` (a state as it resolves) draws, as Narrato writes a slide's chart:
/// its categories along `x`, a series for each value of its `series` or `color`, else one of its
/// `y`, read from its source through its `dataTransform` in `files`; and its rows as a table
/// reads them, for a model shown the slide. None where it draws none, or its rows do not read.
fn charted(doc: &Value, deck: &Deck, files: &dyn SourceFiles, shown: &Snapshot) -> Option<(ChartData, String)> {
    let (_, props) = shown.nodes.iter().find(|(id, _)| doc["nodes"][id.as_str()]["type"] == "chart")?;
    let table = scaena_core::data::read(deck, files, props).ok()?;
    let field = |channel: &str| props.get(channel).and_then(|c| c.get("field")).and_then(Value::as_str);
    let x = table.column(field("x")?)?;
    let y = table.column(field("y")?)?;
    let by = field("series").or_else(|| field("color")).and_then(|f| table.column(f));
    let mut categories: Vec<String> = Vec::new();
    let mut named: Vec<String> = Vec::new();
    for row in &table.rows {
        let category = row[x].label();
        if !categories.contains(&category) {
            categories.push(category);
        }
        let name = by.map(|c| row[c].label()).unwrap_or_else(|| table.columns[y].clone());
        if !named.contains(&name) {
            named.push(name);
        }
    }
    let mut series: Vec<Series> =
        named.iter().map(|n| Series { name: Some(n.clone()), values: vec![Some(0.0); categories.len()] }).collect();
    for row in &table.rows {
        let Datum::Number(v) = row[y] else { continue };
        let c = categories.iter().position(|c| *c == row[x].label())?;
        let name = by.map(|b| row[b].label()).unwrap_or_else(|| table.columns[y].clone());
        let s = named.iter().position(|n| *n == name)?;
        *series[s].values[c].get_or_insert(0.0) += v;
    }
    let mut rows = table.columns.join(", ");
    for row in table.rows.iter().take(ROWS_SHOWN) {
        rows.push('\n');
        rows.push_str(&row.iter().map(Datum::label).collect::<Vec<_>>().join(", "));
    }
    if table.rows.len() > ROWS_SHOWN {
        rows.push_str(&format!("\n… and {} rows more", table.rows.len() - ROWS_SHOWN));
    }
    Some((ChartData { categories, series, unit: None, source_id: None }, rows))
}

/// The most rows of a chart a model is shown.
const ROWS_SHOWN: usize = 40;

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
    let doc = b.deck.to_value().context("the deck")?;
    let none = View { base: &b.files, pending: BTreeMap::new() };
    let written = slide_into(&doc, &theme, slide, place, icons, &scaena_core::data::Texts(&none))?;
    slide_polishing(b, written)
}

/// `written`, a slide [`slide_into`] built for the bundle's deck, made one patch checked as a
/// patch is, its set of texts held to one size and set clear and each fix lint offers on its
/// states taken, as a made deck's are, nothing written: the second half of [`slide_writing`],
/// which the browser's editor module runs on what its generator's module built (lift-up FUNC-1).
pub fn slide_polishing(b: &Bundle, written: Written) -> Result<(SlideWritten, Option<Write>), OpsError> {
    let what = written.what.clone();
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
/// of it, and, where it is written again, its words as the slide shows them whole, in order, with
/// the rows of the chart it draws, read from `files`, the bundle's.
pub fn one_asked(
    doc: &Value,
    place: &Place,
    ask: &str,
    files: &dyn SourceFiles,
) -> Result<scaena_narrato::one::OneSlide, OpsError> {
    let deck = Deck::from_json(&doc.to_string()).map_err(|e| OpsError::new(format!("the deck: {e}")))?;
    let snaps = resolve_states(&deck).map_err(|e| OpsError::new(format!("the deck: {e}")))?;
    let slides = deck.slides();
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
    // What a state shows of its words, as it resolves, in order.
    let words = |state: &str| -> Vec<String> {
        let shown = snaps.iter().find(|s| s.state_id == state);
        (shown.into_iter().flat_map(|s| s.nodes.iter()))
            .filter(|(id, _)| doc["nodes"][id.as_str()]["type"] == "text")
            .map(|(_, props)| scaena_core::reading::words(props))
            .filter(|t| !t.trim().is_empty())
            .collect()
    };
    // A slide's last state: the slide whole, its builds done.
    let last_of = |i: usize| deck.states[*slides[i].last().unwrap_or(&slides[i][0])].id.as_str();
    // The beat of any of a slide's states, the first's first.
    let slide_beat = |i: usize| slides[i].iter().find_map(|k| beat_of(deck.states[*k].id.as_str()));
    // A slide's headline: its beat's claim, else the first words it shows.
    let headline = |i: usize| {
        slide_beat(i)
            .map(|(_, claim)| claim)
            .filter(|c| !c.trim().is_empty())
            .or_else(|| words(last_of(i)).into_iter().next())
    };
    let anchor = here.or(before);
    let (beat, claim) = match anchor.and_then(slide_beat) {
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
        now: here.map(|i| {
            let mut now = words(last_of(i)).join("\n");
            let shown = snaps.iter().find(|s| s.state_id == last_of(i));
            if let Some((_, rows)) = shown.and_then(|shown| charted(doc, &deck, files, shown)) {
                now.push_str("\nIts chart's rows:\n");
                now.push_str(&rows);
            }
            now
        }),
        ask: ask.to_string(),
    })
}

/// The ops that put `beat` after the beat at `after` (a section's index and a beat's in it), in
/// that section; with none, at the end of the deck's last section. Its id.
fn add_beat(patch: &mut Vec<Value>, sections: &[Value], after: Option<(usize, usize)>, beat: Value) -> String {
    let id = beat["id"].as_str().unwrap_or_default().to_string();
    let path = match after {
        Some((s, b)) => format!("/spine/sections/{s}/beats/{}", b + 1),
        None => format!("/spine/sections/{}/beats/-", sections.len().saturating_sub(1)),
    };
    patch.push(json!({ "op": "add", "path": path, "value": beat }));
    id
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
