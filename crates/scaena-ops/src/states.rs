//! The state strip's patches (PLAN 2.35, ADR-0013): a state added after the one shown, as a
//! step of its slide or as a slide of its own, with an id new to the deck, in the beat of the
//! state it follows; and a slide started from one of the theme's layouts, its words in its slots
//! (PLAN 3.30). Moving, renaming, and removing a state are the ops themselves (`move_state`,
//! `rename_state`, `remove_state`, SPEC §7.3).

use crate::{Context, OpsError};
use scaena_core::Deck;
use scaena_core::inserts::slug;
use scaena_core::model::theme::{Prompt, Slot, Theme};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What the strip adds after the state shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Adding {
    /// A step of its slide, right after it, tracking from it: it shows what that state shows
    /// until it is changed.
    Step,
    /// A slide of its own, after the last step of the state's slide: empty (`absolute`), in
    /// the state's layout.
    Slide,
}

/// A state a patch adds: its id, and the patch.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct AddedState {
    pub id: String,
    /// One `add_state`.
    pub patch: Vec<Value>,
}

/// The patch that adds a state after `shown` (PLAN 2.35). A step is named after the state it
/// follows (`revenue-2`, after `revenue`), a slide `slide`, `slide-2`, …: the first that names
/// no state.
pub fn adding(deck: &Deck, shown: &str, what: Adding) -> Result<AddedState, OpsError> {
    let i = deck
        .states
        .iter()
        .position(|s| s.id == shown)
        .ok_or_else(|| OpsError::new(format!("unknown state `{shown}`")))?;
    let slide = deck.states[i].slide.clone().unwrap_or_else(|| shown.to_string());
    let (id, after, state) = match what {
        Adding::Step => {
            let id = fresh(deck, stem(shown));
            (id.clone(), i, json!({ "id": id, "slide": slide }))
        }
        Adding::Slide => {
            let steps = deck.states[i + 1..].iter().take_while(|s| s.slide.as_deref() == Some(slide.as_str())).count();
            let snaps = scaena_core::resolve_states(deck).context("tracking")?;
            let id = fresh(deck, "slide");
            let mut state = json!({ "id": id, "mode": "absolute" });
            if let Some(layout) = &snaps[i].layout {
                state["layout"] = json!(layout);
            }
            (id, i + steps, state)
        }
    };
    Ok(AddedState { id, patch: vec![added(deck, after, state)] })
}

/// `add_state` for `state`, after the `after`th state, in that state's beat.
fn added(deck: &Deck, after: usize, state: Value) -> Value {
    let after = &deck.states[after].id;
    let mut op = json!({ "op": "add_state", "state": state, "after": after });
    let beats = deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats);
    if let Some(beat) = beats.into_iter().find(|b| b.states.contains(after)) {
        op["beat"] = json!(beat.id);
    }
    op
}

