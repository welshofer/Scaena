//! A text's lines drawn as a diagram (PLAN 2.110), as a presentation app turns a list into one:
//! a process, a cycle, or a hierarchy of boxes, each holding its words (PLAN 2.108) and joined by
//! connectors (PLAN 2.109). What it makes is the deck's own nodes in a container that takes the
//! text's place, so every editor moves, restyles, and types in them as it does any other, and
//! the container lays them out again wherever the text's place goes.

use crate::inspect::{deleting, free_id, leaving, reads_on};
use crate::{Context, OpsError};
use scaena_core::Deck;
use scaena_core::document::{NodeType, Props};
use scaena_core::model::theme::Vocabulary;
use scaena_core::model::values::ListItem;
use scaena_core::patch::SemanticOp;
use scaena_core::validate::BundleFiles;
use scaena_engine::theme::Theme;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What a diagram draws a text's lines as (PLAN 2.110).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Diagram {
    /// Steps one after another, each joined to the next by an arrow: across where the text's place
    /// is wider than tall, and down where it is taller.
    Process,
    /// Steps around a ring, each joined to the next by an arrow, and the last to the first.
    Cycle,
    /// A tree by the lines' list levels, each line joined to the one it is under by a line that
    /// turns at right angles, as an organization chart is drawn.
    Hierarchy,
}

/// A diagram made (PLAN 2.110): the container that holds it, and the patch that makes it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Diagrammed {
    /// The container: a stack for a process, a grid for a cycle or a hierarchy.
    pub id: String,
    /// One patch: the container, its boxes, their words, and the connectors, each `add_node`
    /// entering in the state and kept to its slide (PLAN 3.24), then the text deleted from the
    /// state on, as Delete deletes it (PLAN 2.34).
    pub patch: Vec<Value>,
}

/// The most steps a process or a cycle draws, and the most boxes, side by side, and levels a
/// hierarchy draws: past them, the words would be too small to read.
const STEPS: usize = 8;
const BOXES: usize = 24;
const WIDE: usize = 8;
const LEVELS: usize = 5;

/// A cycle's grid: tracks each way, fine enough to set its boxes on a ring.
const RING: u32 = 24;

/// One box of a hierarchy: its words, and the boxes under it.
struct Item {
    words: String,
    under: Vec<Item>,
}

impl Item {
    /// How many boxes at the foot of the tree stand under it, itself where none does.
    fn leaves(&self) -> usize {
        if self.under.is_empty() { 1 } else { self.under.iter().map(Item::leaves).sum() }
    }

    fn levels(&self) -> usize {
        1 + self.under.iter().map(Item::levels).max().unwrap_or(0)
    }

    fn count(&self) -> usize {
        1 + self.under.iter().map(Item::count).sum::<usize>()
    }
}

