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
    let id = match id {
        Some(id) => id.to_string(),
        None => fresh(deck, &layout.map_or_else(|| "slide".to_string(), |name| slug(name, "slide"))),
    };
    let mut state = json!({ "id": id, "mode": "absolute" });
    if let Some(layout) = layout {
        state["layout"] = json!(layout);
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

/// `added`, a slide [`starting`] made, telling `claim` in a beat of its own (dogfood 3): the beat
/// just after the one that tells the state the slide follows, in that section, else last; and
/// not in that beat, which would then make two claims. Refused in a deck with no spine.
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
    Ok(added)
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
/// O.1e); or a chart or a table of a few rows made up for it, written inline. An editor draws it, so a
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
    let node = slot_node(slot, s, scaena_core::direction::mirrored(deck)).filter(|n| n["type"] == "text").ok_or_else(
        || OpsError::new(format!("the slot `{slot}` waits for no words: a picture or a figure goes there")),
    )?;
    let i = deck.state_index(state).ok_or_else(|| OpsError::new(format!("unknown state `{state}`")))?;
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
    let snap =
        snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::new(format!("unknown state `{state}`")))?;
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
                json!({ "op": "add_state", "state": { "id": "slide", "mode": "absolute" }, "after": "close", "beat": "thanks" })
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
        let d = Deck::from_value(&doc).unwrap();
        let found = scaena_core::lint::check(&d, None);
        assert!(!found.iter().any(|f| f.code == "W426" || f.code == "W429"), "{found:#?}");
        // A deck with no spine has nowhere to tell a claim.
        let mut bare = revenue();
        bare.spine = None;
        let started = starting(&bare, &theme, "revenue", Some("bullets")).unwrap();
        assert!(claimed(&bare, started, "Pro will lead.").is_err());
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
