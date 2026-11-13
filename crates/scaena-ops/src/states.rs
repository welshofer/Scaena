//! The state strip's patches (PLAN 2.35, ADR-0013): a state added after the one shown, as a
//! step of its slide or as a slide of its own, with an id new to the deck, in the beat of the
//! state it follows; and a slide started from one of the theme's layouts, its words in its slots
//! (PLAN 3.30). Moving, renaming, and removing a state are the ops themselves (`move_state`,
//! `rename_state`, `remove_state`, SPEC §7.3).

use crate::lint::Write;
use crate::patch::Patched;
use crate::{Bundle, Context, OpsError};
use scaena_core::Deck;
use scaena_core::inserts::slug;
use scaena_core::model::theme::Vocabulary;
use scaena_core::model::theme::{Example, Prompt, PromptShapeKind, Slot, Theme};
use scaena_core::words;
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
    /// A slide started in a layout ([`starting`]): each slot a node fills, and the node.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub filled: Vec<SlotFilled>,
    /// A slide started in a layout: each slot left empty for a picture or a figure, and what its
    /// prompt says goes there.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub empty: Vec<SlotEmpty>,
}

/// A slot of a new slide's layout, and the node put in it (dogfood 3).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct SlotFilled {
    pub slot: String,
    pub node: String,
}

/// A slot of a new slide's layout left empty, and what its prompt says goes there: a picture
/// or a figure, for `add_node` with `at: { in: slot }` to put (dogfood 3).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct SlotEmpty {
    pub slot: String,
    pub waits: String,
}

/// The patch that adds a state after `shown` (PLAN 2.35). A step is named after the state it
/// follows (`revenue-2`, after `revenue`), a slide `slide`, `slide-2`, …: the first that names
/// no state.
pub fn adding(deck: &Deck, shown: &str, what: Adding) -> Result<AddedState, OpsError> {
    let i = deck.states.iter().position(|s| s.id == shown).ok_or_else(|| OpsError::unknown_state(deck, shown))?;
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
    Ok(AddedState { id, patch: vec![added(deck, after, state)], filled: Vec::new(), empty: Vec::new() })
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

/// The names people search for a slide by that the library offers under another name (ADR-0031,
/// decision 7), by layout: Add Slide's search, on every client, finds Roadmap by Kanban, as
/// [`also_called`] says.
pub const ALSO_CALLED: &[(&str, &[&str])] = &[
    ("roadmap", &["Kanban"]),
    ("numbered", &["Learning Objectives"]),
    ("hub-spoke", &["Mind Map"]),
    ("quote", &["Scripture"]),
    ("bio", &["In Memoriam"]),
    ("agenda", &["Order of Service"]),
    ("comparison", &["Problem and Solution"]),
    ("big-question", &["Exit Ticket"]),
    ("table", &["Rubric"]),
    ("closing", &["Thank You"]),
    ("chyron", &["Lower Third Lyrics"]),
    ("fine-print", &["Disclosures"]),
    ("price-chart", &["Stock", "Ticker"]),
];

/// The other names a layout is searched for by, as [`ALSO_CALLED`] gives them: none for most.
pub fn also_called(layout: &str) -> &'static [&'static str] {
    ALSO_CALLED.iter().find(|(name, _)| *name == layout).map_or(&[], |(_, also)| also)
}

/// The patch that starts a slide in `layout`, one of `theme`'s, after `shown`'s slide (PLAN
/// 3.30): one `add_state`, then an `add_node` for each of the layout's slots that says what goes
/// there, words in its role or a shape, then where those leave (PLAN 3.24).
///
/// The slide goes where [`Adding::Slide`] puts one, after the last step of `shown`'s slide, in
/// the beat of the state it follows. It is named after its layout (`bullets`, `bullets-2`, …),
/// absolute, and in the layout. Each slot's text is in the slot's role, with its prompt's words,
/// a list's items each a paragraph, and is named after the slide and the slot
/// (`bullets-header`). A slot's shape is under the words over it (`z` −1), a decoration, but an
/// icon, which stands with them (PLAN G.23). The slots that wait for a picture or a figure it
/// leaves empty. With no layout, the slide is
/// `slide`, blank.
pub fn starting(deck: &Deck, theme: &Theme, shown: &str, layout: Option<&str>) -> Result<AddedState, OpsError> {
    starting_as(deck, theme, shown, layout, None)
}

/// [`starting`], the state named `id` where one is given, and its nodes after it (`id-header`),
/// refused where a state has the name already (`state_start`'s `id`, dogfood 3).
pub fn starting_as(
    deck: &Deck,
    theme: &Theme,
    shown: &str,
    layout: Option<&str>,
    id: Option<&str>,
) -> Result<AddedState, OpsError> {
    if let Some(id) = id
        && deck.states.iter().any(|s| s.id == id)
    {
        return Err(OpsError::new(format!("a state is named `{id}` already")));
    }
    let i = deck.states.iter().position(|s| s.id == shown).ok_or_else(|| OpsError::unknown_state(deck, shown))?;
    let template = match layout {
        Some(name) => Some(theme.layouts.get(name).ok_or_else(|| {
            let known = theme.layouts.keys().map(String::as_str);
            OpsError::new(format!(
                "the theme has no layout `{name}`{}",
                scaena_core::patch::nearest_in("the theme", "layouts", name, known)
            ))
        })?),
        None => None,
    };
    let slide = deck.states[i].slide.clone().unwrap_or_else(|| shown.to_string());
    let after = i + deck.states[i + 1..].iter().take_while(|s| s.slide.as_deref() == Some(slide.as_str())).count();
    let id = match id {
        Some(id) => id.to_string(),
        None => fresh(deck, &layout.map_or_else(|| "slide".to_string(), |name| slug(name, "slide"))),
    };
    let mut state = json!({ "id": id, "mode": "absolute" });
    if let Some(layout) = layout {
        state["layout"] = json!(layout);
    }
    // In a deck that runs on its own, its states held, the slide holds as the state it follows
    // does, so it shows for some time (W323; agent dogfood 6).
    let runs = deck.states.iter().any(|s| s.hold.is_some_and(|h| h > 0.0));
    if let Some(hold) = deck.states[after].hold.filter(|h| runs && *h > 0.0) {
        state["hold"] = json!(hold);
    }
    let mut patch = vec![added(deck, after, state.clone())];
    let mut ids: Vec<String> = Vec::new();
    let mut nodes = Vec::new();
    let (mut filled, mut empty) = (Vec::new(), Vec::new());
    let mut claimed = false;
    for (slot, s) in template.into_iter().flat_map(|t| &t.slots) {
        // Words that say what goes in a slot with no role, a picture or a figure: it waits.
        let Some(mut node) = slot_node(slot, s, scaena_core::direction::mirrored(deck)) else {
            if let (None, Some((waits, _))) = (&s.role, s.prompt.as_ref().and_then(Prompt::text)) {
                empty.push(SlotEmpty { slot: slot.clone(), waits });
            }
            continue;
        };
        coded(&mut node, s, theme);
        told(slot, s, layout.unwrap_or_default(), theme, &mut node, &mut claimed);
        let node_id = named(deck, &format!("{id}-{}", slug(slot, "slot")), &ids);
        filled.push(SlotFilled { slot: slot.clone(), node: node_id.clone() });
        nodes.push((node_id.clone(), serde_json::from_value(node.clone()).context("a slot's node")?));
        patch.push(json!({ "op": "add_node", "id": node_id, "node": node, "state": id }));
        ids.push(node_id);
    }
    // Each stays on the slide: where it leaves, read with the slide in the deck.
    let mut made = deck.clone();
    made.states.insert(after + 1, serde_json::from_value(state).context("a slide")?);
    for op in crate::inspect::leaving(&made, &id, &nodes) {
        patch.push(serde_json::to_value(op).context("a patch")?);
    }
    Ok(AddedState { id, patch, filled, empty })
}

/// A new slide's text in `slot` set as a made deck sets it (dogfood 3, `narrative`): its first
/// headline the slide's claim, shrunk to fit as it is typed over; and its one list, its `body`,
/// grown to fill its place, up to the theme's lede (an agenda's to its titles).
fn told(slot: &str, s: &Slot, layout: &str, theme: &Theme, node: &mut Value, claimed: &mut bool) {
    const HEADS: [&str; 8] = ["header", "headline", "title", "claim", "statement", "question", "ask", "point"];
    let Some(role) = s.role.as_deref() else { return };
    if !*claimed && HEADS.contains(&slot) && matches!(role, "headline" | "title" | "display") {
        *claimed = true;
        node["semantic"] = json!("claim");
        node["fit"] = json!("shrink");
    }
    let most = if layout.starts_with("agenda") { "title" } else { "lede" };
    if slot == "body"
        && node.get("list").is_some()
        && let Some(most) = theme.typography.roles.get(most).map(|r| r.size)
    {
        node["fit"] = json!("grow");
        node["maxSize"] = json!(most);
    }
}

