//! Text as laid out: whether it fits (E100, W202, W203), whether its fonts have its
//! glyphs (E120), how its lines break (W200, W201), how a state aligns it (W220); and a
//! chart's value labels (W310).

use super::{Cx, Laid, Rule};
use crate::layout::Grid;
use crate::sample::Content;
use crate::text::{TextAlign, TextLayout};
use scaena_core::document::Props;
use scaena_core::lint::{Finding, Severity};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Each text node a state draws, with its props.
fn texts<'a>(state: &'a Laid) -> impl Iterator<Item = (&'a str, &'a crate::render::PlacedText, &'a Props)> + 'a {
    state.scene.nodes.iter().filter_map(move |n| match &n.content {
        Content::Text(placed) => Some((n.id.as_str(), placed, state.snapshot.nodes.get(&n.id)?)),
        _ => None,
    })
}

fn fit(props: &Props) -> &str {
    props.get("fit").and_then(Value::as_str).unwrap_or("wrap")
}

/// Where a fix to node `id`'s `fit` goes in state `index`: where its `fit` is set now
/// (its overrides, else the latest state delta that sets it), else its defaults.
pub(super) fn fit_path(cx: &Cx, index: usize, id: &str) -> String {
    let token = id.replace('~', "~0").replace('/', "~1");
    if cx.deck.overrides.get(id).is_some_and(|o| o.contains_key("fit")) {
        return format!("/overrides/{token}/fit");
    }
    for j in (0..=index).rev() {
        if cx.deck.states[j].props.get(id).is_some_and(|p| p.contains_key("fit")) {
            return format!("/states/{j}/props/{token}/fit");
        }
    }
    format!("/nodes/{token}/fit")
}

/// A fix that sets the text at the largest size that fits (`fit: shrink`); lint keeps
/// it only once laying the state out with it shows that it does (`super::verify`).
fn shrink(cx: &Cx, index: usize, id: &str) -> Vec<Value> {
    vec![json!({ "op": "add", "path": fit_path(cx, index, id), "value": "shrink" })]
}

/// E100: text that does not fit its box, under `wrap`, `clip`, `grow`, or `error`; and
/// a table whose rows do not fit its cell.
pub struct E100Overflow;
impl Rule for E100Overflow {
    fn code(&self) -> &'static str {
        "E100"
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let mut out = Vec::new();
        for state in cx.states {
            for (id, placed, props) in texts(state) {
                if !placed.overflow || fit(props) == "shrink" {
                    continue;
                }
                let [_, _, w, h] = placed.cell;
                let t = &placed.text;
                let (tall, wide) = (t.height > h + 0.5, t.width > w + 0.5);
                let how = match (tall, wide) {
                    (true, true) => format!("is {:.0} × {:.0} cu, and its box {:.0} × {:.0}", t.width, t.height, w, h),
                    (true, _) => format!(
                        "needs {:.0} cu of height in {} lines, and its box has {:.0}",
                        t.height,
                        t.lines.len(),
                        h
                    ),
                    _ => format!("has a line {:.0} cu wide, and its box is {:.0}", t.width, w),
                };
                let refused = if fit(props) == "error" { " (`fit: error`: a frame refuses to draw it)" } else { "" };
                out.push(
                    cx.finding(self.code(), self.severity(), state, format!("text `{id}` does not fit: it {how}{refused}"))
                        .at(cx.node_path(id))
                        .node(id)
                        .measure(json!({ "lines": t.lines.len(), "height": t.height, "width": t.width, "box": [w, h] }))
                        .hint("Shorten it, give it a larger slot, or let it shrink (`fit: shrink`, down to its role's `minSize`).")
                        .fix(shrink(cx, state.index, id)),
                );
            }
            for node in &state.scene.nodes {
                if let Content::Table { table, .. } = &node.content
                    && let Some(why) = &table.overflow
                {
                    out.push(
                        cx.finding(
                            self.code(),
                            self.severity(),
                            state,
                            format!("table `{}` does not fit: {why}", node.id),
                        )
                        .at(cx.node_path(&node.id))
                        .node(node.id.clone()),
                    );
                }
            }
        }
        out
    }
}

