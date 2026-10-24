//! E101: content that collides. Two nodes collide when they draw content (text, a
//! chart, a table, an image) on one level and their boxes overlap: what is meant to lie
//! on top says so with a higher `z`, and a node marked `semantic: decoration` is meant to
//! lie anywhere. Two nodes are on one level when they stack at the same `z` where their
//! paint paths part: their own in one container, or the containers they are in, so a card
//! in a stack collides with a note beside the stack when the stack and the note share a
//! `z`. Text counts by its lines as set, not its cell, so a short title in a tall slot
//! collides only where its words are. A box is where it is drawn: through its `transform`
//! and those of what holds it (SPEC §3.3), so a turned label collides where it turns to.
//!
//! W303: a node drawn where the slide's number or footer stands (ADR-0028).
//!
//! W304: a place a slide's layout keeps for a picture or a figure, left empty at rest.
//!
//! W311: a shader painted behind a chart or a table, where they overlap.
//!
//! W313: a chart squashed below a legible plot.
//!
//! W314: a fetched picture's credit that cannot be read (ADR-0030).
//!
//! I405: a text set larger than the body whose descenders come within the theme's gutter of
//! the text under it.

use super::{Cx, Rule};
use crate::render::PlacedText;
use crate::sample::{Content, Scene, SceneNode};
use scaena_core::displaylist::Rect;
use scaena_core::lint::{Finding, Severity};
use scaena_core::model::nodes::TextFit;
use scaena_core::pose;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// How far two boxes may overlap before they collide, each way, canvas units: room for a
/// hung quotation mark or a descender's tail.
const SLACK: f32 = 2.0;

fn overlap(a: Rect, b: Rect) -> Option<[f32; 2]> {
    let w = (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
    let h = (a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1]);
    (w > SLACK && h > SLACK).then_some([w, h])
}

/// A box as laid out, and the map that draws it where a transform moves it.
type Drawn = (Rect, Option<[f64; 6]>);

/// A box of `scene`'s node `id`, where it is drawn.
fn drawn(scene: &Scene, id: &str, rect: Rect) -> Drawn {
    let map = scene.posed(id, true);
    (rect, (map != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]).then_some(map))
}

/// How far two boxes overlap as they are drawn: as [`overlap`] where nothing moves either,
/// else the width and height of what their drawn outlines share.
fn meets(a: Drawn, b: Drawn) -> Option<[f32; 2]> {
    if let ((a, None), (b, None)) = (a, b) {
        return overlap(a, b);
    }
    // A node flattened to a line or a point draws nothing to collide with.
    if [a.1, b.1].iter().flatten().any(|map| pose::invert(map).is_none()) {
        return None;
    }
    let outline = |(rect, map): Drawn| pose::corners(&map.unwrap_or(IDENTITY), rect).to_vec();
    let shared = clip(outline(a), &outline(b));
    let (x0, x1) = shared.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, r), p| (l.min(p[0]), r.max(p[0])));
    let (y0, y1) = shared.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(t, b), p| (t.min(p[1]), b.max(p[1])));
    let [w, h] = [(x1 - x0) as f32, (y1 - y0) as f32];
    (shared.len() >= 3 && w > SLACK && h > SLACK).then_some([w, h])
}