/// The patch that draws text `node`'s lines in `state` as a diagram of `kind` (PLAN 2.110), in
/// the place the text takes there: a container placed as the text is, holding a box for each step
/// with its words, and the connectors between them. `rest` is the text's box at rest there, canvas
/// units, which sizes a process's boxes and sets a hierarchy's to one column's width.
///
/// Each line that holds words is a step, at its list level: a process's or a cycle's steps are its
/// shallowest lines, each deeper line joining the box above it as a line of its own; a hierarchy's
/// boxes stand under the nearest shallower line above them. The boxes are filled with the theme's
/// accent (a hierarchy's below its top with `surface-2`), their words in `body`, centered, shrunk
/// to fit, in the color that reads on the fill. Refused for a node that is no text, or a shape's
/// words, and for lines a diagram of `kind` cannot draw: one step, a cycle of two, more steps than
/// fit, or a hierarchy with no line under another. `files` is the bundle's, which the patch is
/// made on.
///
/// Its boxes are tall enough for two lines of their words: where the text's place is shorter
/// than that, the container takes more of the theme grid's `rows` (canvas units) the place
/// stands on, down and then up, where the text is placed on them by its rows; and a hierarchy's
/// rows stand closer, the gap between them no more than its boxes' height allows.
#[allow(clippy::too_many_arguments)]
pub fn diagramming(
    deck: &Deck,
    theme: &Theme,
    files: &dyn BundleFiles,
    state: &str,
    node: &str,
    kind: Diagram,
    rest: [f32; 4],
    rows: &[[f32; 2]],
) -> Result<Diagrammed, OpsError> {
    let snaps = scaena_core::resolve_states(deck).context("tracking")?;
    let snap =
        snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::new(format!("unknown state `{state}`")))?;
    let found = deck.nodes.get(node).ok_or_else(|| OpsError::new(format!("no node `{node}`")))?;
    let props = snap.nodes.get(node).ok_or_else(|| OpsError::new(format!("`{node}` is not on screen in `{state}`")))?;
    if found.node_type != NodeType::Text {
        return Err(OpsError::new(format!("`{node}` is no text: a diagram is drawn from a text's lines")));
    }
    if scaena_core::tracking::words_of(deck, found).is_some() {
        return Err(OpsError::new(format!("`{node}` is a shape's words: a diagram is drawn from a text of its own")));
    }
    let lines = lines(props);
    if lines.len() < 2 {
        return Err(OpsError::new("a diagram takes two lines or more: put each step on a line of its own"));
    }

    let mut look = Look::of(theme);
    let mut taken: Vec<String> = deck.nodes.keys().cloned().collect();
    let mut name = |base: String| {
        let id = free_id(&taken, &base);
        taken.push(id.clone());
        id
    };
    let (container, title) = match kind {
        Diagram::Process => ("process", "Process"),
        Diagram::Cycle => ("cycle", "Cycle"),
        Diagram::Hierarchy => ("hierarchy", "Hierarchy"),
    };
    let id = name(format!("{node}-{container}"));
    let mut holder = json!({ "type": if kind == Diagram::Process { "stack" } else { "grid" }, "name": title });
    // In the text's place, as the text is placed there, in each format too.
    for key in ["at", "size", "z", "transform"] {
        if let Some(v) = props.get(key) {
            holder[key] = v.clone();
        }
    }
    if let Some(Value::Object(formats)) = props.get("formats") {
        let placed: serde_json::Map<String, Value> = formats
            .iter()
            .filter_map(|(format, layout)| {
                let kept: serde_json::Map<String, Value> = ["at", "size", "transform"]
                    .iter()
                    .filter_map(|k| layout.get(*k).map(|v| (k.to_string(), v.clone())))
                    .collect();
                (!kept.is_empty()).then(|| (format.clone(), Value::Object(kept)))
            })
            .collect();
        if !placed.is_empty() {
            holder["formats"] = Value::Object(placed);
        }
    }

    // Each box, as `(id, words, its own props)`, and each connector, `(from, to)`.
    let mut boxes: Vec<(String, String, Value)> = Vec::new();
    let mut joins: Vec<(String, String)> = Vec::new();
    let [_, _, w, mut h] = rest;
    // The place taller, where the diagram needs it to be: its boxes two lines of words tall.
    let slots = snap.layout.as_deref().and_then(|l| theme.layouts.get(l)).map(|l| &l.slots);
    let room = |holder: &mut Value, h: &mut f32, need: f32| {
        if let Some((at, more)) = holder.get("at").and_then(|at| taller(at, slots, rows, need - *h)) {
            holder["at"] = at;
            *h += more;
        }
        *h
    };
    match kind {
        Diagram::Process | Diagram::Cycle => {
            let steps = steps(&lines);
            let n = steps.len();
            if n < 2 {
                return Err(OpsError::new(
                    "a diagram takes two steps or more: put each step on a line of its own, at one level",
                ));
            }
            if kind == Diagram::Cycle && n < 3 {
                return Err(OpsError::new("a cycle takes three steps or more"));
            }
            if n > STEPS {
                return Err(OpsError::new(format!(
                    "a {container} draws {STEPS} steps at most, and this text has {n}: make some lines \
                     part of the step above them with Tab"
                )));
            }
            let ids: Vec<String> = (1..=n).map(|k| name(format!("{id}-{k}"))).collect();
            if kind == Diagram::Process {
                let across = w >= h;
                let h = match across {
                    true => room(&mut holder, &mut h, look.tall),
                    false => room(&mut holder, &mut h, n as f32 * look.tall + (n - 1) as f32 * look.gap_length),
                };
                holder["axis"] = json!(if across { "x" } else { "y" });
                holder["gap"] = json!(look.gap);
                for (box_id, words) in ids.iter().zip(steps) {
                    let mut own = json!({ "at": { "parent": id } });
                    if across {
                        // As tall as four thirds of its width, where the text's place has the room.
                        let wide = (w - (n - 1) as f32 * look.gap_length) / n as f32;
                        let tall = (100.0 * 0.75 * wide / h.max(1.0)).clamp(10.0, 100.0).round();
                        own["size"] = json!({ "h": format!("{tall}%") });
                        own["at"]["align"] = json!({ "y": "center" });
                    }
                    boxes.push((box_id.clone(), words, own));
                }
                joins.extend(ids.windows(2).map(|p| (p[0].clone(), p[1].clone())));
            } else {
                holder["cols"] = json!(RING);
                holder["rows"] = json!(RING);
                let (span, cells) = ring(n);
                room(&mut holder, &mut h, look.tall * RING as f32 / span[1] as f32);
                for ((box_id, words), [col, row]) in ids.iter().zip(steps).zip(cells) {
                    let at = json!({ "parent": id, "col": [col + 1, col + span[0]], "row": [row + 1, row + span[1]] });
                    boxes.push((box_id.clone(), words, json!({ "at": at })));
                }
                joins.extend((0..n).map(|k| (ids[k].clone(), ids[(k + 1) % n].clone())));
            }
        }
        Diagram::Hierarchy => {
            let top = lines.iter().map(|l| l.0).min().unwrap_or(0);
            let mut at = 0;
            let roots = grow(&lines, &mut at, top);
            if roots.iter().all(|r| r.under.is_empty()) {
                return Err(OpsError::new(
                    "a hierarchy takes lines under others: indent the lines under each with Tab",
                ));
            }
            let (count, wide): (usize, usize) =
                (roots.iter().map(Item::count).sum(), roots.iter().map(Item::leaves).sum());
            let levels = roots.iter().map(Item::levels).max().unwrap_or(1);
            if count > BOXES || wide > WIDE || levels > LEVELS {
                return Err(OpsError::new(format!(
                    "a hierarchy draws {BOXES} boxes at most, {WIDE} side by side, {LEVELS} levels deep, and \
                     this text has {count}, {wide} side by side, {levels} deep"
                )));
            }
            holder["cols"] = json!(wide);
            holder["rows"] = json!(levels);
            let h = room(&mut holder, &mut h, levels as f32 * look.tall + (levels - 1) as f32 * look.gap_length);
            // Its rows closer where its boxes would be shorter than two lines of their words.
            look.closer(theme, (h - levels as f32 * look.tall) / (levels - 1).max(1) as f32);
            holder["gap"] = json!(look.gap);
            // One column's width, as a share of a box's columns with the gaps between them.
            let column = (w - (wide - 1) as f32 * look.gap_length) / wide as f32;
            let mut placed = Placed { id: &id, look: &look, column, boxes: &mut boxes, joins: &mut joins };
            let mut first = 1;
            for root in &roots {
                placed.place(root, 0, first, None, &mut name);
                first += root.leaves();
            }
        }
    }

    // The container, then each box, its words, and the connectors, each entering in `state`.
    let mut adds = vec![(id.clone(), holder)];
    let mut words_ids = Vec::new();
    for (box_id, words, own) in &boxes {
        let lower = own.get("lower").and_then(Value::as_bool).unwrap_or(false);
        let fill = if lower { &look.lower } else { &look.fill };
        let mut shape = json!({ "type": "shape", "kind": "rect" });
        if let Some(radius) = &look.radius {
            shape["radius"] = json!(radius);
        }
        if let Some(fill) = fill {
            shape["fill"] = json!(fill);
        }
        // A box below the top, filled near the slide's own color, outlined in the top's so it
        // stands out from the slide.
        if let (true, Some(outline)) = (lower, &look.outline) {
            shape["stroke"] = outline.clone();
        }
        for (k, v) in own.as_object().into_iter().flatten().filter(|(k, _)| *k != "lower") {
            shape[k] = v.clone();
        }
        adds.push((box_id.clone(), shape));
        let words_id = name(format!("{box_id}-words"));
        let mut text = json!({ "type": "text", "role": look.role, "text": words, "align": "center", "fit": "shrink",
            "at": { "parent": box_id } });
        if let Some(color) = reads_on(theme, fill.as_ref().map(|f| json!(f)).as_ref()) {
            text["style"] = json!({ "color": color });
        }
        words_ids.push(words_id.clone());
        adds.push((words_id, text));
    }
    let mut joined = Vec::new();
    for (from, to) in &joins {
        let line = kind == Diagram::Hierarchy;
        let named = format!("{from}-to-{to}");
        let joint =
            name(if named.len() <= 56 { named } else { format!("{id}-{}", if line { "line" } else { "arrow" }) });
        let mut connect = json!({ "from": from, "to": to });
        if line {
            connect["route"] = json!("elbow");
        }
        let mut shape = json!({ "type": "shape", "kind": if line { "line" } else { "arrow" }, "connect": connect });
        if let Some(stroke) = &look.stroke {
            shape["stroke"] = stroke.clone();
        }
        joined.push(joint.clone());
        adds.push((joint, shape));
    }
    let mut ops = Vec::with_capacity(adds.len());
    for (add, node_value) in adds {
        let node_value = serde_json::from_value(node_value).context("a diagram's node")?;
        ops.push(SemanticOp::AddNode { id: add, node: node_value, state: Some(state.into()), props: None });
    }
    // Kept to the slide: each leaves before what it stands on, a connector before its ends, words
    // before their box, a box before the container.
    let leaves: Vec<String> = joined
        .into_iter()
        .chain(words_ids)
        .chain(boxes.iter().map(|(b, _, _)| b.clone()))
        .chain(std::iter::once(id.clone()))
        .collect();
    ops.extend(leaving(deck, state, &leaves));
    let mut patch: Vec<Value> = ops.iter().map(serde_json::to_value).collect::<Result<_, _>>().context("a patch")?;
    patch.extend(deleting(deck, files, node, state, false)?);
    Ok(Diagrammed { id, patch })
}