/// W203: text under `fit: shrink` that does not fit at its smallest size.
pub struct W203ShrinkFloor;
impl Rule for W203ShrinkFloor {
    fn code(&self) -> &'static str {
        "W203"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let mut out = Vec::new();
        for state in cx.states {
            for (id, placed, props) in texts(state) {
                if placed.overflow && fit(props) == "shrink" {
                    out.push(
                        cx.finding(
                            self.code(),
                            self.severity(),
                            state,
                            format!(
                                "text `{id}` shrank to {:.0}% of its size, its `minSize`, and still does not fit",
                                placed.scale * 100.0
                            ),
                        )
                        .at(cx.node_path(id))
                        .node(id)
                        .measure(json!({ "scale": placed.scale, "height": placed.text.height, "box": [placed.cell[2], placed.cell[3]] }))
                        .hint("Shorten it, or give it a larger slot."),
                    );
                }
            }
        }
        out
    }
}

/// The most lines a text node takes: its own `maxLines`, else its role's.
fn max_lines(cx: &Cx, state: &Laid, props: &Props) -> Option<usize> {
    if let Some(n) = props.get("maxLines").and_then(Value::as_u64) {
        return Some(n as usize);
    }
    let slot = Grid::slot_role(cx.theme, state.snapshot.layout.as_deref(), props.get("at"));
    let role = props.get("role").and_then(Value::as_str).or(slot.as_deref())?;
    cx.theme.text_role(role).ok()?.max_lines.map(|n| n as usize)
}

/// W202: text with more lines than its `maxLines`.
pub struct W202MaxLines;
impl Rule for W202MaxLines {
    fn code(&self) -> &'static str {
        "W202"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let mut out = Vec::new();
        for state in cx.states {
            for (id, placed, props) in texts(state) {
                let Some(max) = max_lines(cx, state, props) else { continue };
                let n = placed.text.lines.len();
                if n <= max {
                    continue;
                }
                let mut f = cx
                    .finding(
                        self.code(),
                        self.severity(),
                        state,
                        format!("text `{id}` sets in {n} lines, and its `maxLines` is {max}"),
                    )
                    .at(cx.node_path(id))
                    .node(id)
                    .measure(json!({ "lines": n, "maxLines": max }))
                    .hint("Shorten it, or let it shrink (`fit: shrink`) to fit its lines.");
                if fit(props) != "shrink" {
                    f = f.fix(shrink(cx, state.index, id));
                }
                out.push(f);
            }
        }
        out
    }
}

/// Every text a node sets: a text node's, a table's cells, a chart's labels.
fn layouts(content: &Content) -> Vec<&TextLayout> {
    match content {
        Content::Text(placed) => vec![&placed.text],
        Content::Table { table, .. } => table.header.iter().chain(&table.cells).map(|c| &c.text).collect(),
        Content::Chart { chart, .. } => chart
            .labels
            .iter()
            .chain(&chart.ticks)
            .chain(&chart.titles)
            .chain(chart.legend.iter().map(|e| &e.label))
            .chain(chart.y_axis.iter().filter_map(|t| t.label.as_ref()))
            .map(|l| &l.text)
            .collect(),
        _ => vec![],
    }
}

/// E120: characters a node sets that its family and its fallbacks have no glyph for
/// (glyph 0, `.notdef`): they draw as boxes.
pub struct E120MissingGlyphs;
impl Rule for E120MissingGlyphs {
    fn code(&self) -> &'static str {
        "E120"
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let mut out = Vec::new();
        for state in cx.states {
            for node in &state.scene.nodes {
                let mut missing: Vec<char> = Vec::new();
                for t in layouts(&node.content) {
                    for run in &t.runs {
                        for (g, &cluster) in run.glyphs.iter().zip(&run.clusters) {
                            if g.id == 0
                                && let Some(c) = t.text.get(cluster..).and_then(|s| s.chars().next())
                                && !missing.contains(&c)
                            {
                                missing.push(c);
                            }
                        }
                    }
                }
                if missing.is_empty() {
                    continue;
                }
                let chars: Vec<String> = missing.iter().map(|c| format!("`{c}` (U+{:04X})", *c as u32)).collect();
                out.push(
                    cx.finding(
                        self.code(),
                        self.severity(),
                        state,
                        format!("`{}` sets {}, which its fonts have no glyph for", node.id, chars.join(", ")),
                    )
                    .at(cx.node_path(&node.id))
                    .node(node.id.clone())
                    .measure(json!({ "missing": missing.iter().map(|c| c.to_string()).collect::<Vec<_>>() }))
                    .hint("Add a family that has them to the role's `fallback` (and its file to the bundle), or change the text."),
                );
            }
        }
        out
    }
}