const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// What of the convex outline `subject` lies inside the convex outline `by` (Sutherland and
/// Hodgman): each of `by`'s sides cuts away what lies outside it. Either may run either way
/// round.
fn clip(subject: Vec<[f64; 2]>, by: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let cross = |o: [f64; 2], a: [f64; 2], p: [f64; 2]| (a[0] - o[0]) * (p[1] - o[1]) - (a[1] - o[1]) * (p[0] - o[0]);
    // Which side is inside: the side the outline's own points lie on.
    let area: f64 = (0..by.len()).map(|i| cross([0.0, 0.0], by[i], by[(i + 1) % by.len()])).sum();
    let inside = |o, a, p| if area >= 0.0 { cross(o, a, p) >= 0.0 } else { cross(o, a, p) <= 0.0 };
    let mut out = subject;
    for i in 0..by.len() {
        let (o, a) = (by[i], by[(i + 1) % by.len()]);
        let points = std::mem::take(&mut out);
        for (k, &p) in points.iter().enumerate() {
            let q = points[(k + 1) % points.len()];
            let (pin, qin) = (inside(o, a, p), inside(o, a, q));
            if pin {
                out.push(p);
            }
            if pin != qin {
                // Where `p` to `q` crosses the side's line.
                let (dp, dq) = (cross(o, a, p), cross(o, a, q));
                let t = dp / (dp - dq);
                out.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

/// Where two nodes' paint paths part: the first entry they differ in.
fn parting(a: &SceneNode, b: &SceneNode) -> usize {
    a.paint.iter().zip(&b.paint).take_while(|(x, y)| x == y).count()
}

/// Whether two nodes stack at the same `z` where their paint paths part: in one container,
/// their own; in two, those of the containers (or the node) the paths reach first apart.
fn one_level(a: &SceneNode, b: &SceneNode) -> bool {
    let at = parting(a, b);
    matches!((a.paint.get(at), b.paint.get(at)), (Some(x), Some(y)) if x.0 == y.0)
}

pub struct E101Collision;

/// Two nodes that collide: the state they first do, by how much, and every state they do.
struct Pair {
    first: usize,
    overlap: [f32; 2],
    states: Vec<String>,
}
impl Rule for E101Collision {
    fn code(&self) -> &'static str {
        "E101"
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        // Each pair once: where it first collides, and every state it collides in, with
        // what stacks for each where their paint paths part: the node, or a container it is in.
        let mut pairs: BTreeMap<(String, String), (Pair, [String; 2])> = BTreeMap::new();
        for state in cx.states {
            let content: Vec<(&SceneNode, Drawn)> = state
                .scene
                .nodes
                .iter()
                .filter(|n| {
                    let props = state.snapshot.nodes.get(&n.id);
                    props.and_then(|p| p.get("semantic")).and_then(Value::as_str) != Some("decoration")
                })
                .filter_map(|n| Some((n, drawn(&state.scene, &n.id, n.ink()?))))
                .collect();
            for (i, (a, ra)) in content.iter().enumerate() {
                for (b, rb) in &content[i + 1..] {
                    if !one_level(a, b) {
                        continue;
                    }
                    let Some(by) = meets(*ra, *rb) else { continue };
                    let (a, b) = if a.id < b.id { (a, b) } else { (b, a) };
                    let at = parting(a, b);
                    let stacks =
                        |n: &SceneNode| cx.deck.nodes.get_index(n.paint[at].1).map_or(&n.id, |(id, _)| id).clone();
                    let (pair, _) = pairs.entry((a.id.clone(), b.id.clone())).or_insert_with(|| {
                        (Pair { first: state.index, overlap: by, states: Vec::new() }, [stacks(a), stacks(b)])
                    });
                    pair.states.push(state.snapshot.state_id.clone());
                }
            }
        }
        pairs
            .into_iter()
            .map(|((a, b), (Pair { first, overlap: [w, h], states }, [sa, sb]))| {
                let state = cx.laid(first);
                // In two containers, what has the same `z` is what stacks where they part.
                let level = |node: &str, stacks: &str| {
                    if node == stacks { format!("`{node}`") } else { format!("`{stacks}`, which `{node}` is in,") }
                };
                let at = if (sa.as_str(), sb.as_str()) == (a.as_str(), b.as_str()) {
                    " at the same `z`".to_string()
                } else {
                    format!(", and nothing puts one over the other: {} has the same `z` as {}", level(&a, &sa), level(&b, &sb))
                        .trim_end_matches(',')
                        .to_string()
                };
                cx.finding(
                    self.code(),
                    self.severity(),
                    state,
                    format!("`{a}` and `{b}` overlap by {w:.0} × {h:.0} cu{at}"),
                )
                .at(cx.node_path(&b))
                .node(b.clone())
                .measure(json!({ "nodes": [a, b], "overlap": [w, h], "states": states }))
                .hint("Move one to another slot or cell; if one is meant to lie over the other, give it, or the container it is in, a higher `z`, or mark a decoration `semantic: decoration`.")
            })
            .collect()
    }
}

/// W303: a node that draws content where the slide's number or footer stands (ADR-0028), which
/// is drawn over it. One finding per node and piece of furniture, at the node, naming every state;
/// a decoration lies anywhere.
pub struct W303UnderFurniture;

impl Rule for W303UnderFurniture {
    fn code(&self) -> &'static str {
        "W303"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let mut pairs: BTreeMap<(String, String), Pair> = BTreeMap::new();
        for state in cx.states {
            for item in &state.scene.furniture {
                let Some(stands) = item.ink() else { continue };
                for node in &state.scene.nodes {
                    let props = state.snapshot.nodes.get(&node.id);
                    if props.and_then(|p| p.get("semantic")).and_then(Value::as_str) == Some("decoration") {
                        continue;
                    }
                    let Some(ink) = node.ink() else { continue };
                    let Some(by) = meets((stands, None), drawn(&state.scene, &node.id, ink)) else { continue };
                    let pair = pairs.entry((node.id.clone(), item.id.clone())).or_insert_with(|| Pair {
                        first: state.index,
                        overlap: by,
                        states: Vec::new(),
                    });
                    pair.states.push(state.snapshot.state_id.clone());
                }
            }
        }
        pairs
            .into_iter()
            .map(|((node, item), Pair { first, overlap: [w, h], states })| {
                let what = if item == crate::furniture::NUMBER { "number" } else { "footer" };
                cx.finding(
                    self.code(),
                    self.severity(),
                    cx.laid(first),
                    format!("`{node}` lies under the slide's {what} by {w:.0} × {h:.0} cu"),
                )
                .at(cx.node_path(&node))
                .node(node)
                .measure(json!({ "furniture": what, "overlap": [w, h], "states": states }))
                .hint("Move it clear of the margin the slide's number and footer stand in, or leave them off this slide (`furniture: false` on its first state).")
            })
            .collect()
    }
}

/// W304: a place a slide's layout keeps for a picture or a figure, left empty where the slide
/// comes to rest (its last state): its slot's prompt waits for one (PLAN 3.30), no node is placed
/// in it, no ink in front, decoration aside, is drawn in it, and no picture or backdrop is drawn
/// there: what an editor outlines as a place still waiting, with nothing else standing in it. One
/// finding per slide and place.
pub struct W304EmptyPlace;

impl Rule for W304EmptyPlace {
    fn code(&self) -> &'static str {
        "W304"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        use scaena_core::model::theme::{Example, Prompt};
        let mut boxes: BTreeMap<String, Vec<crate::guides::SlotBox>> = BTreeMap::new();
        let mut out = Vec::new();
        for state in cx.states {
            let at = &cx.deck.states[state.index];
            // At rest: the slide's last state, as the light table and the PDF show it.
            let next = cx.deck.states.get(state.index + 1);
            if next.is_some_and(|n| cx.deck.slide_of(n) == cx.deck.slide_of(at)) {
                continue;
            }
            let Some(name) = state.snapshot.layout.as_deref() else { continue };
            let Some(slots) = cx.theme.slots(name) else { continue };
            if !boxes.contains_key(name) {
                let Ok(laid) = crate::guides::slots(cx.deck, cx.theme, name, cx.format) else { continue };
                boxes.insert(name.to_string(), laid);
            }
            let placed: Vec<&str> = (state.snapshot.nodes.values())
                .filter_map(|p| p.get("at").filter(|a| a.get("parent").is_none())?.get("in")?.as_str())
                .collect();
            // What fills a place: ink in front drawn in it, decoration aside, or a picture or a
            // backdrop drawn there at all, as "a picture, or a backdrop" asks.
            let stands = |id: &str| {
                let pictured = (cx.deck.nodes.get(id)).is_some_and(|n| {
                    matches!(
                        n.node_type,
                        scaena_core::document::NodeType::Image | scaena_core::document::NodeType::Shader
                    )
                });
                pictured
                    || state.snapshot.nodes.get(id).is_some_and(|p| {
                        p.get("z").and_then(Value::as_f64).unwrap_or(0.0) >= 0.0
                            && p.get("semantic").and_then(Value::as_str) != Some("decoration")
                    })
            };
            // A slide that still says its slots' prompts, or shows nothing of its own but the cards
            // and rules it came with, is being written: its places wait with its words, as Add
            // Slide leaves them.
            let prompted = |p: &scaena_core::document::Props| {
                let slot = p.get("at").filter(|a| a.get("parent").is_none()).and_then(|a| a.get("in")?.as_str());
                let said = slot.and_then(|s| slots.get(s)?.prompt.as_ref()?.text()).map(|(text, _)| text);
                said.is_some_and(|said| p.get("text").and_then(Value::as_str) == Some(said.as_str()))
            };
            let decoration =
                |p: &scaena_core::document::Props| p.get("semantic").and_then(Value::as_str) == Some("decoration");
            let nodes = || state.snapshot.nodes.values();
            if nodes().any(prompted) || nodes().all(decoration) {
                continue;
            }
            for b in &boxes[name] {
                let Some(slot) = slots.get(&b.name) else { continue };
                // Words go where a role is; a card or a rule comes with the slide.
                if slot.role.is_some() || slot.prompt.as_ref().and_then(Prompt::text).is_none() {
                    continue;
                }
                let covered = (state.scene.nodes.iter()).any(|n| {
                    // Where it draws: a text's lines, else its box, a backdrop's and a card's too.
                    let area = n.ink().or(match &n.content {
                        Content::Shader(shader) => Some(shader.rect),
                        Content::Shape(shape) => Some(shape.rect),
                        _ => None,
                    });
                    stands(&n.id)
                        && area.is_some_and(|r| meets((b.rect, None), drawn(&state.scene, &n.id, r)).is_some())
                });
                if placed.contains(&b.name.as_str()) || covered {
                    continue;
                }
                let figure = matches!(slot.example, Some(Example::Chart(_) | Example::Table(_)));
                let what = if figure { "a figure" } else { "a picture" };
                out.push(
                    cx.finding(
                        self.code(),
                        self.severity(),
                        state,
                        format!("slot `{}` of layout `{name}` waits for {what}, and nothing fills it", b.name),
                    )
                    .at(format!("/states/{}", state.index))
                    .measure(json!({ "slot": b.name, "layout": name, "rect": b.rect, "figure": figure }))
                    .hint("Choose a layout that waits for none, or put a picture or a figure in its place."),
                );
            }
        }
        out
    }
}

/// W311: a shader painted behind a chart or a table, where they overlap. Data reads
/// against a plain surface; a mesh, noise, or particles under it read as noise in the data
/// (SPEC §3.8). One finding per shader and data node, at the shader, naming every state.
pub struct W311ShaderBehindData;

impl Rule for W311ShaderBehindData {
    fn code(&self) -> &'static str {
        "W311"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        // Each shader and the chart or table it is under: which of the two that is, and where.
        let mut pairs: BTreeMap<(String, String), (&str, Pair)> = BTreeMap::new();
        for state in cx.states {
            // Scene nodes come in paint order: a shader before a chart is under it.
            let mut shaders: Vec<(&SceneNode, Drawn)> = Vec::new();
            for node in &state.scene.nodes {
                let (what, cell) = match &node.content {
                    Content::Shader(s) => {
                        if node.opacity > 0.0 {
                            shaders.push((node, drawn(&state.scene, &node.id, s.rect)));
                        }
                        continue;
                    }
                    Content::Chart { cell, .. } => ("chart", *cell),
                    Content::Table { cell, .. } => ("table", *cell),
                    _ => continue,
                };
                let cell = drawn(&state.scene, &node.id, cell);
                for (shader, rect) in &shaders {
                    let Some(by) = meets(*rect, cell) else { continue };
                    let key = (shader.id.clone(), node.id.clone());
                    let first = Pair { first: state.index, overlap: by, states: Vec::new() };
                    let (_, pair) = pairs.entry(key).or_insert((what, first));
                    pair.states.push(state.snapshot.state_id.clone());
                }
            }
        }
        pairs
            .into_iter()
            .map(|((shader, data), (what, Pair { first, overlap: [w, h], states }))| {
                let state = cx.laid(first);
                cx.finding(
                    self.code(),
                    self.severity(),
                    state,
                    format!("shader `{shader}` is painted behind {what} `{data}`, over {w:.0} × {h:.0} cu of it"),
                )
                .at(cx.node_path(&shader))
                .node(shader.clone())
                .measure(json!({ "shader": shader, "data": data, "overlap": [w, h], "states": states }))
                .hint("Keep shaders off data slides: drop the shader from these states, or keep it to where no chart or table is.")
            })
            .collect()
    }
}