/// The patch that starts a slide in `layout`, one of `theme`'s, after `shown`'s slide (PLAN
/// 3.30): one `add_state`, then an `add_node` for each of the layout's slots that says what goes
/// there, words in its role or a shape, then where those leave (PLAN 3.24).
///
/// The slide goes where [`Adding::Slide`] puts one, after the last step of `shown`'s slide, in
/// the beat of the state it follows. It is named after its layout (`bullets`, `bullets-2`, …),
/// absolute, and in the layout. Each slot's text is in the slot's role, with its prompt's words,
/// a list's items each a paragraph, and is named after the slide and the slot
/// (`bullets-header`). A slot's shape is under the words over it (`z` −1), a decoration. The
/// slots that wait for a picture or a figure it leaves empty. With no layout, the slide is
/// `slide`, blank.
pub fn starting(deck: &Deck, theme: &Theme, shown: &str, layout: Option<&str>) -> Result<AddedState, OpsError> {
    let i = deck
        .states
        .iter()
        .position(|s| s.id == shown)
        .ok_or_else(|| OpsError::new(format!("unknown state `{shown}`")))?;
    let template = match layout {
        Some(name) => {
            Some(theme.layouts.get(name).ok_or_else(|| OpsError::new(format!("the theme has no layout `{name}`")))?)
        }
        None => None,
    };
    let slide = deck.states[i].slide.clone().unwrap_or_else(|| shown.to_string());
    let after = i + deck.states[i + 1..].iter().take_while(|s| s.slide.as_deref() == Some(slide.as_str())).count();
    let id = fresh(deck, &layout.map_or_else(|| "slide".to_string(), |name| slug(name, "slide")));
    let mut state = json!({ "id": id, "mode": "absolute" });
    if let Some(layout) = layout {
        state["layout"] = json!(layout);
    }
    let mut patch = vec![added(deck, after, state.clone())];
    let mut ids: Vec<String> = Vec::new();
    for (slot, s) in template.into_iter().flat_map(|t| &t.slots) {
        // Words that say what goes in a slot with no role, a picture or a figure: it waits.
        let Some(node) = slot_node(slot, s) else { continue };
        let node_id = named(deck, &format!("{id}-{}", slug(slot, "slot")), &ids);
        patch.push(json!({ "op": "add_node", "id": node_id, "node": node, "state": id }));
        ids.push(node_id);
    }
    // Each stays on the slide: where it leaves, read with the slide in the deck.
    let mut made = deck.clone();
    made.states.insert(after + 1, serde_json::from_value(state).context("a slide")?);
    for op in crate::inspect::leaving(&made, &id, &ids) {
        patch.push(serde_json::to_value(op).context("a patch")?);
    }
    Ok(AddedState { id, patch })
}

/// What a new slide puts in `slot`, as its prompt says (PLAN 3.30): words to type over in the
/// slot's role, a list's items each a paragraph; or a card or a rule, under the words in the
/// slots over it (`z` −1), which a reader passes over. None for a slot that waits for a picture or
/// a figure, or says nothing of what goes there.
fn slot_node(slot: &str, s: &Slot) -> Option<Value> {
    let prompt = s.prompt.as_ref()?;
    match (prompt, prompt.text(), &s.role) {
        (_, Some((text, list)), Some(role)) => {
            let mut node = json!({ "type": "text", "role": role, "text": text, "at": { "in": slot } });
            if let Some(kind) = list {
                node["list"] = json!(vec![json!({ "kind": kind }); text.split('\n').count()]);
            }
            Some(node)
        }
        (Prompt::Shape(shape), _, _) => {
            let mut node = json!({ "type": "shape", "kind": shape.shape, "at": { "in": slot }, "z": -1 });
            if let Some(fill) = &shape.fill {
                node["fill"] = json!(fill);
            }
            node["semantic"] = json!("decoration");
            Some(node)
        }
        _ => None,
    }
}

/// The first of `base`, `base-2`, `base-3`, … that names no node of `deck` nor any of `ids`.
fn named(deck: &Deck, base: &str, ids: &[String]) -> String {
    let taken = |n: &str| deck.nodes.contains_key(n) || ids.iter().any(|i| i == n);
    (1..)
        .map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") })
        .find(|n| !taken(n))
        .expect("some number is free")
}

/// A node a patch adds: its id, and the patch.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Filled {
    pub id: String,
    /// One `add_node`, then the `hide_node`s that keep it to its slide.
    pub patch: Vec<Value>,
}