/// `added`, a slide [`starting`] made, telling `claim`: in the beat the spine planned that makes
/// it already, the one telling no slide yet first (agent dogfood 8); else in a beat of its own
/// (dogfood 3), just after the one that tells the state the slide follows, in that section, else
/// last, and not in that beat, which would then make two claims. Its headline says the claim.
/// Refused in a deck with no spine.
pub fn claimed(deck: &Deck, mut added: AddedState, claim: &str) -> Result<AddedState, OpsError> {
    let spine = deck.spine.as_ref().filter(|s| !s.sections.is_empty()).ok_or_else(|| {
        OpsError::new(
            "the deck has no spine to tell the claim in: start the slide with no claim, or give the deck a spine first",
        )
    })?;
    let first =
        added.patch.first_mut().and_then(Value::as_object_mut).ok_or_else(|| OpsError::new("no slide added"))?;
    first.remove("beat");
    let after = first.get("after").and_then(Value::as_str).unwrap_or_default().to_string();
    // A beat the spine planned that makes this claim already is the one the slide tells (agent
    // dogfood 8: W429 asks for a slide with its claim, which a beat of its own would make twice).
    let same = |c: &str| c.trim().eq_ignore_ascii_case(claim.trim());
    let planned = (spine.sections.iter().enumerate())
        .flat_map(|(s, section)| section.beats.iter().enumerate().map(move |(b, beat)| (s, b, beat)))
        .filter(|(_, _, beat)| same(&beat.claim))
        .min_by_key(|(_, _, beat)| !beat.states.is_empty());
    if let Some((s, b, beat)) = planned {
        let states: Vec<&str> = beat.states.iter().map(String::as_str).chain([added.id.as_str()]).collect();
        let path = format!("/spine/sections/{s}/beats/{b}/states");
        added.patch.push(json!({ "op": "add", "path": path, "value": states }));
        headlined(&mut added, claim);
        return Ok(added);
    }
    let told = spine
        .sections
        .iter()
        .enumerate()
        .find_map(|(s, section)| section.beats.iter().position(|b| b.states.contains(&after)).map(|b| (s, b + 1)));
    let last = spine.sections.len() - 1;
    let (s, b) = told.unwrap_or((last, spine.sections[last].beats.len()));
    let taken = |id: &str| spine.sections.iter().flat_map(|s| &s.beats).any(|b| b.id == id);
    let id = (1..)
        .map(|n| if n == 1 { added.id.clone() } else { format!("{}-{n}", added.id) })
        .find(|id| !taken(id))
        .expect("some number is free");
    let beat = json!({ "id": id, "claim": claim, "states": [added.id] });
    added.patch.push(json!({ "op": "add", "path": format!("/spine/sections/{s}/beats/{b}"), "value": beat }));
    headlined(&mut added, claim);
    Ok(added)
}

/// The headline `added` was started with says `claim`, in place of the words to type over that
/// tell what goes there (agent dogfood 6: "What this slide says" under a claim given).
fn headlined(added: &mut AddedState, claim: &str) {
    let headline = (added.patch.iter_mut())
        .find(|op| op["op"] == "add_node" && op["node"]["semantic"] == "claim" && op["node"]["text"].is_string());
    if let Some(op) = headline {
        op["node"]["text"] = json!(claim.trim());
    }
}

/// A slide started and written into the bundle (`state_start`, `scaena start`): the new state's
/// id, and what the patch did, as `deck_patch` reports a patch.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Started {
    /// The id of the state added: named after its layout (`bullets`, `bullets-2`, …), or
    /// `slide` with none.
    pub id: String,
    /// Each slot a node fills, and the node, to put words in with `replace_text` or `set_text`.
    pub filled: Vec<SlotFilled>,
    /// Each slot left empty for a picture or a figure, and what goes there.
    pub empty: Vec<SlotEmpty>,
    #[serde(flatten)]
    pub patched: Patched,
}

/// What the history says [`start`]'s patch did: `Slide added as Bullets`, or `Slide added`.
pub fn said(theme: &Theme, layout: Option<&str>) -> String {
    match layout {
        Some(name) => format!("Slide added as {}", words::named(Vocabulary::Layout, name, theme)),
        None => "Slide added".to_string(),
    }
}

/// [`start`] with nothing written: [`starting`]'s patch checked as a patch is (refused where it
/// would leave the deck invalid), and the bundle it writes, for a client that keeps its bundle in
/// memory.
pub fn started(
    b: &Bundle,
    shown: &str,
    layout: Option<&str>,
    claim: Option<&str>,
    id: Option<&str>,
) -> Result<(Started, Option<Write>), OpsError> {
    let theme = crate::theme(b)?;
    let mut added = starting_as(&b.deck, &theme, shown, layout, id)?;
    if let Some(claim) = claim {
        added = claimed(&b.deck, added, claim)?;
    }
    let (patched, write) = crate::patch::patching(b, &Value::Array(added.patch), Some(&said(&theme, layout)))?;
    Ok((Started { id: added.id, filled: added.filled, empty: added.empty, patched }, write))
}

/// A slide started in `layout`, one of the theme's, after `shown`'s slide, as Add Slide starts one
/// (PLAN 3.30), written into the bundle as one change: [`starting`]'s patch, applied as
/// `deck_patch` applies one, all or none, refused where it would leave the deck invalid. A dry
/// run says what it would do and writes nothing. The theme's layouts are its `layouts`, which
/// `inspect --layouts` lists for a state, each by the name a person calls it. With `claim`, the
/// slide tells it in a beat of its own ([`claimed`]); with `id`, the state is named so.
pub fn start(
    b: &Bundle,
    shown: &str,
    layout: Option<&str>,
    claim: Option<&str>,
    id: Option<&str>,
    dry_run: bool,
) -> Result<Started, OpsError> {
    let (mut started, write) = started(b, shown, layout, claim, id)?;
    match write {
        Some(write) if !dry_run => crate::lint::write(b, write)?,
        _ => started.patched.applied &= !dry_run,
    }
    Ok(started)
}

/// The patch that makes a new deck's title slide after `shown`'s slide (PLAN 3.30): in the theme's
/// `title` layout, its words to type over and each place that waits with an `example` given it,
/// the theme's backdrop where a picture goes, as the New dialog draws the theme. So a new deck
/// opens as its theme was shown, with no place left empty for lint to find (W304); Add Slide's
/// [`starting`] leaves such a place waiting. Where the theme's example cannot be drawn, the slide
/// [`starting`] makes.
pub fn titled(deck: &Deck, theme: &Theme, shown: &str) -> Result<AddedState, OpsError> {
    example(deck, theme, shown, "title").or_else(|_| starting(deck, theme, shown, Some("title")))
}

/// The patch that draws `layout`'s picture after `shown`'s slide (PLAN 3.31): the slide
/// [`starting`] makes, and in each slot that waits with an `example`, what it says: a picture
/// drawn by the theme's shader preset, under the cards and the words over it, each picture on the
/// slide with a seed of its own; a map's place, that picture with a pin and a label over it (PLAN
/// O.1e); a chart or a table of a few rows made up for it, written inline; or code, set in the text
/// the slot starts with (theme format 0.29). An editor draws it, so a
/// layout's picture shows what each slot is for, and never saves it.
pub fn example(deck: &Deck, theme: &Theme, shown: &str, layout: &str) -> Result<AddedState, OpsError> {
    let mut started = starting(deck, theme, shown, Some(layout))?;
    let template =
        theme.layouts.get(layout).ok_or_else(|| OpsError::new(format!("the theme has no layout `{layout}`")))?;
    let presets = theme.shaders.as_ref().and_then(|s| s.presets.as_ref());
    let mut ids: Vec<String> = Vec::new();
    let mut sources = serde_json::Map::new();
    let mut seed = 0u64;
    for (slot, s) in &template.slots {
        let Some(shown_as) = &s.example else { continue };
        let id = named(deck, &format!("{}-{}", started.id, slug(slot, "slot")), &ids);
        let node = match shown_as {
            Example::Picture(preset) => {
                let Some(p) = presets.and_then(|p| p.get(preset)) else {
                    return Err(OpsError::new(format!("the theme has no shader preset `{preset}`")));
                };
                seed += 1;
                // Under the cards (`z` −1) and the words over it, as a picture placed there lies.
                json!({ "type": "shader", "kind": p.kind, "preset": preset, "seed": seed, "at": { "in": slot }, "z": -2 })
            }
            Example::Chart(kind) => {
                let (rows, schema, mut node) = example_chart(*kind);
                sources.insert(id.clone(), json!({ "source": { "inline": rows }, "schema": schema }));
                node["data"] = json!(format!("@{id}"));
                node["at"] = json!({ "in": slot });
                node
            }
            Example::Map(preset) => {
                let Some(p) = presets.and_then(|p| p.get(preset)) else {
                    return Err(OpsError::new(format!("the theme has no shader preset `{preset}`")));
                };
                seed += 1;
                // A pin, its tip at the place's middle, and a label under it, over the picture: a
                // map's place, drawn with no provider's map (ADR-0030).
                let pin = named(deck, &format!("{id}-pin"), &ids);
                let pinned = json!({
                    "type": "shape", "kind": "path", "path": MAP_PIN, "view": [0, 0, 1000, 1000], "fill": "accent",
                    "semantic": "decoration", "at": { "in": slot },
                });
                started.patch.push(json!({ "op": "add_node", "id": pin, "node": pinned, "state": started.id }));
                ids.push(pin);
                let label = named(deck, &format!("{id}-label"), &ids);
                let labeled = json!({
                    "type": "text", "role": "label", "text": "A place", "at": { "in": slot },
                    "align": { "x": "center", "y": "center" }, "transform": { "translate": [0, 40] },
                });
                started.patch.push(json!({ "op": "add_node", "id": label, "node": labeled, "state": started.id }));
                ids.push(label);
                json!({ "type": "shader", "kind": p.kind, "preset": preset, "seed": seed, "at": { "in": slot }, "z": -2 })
            }
            Example::Code(code) => {
                // The text the slot starts with, set to the code: a code slot's is its prompt's
                // words as Plain Text (`coded`).
                let started_in = started
                    .patch
                    .iter_mut()
                    .find(|op| op["op"] == "add_node" && op["node"]["at"]["in"] == slot.as_str());
                let node = json!({ "type": "text", "text": code.text, "code": { "language": code.language }, "at": { "in": slot } });
                match started_in {
                    Some(op) => {
                        op["node"]["text"] = json!(code.text);
                        op["node"]["code"] = json!({ "language": code.language });
                        continue;
                    }
                    None => node,
                }
            }
            Example::Table(columns) => {
                let rows = example_table(*columns);
                let schema: serde_json::Map<String, Value> = rows
                    .first()
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                    .map(|(k, _)| (k.clone(), json!("string")))
                    .collect();
                sources.insert(id.clone(), json!({ "source": { "inline": rows }, "schema": schema }));
                json!({ "type": "table", "data": format!("@{id}"), "at": { "in": slot } })
            }
        };
        started.patch.push(json!({ "op": "add_node", "id": id, "node": node, "state": started.id }));
        ids.push(id);
    }
    if !sources.is_empty() {
        // The rows first, so the nodes that read them find them.
        let at = started.patch.iter().position(|op| op["op"] == "add_node").unwrap_or(started.patch.len());
        let ops: Vec<Value> = if deck.data.is_empty() {
            vec![json!({ "op": "add", "path": "/data", "value": sources })]
        } else {
            sources
                .into_iter()
                .map(|(name, v)| json!({ "op": "add", "path": format!("/data/{name}"), "value": v }))
                .collect()
        };
        started.patch.splice(at..at, ops);
    }
    Ok(started)
}