/// The least a chart's plot may measure across or down and still be read, canvas units,
/// where the canvas's shorter side is 1080 cu: about five lines of 12 pt chart text.
const LEGIBLE_PLOT: f32 = 120.0;

/// W313: a chart squashed below a legible plot: its plot, the room its marks have once its
/// axes, labels, titles, and legend have theirs, under `LEGIBLE_PLOT` across or down at
/// presentation size, as a short cell in a theme's grid leaves it. One finding per chart,
/// at its smallest, naming every state it is squashed in.
pub struct W313ChartSquashed;

/// A squashed chart: its plot at its smallest, its cell then, and where.
struct Squashed {
    plot: [f32; 2],
    cell: [f32; 2],
    first: usize,
    states: Vec<String>,
}

impl Rule for W313ChartSquashed {
    fn code(&self) -> &'static str {
        "W313"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let floor = LEGIBLE_PLOT * cx.deck.canvas.width.min(cx.deck.canvas.height) as f32 / 1080.0;
        let mut squashed: BTreeMap<String, Squashed> = BTreeMap::new();
        for state in cx.states {
            for node in &state.scene.nodes {
                let Content::Chart { cell, chart } = &node.content else { continue };
                // As drawn: a chart scaled down squashes its plot; one flattened draws none.
                // Small multiples by their smallest panel's (PLAN 1.31).
                let [along, across] = pose::stretch(&state.scene.posed(&node.id, true)).map(|k| k as f32);
                let plot = (chart.each(*cell).into_iter())
                    .map(|(.., c)| [c.plot[2], c.plot[3]])
                    .min_by(|a, b| a[0].min(a[1]).total_cmp(&b[0].min(b[1])))
                    .unwrap_or([chart.plot[2], chart.plot[3]]);
                let [w, h] = [plot[0] * along, plot[1] * across];
                if w.min(h) >= floor || w.min(h) <= 0.0 || node.opacity <= 0.0 {
                    continue;
                }
                let entry = squashed.entry(node.id.clone()).or_insert_with(|| Squashed {
                    plot: [w, h],
                    cell: [cell[2], cell[3]],
                    first: state.index,
                    states: Vec::new(),
                });
                if w.min(h) < entry.plot[0].min(entry.plot[1]) {
                    (entry.plot, entry.cell, entry.first) = ([w, h], [cell[2], cell[3]], state.index);
                }
                entry.states.push(state.snapshot.state_id.clone());
            }
        }
        squashed
            .into_iter()
            .map(|(chart, Squashed { plot: [w, h], cell: [cw, ch], first, states })| {
                let (way, short) = if h <= w { ("down", h) } else { ("across", w) };
                cx.finding(
                    self.code(),
                    self.severity(),
                    cx.laid(first),
                    format!(
                        "chart `{chart}` has {short:.0} cu {way} to plot in, in a {cw:.0} × {ch:.0} cu cell: under {floor:.0} \
                         cu, its marks are too squashed to read"
                    ),
                )
                .at(cx.node_path(&chart))
                .node(chart.clone())
                .measure(json!({ "plot": [w, h], "cell": [cw, ch], "needs": floor, "states": states }))
                .hint("Give the chart more room: a taller slot or more rows of the grid, or less beside it in its cell (a legend at the side, a title).")
            })
            .collect()
    }
}