/// What a diagram draws in, from the theme: names it defines, so each reads as the deck's.
struct Look {
    /// The boxes' fill: the theme's accent, or its first color.
    fill: Option<String>,
    /// A hierarchy's boxes below its top: `surface-2`, where the theme has it.
    lower: Option<String>,
    /// The boxes' corners: `radius.3`, or the theme's largest step short of it.
    radius: Option<String>,
    /// Between the boxes, room for a connector: `space.7`, or the largest step short of it.
    gap: String,
    gap_length: f32,
    /// The words' role: `body`, or the theme's first.
    role: String,
    /// The connectors' stroke: the `regular` width in `onSurfaceMuted`, where the theme has them.
    stroke: Option<Value>,
    /// The outline of a hierarchy's boxes below its top: the `regular` width in the top's fill.
    outline: Option<Value>,
    /// The least a box is tall: two lines of its words, canvas units.
    tall: f32,
}

impl Look {
    fn of(theme: &Theme) -> Look {
        let colors = theme.names(Vocabulary::Color);
        let has = |name: &str| colors.iter().any(|c| c == name);
        let fill = if has("accent") { Some("accent".to_string()) } else { colors.first().cloned() };
        let lower = if has("surface-2") { Some("surface-2".to_string()) } else { fill.clone() };
        let radii = theme.names(Vocabulary::Radius);
        let radius = radii.get(3).or(radii.last()).cloned();
        let scale = &theme.tokens.space.scale;
        let step = scale.len().min(8).saturating_sub(1);
        let gap = format!("space.{step}");
        let gap_length = scale.get(step).map_or(0.0, |s| s.0 as f32);
        let roles = theme.names(Vocabulary::TextRole);
        let role = roles.iter().find(|r| *r == "body").or(roles.first()).cloned().unwrap_or_else(|| "body".into());
        let strokes = theme.names(Vocabulary::Stroke);
        let mut stroke = serde_json::Map::new();
        if strokes.iter().any(|s| s == "regular") {
            stroke.insert("width".into(), json!("regular"));
        }
        if has("onSurfaceMuted") {
            stroke.insert("paint".into(), json!("onSurfaceMuted"));
        }
        let stroke = (!stroke.is_empty()).then_some(Value::Object(stroke));
        let outline = (fill.as_ref().filter(|_| lower != fill)).map(|paint| {
            let mut outline = json!({ "paint": paint });
            if strokes.iter().any(|s| s == "regular") {
                outline["width"] = json!("regular");
            }
            outline
        });
        let line = theme.typography.roles.get(&role).map_or(40.0, |r| (r.size * r.leading) as f32);
        Look { fill, lower, radius, gap, gap_length, role, stroke, outline, tall: 2.0 * line }
    }