/// A map's pin on a view of 1000 × 1000 (PLAN O.1e): a drop whose tip stands at the middle, as a
/// pin marks its place.
const MAP_PIN: &str = "M500 380C470 380 446 404 446 434C446 470 500 500 500 500C500 500 554 470 554 434C554 404 530 380 500 380ZM500 412C512 412 522 422 522 434C522 446 512 456 500 456C488 456 478 446 478 434C478 422 488 412 500 412Z";
/// The patch that draws the picture a picker shows of a slide started after `shown`'s slide
/// (PLAN 3.30, 3.31): in `layout`, the slide [`example`] makes, and a box of the theme's second
/// surface (`surface-2`, else `line`) under the words in each other slot that waits for a picture
/// or a figure; with none, the blank slide [`starting`] makes. What Add Slide draws of each layout,
/// and what a theme's contact sheet draws (PLAN T.0e); an editor never saves it.
pub fn pictured(deck: &Deck, theme: &Theme, shown: &str, layout: Option<&str>) -> Result<AddedState, OpsError> {
    let Some(name) = layout else { return starting(deck, theme, shown, None) };
    let mut started = example(deck, theme, shown, name)?;
    let fill = ["surface-2", "line"].into_iter().find(|c| theme.tokens.roles.contains_key(*c));
    let waits = |s: &Slot| s.role.is_none() && s.example.is_none() && matches!(s.prompt, Some(Prompt::Words(_)));
    let waiting = theme.layouts.get(name).into_iter().flat_map(|t| &t.slots).filter(|(_, s)| waits(s));
    for (n, (slot, _)) in waiting.enumerate() {
        let Some(fill) = fill else { break };
        let node = json!({ "type": "shape", "kind": "rect", "fill": fill, "at": { "in": slot }, "z": -1 });
        let id = format!("{}-waiting-{n}", started.id);
        started.patch.push(json!({ "op": "add_node", "id": id, "node": node, "state": started.id }));
    }
    Ok(started)
}

/// A few rows made up for a chart of `kind` (PLAN 3.31), their schema, and the chart that reads
/// them, its data not yet named: six months for a bar, a dot, a line, or an area; three kinds of
/// work a quarter for a stack; four slices of a donut; a scatter's dozen points; two years of four
/// teams for a slope or a range; a week's forecast for a weather chart.
fn example_chart(kind: scaena_core::model::nodes::ChartKind) -> (Vec<Value>, Value, Value) {
    use scaena_core::model::nodes::ChartKind;
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun"];
    let ordinal = |field: &str| json!({ "field": field, "type": "ordinal" });
    let quantity = |field: &str| json!({ "field": field, "type": "quantitative" });
    let nominal = |field: &str| json!({ "field": field, "type": "nominal" });
    match kind {
        ChartKind::Bar | ChartKind::Dot | ChartKind::Line | ChartKind::Area => {
            let values = [42.0, 58.0, 51.0, 73.0, 88.0, 95.0];
            let rows = months.iter().zip(values).map(|(m, v)| json!({ "month": m, "value": v })).collect();
            let schema = json!({ "month": "string", "value": "number" });
            (rows, schema, json!({ "type": "chart", "kind": kind, "x": ordinal("month"), "y": quantity("value") }))
        }
        ChartKind::StackedBar => {
            let mut rows = Vec::new();
            for (q, values) in ["Q1", "Q2", "Q3", "Q4"].iter().zip([
                [30.0, 20.0, 10.0],
                [36.0, 22.0, 14.0],
                [40.0, 26.0, 18.0],
                [48.0, 30.0, 20.0],
            ]) {
                for (kind, v) in ["Build", "Repair", "Survey"].iter().zip(values) {
                    rows.push(json!({ "quarter": q, "kind": kind, "hours": v }));
                }
            }
            let node = json!({ "type": "chart", "kind": kind, "x": ordinal("quarter"), "y": quantity("hours"), "series": nominal("kind") });
            (rows, json!({ "quarter": "string", "kind": "string", "hours": "number" }), node)
        }
        ChartKind::Donut => {
            let rows = [("Grants", 184.0), ("Members", 124.0), ("Partners", 60.0), ("Events", 32.0)]
                .iter()
                .map(|(s, v)| json!({ "source": s, "amount": v }))
                .collect();
            let schema = json!({ "source": "string", "amount": "number" });
            (rows, schema, json!({ "type": "chart", "kind": kind, "x": nominal("source"), "y": quantity("amount") }))
        }
        ChartKind::Scatter => {
            let points = [
                (2.0, 1.1),
                (3.0, 1.4),
                (4.0, 1.2),
                (5.0, 2.1),
                (6.0, 2.4),
                (7.0, 2.2),
                (8.0, 3.0),
                (9.0, 3.4),
                (10.0, 3.1),
                (11.0, 4.0),
                (12.0, 4.3),
                (13.0, 4.8),
            ];
            let rows = points.iter().map(|(x, y)| json!({ "grade": x, "loss": y })).collect();
            let schema = json!({ "grade": "number", "loss": "number" });
            (rows, schema, json!({ "type": "chart", "kind": kind, "x": quantity("grade"), "y": quantity("loss") }))
        }
        ChartKind::Slope | ChartKind::Range => {
            let mut rows = Vec::new();
            for (team, (a, b)) in
                ["North", "South", "East", "West"].iter().zip([(42.0, 68.0), (55.0, 61.0), (38.0, 72.0), (60.0, 49.0)])
            {
                rows.push(json!({ "year": "2025", "team": team, "score": a }));
                rows.push(json!({ "year": "2026", "team": team, "score": b }));
            }
            let node = match kind {
                ChartKind::Slope => {
                    json!({ "type": "chart", "kind": kind, "x": ordinal("year"), "y": quantity("score"), "series": nominal("team") })
                }
                _ => {
                    json!({ "type": "chart", "kind": kind, "x": ordinal("team"), "y": quantity("score"), "series": nominal("year") })
                }
            };
            (rows, json!({ "year": "string", "team": "string", "score": "number" }), node)
        }
        ChartKind::Weather => {
            // A week of June, by the day, as a forecast reads (ADR-0030).
            let week = [(2, 24.0, 15.0, 10.0), (0, 27.0, 16.0, 0.0), (61, 21.0, 14.0, 80.0), (3, 22.0, 13.0, 30.0)];
            let week = week.iter().chain(&[(1, 25.0, 14.0, 5.0), (95, 23.0, 17.0, 60.0), (0, 28.0, 18.0, 0.0)]);
            let rows = week
                .enumerate()
                .map(|(i, (sky, high, low, rain))| {
                    json!({ "date": format!("2026-06-{:02}", i + 1), "sky": sky, "high": high, "low": low, "rain": rain })
                })
                .collect();
            let schema =
                json!({ "date": "date", "sky": "number", "high": "number", "low": "number", "rain": "number" });
            (rows, schema, json!({ "type": "chart", "kind": kind, "x": { "field": "date", "type": "temporal" } }))
        }
    }
}

/// A few rows made up for a table of `columns` columns (PLAN 3.31): a plan and how it went; or,
/// of seven, a month's calendar, a week to each row, with what falls on a few of its days (PLAN
/// S.2).
fn example_table(columns: u32) -> Vec<Value> {
    if columns >= 7 {
        let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        let falls = |day: u32| match day {
            3 => " Kickoff",
            12 => " Review",
            19 => " Launch",
            27 => " Retro",
            _ => "",
        };
        return (0..5)
            .map(|week| {
                Value::Object(
                    days.iter()
                        .enumerate()
                        .map(|(i, d)| {
                            let day = week * 7 + i as u32 + 1;
                            let cell = if day <= 30 { format!("{day}{}", falls(day)) } else { String::new() };
                            (d.to_string(), json!(cell))
                        })
                        .collect(),
                )
            })
            .collect();
    }
    let rows = [
        ("Trail crews", "$48,000", "$51,200", "On track", "Q2", "Ana"),
        ("Bridges", "$32,000", "$29,400", "Done", "Q1", "Ben"),
        ("Signs", "$9,500", "$11,000", "Over", "Q3", "Cai"),
        ("Surveys", "$6,000", "$5,100", "Done", "Q2", "Dee"),
    ];
    let names = ["Item", "Planned", "Spent", "Status", "When", "Lead"];
    let n = columns.clamp(2, 6) as usize;
    rows.iter()
        .map(|r| {
            let cells = [r.0, r.1, r.2, r.3, r.4, r.5];
            Value::Object(names.iter().zip(cells).take(n).map(|(k, v)| (k.to_string(), json!(v))).collect())
        })
        .collect()
}

/// What a new slide puts in `slot`, as its prompt says (PLAN 3.30): words to type over in the
/// slot's role, a list's items each a paragraph; a card or a rule, under the words in the slots
/// over it (`z` −1), which a reader passes over; or an icon, for a person to swap for another
/// (PLAN G.23). None for a slot that waits for a picture or a figure, or says nothing of what
/// goes there.
///
/// In a deck that reads right to left (`mirrored`), a shape's points stand mirrored in its box, as
/// the slot does on the slide (PLAN T.5a).
fn slot_node(slot: &str, s: &Slot, mirrored: bool) -> Option<Value> {
    let prompt = s.prompt.as_ref()?;
    match (prompt, prompt.text(), &s.role) {
        (_, Some((text, list)), Some(role)) => {
            let mut node = json!({ "type": "text", "role": role, "text": text, "at": { "in": slot } });
            if let Some(kind) = list {
                node["list"] = json!(vec![json!({ "kind": kind }); text.split('\n').count()]);
            }
            Some(node)
        }
        (Prompt::Shape(shape), _, _) if shape.shape == PromptShapeKind::Icon => {
            let mut node = shape.icon_node()?;
            node["at"] = json!({ "in": slot });
            Some(node)
        }
        (Prompt::Shape(shape), _, _) => {
            let mut node = json!({ "type": "shape", "kind": shape.shape, "at": { "in": slot }, "z": -1 });
            if let Some(fill) = &shape.fill {
                node["fill"] = json!(fill);
            }
            if let Some(stroke) = &shape.stroke {
                node["stroke"] = json!({ "paint": stroke, "width": "thin" });
            }
            if let Some(points) = &shape.points {
                node["points"] = if mirrored { json!(scaena_core::direction::flipped(points)) } else { json!(points) };
            }
            node["semantic"] = json!("decoration");
            Some(node)
        }
        _ => None,
    }
}