/// W314: a fetched picture's credit that cannot be read (ADR-0030): set under 12 pt at
/// presentation size, cut by its box or the canvas, covered by a node drawn over it, decoration
/// included, or covering more than a quarter of what shows of its picture. One finding per image,
/// at the image, naming every state; its fix, where the credit stands inside the picture, sets it
/// below (`credit: below`), which lint keeps only where it reads there.
pub struct W314Credit;

impl Rule for W314Credit {
    fn code(&self) -> &'static str {
        "W314"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        // Each image's first reason, the state it is first found in, and every state.
        let mut found: BTreeMap<String, (usize, String, bool, Vec<String>)> = BTreeMap::new();
        let short = cx.deck.canvas.width.min(cx.deck.canvas.height) as f32;
        let canvas = [0.0, 0.0, cx.deck.canvas.width as f32, cx.deck.canvas.height as f32];
        for state in cx.states {
            for (k, node) in state.scene.nodes.iter().enumerate() {
                // The credit, box-local; its words; the box; and what shows of a picture.
                let (rect, text, cell, inside, shown) = match &node.content {
                    Content::Image(image) => {
                        let Some(credit) = &image.marks.credit else { continue };
                        (credit.rect, &credit.text, image.rect, credit.fill.is_some(), image.shown())
                    }
                    // A chart's or a table's data's credit, at its foot (ADR-0030).
                    Content::Chart { cell, chart } => {
                        let Some(foot) = &chart.credit else { continue };
                        let rect = [foot.origin[0], foot.origin[1], foot.text.width, foot.text.height];
                        (rect, &foot.text, *cell, false, [0.0, 0.0, cell[2], cell[3]])
                    }
                    Content::Table { cell, table } => {
                        let Some(foot) = &table.credit else { continue };
                        let rect = [foot.origin[0], foot.origin[1], foot.text.width, foot.text.height];
                        (rect, &foot.text, *cell, false, [0.0, 0.0, cell[2], cell[3]])
                    }
                    _ => continue,
                };
                if node.opacity <= 0.0 {
                    continue;
                }
                let [x, y, _, _] = cell;
                let card = [rect[0] + x, rect[1] + y, rect[2], rect[3]];
                let map = state.scene.posed(&node.id, true);
                let scale = pose::stretch(&map)[1] as f32;
                let size = text.runs.iter().map(|r| r.size).fold(f32::INFINITY, f32::min) * scale;
                let drawn_card = pose::bounds(&map, card);
                let words = text.lines.iter().map(|l| l.width).fold(0.0f32, f32::max);
                let why = if size.is_finite() && size < crate::render::MIN_CREDIT * short / 1080.0 - 0.01 {
                    Some(format!("is set at {:.0} cu, under 12 pt at presentation size", size))
                } else if !contains(canvas, drawn_card) {
                    Some("runs off the canvas".to_string())
                } else if !contains([0.0, 0.0, cell[2], cell[3]], rect)
                    || (inside && !contains(shown, rect))
                    || words > text.measure + 0.5
                {
                    Some("is cut by its box".to_string())
                } else if let Some(over) =
                    state.scene.nodes[k + 1..].iter().chain(&state.scene.furniture).filter(|n| n.opacity > 0.0).find(
                        |n| {
                            drawn_extent(n)
                                .is_some_and(|r| meets(drawn(&state.scene, &n.id, r), (card, Some(map))).is_some())
                        },
                    )
                {
                    Some(format!("is covered by `{}`", over.id))
                } else if inside && rect[2] * rect[3] > 0.25 * shown[2] * shown[3] {
                    Some("covers more than a quarter of the picture".to_string())
                } else {
                    None
                };
                let Some(why) = why else { continue };
                let entry = found.entry(node.id.clone()).or_insert_with(|| (state.index, why, inside, Vec::new()));
                entry.3.push(state.snapshot.state_id.clone());
            }
        }
        found
            .into_iter()
            .map(|(node, (first, why, inside, states))| {
                let f = cx
                    .finding(self.code(), self.severity(), cx.laid(first), format!("the credit on `{node}` {why}"))
                    .at(cx.node_path(&node))
                    .node(node.clone())
                    .measure(json!({ "states": states }))
                    .hint("Set the credit below the picture (`credit: below`), make it larger, or move what lies over it.");
                if inside { f.fix(vec![json!({ "op": "add", "path": prop_path(cx, first, &node, "credit"), "value": "below" })]) } else { f }
            })
            .collect()
    }
}