    /// The gap the largest step of the theme's space scale, up to the one it is, that is no more
    /// than `most` canvas units; `space.4` at the least, room for a connector to stand off the
    /// boxes it joins.
    fn closer(&mut self, theme: &Theme, most: f32) {
        let scale = &theme.tokens.space.scale;
        let now = scale.iter().position(|s| s.0 as f32 == self.gap_length).unwrap_or(scale.len().saturating_sub(1));
        let least = now.min(4);
        let fits = (least..=now).rev().find(|&i| scale[i].0 as f32 <= most).unwrap_or(least);
        if let Some(step) = scale.get(fits).filter(|_| fits < now) {
            self.gap = format!("space.{fits}");
            self.gap_length = step.0 as f32;
        }
    }
}

/// Place `at` on the theme grid's `rows` (canvas units) by a 1-based row range, grown by whole rows
/// until it is `more` canvas units taller, or as near as the grid goes: down first, then up. A slot
/// of `slots`, the layout's, too short is left for its cells, grown so. The place, and how much
/// taller it is. None where it is no taller: placed otherwise, in a container, or tall enough.
fn taller(
    at: &Value,
    slots: Option<&indexmap::IndexMap<String, scaena_core::model::theme::Slot>>,
    rows: &[[f32; 2]],
    more: f32,
) -> Option<(Value, f32)> {
    if more <= 0.0 || at.get("parent").is_some() || at.get("rect").is_some() {
        return None;
    }
    let at = match at.get("in").and_then(Value::as_str) {
        Some(slot) => {
            let slot = slots?.get(slot)?;
            json!({ "col": serde_json::to_value(slot.col.as_ref()?).ok()?, "row": serde_json::to_value(slot.row.as_ref()?).ok()? })
        }
        None => at.clone(),
    };
    let at = &at;
    let n = |v: &Value| v.as_u64().filter(|n| *n >= 1).map(|n| n as usize);
    let (a, b) = match at.get("row")? {
        Value::Array(ab) if ab.len() == 2 => (n(&ab[0])?, n(&ab[1])?),
        one => (n(one)?, n(one)?),
    };
    if a > b || b > rows.len() {
        return None;
    }
    let tall = |a: usize, b: usize| rows[b - 1][1] - rows[a - 1][0];
    let (mut top, mut foot) = (a, b);
    while tall(top, foot) - tall(a, b) < more && foot < rows.len() {
        foot += 1;
    }
    while tall(top, foot) - tall(a, b) < more && top > 1 {
        top -= 1;
    }
    let grown = tall(top, foot) - tall(a, b);
    let mut at = at.clone();
    at["row"] = json!([top, foot]);
    (grown > 0.0).then_some((at, grown))
}