/// The patch that fills `slot`, a slot of `state`'s layout that waits for words (PLAN 3.30), as
/// a new slide in the layout fills it: a text in the slot's role with its prompt's words, a
/// list's items each a paragraph, placed in the slot and named after the slide and the slot
/// (`bullets-header`). It enters in `state` and stays on its slide (PLAN 3.24). What a press on
/// an empty slot's words puts there, as a presentation app's placeholder fills.
pub fn filling(deck: &Deck, theme: &Theme, state: &str, slot: &str) -> Result<Filled, OpsError> {
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let snap =
        snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::new(format!("unknown state `{state}`")))?;
    let layout = snap.layout.as_deref().ok_or_else(|| OpsError::new(format!("`{state}` is in no layout")))?;
    let s = theme
        .layouts
        .get(layout)
        .and_then(|l| l.slots.get(slot))
        .ok_or_else(|| OpsError::new(format!("the layout `{layout}` has no slot `{slot}`")))?;
    let node = slot_node(slot, s).filter(|n| n["type"] == "text").ok_or_else(|| {
        OpsError::new(format!("the slot `{slot}` waits for no words: a picture or a figure goes there"))
    })?;
    let i = deck.state_index(state).ok_or_else(|| OpsError::new(format!("unknown state `{state}`")))?;
    let slide = deck.slide_of(&deck.states[i]);
    let id = named(deck, &format!("{slide}-{}", slug(slot, "slot")), &[]);
    let mut patch = vec![json!({ "op": "add_node", "id": id, "node": node, "state": state })];
    for op in crate::inspect::leaving(deck, state, std::slice::from_ref(&id)) {
        patch.push(serde_json::to_value(op).context("a patch")?);
    }
    Ok(Filled { id, patch })
}

/// A slot of the state's layout that waits for what its prompt says goes there (PLAN 3.30): an
/// editor outlines it, with the prompt's words, until something is placed in it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Waiting {
    pub slot: String,
    /// Its box in the format shown, `[x, y, width, height]`, canvas units.
    pub rect: [f32; 4],
    /// What goes there, as the prompt says it: its first line.
    pub words: String,
    /// Whether words go there, typed in the slot's role; else a picture or a figure.
    pub typed: bool,
    /// The slot's text role, where words go there: what a text put in it is set in, which an
    /// editor inserts when the outline is pressed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// The slots of `state`'s layout, in `format`, that say what goes in them in words and that
/// nothing the state shows is placed in, in the order the theme writes them (PLAN 3.30): the
/// places a new slide's picture or figure goes, and words taken out. None for a state with no
/// layout. A slot whose prompt is a shape is a card or a rule, made with the slide.
pub fn waiting(
    deck: &Deck,
    theme: &scaena_engine::theme::Theme,
    state: &str,
    format: Option<&str>,
) -> Result<Vec<Waiting>, OpsError> {
    let Some((name, boxes)) = scaena_engine::guides::layout(deck, theme, state, format).context("the layout")? else {
        return Ok(Vec::new());
    };
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let snap =
        snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::new(format!("unknown state `{state}`")))?;
    let filled: Vec<&str> = snap
        .nodes
        .values()
        .filter_map(|p| p.get("at").filter(|a| a.get("parent").is_none())?.get("in")?.as_str())
        .collect();
    let Some(layout) = theme.layouts.get(&name) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for b in boxes {
        let Some(slot) = layout.slots.get(&b.name) else { continue };
        let Some((text, _)) = slot.prompt.as_ref().and_then(Prompt::text) else { continue };
        if filled.contains(&b.name.as_str()) {
            continue;
        }
        let words = text.lines().next().unwrap_or_default().to_string();
        out.push(Waiting { slot: b.name, rect: b.rect, words, typed: slot.role.is_some(), role: slot.role.clone() });
    }
    Ok(out)
}

/// `id` without a step's number: `revenue` of `revenue-2`.
fn stem(id: &str) -> &str {
    match id.rsplit_once('-') {
        Some((stem, n)) if !stem.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => stem,
        _ => id,
    }
}