/// I405: a text set larger than the body whose descenders take its ink within the theme's
/// gutter of a text under it, on one level, that its baseline stands two gutters or more above: a
/// big number's tail over its caption, never a title's over its subtitle or its paragraph. What collides is E101's; this is the room short of it. One
/// finding per pair, at the text above, naming every state. Its fix is the step the generator
/// takes ([`step`]): a twentieth smaller, as `fit: shrink` under a `maxSize`, stepped again by
/// `super::verify` until the text comes clear, never under half its size.
pub struct I405Crowded;

/// A text's last line as set: its baseline and its ink's foot, canvas y, its size as set and its
/// own, and its ink.
struct Foot {
    baseline: f32,
    ink: Rect,
    size: f32,
    own: f32,
}

/// A text crowding the one under it: the state it first does, by how little room, how far its tail
/// reaches below its baseline, its size there as set and its own, how it fits, and every state it
/// does.
struct Crowd {
    first: usize,
    gap: f32,
    descent: f32,
    size: f32,
    own: f32,
    fit: TextFit,
    states: Vec<String>,
}

fn foot(placed: &PlacedText, ink: Rect) -> Option<Foot> {
    let t = &placed.text;
    let last = t.lines.len().checked_sub(1)?;
    let size = t.runs.iter().filter(|r| r.line == last).map(|r| r.size).fold(0.0, f32::max);
    (size > 0.0).then(|| Foot {
        baseline: placed.origin[1] + t.lines[last].baseline,
        ink,
        size,
        own: size / placed.scale.max(f32::EPSILON),
    })
}