/// A hierarchy's boxes as they are placed: each on its level's row, over the columns of the boxes
/// at the foot of the tree under it, one column wide and centered there.
struct Placed<'a> {
    id: &'a str,
    look: &'a Look,
    /// One column's width, canvas units.
    column: f32,
    boxes: &'a mut Vec<(String, String, Value)>,
    joins: &'a mut Vec<(String, String)>,
}

impl Placed<'_> {
    fn place(
        &mut self,
        item: &Item,
        level: usize,
        first: usize,
        over: Option<&str>,
        name: &mut dyn FnMut(String) -> String,
    ) {
        let box_id = name(format!("{}-{}", self.id, self.boxes.len() + 1));
        let span = item.leaves();
        let mut own =
            json!({ "at": { "parent": self.id, "col": [first, first + span - 1], "row": [level + 1, level + 1] } });
        if span > 1 {
            // A share of the box's columns and the gaps between them that is one column.
            let whole = span as f32 * self.column + (span - 1) as f32 * self.look.gap_length;
            let share = (100.0 * self.column / whole.max(1.0) * 100.0).round() / 100.0;
            own["size"] = json!({ "w": format!("{share}%") });
            own["at"]["align"] = json!({ "x": "center" });
        }
        if level > 0 {
            own["lower"] = json!(true);
        }
        self.boxes.push((box_id.clone(), item.words.clone(), own));
        if let Some(over) = over {
            self.joins.push((over.to_string(), box_id.clone()));
        }
        let mut at = first;
        for under in &item.under {
            self.place(under, level + 1, at, Some(&box_id), name);
            at += under.leaves();
        }
    }
}