/// A text a slot set in the theme's code role starts (PLAN 2.138, ADR-0029): a Plain Text code
/// block, its prompt's words its code, which a paste or the language chip then names.
fn coded(node: &mut Value, s: &Slot, theme: &Theme) {
    if node["type"] == "text" && s.role.is_some() && s.role.as_deref() == theme.code_role() {
        node["code"] = json!({ "language": "plain" });
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
    let snap = snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::unknown_state(deck, state))?;
    let layout = snap.layout.as_deref().ok_or_else(|| OpsError::new(format!("`{state}` is in no layout")))?;
    let s = theme
        .layouts
        .get(layout)
        .and_then(|l| l.slots.get(slot))
        .ok_or_else(|| OpsError::new(format!("the layout `{layout}` has no slot `{slot}`")))?;
    let mut node =
        slot_node(slot, s, scaena_core::direction::mirrored(deck)).filter(|n| n["type"] == "text").ok_or_else(
            || OpsError::new(format!("the slot `{slot}` waits for no words: a picture or a figure goes there")),
        )?;
    coded(&mut node, s, theme);
    let i = deck.state_index(state).ok_or_else(|| OpsError::unknown_state(deck, state))?;
    let slide = deck.slide_of(&deck.states[i]);
    let id = named(deck, &format!("{slide}-{}", slug(slot, "slot")), &[]);
    let made = [(id.clone(), serde_json::from_value(node.clone()).context("a slot's node")?)];
    let mut patch = vec![json!({ "op": "add_node", "id": id, "node": node, "state": state })];
    for op in crate::inspect::leaving(deck, state, &made) {
        patch.push(serde_json::to_value(op).context("a patch")?);
    }
    Ok(Filled { id, patch })
}

/// Whether `node`, as `state` shows it, still reads as the prompt of the slot it is placed in
/// (PLAN 3.30): words a new slide in the layout put there, as they were put. An editor selects
/// them whole as typing begins in them, so typing replaces them, as a placeholder's are.
pub fn prompted(deck: &Deck, theme: &Theme, state: &str, node: &str) -> Result<bool, OpsError> {
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let snap = snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::unknown_state(deck, state))?;
    let Some(props) = snap.nodes.get(node) else { return Ok(false) };
    let slot = props.get("at").filter(|a| a.get("parent").is_none()).and_then(|a| a.get("in")?.as_str());
    let prompt = (snap.layout.as_deref().and_then(|l| theme.layouts.get(l)))
        .zip(slot)
        .and_then(|(layout, slot)| layout.slots.get(slot)?.prompt.as_ref()?.text());
    Ok(matches!((prompt, props.get("text").and_then(Value::as_str)), (Some((words, _)), Some(text)) if words == text))
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
    /// Whether a picture goes there: no words, and no chart or table its example (PLAN 2.107),
    /// which an editor's Make a Picture paints.
    pub picture: bool,
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
    let snap = snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::unknown_state(deck, state))?;
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
        let typed = slot.role.is_some();
        let picture = !typed && !matches!(slot.example, Some(Example::Chart(_) | Example::Table(_)));
        out.push(Waiting { slot: b.name, rect: b.rect, words, typed, picture, role: slot.role.clone() });
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

/// A slide folded into the one before it (editor dogfood 6): its words joined to that slide's
/// place for words, its notes after that slide's, and its states taken out as `remove_slide`
/// takes them, one patch; or why it cannot be.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Folded {
    /// The first state of the slide it folds into: what the editor shows after.
    pub into: String,
    /// The patch, by `user`: one step to undo. Empty where it is refused.
    pub patch: Vec<Value>,
    /// What it did, in a person's words: "Folded slide 7 into slide 6".
    pub said: String,
    /// Why it cannot be folded, in a person's words, where it cannot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
    /// Where its words went: the slide before's place for words, or, where it has none, its notes.
    pub to: FoldsTo,
}

/// Where a folded slide's words go (editor dogfood 6): the slide before's place for words, or,
/// where it has none (a big number's, a picture's), that slide's notes, ahead of the folded
/// slide's notes, so nothing is lost and the room sees what the slide showed. What an editor
/// says before the fold ("Fold into slide 6", "Fold into slide 6's notes") and after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FoldsTo {
    Place,
    Notes,
}

/// The text of `snap` a fold joins words to: its first text in the body's role, else the lede's,
/// else a quote's, none its headline or code.
fn joined_place<'a>(
    deck: &Deck,
    snap: &'a scaena_core::Snapshot,
) -> Option<(&'a String, &'a scaena_core::document::Props)> {
    let head = scaena_core::reading::headline_node(deck, snap);
    let is_text = |id: &str| deck.nodes.get(id).is_some_and(|n| n.node_type == scaena_core::document::NodeType::Text);
    JOINED.iter().find_map(|role| {
        (snap.nodes.iter()).find(|(id, p)| {
            is_text(id)
                && Some(id.as_str()) != head.as_deref()
                && p.get("code").is_none()
                && p.get("role").and_then(Value::as_str) == Some(role)
        })
    })
}

/// Where the words of each slide but the first would go, folded into the slide before it, by the
/// slide's states' ids, laying nothing out: what an editor names before the fold. `snaps` are the
/// deck's states resolved.
pub fn folds_to(deck: &Deck, snaps: &[scaena_core::Snapshot]) -> Vec<(String, FoldsTo)> {
    let slides = deck.slides();
    let mut out = Vec::new();
    for pair in slides.windows(2) {
        let Some(was) = pair[0].last().and_then(|&i| snaps.get(i)) else { continue };
        let to = if joined_place(deck, was).is_some() { FoldsTo::Place } else { FoldsTo::Notes };
        out.extend(pair[1].iter().map(|&i| (deck.states[i].id.clone(), to)));
    }
    out
}

/// The roles whose words a fold carries: none of a slide's labels, a chart's axes, or a caption's
/// credit, which say what the slide is, not what it says.
fn carried(props: &scaena_core::document::Props) -> bool {
    let role = props.get("role").and_then(Value::as_str);
    props.get("code").is_none() && !matches!(role, Some("label" | "kicker" | "caption" | "axis" | "value"))
}

/// The roles a fold may join words to: a slide's place for words, the body first, then its lede.
const JOINED: [&str; 3] = ["body", "lede", "quote"];