impl Rule for I405Crowded {
    fn code(&self) -> &'static str {
        "I405"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let Some(body) = cx.theme.typography.roles.get("body").map(|r| r.size as f32) else { return Vec::new() };
        let gutter = cx.theme.grid.gutter as f32;
        // Each pair once: where it first crowds, by how little room, its size there, every state.
        let mut pairs: BTreeMap<(String, String), Crowd> = BTreeMap::new();
        for state in cx.states {
            let texts: Vec<(&SceneNode, &PlacedText, Rect)> = (state.scene.nodes.iter())
                .filter(|n| {
                    let props = state.snapshot.nodes.get(&n.id);
                    props.and_then(|p| p.get("semantic")).and_then(Value::as_str) != Some("decoration")
                })
                .filter(|n| state.scene.posed(&n.id, true) == IDENTITY)
                .filter_map(|n| match &n.content {
                    Content::Text(placed) => Some((n, placed, n.ink()?)),
                    _ => None,
                })
                .collect();
            for (a, placed, ink) in &texts {
                let Some(f) = foot(placed, *ink) else { continue };
                let bottom = f.ink[1] + f.ink[3];
                if f.size <= body || bottom - f.baseline <= SLACK {
                    continue;
                }
                for (b, _, under) in &texts {
                    if a.id == b.id || !one_level(a, b) {
                        continue;
                    }
                    let across = (f.ink[0] + f.ink[2]).min(under[0] + under[2]) - f.ink[0].max(under[0]);
                    let gap = under[1] - bottom;
                    // Two gutters or more under its baseline, a hero's room, its descenders within one
                    // of it, and not across it: what collides is E101's. A title's tail over its
                    // subtitle, or a figure's over its name, a gutter and a half under, is the theme's.
                    if across <= SLACK || under[1] - f.baseline < 2.0 * gutter || gap >= gutter || gap < -SLACK {
                        continue;
                    }
                    let crowd = pairs.entry((a.id.clone(), b.id.clone())).or_insert_with(|| Crowd {
                        first: state.index,
                        gap,
                        descent: bottom - f.baseline,
                        size: f.size,
                        own: f.own,
                        fit: placed.fit,
                        states: Vec::new(),
                    });
                    crowd.states.push(state.snapshot.state_id.clone());
                }
            }
        }
        pairs
            .into_iter()
            .map(|((a, b), Crowd { first, gap, descent, size, own, fit, states })| {
                let state = cx.laid(first);
                // The size that leaves the gutter where its baseline stays as it is set smaller, its
                // tail shortening with it: a text set on its foot, as a slot's figure is, which the
                // generator steps to at once. One set from its top rises too, and needs less.
                let aim = size * (gap + descent - gutter) / descent;
                let f = cx
                    .finding(
                        self.code(),
                        self.severity(),
                        state,
                        format!(
                            "`{a}`'s descenders come {:.0} cu above `{b}`, under the theme's gutter of {gutter:.0}",
                            gap.max(0.0)
                        ),
                    )
                    .at(cx.node_path(&a))
                    .node(a.clone())
                    .measure(json!({
                        "nodes": [a, b], "gap": gap, "gutter": gutter, "size": size, "aim": aim, "own": own, "states": states
                    }))
                    .hint("Set it smaller, or give the text under it more room.");
                match step(cx, first, &a, size, own, fit) {
                    Some(fix) => f.fix(fix),
                    None => f,
                }
            })
            .collect()
    }
}