/// The text's lines that hold words, each at its list level: 0 where it is no list's item.
fn lines(props: &Props) -> Vec<(u8, String)> {
    let text = scaena_core::reading::words(props);
    let paragraphs = scaena_core::lists::paragraphs(&text);
    let list: Vec<Option<ListItem>> =
        props.get("list").and_then(|l| serde_json::from_value(l.clone()).ok()).unwrap_or_default();
    let items = scaena_core::lists::items(&list, paragraphs.len());
    paragraphs
        .into_iter()
        .zip(items)
        .filter_map(|(range, item)| {
            let words = text[range].trim();
            (!words.is_empty()).then(|| (item.map_or(0, |i| i.depth()), words.to_string()))
        })
        .collect()
}

/// A process's or a cycle's steps: each of the shallowest lines, with each deeper line after it a
/// line of its own in its box.
fn steps(lines: &[(u8, String)]) -> Vec<String> {
    let top = lines.iter().map(|l| l.0).min().unwrap_or(0);
    let mut out: Vec<String> = Vec::new();
    for (depth, words) in lines {
        match out.last_mut() {
            Some(last) if *depth > top => {
                last.push('\n');
                last.push_str(words);
            }
            _ => out.push(words.clone()),
        }
    }
    out
}

/// A hierarchy's boxes from `lines`, from `at` on, at `level` or deeper: each line, and under it the
/// deeper lines that follow it.
fn grow(lines: &[(u8, String)], at: &mut usize, level: u8) -> Vec<Item> {
    let mut out = Vec::new();
    while let Some((depth, words)) = lines.get(*at).filter(|(d, _)| *d >= level) {
        *at += 1;
        let under = grow(lines, at, depth + 1);
        out.push(Item { words: words.clone(), under });
    }
    out
}

