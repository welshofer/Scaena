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
//! W311: a shader painted behind a chart or a table, where they overlap.
//!
//! W313: a chart squashed below a legible plot.

use super::{Cx, Rule};
use crate::sample::{Content, Scene, SceneNode};
use scaena_core::displaylist::Rect;
use scaena_core::lint::{Finding, Severity};
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
                let [along, across] = pose::stretch(&state.scene.posed(&node.id, true)).map(|k| k as f32);
                let [w, h] = [chart.plot[2] * along, chart.plot[3] * across];
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