/// The words a slide folded at `last` carries, one line each, in the order it reads (SPEC §3.12):
/// its headline first, then each other text it carries, by paragraph. A big number's figure goes
/// with its meaning, as health reads one ("12,000: riders a day"): a slide's one figure with its
/// headline, as the slide is read by it, and each of several with the text read after it; a
/// figure the slide before (`was`) shows already is not carried again.
fn folded_words(
    b: &Bundle,
    deck: &Deck,
    last: &scaena_core::Snapshot,
    was: &scaena_core::Snapshot,
) -> Result<Vec<String>, OpsError> {
    let theme = crate::theme(b)?;
    let data = crate::lint::data_files(b)?;
    let mut engine = crate::lint::engine_with(b, &theme, None)?;
    let mut read = Vec::new();
    engine.frames_at_rest(deck, &theme, &data, None, std::slice::from_ref(&last.state_id), |_, list| {
        read = scaena_core::reading::parts(deck, &data, last, &list);
        Ok::<(), OpsError>(())
    })?;
    let is_text = |id: &str| deck.nodes.get(id).is_some_and(|n| n.node_type == scaena_core::document::NodeType::Text);
    let figure = |p: &scaena_core::document::Props| {
        matches!(p.get("role").and_then(Value::as_str), Some("numeral" | "hero" | "figure"))
    };
    let said = |p: &scaena_core::document::Props| {
        scaena_core::reading::words(p).split_whitespace().collect::<Vec<_>>().join(" ")
    };
    // A figure the slide before shows, by its words.
    let shown_before = |words: &str| was.nodes.iter().any(|(id, p)| is_text(id) && figure(p) && said(p) == words);
    let heading = scaena_core::reading::headline_node(deck, last);
    // The texts it carries, in the order it reads, each once.
    let mut texts: Vec<(&String, &scaena_core::document::Props)> = Vec::new();
    for part in &read {
        if let Some((id, p)) = last.nodes.get_key_value(&part.node)
            && is_text(id)
            && Some(id.as_str()) != heading.as_deref()
            && carried(p)
            && !texts.iter().any(|(t, _)| *t == id)
        {
            texts.push((id, p));
        }
    }
    let figures: Vec<String> = texts.iter().filter(|(_, p)| figure(p)).map(|(_, p)| said(p)).collect();
    let mut lines = Vec::new();
    let head = heading.as_deref().and_then(|h| last.nodes.get(h)).map(said).filter(|h| !h.is_empty());
    // One figure goes with the headline, as `reading::headline` reads the slide.
    let one = figures.len() == 1 && !shown_before(&figures[0]);
    match (one, head) {
        (true, _) => lines.push(scaena_core::reading::headline(deck, last).unwrap_or_else(|| figures[0].clone())),
        (false, Some(head)) => lines.push(head),
        (false, None) => {}
    }
    let mut pending: Option<String> = None;
    for (_, p) in texts {
        let words = said(p);
        if figure(p) {
            if let Some(alone) = pending.take() {
                lines.push(alone);
            }
            if figures.len() > 1 && !shown_before(&words) {
                pending = Some(words);
            }
            continue;
        }
        let mut paragraphs = scaena_core::reading::words(p)
            .split('\n')
            .map(|w| w.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .into_iter();
        if let Some(fig) = pending.take() {
            match paragraphs.next() {
                Some(meaning) => lines.push(format!("{fig}: {meaning}")),
                None => lines.push(fig),
            }
        }
        lines.extend(paragraphs);
    }
    lines.extend(pending);
    Ok(lines)
}

/// The steps a fold adds where slide `prev` builds `target` in by bullet (PLAN 2.129): its last
/// step held to the paragraphs it drew of `written` before the fold, then a step for each of the
/// `added` paragraphs folded in after them, the last drawing the text whole, each the slide's,
/// named after it as a build's steps are, in the beat of the step it follows. None where no step
/// of the slide before its last draws part of the text, or its last does not draw it whole.
fn built_in(
    deck: &Deck,
    snaps: &[scaena_core::Snapshot],
    prev: &[usize],
    target: &str,
    shown: &scaena_core::document::Props,
    added: usize,
) -> Result<Vec<Value>, OpsError> {
    let part = |k: usize| snaps.get(k).and_then(|s| s.nodes.get(target)).is_some_and(|p| p.contains_key("paragraphs"));
    let Some(&last) = prev.last() else { return Ok(Vec::new()) };
    // The text as the slide's last step draws it: whole, its `paragraphs` none, or through its last.
    let had = scaena_core::builds::count(shown);
    let whole = (snaps.get(last).and_then(|s| s.nodes.get(target)))
        .and_then(|p| p.get("paragraphs"))
        .is_none_or(|p| p.is_null() || p.get(1).and_then(Value::as_u64).is_some_and(|end| end as usize >= had));
    let earlier = &prev[..prev.len() - 1];
    if added == 0 || !earlier.iter().any(|&k| part(k)) || !whole {
        return Ok(Vec::new());
    }
    let esc = |s: &str| s.replace('~', "~0").replace('/', "~1");
    // The paragraphs the text had, as the format counts them from 1; those folded in follow them.
    // The last step's own props of the text, held to those it drew.
    let doc = deck.to_value()?;
    let mut own = doc["states"][last]["props"][target].as_object().cloned().unwrap_or_default();
    own.insert("paragraphs".into(), json!([1, had.max(1)]));
    let mut ops = vec![match doc["states"][last]["props"].is_object() {
        true => json!({ "op": "add", "path": format!("/states/{last}/props/{}", esc(target)), "value": own }),
        false => json!({ "op": "add", "path": format!("/states/{last}/props"), "value": { target: own } }),
    }];
    let slide = deck.slide_of(&deck.states[last]).to_string();
    let base = match slide.rsplit_once('-') {
        Some((stem, n)) if !stem.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => stem,
        _ => slide.as_str(),
    };
    let mut taken: Vec<String> = deck.states.iter().map(|s| s.id.clone()).collect();
    let beat = (deck.spine.iter().flat_map(|s| s.sections.iter()).enumerate())
        .flat_map(|(s, section)| section.beats.iter().enumerate().map(move |(b, beat)| (s, b, beat)))
        .find(|(_, _, b)| b.states.iter().any(|id| *id == deck.states[last].id));
    for k in 1..=added {
        let id = (2..).map(|n| format!("{base}-{n}")).find(|id| !taken.contains(id)).expect("some number is free");
        taken.push(id.clone());
        let drawn = if k == added { Value::Null } else { json!([1, had + k]) };
        let mut props = serde_json::Map::new();
        props.insert(target.to_string(), json!({ "paragraphs": drawn }));
        ops.push(json!({
            "op": "add",
            "path": format!("/states/{}", last + k),
            "value": { "id": id, "slide": slide, "props": props },
        }));
        if let Some((s, b, _)) = beat {
            ops.push(json!({ "op": "add", "path": format!("/spine/sections/{s}/beats/{b}/states/-"), "value": id }));
        }
    }
    Ok(ops)
}

/// The patch that folds the slide of `state` into the slide before it (editor dogfood 6), as a
/// Story row and the slides' menu offer it where a slide says again what the one before said, or
/// adds least to it: the folded slide's headline, then each other text it shows at its last step,
/// each paragraph appended to the previous slide's place for words at its last step (a list's
/// items appended as items); its notes, and each later step's, after the previous slide's, a blank
/// line between; and its
/// states taken out by `remove_slide`, which takes the beat it alone told. That beat's claim is
/// not lost: it joins the previous beat's notes, never its evidence, which health and W423 read
/// as sources. Where the slide before has no place for words (a big number's), its words lead its
/// notes there instead ([`FoldsTo::Notes`]), and so do words that would run past their place
/// (E100), as laying the deck out with the patch finds (W1's dogfood 16). Refused, saying why,
/// where there is no slide before it.
///
/// With `into`, a state of an earlier slide, the slide folds into that slide in place of the one
/// before it, as a Story row offers where a slide says again what an earlier one said (R1's
/// #1161): its words join that slide's place, its notes follow that slide's, and its beat's
/// claim joins that slide's beat's notes; the slides between are left as they were. Refused,
/// saying why, where `into` is the slide itself or one after it.
pub fn folding(b: &Bundle, state: &str, into: Option<&str>) -> Result<Folded, OpsError> {
    folded_to(b, state, into, true)
}

/// [`folding`], the folded slide's words joining the place for words of the slide it folds into
/// where `placed` and it has one, else that slide's notes: where they would run past the place,
/// the fold puts them in the notes instead of refusing (W1's dogfood 16, the coordinator's call),
/// which the dry run and the editors' button say beforehand.
fn folded_to(b: &Bundle, state: &str, into: Option<&str>, placed: bool) -> Result<Folded, OpsError> {
    let deck = &b.deck;
    let i = deck.state_index(state).ok_or_else(|| OpsError::unknown_state(deck, state))?;
    let slides = deck.slides();
    let k = slides.iter().position(|s| s.contains(&i)).ok_or_else(|| OpsError::unknown_state(deck, state))?;
    let (folded, n) = (&slides[k], k + 1);
    // The slide it folds into, by its place: the one before it unless `into` names another.
    let t = match into {
        None => k.checked_sub(1),
        Some(into) => {
            let j = deck.state_index(into).ok_or_else(|| OpsError::unknown_state(deck, into))?;
            let t = slides.iter().position(|s| s.contains(&j)).ok_or_else(|| OpsError::unknown_state(deck, into))?;
            if t >= k {
                let id = deck.states[slides[t][0]].id.clone();
                let why = match t == k {
                    true => format!("Slide {n} cannot be folded into itself."),
                    false => format!("Slide {n} folds only into a slide before it, and slide {} is after it.", t + 1),
                };
                return Ok(Folded {
                    into: id,
                    patch: Vec::new(),
                    said: String::new(),
                    refused: Some(why),
                    to: FoldsTo::Place,
                });
            }
            Some(t)
        }
    };
    let refused = |into: String, why: String| Folded {
        into,
        patch: Vec::new(),
        said: String::new(),
        refused: Some(why),
        to: FoldsTo::Place,
    };
    let Some(t) = t else {
        return Ok(refused(
            deck.states[folded[0]].id.clone(),
            format!("Slide {n} has no slide before it to fold into."),
        ));
    };
    let prev = &slides[t];
    // The slide it folds into, by its number, as messages name it.
    let k = t + 1;
    let into = deck.states[prev[0]].id.clone();
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let (was, last) = (&snaps[*prev.last().unwrap_or(&prev[0])], &snaps[*folded.last().unwrap_or(&folded[0])]);
    // The previous slide's place for words, where it has one; else its notes take them.
    let target = if placed { joined_place(deck, was) } else { None };
    // The folded slide's words, in the order it reads (SPEC §3.12), its headline first.
    let lines = folded_words(b, deck, last, was)?;
    let joined = lines.join("\n");
    let mut patch = Vec::new();
    if let Some((target, props)) = target
        && !joined.is_empty()
    {
        let written = scaena_core::reading::words(props);
        let at = written.chars().count();
        let text = if written.trim().is_empty() { joined.clone() } else { format!("\n{joined}") };
        let shown = &deck.states[*prev.last().unwrap_or(&prev[0])].id;
        patch.push(json!({ "op": "replace_text", "node": target, "from": at, "to": at, "text": text, "state": shown }));
        // Where the slide builds its words in by bullet, each paragraph folded in comes on a click
        // of its own after the build's last, as the build brings in its own, never all at once on
        // its last step (R1's agent dogfood 9).
        patch.extend(built_in(deck, &snaps, prev, target, props, lines.len())?);
    }
    // Notes, each after what is there with a blank line: the folded slide's after the previous
    // slide's, where they live (its first state's own, else its beat's), and the claim of a beat
    // the fold leaves with no slide after the previous beat's.
    let ids = |slide: &[usize]| slide.iter().map(|&s| deck.states[s].id.clone()).collect::<Vec<_>>();
    let (gone, kept) = (ids(folded), ids(prev));
    let after = |was: &str, more: &str| match (was.trim().is_empty(), more.trim().is_empty()) {
        (_, true) => was.to_string(),
        (true, false) => more.trim().to_string(),
        (false, false) => format!("{}\n\n{}", was.trim_end(), more.trim()),
    };
    let beats: Vec<(usize, usize, &scaena_core::document::Beat)> = (deck.spine.iter().flat_map(|s| s.sections.iter()))
        .enumerate()
        .flat_map(|(s, section)| section.beats.iter().enumerate().map(move |(b, beat)| (s, b, beat)))
        .collect();
    let beat_of = |state: &str| beats.iter().find(|(_, _, b)| b.states.iter().any(|s| s == state));
    let mut beat_notes: Vec<((usize, usize), String)> = Vec::new();
    let to_beat = |at: (usize, usize), was: &str, more: &str, notes: &mut Vec<((usize, usize), String)>| match notes
        .iter_mut()
        .find(|(b, _)| *b == at)
    {
        Some((_, text)) => *text = after(text, more),
        None => notes.push((at, after(was, more))),
    };
    // The folded slide's notes as its presenter reads them, but a beat's it shares with the slide
    // before, which that slide reads already.
    let first_gone = &deck.states[folded[0]];
    let shared = first_gone.notes.is_none()
        && beat_of(&first_gone.id).is_some_and(|(s, b, _)| beat_of(&kept[0]).is_some_and(|(t, c, _)| (s, b) == (t, c)));
    let mut folded_notes =
        if shared { String::new() } else { scaena_core::spine::notes(deck, &gone[0]).unwrap_or("").to_string() };
    // Each later step's own notes after them, as a presenter reads them in turn.
    for &step in &folded[1..] {
        if let Some(own) = deck.states[step].notes.as_deref() {
            folded_notes = after(&folded_notes, own);
        }
    }
    // With no place for them, its words lead its notes there.
    if target.is_none() {
        folded_notes = after(&joined, &folded_notes);
    }
    // The notes of the slide it folds into's first state, where they are written there.
    let first = &deck.states[prev[0]];
    let mut first_notes: Option<String> = None;
    if !folded_notes.trim().is_empty() {
        match (&first.notes, beat_of(&first.id)) {
            (None, Some((s, b, beat))) => {
                to_beat((*s, *b), beat.notes.as_deref().unwrap_or(""), &folded_notes, &mut beat_notes)
            }
            (own, _) => first_notes = Some(after(own.as_deref().unwrap_or(""), &folded_notes)),
        }
    }
    // The claim of each beat the fold leaves with no slide: in the notes of the beat that tells
    // the slide it folds into, else, where no beat tells it, that slide's own, so no claim is lost.
    let told = beat_of(kept.last().unwrap_or(&kept[0]));
    for (_, _, beat) in &beats {
        let alone = !beat.states.is_empty() && beat.states.iter().all(|s| gone.contains(s));
        if !alone || beat.claim.trim().is_empty() {
            continue;
        }
        match told {
            Some((s, b, kept_in)) => {
                to_beat((*s, *b), kept_in.notes.as_deref().unwrap_or(""), &beat.claim, &mut beat_notes)
            }
            None => {
                let was = first_notes.take().or_else(|| first.notes.clone()).unwrap_or_default();
                first_notes = Some(after(&was, &beat.claim));
            }
        }
    }
    if let Some(notes) = first_notes {
        patch.push(json!({ "op": "add", "path": format!("/states/{}/notes", prev[0]), "value": notes }));
    }
    for ((s, b), notes) in beat_notes {
        patch.push(json!({ "op": "add", "path": format!("/spine/sections/{s}/beats/{b}/notes"), "value": notes }));
    }
    patch.push(json!({ "op": "remove_slide", "slide": gone[0] }));
    // Checked as a patch is, and laid out: words that run past their place are no fold.
    let (patched, _) = crate::patch::patching(b, &Value::Array(patch.clone()), Some("Fold slide"))?;
    if patched.refused {
        let why = patched.added.iter().find(|f| f.severity == scaena_core::lint::Severity::Error);
        let why = why.map_or_else(|| "the deck would not read".to_string(), |f| f.message.clone());
        return Ok(refused(into, format!("Slide {n} cannot be folded into slide {k}: {why}.")));
    }
    if let Some((target, _)) = target
        && patched.added.iter().any(|f| f.code == "E100" && f.node.as_deref() == Some(target.as_str()))
    {
        return folded_to(b, state, Some(&into), false);
    }
    let (said, to) = match target {
        Some(_) => (format!("Folded slide {n} into slide {k}"), FoldsTo::Place),
        None => (format!("Folded slide {n} into slide {k}'s notes"), FoldsTo::Notes),
    };
    Ok(Folded { into, patch, said, refused: None, to })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b1() -> Bundle {
        crate::open(std::path::Path::new("../../tests/bench/b1.scaena")).unwrap()
    }

    /// `patch` applied to `b`'s deck, as the editor applies it.
    fn applied(b: &Bundle, patch: &[Value]) -> Deck {
        let files = crate::lint::View::of(b);
        let doc = scaena_core::patch::compile(&b.deck.to_value().unwrap(), patch, &files).unwrap().doc;
        Deck::from_value(&doc).unwrap()
    }

    /// B1 with slides 3 and 4 written short, so the one's words fit the other's place.
    fn short() -> Bundle {
        let mut b = b1();
        for (state, node, text) in [
            ("belief-1-why", "body", "Frames were the mistake."),
            ("belief-2-why", "body", "Name it once."),
            ("belief-2-line", "aside", "One scene."),
        ] {
            let i = b.deck.state_index(state).unwrap();
            b.deck.states[i].props.insert(node.into(), serde_json::from_value(json!({ "text": text })).unwrap());
        }
        b
    }

    /// The words `node` shows at `state` of `deck`.
    fn shown(deck: &Deck, state: &str, node: &str) -> String {
        let snaps = scaena_core::resolve_states(deck).unwrap();
        let snap = snaps.iter().find(|s| s.state_id == state).unwrap();
        scaena_core::reading::words(&snap.nodes[node])
    }

    /// A `fold_slide` in a patch folds as the editors' button does (agent dogfood 7): over MCP
    /// and the CLI it makes `folding`'s patch, says what it folds where before and after, stands
    /// alone in its patch, and is refused, saying why, where the fold is.
    #[test]
    fn a_fold_slide_op_folds_as_the_editors_fold_and_says_where() {
        let b = short();
        let ops = json!([{ "op": "fold_slide", "slide": "belief-2-why" }]);
        let dry = crate::patch::reported(&b, &ops, true, false).unwrap();
        let fold = dry.folded.as_ref().expect("what it folds where");
        assert_eq!((fold.into.as_str(), fold.to), ("belief-1", FoldsTo::Place), "{fold:?}");
        assert!(fold.says.starts_with("Folds slide 4 into slide 3: its words join"), "{}", fold.says);
        assert!(!dry.applied && dry.summary.slides_removed == ["belief-2"], "{dry:?}");
        let (patched, write) = crate::patch::patching(&b, &ops, None).unwrap();
        let folded = folding(&b, "belief-2-why", None).unwrap();
        assert_eq!(applied(&b, &folded.patch).to_json().unwrap(), write.as_ref().unwrap().deck.to_json().unwrap());
        assert!(patched.applied);
        assert_eq!(write.unwrap().why.message, "patch: fold_slide");
        let with = json!([{ "op": "fold_slide", "slide": "belief-2" }, { "op": "remove_slide", "slide": "belief-1" }]);
        let alone = crate::patch::reported(&b, &with, true, false).unwrap_err();
        assert!(alone.message.contains("stands alone in its patch") && alone.op == Some(0), "{alone:?}");
        let first = json!([{ "op": "fold_slide", "slide": b.deck.states[0].id }]);
        let none = crate::patch::reported(&b, &first, true, false).unwrap_err();
        assert!(none.message.contains("has no slide before it"), "{none:?}");
    }

    /// A slide folded into the one before it (editor dogfood 6): its headline, then its body,
    /// after the previous slide's words at its last step; its states gone with the beat it alone
    /// told, whose claim joins the previous beat's notes; one patch, said in a person's words.
    #[test]
    fn a_slide_folds_into_the_one_before_it_its_claim_kept_in_the_notes() {
        let b = short();
        let before = shown(&b.deck, "belief-1-line", "body");
        let headline = shown(&b.deck, "belief-2-line", "headline");
        let body = shown(&b.deck, "belief-2-line", "body");
        let aside = shown(&b.deck, "belief-2-line", "aside");
        // Any of its states names the slide.
        let folded = folding(&b, "belief-2-why", None).unwrap();
        assert_eq!(folded.refused, None, "{folded:?}");
        assert_eq!((folded.into.as_str(), folded.said.as_str()), ("belief-1", "Folded slide 4 into slide 3"));
        assert_eq!(folded.patch.last(), Some(&json!({ "op": "remove_slide", "slide": "belief-2" })));
        let deck = applied(&b, &folded.patch);
        assert_eq!(shown(&deck, "belief-1-line", "body"), format!("{before}\n{headline}\n{body}\n{aside}"));
        assert_eq!(deck.slides().len(), b.deck.slides().len() - 1);
        assert!(deck.state_index("belief-2").is_none() && deck.state_index("belief-2-line").is_none());
        let beats: Vec<_> = deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats).collect();
        assert!(beats.iter().all(|beat| beat.id != "belief-2"), "the beat it alone told goes");
        let kept = beats.iter().find(|beat| beat.id == "belief-1").unwrap();
        assert_eq!(kept.notes.as_deref(), Some("Identity is the primitive."), "its claim is not lost");
        assert!(kept.evidence.iter().all(|e| !e.contains("Identity")), "and is no source");
        // The slide after it shows what it showed.
        assert_eq!(shown(&deck, "belief-3-line", "body"), shown(&b.deck, "belief-3-line", "body"));
    }

    /// A slide folded into one built by bullet brings each of its paragraphs in on a click of its
    /// own after the build's last (R1's agent dogfood 9: a Spanish deck's steps went 1, 2, then 7),
    /// the build's last step drawing what it drew before, the steps the slide's and its beat's.
    #[test]
    fn a_fold_into_a_slide_built_by_bullet_adds_a_step_for_each_paragraph() {
        let mut b = short();
        let i = b.deck.state_index("belief-1-why").unwrap();
        // Built as By Bullet leaves it: the body's first paragraph, then the whole of it.
        let two = json!({ "text": "Frames were the mistake.\nName it once.", "paragraphs": [1, 1] });
        b.deck.states[i].props.insert("body".into(), serde_json::from_value(two).unwrap());
        let j = b.deck.state_index("belief-1-line").unwrap();
        b.deck.states[j].props.insert("body".into(), serde_json::from_value(json!({ "paragraphs": null })).unwrap());
        let steps = |deck: &Deck| -> Vec<(String, Option<Value>)> {
            let slide = deck.slides().into_iter().find(|s| deck.states[s[0]].id == "belief-1").unwrap();
            (slide.into_iter().map(|k| &deck.states[k]))
                .map(|s| (s.id.clone(), s.props.get("body").and_then(|p| p.get("paragraphs")).cloned()))
                .collect()
        };
        let built = steps(&b.deck);
        let drawn: Vec<_> = built.iter().filter_map(|(_, p)| p.clone()).collect();
        assert!(drawn.len() >= 2, "the build's steps say what they draw: {built:?}");
        let folded = folding(&b, "belief-2-why", None).unwrap();
        assert_eq!(folded.refused, None, "{folded:?}");
        let deck = applied(&b, &folded.patch);
        let after = steps(&deck);
        let words = shown(&deck, after.last().map(|(id, _)| id.as_str()).unwrap(), "body");
        let added = words.split('\n').count() - 2;
        assert_eq!(after.len(), built.len() + added, "a step for each paragraph folded in: {after:?}");
        // Each step after the build draws one paragraph more than the one before it, the last all.
        let ends: Vec<Option<u64>> =
            after.iter().map(|(_, p)| p.as_ref().and_then(|p| p.get(1)).and_then(Value::as_u64)).collect();
        let tail = &ends[built.len() - 1..];
        for pair in tail.windows(2) {
            if let [Some(a), Some(b)] = pair {
                assert_eq!(*b, a + 1, "{after:?}");
            }
        }
        assert_eq!(ends.last(), Some(&None), "the last draws the text whole: {after:?}");
        let beat = (deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats)).find(|b| b.id == "belief-1");
        let told = beat.unwrap().states.clone();
        assert!(after.iter().all(|(id, _)| told.contains(id)), "the beat tells every step: {told:?}");
    }

    /// A slide folded into one no beat tells keeps its beat's claim (W1's review of #1175): the
    /// beat goes with the slide, and its claim joins that slide's own notes, after the notes the
    /// fold puts there, as it joins the notes of the beat that tells the slide where one does.
    #[test]
    fn a_fold_into_a_slide_no_beat_tells_keeps_the_claim_in_its_notes() {
        let mut b = short();
        for beat in b.deck.spine.iter_mut().flat_map(|s| s.sections.iter_mut()).flat_map(|s| s.beats.iter_mut()) {
            beat.states.retain(|s| !s.starts_with("belief-1"));
            if beat.id == "belief-2" {
                beat.claim = "Naming beats framing every time.".into();
            }
        }
        let two = b.deck.state_index("belief-2").unwrap();
        b.deck.states[two].notes = Some("Name every node once.".into());
        let folded = folding(&b, "belief-2", None).unwrap();
        assert_eq!(folded.refused, None, "{folded:?}");
        let deck = applied(&b, &folded.patch);
        let one = deck.state_index("belief-1").unwrap();
        assert_eq!(
            deck.states[one].notes.as_deref(),
            Some("Name every node once.\n\nNaming beats framing every time."),
            "{:?}",
            folded.patch
        );
        assert!(deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats).all(|beat| beat.id != "belief-2"));
    }

    /// Its notes follow the previous slide's, a blank line between, where they are read: the
    /// previous slide's own, else its beat's.
    #[test]
    fn a_folded_slides_notes_follow_the_notes_of_the_slide_before() {
        let mut b = short();
        let at = |deck: &Deck, id: &str| deck.state_index(id).unwrap();
        let (one, two) = (at(&b.deck, "belief-1"), at(&b.deck, "belief-2"));
        b.deck.states[two].notes = Some("Name every node once.".into());
        let line = at(&b.deck, "belief-2-line");
        b.deck.states[line].notes = Some("Then the line.".into());
        b.deck.states[one].notes = Some("Frames were never the point.".into());
        let deck = applied(&b, &folding(&b, "belief-2", None).unwrap().patch);
        assert_eq!(
            deck.states[at(&deck, "belief-1")].notes.as_deref(),
            Some("Frames were never the point.\n\nName every node once.\n\nThen the line.")
        );
        // With no notes of its own, the previous slide reads its beat's: they follow those there.
        b.deck.states[one].notes = None;
        let deck = applied(&b, &folding(&b, "belief-2", None).unwrap().patch);
        assert_eq!(deck.states[at(&deck, "belief-1")].notes, None);
        let beat = (deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats)).find(|b| b.id == "belief-1");
        assert_eq!(
            beat.unwrap().notes.as_deref(),
            Some("Name every node once.\n\nThen the line.\n\nIdentity is the primitive.")
        );
    }

    /// No fold where there is no slide before: said in a person's words, with no patch.
    #[test]
    fn a_fold_with_no_slide_before_it_is_refused_saying_why() {
        let b = b1();
        let first = folding(&b, "cover", None).unwrap();
        assert_eq!(first.refused.as_deref(), Some("Slide 1 has no slide before it to fold into."));
        assert!(first.patch.is_empty());
        assert!(folding(&b, "nowhere", None).is_err());
    }

    /// Where the slide before has no place for words, a title's or a big number's, the folded
    /// slide's words lead its notes there, before its own notes and its beat's claim: nothing lost,
    /// and the room sees the slide as it was. Said so before the fold and after it.
    #[test]
    fn a_fold_with_no_place_for_its_words_puts_them_in_the_notes() {
        let mut b = b1();
        let goal = b.deck.state_index("goal").unwrap();
        b.deck.states[goal].notes = Some("Say why now.".into());
        let snaps = scaena_core::resolve_states(&b.deck).unwrap();
        let to = folds_to(&b.deck, &snaps);
        assert_eq!(
            to.iter().find(|(s, _)| s == "goal-bar").map(|(_, t)| *t),
            Some(FoldsTo::Notes),
            "the cover takes no words"
        );
        assert_eq!(to.iter().find(|(s, _)| s == "belief-2").map(|(_, t)| *t), Some(FoldsTo::Place));
        assert!(to.iter().all(|(s, _)| s != "cover"), "the first slide folds into none");
        let headline = shown(&b.deck, "goal-bar", "headline");
        let body = shown(&b.deck, "goal-bar", "body");
        let folded = folding(&b, "goal", None).unwrap();
        assert_eq!(
            (folded.said.as_str(), folded.to, folded.refused.as_deref()),
            ("Folded slide 2 into slide 1's notes", FoldsTo::Notes, None)
        );
        let deck = applied(&b, &folded.patch);
        assert_eq!(shown(&deck, "cover", "title"), shown(&b.deck, "cover", "title"), "the cover shows what it showed");
        let beat =
            (deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats)).find(|b| b.id == "opening").unwrap();
        let notes = beat.notes.as_deref().unwrap_or("");
        assert!(notes.starts_with(&format!("{headline}\n{body}\n")), "{notes}");
        assert!(notes.contains("\n\nSay why now.\n\nBuild the tool we would design today"), "{notes}");
    }

    fn trails() -> Bundle {
        crate::open(std::path::Path::new("../../docs/examples/trails.deck.json")).unwrap()
    }

    /// A slide's words fold in the order it reads (SPEC §3.12), not the order its nodes are
    /// written: trails' budget, its note put in front of its reason, reads its reason first.
    #[test]
    fn a_slides_words_fold_in_the_order_it_reads() {
        let mut b = trails();
        let budget = b.deck.state_index("budget").unwrap();
        b.deck.states[budget].props.insert("budget-aside".into(), serde_json::from_value(json!({ "z": 10 })).unwrap());
        let before = shown(&b.deck, "funding", "funding-body");
        let folded = folding(&b, "budget", None).unwrap();
        assert_eq!(folded.refused, None, "{folded:?}");
        let deck = applied(&b, &folded.patch);
        assert_eq!(
            shown(&deck, "funding", "funding-body"),
            format!(
                "{before}\nWe came in $6,600 under budget\nCrews drove farther to reach the Alpine segments.\nFood and travel ran $7,200 over."
            ),
            "the reason, written after the note, is read before it"
        );
    }

    /// A big number's figure folds with its meaning, as health reads the slide ("6,820 volunteer
    /// hours this season"), and is not said again where the slide before shows it already.
    #[test]
    fn a_big_numbers_figure_folds_with_its_meaning_unless_shown_before() {
        let b = trails();
        let notes = |deck: &Deck| scaena_core::spine::notes(deck, "promise").unwrap_or("").to_string();
        let folded = folding(&b, "hours", None).unwrap();
        assert_eq!((folded.to, folded.said.as_str()), (FoldsTo::Notes, "Folded slide 4 into slide 3's notes"));
        let carried = notes(&applied(&b, &folded.patch));
        assert!(
            carried.contains("6,820 volunteer hours this season\nMore than three full-time years of work"),
            "{carried}"
        );
        // The slide before shows the figure: its meaning goes, the figure not said twice.
        let mut b = trails();
        let promise = b.deck.state_index("promise").unwrap();
        b.deck.states[promise].props.insert(
            "promise-sub".into(),
            serde_json::from_value(json!({ "role": "numeral", "text": "6,820" })).unwrap(),
        );
        let carried = notes(&applied(&b, &folding(&b, "hours", None).unwrap().patch));
        assert!(carried.contains("volunteer hours this season\nMore than three"), "{carried}");
        assert!(!carried.lines().any(|l| l.starts_with("6,820")), "{carried}");
    }

    /// Words that would run past the place they join (E100), laid out, go into that slide's notes
    /// instead, which the fold says (W1's dogfood 16): B1's slides each fill their place, so
    /// slide 4's whole paragraph and quote have no room on slide 3, and lead its notes.
    #[test]
    fn a_fold_whose_words_would_run_past_their_place_goes_into_its_notes() {
        let b = b1();
        let folded = folding(&b, "belief-2", None).unwrap();
        assert_eq!(folded.refused, None);
        assert_eq!((folded.to, folded.said.as_str()), (FoldsTo::Notes, "Folded slide 4 into slide 3's notes"));
        let snaps = scaena_core::resolve_states(&b.deck).unwrap();
        let (was, gone) = (b.deck.state_index("belief-1").unwrap(), b.deck.state_index("belief-2").unwrap());
        let words = folded_words(&b, &b.deck, &snaps[gone], &snaps[was]).unwrap();
        let deck = applied(&b, &folded.patch);
        assert!(deck.state_index("belief-2").is_none(), "slide 4 is gone");
        let notes = scaena_core::spine::notes(&deck, "belief-1").unwrap_or_default().to_string();
        assert!(words.iter().all(|w| notes.contains(w.as_str())), "{notes}\n{words:?}");
    }

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
                // The deck runs on its own, so the slide holds as `mix`, the state it follows, does.
                json!({ "op": "add_state", "state": { "id": "bullets", "mode": "absolute", "layout": "bullets", "hold": 6000.0 }, "after": "mix", "beat": "doubled" }),
                json!({ "op": "add_node", "id": "bullets-header", "state": "bullets", "node": {
                    "type": "text", "role": "headline", "text": "What this slide says", "at": { "in": "header" },
                    "semantic": "claim", "fit": "shrink" } }),
                json!({ "op": "add_node", "id": "bullets-body", "state": "bullets", "node": {
                    "type": "text", "role": "lede", "text": "The first point\nThe second point\nThe third point",
                    "at": { "in": "body" }, "list": [bullet, bullet, bullet], "fit": "grow", "maxSize": 40.0 } }),
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
                json!({ "op": "add_state", "state": { "id": "slide", "mode": "absolute", "hold": 3000.0 }, "after": "close", "beat": "thanks" })
            ]
        );
        assert!(starting(&deck, &theme, "revenue", Some("nowhere")).is_err());
        assert!(starting(&deck, &theme, "nowhere", Some("bullets")).is_err());
    }

    /// A slide started as an agent starts one is set as a made deck's is (dogfood 3): its headline
    /// the slide's claim, shrunk to fit, its list grown to fill its place; named as asked; and it
    /// says each slot it filled, with the node, and each it left for a picture.
    #[test]
    fn a_slide_started_is_set_as_a_made_one_and_says_its_slots() {
        let deck = revenue();
        let theme =
            scaena_engine::theme::Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json"))
                .unwrap();
        let started = starting_as(&deck, &theme, "revenue", Some("bullets"), Some("pricing")).unwrap();
        assert_eq!(started.id, "pricing");
        let node = |id: &str| {
            started
                .patch
                .iter()
                .find(|op| op["op"] == "add_node" && op["id"] == id)
                .map(|op| op["node"].clone())
                .unwrap()
        };
        let header = node("pricing-header");
        assert_eq!((header["semantic"].as_str(), header["fit"].as_str()), (Some("claim"), Some("shrink")));
        let body = node("pricing-body");
        assert_eq!(body["fit"], "grow");
        assert_eq!(body["maxSize"], json!(theme.typography.roles["lede"].size));
        assert!(body.get("semantic").is_none());
        let slots: Vec<(&str, &str)> = started.filled.iter().map(|f| (f.slot.as_str(), f.node.as_str())).collect();
        assert!(
            slots.contains(&("header", "pricing-header")) && slots.contains(&("body", "pricing-body")),
            "{slots:?}"
        );
        assert!(started.empty.is_empty());
        assert!(starting_as(&deck, &theme, "revenue", Some("bullets"), Some("revenue")).is_err());

        let pictured = starting(&deck, &theme, "revenue", Some("art-left")).unwrap();
        assert_eq!(pictured.empty.len(), 1, "{:?}", pictured.empty);
        assert!(!pictured.filled.iter().any(|f| f.slot == pictured.empty[0].slot));
    }

    /// A slide started in a deck that runs on its own holds as the state it follows does, so it
    /// shows for some time (W323); in a deck presented by clicks, it holds as they do, for none
    /// (agent dogfood 6).
    #[test]
    fn a_slide_started_in_a_deck_that_runs_holds_as_its_neighbour_does() {
        let theme =
            scaena_engine::theme::Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json"))
                .unwrap();
        let mut deck = revenue();
        for state in &mut deck.states {
            state.hold = Some(5500.0);
        }
        let started = starting(&deck, &theme, "revenue", Some("bullets")).unwrap();
        assert_eq!(started.patch[0]["state"]["hold"], json!(5500.0), "{}", started.patch[0]);
        let mut clicked = revenue();
        for state in &mut clicked.states {
            state.hold = None;
        }
        let started = starting(&clicked, &theme, "revenue", Some("bullets")).unwrap();
        assert!(started.patch[0]["state"].get("hold").is_none(), "{}", started.patch[0]);
    }

    /// A slide started with a claim tells it in a beat of its own, just after the beat of the
    /// state it follows, not in that beat, which would then make two claims (dogfood 3, T3).
    #[test]
    fn a_slide_started_with_a_claim_tells_it_in_a_beat_of_its_own() {
        let deck = revenue();
        let theme =
            scaena_engine::theme::Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json"))
                .unwrap();
        let started = starting(&deck, &theme, "revenue", Some("bullets")).unwrap();
        let started = claimed(&deck, started, "Pro will lead next year.").unwrap();
        assert!(started.patch[0].get("beat").is_none(), "{}", started.patch[0]);
        let doc = scaena_core::patch::compile(&deck.to_value().unwrap(), &started.patch, &NoFiles).unwrap().doc;
        let beats: Vec<(&str, &Value)> = doc["spine"]["sections"][1]["beats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| (b["id"].as_str().unwrap(), &b["states"]))
            .collect();
        assert_eq!(beats, [("doubled", &json!(["revenue", "mix"])), ("bullets", &json!(["bullets"]))]);
        assert_eq!(doc["spine"]["sections"][1]["beats"][1]["claim"], "Pro will lead next year.");
        // Its headline says the claim, not the words that tell what goes there (agent dogfood 6).
        let said: Vec<&str> = (doc["nodes"].as_object().unwrap().iter())
            .filter(|(id, n)| id.starts_with("bullets") && n["semantic"] == "claim")
            .filter_map(|(_, n)| n["text"].as_str())
            .collect();
        assert_eq!(said, ["Pro will lead next year."]);
        let d = Deck::from_value(&doc).unwrap();
        let found = scaena_core::lint::check(&d, None);
        assert!(!found.iter().any(|f| f.code == "W426" || f.code == "W429"), "{found:#?}");
        // A deck with no spine has nowhere to tell a claim.
        let mut bare = revenue();
        bare.spine = None;
        let started = starting(&bare, &theme, "revenue", Some("bullets")).unwrap();
        assert!(claimed(&bare, started, "Pro will lead.").is_err());
    }

    /// A slide started with the claim of a beat the spine planned, as W429 asks, tells it in that
    /// beat, not in a second one making the claim again (agent dogfood 8).
    #[test]
    fn a_slide_started_with_a_planned_beats_claim_tells_it_in_that_beat() {
        let mut deck = revenue();
        let mut doc = deck.to_value().unwrap();
        let planned = json!({ "id": "lead", "claim": "Pro will lead next year.", "states": [] });
        doc["spine"]["sections"][1]["beats"].as_array_mut().unwrap().push(planned);
        deck = Deck::from_value(&doc).unwrap();
        assert!(scaena_core::lint::check(&deck, None).iter().any(|f| f.code == "W429"));
        let theme =
            scaena_engine::theme::Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json"))
                .unwrap();
        let started = starting(&deck, &theme, "revenue", Some("bullets")).unwrap();
        let started = claimed(&deck, started, " pro will lead next year.").unwrap();
        let doc = scaena_core::patch::compile(&doc, &started.patch, &NoFiles).unwrap().doc;
        let beats: Vec<(&str, &Value)> = (doc["spine"]["sections"][1]["beats"].as_array().unwrap().iter())
            .map(|b| (b["id"].as_str().unwrap(), &b["states"]))
            .collect();
        assert_eq!(beats, [("doubled", &json!(["revenue", "mix"])), ("lead", &json!(["bullets"]))]);
        let found = scaena_core::lint::check(&Deck::from_value(&doc).unwrap(), None);
        assert!(!found.iter().any(|f| ["W425", "W426", "W429"].contains(&f.code.as_str())), "{found:#?}");
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
        let said: Vec<(&str, &str, bool, bool)> =
            waits.iter().map(|w| (w.slot.as_str(), w.words.as_str(), w.typed, w.picture)).collect();
        assert_eq!(said, [("art", "A picture", false, true)]);
        // A place for a chart is no picture's (PLAN 2.107).
        let charted = patched(&starting(&deck, &theme, "revenue", Some("bar-chart")).unwrap().patch);
        let chart: Vec<(String, bool)> =
            waiting(&charted, &theme, "bar-chart", None).unwrap().into_iter().map(|w| (w.slot, w.picture)).collect();
        assert_eq!(chart, [("main".into(), false)]);
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

    /// A text still reads as its slot's prompt until its words change (PLAN 3.30): what an editor
    /// selects whole as typing begins; a text the deck wrote reads as none.
    #[test]
    fn a_text_reads_as_its_slots_prompt_until_its_words_change() {
        let deck = revenue();
        let theme =
            scaena_engine::theme::Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json"))
                .unwrap();
        let patched = |ops: &[Value]| {
            let doc = scaena_core::patch::compile(&deck.to_value().unwrap(), ops, &NoFiles).unwrap().doc;
            Deck::from_value(&doc).unwrap()
        };
        let started = starting(&deck, &theme, "revenue", Some("bullets")).unwrap();
        let made = patched(&started.patch);
        assert!(prompted(&made, &theme, "bullets", "bullets-header").unwrap());
        assert!(prompted(&made, &theme, "bullets", "bullets-body").unwrap(), "a list's items too");
        let mut ops = started.patch.clone();
        ops.push(json!({ "op": "replace_text", "node": "bullets-header", "state": "bullets", "from": 0, "to": 4, "text": "Why" }));
        assert!(!prompted(&patched(&ops), &theme, "bullets", "bullets-header").unwrap(), "words typed over");
        assert!(!prompted(&deck, &theme, "intro", "title").unwrap(), "the deck's own words");
        assert!(!prompted(&made, &theme, "bullets", "nowhere").unwrap());
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