/// W200: a paragraph's last line with fewer words than `minLastLineWords`, which
/// breaking could not give more.
pub struct W200Widow;
impl Rule for W200Widow {
    fn code(&self) -> &'static str {
        "W200"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let mut out = Vec::new();
        for state in cx.states {
            for (id, placed, _) in texts(state) {
                if !placed.text.widow {
                    continue;
                }
                let last =
                    placed.text.lines.last().map(|l| placed.text.text[l.text.clone()].trim()).unwrap_or_default();
                out.push(
                    cx.finding(self.code(), self.severity(), state, format!("text `{id}` ends on a widow: \"{last}\""))
                        .at(cx.node_path(id))
                        .node(id)
                        .hint(
                            "Rewrite the last sentence, or change the box's width, so the last line takes more words.",
                        ),
                );
            }
        }
        out
    }
}

/// W201: a line wider than its measure: a word too long to break at it. One wider than
/// its box is E100's.
pub struct W201Measure;
impl Rule for W201Measure {
    fn code(&self) -> &'static str {
        "W201"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let mut out = Vec::new();
        for state in cx.states {
            for (id, placed, _) in texts(state) {
                let t = &placed.text;
                let wide = t.lines.iter().find(|l| l.width > t.measure + 0.5 && l.width <= placed.cell[2] + 0.5);
                let Some(line) = wide else { continue };
                let text = t.text[line.text.clone()].trim();
                out.push(
                    cx.finding(
                        self.code(),
                        self.severity(),
                        state,
                        format!("text `{id}` has a line {:.0} cu wide, past its measure of {:.0}: \"{text}\"", line.width, t.measure),
                    )
                    .at(cx.node_path(id))
                    .node(id)
                    .measure(json!({ "width": line.width, "measure": t.measure }))
                    .hint("A word longer than the measure cannot break: shorten it, allow hyphenation (`hyphenate`), or widen the measure."),
                );
            }
        }
        out
    }
}

/// W220: paragraphs (text of two lines or more) aligned more than one way in a state.
pub struct W220MixedAlignment;
impl Rule for W220MixedAlignment {
    fn code(&self) -> &'static str {
        "W220"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let mut out = Vec::new();
        for state in cx.states {
            let mut by: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
            for (id, placed, _) in texts(state) {
                if placed.text.lines.len() >= 2 {
                    let align = match placed.text.align {
                        TextAlign::Start => "start",
                        TextAlign::Center => "center",
                        TextAlign::End => "end",
                    };
                    by.entry(align).or_default().push(id);
                }
            }
            if by.len() > 1 {
                let said: Vec<String> = by.iter().map(|(a, ids)| format!("{a}: {}", ids.join(", "))).collect();
                out.push(
                    cx.finding(
                        self.code(),
                        self.severity(),
                        state,
                        format!(
                            "state `{}` aligns its paragraphs more than one way ({})",
                            state.snapshot.state_id,
                            said.join("; ")
                        ),
                    )
                    .at(format!("/states/{}", state.index))
                    .measure(json!(by))
                    .hint("Align the paragraphs of a state one way; let the slots' `align` decide."),
                );
            }
        }
        out
    }
}

/// W310: chart value labels that overlap, which `labels.collide` does not resolve.
pub struct W310LabelCollision;
impl Rule for W310LabelCollision {
    fn code(&self) -> &'static str {
        "W310"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let mut out = Vec::new();
        for state in cx.states {
            for node in &state.scene.nodes {
                let Content::Chart { chart, .. } = &node.content else { continue };
                for (a, b) in &chart.covers {
                    out.push(
                        cx.finding(
                            self.code(),
                            self.severity(),
                            state,
                            format!("chart `{}`: the value label of `{a}` covers the mark of `{b}`", node.id),
                        )
                        .at(format!("{}/labels", cx.node_path(&node.id)))
                        .node(node.id.clone())
                        .measure(json!({ "marks": [a, b] }))
                        .hint("Set `labels.collide` to `hide`, show fewer labels (`labels.show`), or give the chart more room."),
                    );
                }
                for (a, b) in &chart.collisions {
                    out.push(
                        cx.finding(
                            self.code(),
                            self.severity(),
                            state,
                            format!("chart `{}`: the value labels of `{a}` and `{b}` overlap", node.id),
                        )
                        .at(format!("{}/labels", cx.node_path(&node.id)))
                        .node(node.id.clone())
                        .measure(json!({ "marks": [a, b] }))
                        .hint("Set `labels.collide` to `hide` or `nudge`, show fewer labels (`labels.show`), or give the chart more room."),
                    );
                }
            }
        }
        out
    }
}