/// The generator's step for text `id` set at `size` in state `index`, its own size `own`: a
/// twentieth smaller, on a quarter of a unit, as `fit: shrink` under a `maxSize`, or under a
/// `maxSize` alone where it grows and stays at its own size or above. None under half its own.
fn step(cx: &Cx, index: usize, id: &str, size: f32, own: f32, fit: TextFit) -> Option<Vec<Value>> {
    let next = (f64::from(size) * 0.95 * 4.0).floor() / 4.0;
    if next < f64::from(own) * 0.5 {
        return None;
    }
    let mut ops = vec![json!({ "op": "add", "path": prop_path(cx, index, id, "maxSize"), "value": next })];
    if !(fit == TextFit::Shrink || fit == TextFit::Grow && next >= f64::from(own)) {
        ops.insert(0, json!({ "op": "add", "path": super::text::fit_path(cx, index, id), "value": "shrink" }));
    }
    Some(ops)
}

/// Whether `outer` holds `inner`, within half a unit.
fn contains(outer: Rect, inner: Rect) -> bool {
    inner[0] >= outer[0] - 0.5
        && inner[1] >= outer[1] - 0.5
        && inner[0] + inner[2] <= outer[0] + outer[2] + 0.5
        && inner[1] + inner[3] <= outer[1] + outer[3] + 0.5
}

/// What a node draws over, as laid out: its ink, or a shape's or a shader's box.
fn drawn_extent(node: &SceneNode) -> Option<Rect> {
    match &node.content {
        Content::Shape(s) => Some(s.rect),
        Content::Shader(s) => Some(s.rect),
        _ => node.ink(),
    }
}

/// Where a fix to node `id`'s `key` goes in state `index`: where it is set now (its overrides,
/// else the latest state delta that sets it), else its defaults.
fn prop_path(cx: &Cx, index: usize, id: &str, key: &str) -> String {
    let token = id.replace('~', "~0").replace('/', "~1");
    if cx.deck.overrides.get(id).is_some_and(|o| o.contains_key(key)) {
        return format!("/overrides/{token}/{key}");
    }
    for j in (0..=index).rev() {
        if cx.deck.states[j].props.get(id).is_some_and(|p| p.contains_key(key)) {
            return format!("/states/{j}/props/{token}/{key}");
        }
    }
    format!("/nodes/{token}/{key}")
}