/// The first of `base`, `base-2`, `base-3`, … that names no state of `deck`.
fn fresh(deck: &Deck, base: &str) -> String {
    let taken = |id: &str| deck.states.iter().any(|s| s.id == id);
    (1..)
        .map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") })
        .find(|id| !taken(id))
        .expect("some number is free")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revenue() -> Deck {
        Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap()
    }

    #[test]
    fn a_step_follows_the_state_shown_in_its_slide_and_a_slide_follows_the_slide() {
        let deck = revenue();
        // `mix` builds on `revenue`'s slide: a step after `revenue` joins it, in the beat.
        let step = adding(&deck, "revenue", Adding::Step).unwrap();
        assert_eq!(step.id, "revenue-2");
        assert_eq!(
            step.patch,
            [
                json!({ "op": "add_state", "state": { "id": "revenue-2", "slide": "revenue" }, "after": "revenue", "beat": "doubled" })
            ]
        );
        // A slide goes after `mix`, the slide's last step, empty, in its layout.
        let slide = adding(&deck, "revenue", Adding::Slide).unwrap();
        assert_eq!(
            slide.patch,
            [
                json!({ "op": "add_state", "state": { "id": "slide", "mode": "absolute", "layout": "figure" }, "after": "mix", "beat": "doubled" })
            ]
        );
        assert_eq!(stem("revenue-2"), "revenue");
        assert_eq!(stem("q3-2026"), "q3");
        assert_eq!(stem("-2"), "-2");
        assert!(adding(&deck, "nowhere", Adding::Step).is_err());
    }

    /// A slide started from a layout (PLAN 3.30): after the slide shown, in its layout, a text in
    /// each slot that says what goes there, in its role, and none where another slide would show
    /// it.
    #[test]
    fn a_slide_started_from_a_layout_has_its_words_in_its_slots_and_keeps_them() {
        let deck = revenue();
        let theme =
            scaena_engine::theme::Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json"))
                .unwrap();
        let started = starting(&deck, &theme, "revenue", Some("bullets")).unwrap();
        assert_eq!(started.id, "bullets");
        let bullet = json!({ "kind": "bullet" });
        assert_eq!(
            started.patch,
            [
                json!({ "op": "add_state", "state": { "id": "bullets", "mode": "absolute", "layout": "bullets" }, "after": "mix", "beat": "doubled" }),
                json!({ "op": "add_node", "id": "bullets-header", "state": "bullets", "node": {
                    "type": "text", "role": "headline", "text": "What this slide says", "at": { "in": "header" } } }),
                json!({ "op": "add_node", "id": "bullets-body", "state": "bullets", "node": {
                    "type": "text", "role": "body", "text": "The first point\nThe second point\nThe third point",
                    "at": { "in": "body" }, "list": [bullet, bullet, bullet] } }),
                // `close` tracks from the state before it, now the new slide: they leave there.
                json!({ "op": "hide_node", "node": "bullets-header", "state": "close" }),
                json!({ "op": "hide_node", "node": "bullets-body", "state": "close" }),
            ]
        );
        // A second takes the next name, and a blank one has no layout and nothing in it.
        let mut with = deck.clone();
        with.states.push(serde_json::from_value(json!({ "id": "bullets", "mode": "absolute" })).unwrap());
        assert_eq!(starting(&with, &theme, "revenue", Some("bullets")).unwrap().id, "bullets-2");
        let blank = starting(&deck, &theme, "close", None).unwrap();
        assert_eq!(
            blank.patch,
            [
                json!({ "op": "add_state", "state": { "id": "slide", "mode": "absolute" }, "after": "close", "beat": "thanks" })
            ]
        );
        assert!(starting(&deck, &theme, "revenue", Some("nowhere")).is_err());
        assert!(starting(&deck, &theme, "nowhere", Some("bullets")).is_err());
    }

    /// What waits on a slide (PLAN 3.30): a slide started in `art-left` waits for its picture,
    /// its words in place; with its header taken out, the header waits too; and a state with no
    /// layout waits for nothing.
    #[test]
    fn a_slot_with_nothing_in_it_waits_for_what_its_prompt_says() {
        let deck = revenue();
        let theme =
            scaena_engine::theme::Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json"))
                .unwrap();
        let started = starting(&deck, &theme, "revenue", Some("art-left")).unwrap();
        let patched = |ops: &[Value]| {
            let doc = scaena_core::patch::compile(&deck.to_value().unwrap(), ops, &NoFiles).unwrap().doc;
            Deck::from_value(&doc).unwrap()
        };
        let made = patched(&started.patch);
        let waits = waiting(&made, &theme, "art-left", None).unwrap();
        let said: Vec<(&str, &str, bool)> =
            waits.iter().map(|w| (w.slot.as_str(), w.words.as_str(), w.typed)).collect();
        assert_eq!(said, [("art", "A picture", false)]);
        assert_eq!(waits[0].rect, [120.0, 248.0, 686.0, 736.0], "columns 1 to 5, rows 3 to 12");
        let mut ops = started.patch.clone();
        ops.push(json!({ "op": "remove_node", "id": "art-left-header" }));
        let without = patched(&ops);
        let slots: Vec<(String, Option<String>)> =
            waiting(&without, &theme, "art-left", None).unwrap().into_iter().map(|w| (w.slot, w.role)).collect();
        assert_eq!(
            slots,
            [("header".into(), Some("headline".into())), ("art".into(), None)],
            "in the order the theme writes them, words in the slot's role"
        );
        let blank = patched(&starting(&deck, &theme, "revenue", None).unwrap().patch);
        assert!(waiting(&blank, &theme, "slide", None).unwrap().is_empty());
    }

    /// A slot whose words were taken out is filled as a new slide fills it (PLAN 3.30): the
    /// header's words in its role, the points as their list, each named after the slide and the
    /// slot and kept to it; and no words fill a picture's slot.
    #[test]
    fn an_empty_slot_is_filled_as_a_new_slide_fills_it() {
        let deck = revenue();
        let theme =
            scaena_engine::theme::Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json"))
                .unwrap();
        let patched = |ops: &[Value]| {
            let doc = scaena_core::patch::compile(&deck.to_value().unwrap(), ops, &NoFiles).unwrap().doc;
            Deck::from_value(&doc).unwrap()
        };
        let started = starting(&deck, &theme, "revenue", Some("bullets")).unwrap();
        let mut ops = started.patch.clone();
        ops.push(json!({ "op": "remove_node", "id": "bullets-header" }));
        ops.push(json!({ "op": "remove_node", "id": "bullets-body" }));
        let emptied = patched(&ops);
        let header = filling(&emptied, &theme, "bullets", "header").unwrap();
        assert_eq!(header.id, "bullets-header");
        assert_eq!(
            header.patch,
            [
                json!({ "op": "add_node", "id": "bullets-header", "state": "bullets", "node":
                    { "type": "text", "role": "headline", "text": "What this slide says", "at": { "in": "header" } } }),
                json!({ "op": "hide_node", "node": "bullets-header", "state": "close" }),
            ],
            "what starting put there, kept to the slide"
        );
        let body = filling(&emptied, &theme, "bullets", "body").unwrap();
        assert_eq!(body.patch[0]["node"]["text"], "The first point\nThe second point\nThe third point");
        assert_eq!(
            body.patch[0]["node"]["list"],
            json!([{ "kind": "bullet" }, { "kind": "bullet" }, { "kind": "bullet" }])
        );
        let filled = patched(&[ops, header.patch, body.patch].concat());
        assert!(waiting(&filled, &theme, "bullets", None).unwrap().is_empty(), "nothing waits once both are filled");
        let pictured = patched(&starting(&deck, &theme, "revenue", Some("art-left")).unwrap().patch);
        let art = filling(&pictured, &theme, "art-left", "art").unwrap_err();
        assert!(art.to_string().contains("waits for no words"), "{art}");
    }

    /// No file: the patches here read none.
    struct NoFiles;
    impl scaena_core::validate::BundleFiles for NoFiles {
        fn exists(&self, _: &str) -> bool {
            false
        }
        fn read_text(&self, _: &str) -> Option<String> {
            None
        }
    }
}