/// Where a cycle's `n` boxes stand on its grid of [`RING`] tracks each way: their span, columns
/// by rows, and each one's first column and row, from 0. The first stands at the top, the rest
/// clockwise, their middles on an ellipse that fills the grid; the largest span whose boxes keep
/// a track clear of each other.
fn ring(n: usize) -> ([u32; 2], Vec<[u32; 2]>) {
    let points: Vec<[f64; 2]> = (0..n)
        .map(|k| {
            let turn = std::f64::consts::TAU * k as f64 / n as f64;
            // Rounded, so a ring's two sides mirror each other to the track.
            let even = |v: f64| (v * 1e6).round() / 1e6;
            [even(turn.sin()), even(-turn.cos())]
        })
        .collect();
    let bounds =
        |axis: usize| points.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p[axis]), hi.max(p[axis])));
    let (x, y) = (bounds(0), bounds(1));
    let place = |span: [u32; 2]| -> Vec<[u32; 2]> {
        let fit = |v: f64, (lo, hi): (f64, f64), room: f64| {
            let at = if hi - lo < 1e-9 { room / 2.0 } else { (v - lo) / (hi - lo) * room };
            (at + 0.5).floor().clamp(0.0, room) as u32
        };
        let (rw, rh) = (f64::from(RING - span[0]), f64::from(RING - span[1]));
        points.iter().map(|p| [fit(p[0], x, rw), fit(p[1], y, rh)]).collect()
    };
    let clear = |span: [u32; 2], cells: &[[u32; 2]]| {
        cells.iter().enumerate().all(|(i, a)| {
            cells[i + 1..].iter().all(|b| {
                a[0] > b[0] + span[0] || b[0] > a[0] + span[0] || a[1] > b[1] + span[1] || b[1] > a[1] + span[1]
            })
        })
    };
    const SPANS: [[u32; 2]; 7] = [[8, 6], [8, 5], [8, 4], [6, 5], [6, 4], [4, 4], [4, 3]];
    for span in SPANS {
        let cells = place(span);
        if clear(span, &cells) {
            return (span, cells);
        }
    }
    let span = SPANS[SPANS.len() - 1];
    (span, place(span))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REVENUE: &str = include_str!("../../../docs/examples/revenue.deck.json");

    struct Examples;
    impl BundleFiles for Examples {
        fn exists(&self, path: &str) -> bool {
            std::path::Path::new("../../docs/examples").join(path).is_file()
        }
        fn read_text(&self, path: &str) -> Option<String> {
            std::fs::read_to_string(std::path::Path::new("../../docs/examples").join(path)).ok()
        }
    }

    /// The revenue example, its intro's subtitle four lines, the second under the first.
    fn listed() -> Value {
        let mut doc: Value = serde_json::from_str(REVENUE).unwrap();
        doc["nodes"]["subtitle"]["text"] = json!("Plan\nWho\nBuild\nShip");
        doc["nodes"]["subtitle"]["list"] =
            json!([{ "kind": "bullet" }, { "kind": "bullet", "level": 1 }, { "kind": "bullet" }, { "kind": "bullet" }]);
        doc
    }

    #[test]
    fn a_texts_lines_are_drawn_as_a_diagram_in_its_place() {
        let theme = Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        let doc = listed();
        let deck = Deck::from_value(&doc).unwrap();
        let rest = [120.0, 700.0, 1680.0, 240.0];
        let draw = |kind| diagramming(&deck, &theme, &Examples, "intro", "subtitle", kind, rest, &[]);
        let ops = |made: &Diagrammed, op: &str| -> Vec<Value> {
            made.patch.iter().filter(|o| o["op"] == op).cloned().collect()
        };

        // A process: a stack in the text's place, across, its steps the shallowest lines, the line
        // under the first a line of its own in the first box; an arrow between each two.
        let made = draw(Diagram::Process).unwrap();
        assert_eq!(made.id, "subtitle-process");
        let added = ops(&made, "add_node");
        let holder = &added[0]["node"];
        assert_eq!((holder["type"].as_str(), holder["axis"].as_str()), (Some("stack"), Some("x")), "{holder}");
        assert_eq!(holder["at"], json!({ "in": "subtitle" }));
        let words: Vec<&str> =
            added.iter().filter(|o| o["node"]["type"] == "text").filter_map(|o| o["node"]["text"].as_str()).collect();
        assert_eq!(words, ["Plan\nWho", "Build", "Ship"]);
        let box_ = &added[1]["node"];
        assert_eq!((box_["kind"].as_str(), box_["fill"].as_str()), (Some("rect"), Some("accent")), "{box_}");
        // As tall as four thirds of its width where the place has room: here, all of it.
        assert_eq!(box_["size"]["h"], "100%", "{box_}");
        let words = &added[2]["node"];
        assert_eq!((words["style"]["color"].as_str(), words["fit"].as_str()), (Some("onAccent"), Some("shrink")));
        let arrows: Vec<&Value> = added.iter().filter(|o| o["node"]["connect"].is_object()).collect();
        assert_eq!(arrows.len(), 2);
        assert_eq!(arrows[0]["node"]["connect"], json!({ "from": "subtitle-process-1", "to": "subtitle-process-2" }));
        // Made, it validates, shows on the intro's slide alone, and the subtitle is gone from it.
        let compiled = scaena_core::patch::compile(&doc, &made.patch, &Examples).unwrap();
        let after = Deck::from_value(&compiled.doc).unwrap();
        let snaps = scaena_core::resolve_states(&after).unwrap();
        let shows = |id: &str| -> Vec<&str> {
            snaps.iter().filter(|s| s.nodes.contains_key(id)).map(|s| s.state_id.as_str()).collect()
        };
        for id in [
            "subtitle-process",
            "subtitle-process-1",
            "subtitle-process-1-words",
            "subtitle-process-1-to-subtitle-process-2",
        ] {
            assert_eq!(shows(id), ["intro"], "{id}");
        }
        assert!(shows("subtitle").is_empty(), "the text gone");
        assert!(scaena_core::validate::validate(&after).is_empty(), "it validates");

        // A cycle: a grid, its boxes on a ring, and an arrow from the last to the first.
        let made = draw(Diagram::Cycle).unwrap();
        let added = ops(&made, "add_node");
        assert_eq!((added[0]["node"]["type"].as_str(), added[0]["node"]["cols"].as_u64()), (Some("grid"), Some(24)));
        assert_eq!(added[1]["node"]["at"], json!({ "parent": "subtitle-cycle", "col": [9, 16], "row": [1, 6] }));
        let last = added.iter().rev().find(|o| o["node"]["connect"].is_object()).unwrap();
        assert_eq!(last["node"]["connect"], json!({ "from": "subtitle-cycle-3", "to": "subtitle-cycle-1" }));

        // A hierarchy: a grid of the tree's foot by its levels, a line from each box down to each
        // under it, the lower boxes on `surface-2`.
        let made = draw(Diagram::Hierarchy).unwrap();
        let added = ops(&made, "add_node");
        assert_eq!((added[0]["node"]["cols"].as_u64(), added[0]["node"]["rows"].as_u64()), (Some(3), Some(2)));
        let lines: Vec<&Value> = added.iter().filter(|o| o["node"]["connect"].is_object()).collect();
        assert_eq!(lines.len(), 1);
        let line = &lines[0]["node"];
        assert_eq!((line["kind"].as_str(), line["connect"]["route"].as_str()), (Some("line"), Some("elbow")));
        let who = added.iter().find(|o| o["node"]["text"] == "Who").unwrap();
        let under = added.iter().find(|o| o["id"] == who["node"]["at"]["parent"]).unwrap();
        assert_eq!(
            (under["node"]["fill"].as_str(), under["node"]["at"]["row"].clone()),
            (Some("surface-2"), json!([2, 2]))
        );

        // Refused: one line, a hierarchy of lines at one level, a cycle of two, what is no text.
        let refused = |node: &str, kind| {
            diagramming(&deck, &theme, &Examples, "intro", node, kind, rest, &[]).unwrap_err().to_string()
        };
        assert!(refused("title", Diagram::Process).contains("two lines or more"));
        let mut flat = listed();
        flat["nodes"]["subtitle"]["list"] = Value::Null;
        flat["nodes"]["subtitle"].as_object_mut().unwrap().remove("list");
        flat["nodes"]["subtitle"]["text"] = json!("Plan\nBuild");
        let flat = Deck::from_value(&flat).unwrap();
        let said =
            |kind| diagramming(&flat, &theme, &Examples, "intro", "subtitle", kind, rest, &[]).unwrap_err().to_string();
        assert!(said(Diagram::Hierarchy).contains("lines under others"));
        assert!(said(Diagram::Cycle).contains("three steps or more"));
        let chart = diagramming(&deck, &theme, &Examples, "revenue", "rev", Diagram::Process, rest, &[]).unwrap_err();
        assert!(chart.to_string().contains("no text"), "{chart}");
    }

    #[test]
    fn a_cycles_boxes_stand_on_a_ring_clear_of_each_other() {
        // Three: the first at the top, the others mirrored at the foot.
        let (span, cells) = ring(3);
        assert_eq!(span, [8, 6]);
        assert_eq!(cells, vec![[8, 0], [16, 18], [0, 18]]);
        // Four: a diamond.
        assert_eq!(ring(4).1, vec![[8, 0], [16, 9], [8, 18], [0, 9]]);
        // Up to eight, smaller as they crowd, the first always centered at the top.
        for n in 3..=8 {
            let (span, cells) = ring(n);
            assert_eq!(cells[0][0] * 2 + span[0], RING, "{n}: {cells:?}");
            assert!(cells.iter().all(|c| c[0] + span[0] <= RING && c[1] + span[1] <= RING), "{n}: {cells:?}");
        }
    }

    #[test]
    fn lines_make_steps_and_a_tree_by_their_levels() {
        let lines: Vec<(u8, String)> =
            [(0, "Plan"), (1, "Who"), (0, "Build"), (2, "How"), (1, "With what"), (0, "Ship")]
                .into_iter()
                .map(|(d, w)| (d, w.to_string()))
                .collect();
        assert_eq!(steps(&lines), ["Plan\nWho", "Build\nHow\nWith what", "Ship"]);
        let roots = grow(&lines, &mut 0, 0);
        let said = |items: &[Item]| items.iter().map(|i| format!("{}:{}", i.words, i.under.len())).collect::<Vec<_>>();
        assert_eq!(said(&roots), ["Plan:1", "Build:2", "Ship:0"]);
        assert_eq!(said(&roots[1].under), ["How:0", "With what:0"]);
        assert_eq!((roots[1].leaves(), roots[1].levels(), roots.iter().map(Item::count).sum::<usize>()), (2, 2, 6));
    }
}
