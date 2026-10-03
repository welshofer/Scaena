//! E101: content that collides. Two nodes collide when they draw content (text, a
//! chart, a table, an image) in the same container at the same `z`, and their boxes
//! overlap: what is meant to lie on top says so with a higher `z`, and a node marked
//! `semantic: decoration` is meant to lie anywhere. Text counts by its lines as set, not
//! its cell, so a short title in a tall slot collides only where its words are.
//!
//! W311: a shader painted behind a chart or a table, where they overlap.

use super::{Cx, Rule};
use crate::sample::{Content, SceneNode};
use scaena_core::displaylist::Rect;
use scaena_core::lint::{Finding, Severity};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// How far two boxes may overlap before they collide, each way, canvas units: room for a
/// hung quotation mark or a descender's tail.
const SLACK: f32 = 2.0;

/// What a node draws, as a box: text by its lines, the rest by its cell.
fn ink(node: &SceneNode) -> Option<Rect> {
    match &node.content {
        Content::Text(placed) => {
            let t = &placed.text;
            let left = t.lines.iter().map(|l| l.x - l.hang).fold(f32::INFINITY, f32::min);
            let right = t.lines.iter().map(|l| l.x + l.width + l.hang_end).fold(f32::NEG_INFINITY, f32::max);
            (left.is_finite() && right > left)
                .then(|| [placed.origin[0] + left, placed.origin[1], right - left, t.height])
        }
        Content::Chart { cell, .. } | Content::Table { cell, .. } => Some(*cell),
        Content::Image(image) => Some(image.rect),
        Content::Shape(_) | Content::Shader(_) => None,
    }
}

fn overlap(a: Rect, b: Rect) -> Option<[f32; 2]> {
    let w = (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
    let h = (a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1]);
    (w > SLACK && h > SLACK).then_some([w, h])
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
        // Each pair once: where it first collides, and every state it collides in.
        let mut pairs: BTreeMap<(String, String), Pair> = BTreeMap::new();
        for state in cx.states {
            let content: Vec<(&SceneNode, Rect)> = state
                .scene
                .nodes
                .iter()
                .filter(|n| {
                    let props = state.snapshot.nodes.get(&n.id);
                    props.and_then(|p| p.get("semantic")).and_then(Value::as_str) != Some("decoration")
                })
                .filter_map(|n| Some((n, ink(n)?)))
                .collect();
            let parent = |n: &SceneNode| state.scene.tree.get(&n.id).and_then(|p| p.parent.clone());
            let z = |n: &SceneNode| n.paint.last().map(|p| p.0);
            for (i, (a, ra)) in content.iter().enumerate() {
                for (b, rb) in &content[i + 1..] {
                    if parent(a) != parent(b) || z(a) != z(b) {
                        continue;
                    }
                    let Some(by) = overlap(*ra, *rb) else { continue };
                    let key = if a.id < b.id { (a.id.clone(), b.id.clone()) } else { (b.id.clone(), a.id.clone()) };
                    let pair = pairs.entry(key).or_insert(Pair { first: state.index, overlap: by, states: Vec::new() });
                    pair.states.push(state.snapshot.state_id.clone());
                }
            }
        }
        pairs
            .into_iter()
            .map(|((a, b), Pair { first, overlap: [w, h], states })| {
                let state = &cx.states[first];
                cx.finding(
                    self.code(),
                    self.severity(),
                    state,
                    format!("`{a}` and `{b}` overlap by {w:.0} × {h:.0} cu at the same `z`"),
                )
                .at(cx.node_path(&b))
                .node(b.clone())
                .measure(json!({ "nodes": [a, b], "overlap": [w, h], "states": states }))
                .hint("Move one to another slot or cell; if one is meant to lie over the other, give it a higher `z`, or mark a decoration `semantic: decoration`.")
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
            let mut shaders: Vec<(&SceneNode, Rect)> = Vec::new();
            for node in &state.scene.nodes {
                let (what, cell) = match &node.content {
                    Content::Shader(s) => {
                        if node.opacity > 0.0 {
                            shaders.push((node, s.rect));
                        }
                        continue;
                    }
                    Content::Chart { cell, .. } => ("chart", *cell),
                    Content::Table { cell, .. } => ("table", *cell),
                    _ => continue,
                };
                for (shader, rect) in &shaders {
                    let Some(by) = overlap(*rect, cell) else { continue };
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
                let state = &cx.states[first];
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
