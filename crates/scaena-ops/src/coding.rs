//! The commands that make a text code and take code away (PLAN 2.139, ADR-0029 decision 8): Make
//! Code, Not Code, and Tidy Code, each one patch, which leaves the deck valid, and one step to
//! undo.
//!
//! - **Make Code** gives a text `code`, in the language named or the one its words read as, else
//!   plain text, and the theme's code role; folds its runs into its `text`; and takes away what a
//!   code text does not take (E106): its `list`, `fit: wrap`, `wrap`, `measure`, `hyphenate`,
//!   `hangingPunctuation`, `opticalMargins`, `minLastLineWords`, and `style.case`, wherever the
//!   node's are written: its own props, each state's delta, and the deck's `overrides`.
//! - **Not Code** takes `code` away from the node, from every delta, and from the overrides, and
//!   sets the text in `body` where it was set in the code role.
//! - **Tidy Code** tidies a code text as a paste is tidied (`scaena_core::code::tidy`), one
//!   `replace_text` for each run of lines it changes, written where the text lives.

use crate::OpsError;
use scaena_core::document::{Deck, NodeType, Props};
use scaena_core::model::Theme;
use serde_json::{Value, json};

/// What a code text does not take beside its `code` (E106), but `runs`, which are folded into its
/// text, and `style.case`.
const NOT_CODE: [&str; 7] =
    ["list", "wrap", "measure", "hyphenate", "hangingPunctuation", "opticalMargins", "minLastLineWords"];

/// The patch that makes `node` code in `language`, or, where none is named, in the language its
/// words read as (`scaena_core::code::detect`), else plain text. Refused for a node that is no
/// text, or is code already.
pub fn coding(deck: &Deck, theme: &Theme, node: &str, language: Option<&str>) -> Result<Vec<Value>, OpsError> {
    let found = text_node(deck, node)?;
    if found.props.get("code").is_some_and(Value::is_object) {
        return Err(OpsError::new(format!("`{node}` is code already")));
    }
    let language = match language.map(str::trim).filter(|l| !l.is_empty()) {
        Some(named) => named.to_ascii_lowercase(),
        None => {
            let words = written(&found.props).unwrap_or_default();
            let hints = scaena_core::code::Hints::default();
            let found = scaena_core::code::detect(&words, &hints).language().map_or("plain", |l| l.name);
            found.to_string()
        }
    };
    let mut ops = Vec::new();
    // Its own props, each delta, and the overrides: what a code text takes no more of.
    for (at, props) in written_at(deck, node) {
        if let Some(text) = props.get("runs").and_then(|_| written(props)) {
            ops.push(json!({ "op": "remove", "path": format!("{at}/runs") }));
            ops.push(json!({ "op": "add", "path": format!("{at}/text"), "value": text }));
        }
        for key in NOT_CODE {
            if props.contains_key(key) {
                ops.push(json!({ "op": "remove", "path": format!("{at}/{key}") }));
            }
        }
        if props.get("fit").and_then(Value::as_str) == Some("wrap") {
            ops.push(json!({ "op": "remove", "path": format!("{at}/fit") }));
        }
        if props.get("style").and_then(|s| s.get("case")).is_some() {
            ops.push(json!({ "op": "remove", "path": format!("{at}/style/case") }));
        }
    }
    let own = format!("/nodes/{}", esc(node));
    ops.push(json!({ "op": "add", "path": format!("{own}/code"), "value": { "language": language } }));
    match theme.code_role() {
        Some(role) => ops.push(json!({ "op": "add", "path": format!("{own}/role"), "value": role })),
        None if found.props.contains_key("role") => ops.push(json!({ "op": "remove", "path": format!("{own}/role") })),
        None => {}
    }
    Ok(ops)
}

/// The patch that takes `code` away from `node`, its own, each delta's, and the overrides', and
/// sets it in `body` where it was set in the theme's code role. Refused for a node that is no text,
/// or is no code anywhere.
pub fn uncoding(deck: &Deck, theme: &Theme, node: &str) -> Result<Vec<Value>, OpsError> {
    text_node(deck, node)?;
    let code_role = theme.code_role();
    let mut ops = Vec::new();
    for (at, props) in written_at(deck, node) {
        if props.contains_key("code") {
            ops.push(json!({ "op": "remove", "path": format!("{at}/code") }));
        }
        if code_role.is_some() && props.get("role").and_then(Value::as_str) == code_role {
            ops.push(json!({ "op": "add", "path": format!("{at}/role"), "value": "body" }));
        }
    }
    if !ops.iter().any(|op| op["path"].as_str().is_some_and(|p| p.ends_with("/code"))) {
        return Err(OpsError::new(format!("`{node}` is no code")));
    }
    Ok(ops)
}

/// The patch that tidies `node`'s code as `state` shows it, by its language and the theme's tab
/// stops (`scaena_core::code::tidy`): a `replace_text` for each run of lines it changes, last
/// first, so each counts characters as the text stands before it. Empty where it is tidy. Refused
/// for a node that is no code there.
pub fn tidying(deck: &Deck, theme: &Theme, state: &str, node: &str) -> Result<Vec<Value>, OpsError> {
    text_node(deck, node)?;
    let snaps = scaena_core::resolve_states(deck).map_err(|e| OpsError::new(e.to_string()))?;
    let snap =
        snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::new(format!("unknown state `{state}`")))?;
    let mut props = snap
        .nodes
        .get(node)
        .cloned()
        .ok_or_else(|| OpsError::new(format!("`{node}` is not on screen in `{state}`")))?;
    if let Some(over) = deck.overrides.get(node) {
        scaena_core::tracking::merge_props(&mut props, over);
    }
    let language = props.get("code").and_then(|c| c.get("language")).and_then(Value::as_str);
    let Some(language) = language else {
        return Err(OpsError::new(format!("`{node}` is no code in `{state}`: make it code first")));
    };
    let text = written(&props).unwrap_or_default();
    let tab = theme.code.as_ref().and_then(|c| c.tab).map_or(4, usize::from);
    let tidied = scaena_core::code::tidy(&text, scaena_core::code::lang(language), tab).text;
    Ok(changed_runs(&text, &tidied)
        .into_iter()
        .rev()
        .map(|(from, to, with)| json!({ "op": "replace_text", "node": node, "from": from, "to": to, "text": with, "state": state }))
        .collect())
}

/// Each run of lines `tidied` changes in `text`: where it starts and ends in `text`, in characters,
/// and what stands there in `tidied`. Line for line where both have as many lines; else the lines
/// between those both start and end with.
fn changed_runs(text: &str, tidied: &str) -> Vec<(usize, usize, String)> {
    if text == tidied {
        return Vec::new();
    }
    let ours: Vec<&str> = text.split('\n').collect();
    let theirs: Vec<&str> = tidied.split('\n').collect();
    // Where each of `text`'s lines starts, in characters, and the text's end.
    let mut starts = Vec::with_capacity(ours.len() + 1);
    let mut at = 0;
    for line in &ours {
        starts.push(at);
        at += line.chars().count() + 1;
    }
    starts.push(at);
    // The run of `text`'s lines from `a` to `b`, without the break after the last.
    let span = |a: usize, b: usize| (starts[a], starts[b] - 1);
    let mut runs = Vec::new();
    if ours.len() == theirs.len() {
        let mut k = 0;
        while k < ours.len() {
            if ours[k] == theirs[k] {
                k += 1;
                continue;
            }
            let first = k;
            while k < ours.len() && ours[k] != theirs[k] {
                k += 1;
            }
            let (from, to) = span(first, k);
            runs.push((from, to, theirs[first..k].join("\n")));
        }
        return runs;
    }
    let head = ours.iter().zip(&theirs).take_while(|(a, b)| a == b).count();
    let tail = ours[head..].iter().rev().zip(theirs[head..].iter().rev()).take_while(|(a, b)| a == b).count();
    let (from, to) = span(head, ours.len() - tail);
    vec![(from, to, theirs[head..theirs.len() - tail].join("\n"))]
}

/// The patch that sets code text `node`'s lines in focus in `state` to `focus`, each span its
/// first and last line counted from 1, none showing every line (PLAN 2.140): Focus on These
/// Lines, the inspector's Lines in Focus, and Clear Focus. It is written into `state`'s own props,
/// and each state after it that would show another focus than it did is given back the one it
/// showed, so the states after it are as they were. Empty where `state` shows `focus` already.
/// Refused for a node that is no code there, or whose focus the deck's `overrides` set in every
/// state.
pub fn focusing(deck: &Deck, state: &str, node: &str, focus: &[(u64, u64)]) -> Result<Vec<Value>, OpsError> {
    let (i, lines) = code_in(deck, state, node)?;
    let value = focused(focus, lines);
    let mut walked = deck.clone();
    put_focus(&mut walked.states[i], node, value.clone());
    let mut ops = vec![json!({ "op": "set_prop", "state": state, "node": node, "prop": "code/focus", "value": value })];
    ops.extend(kept(deck, &mut walked, node, state)?);
    // A focus it shows already is nothing to do.
    let before = scaena_core::resolve_states(deck).map_err(|e| OpsError::new(e.to_string()))?;
    let after = scaena_core::resolve_states(&walked).map_err(|e| OpsError::new(e.to_string()))?;
    if focus_of(&before[i], node) == focus_of(&after[i], node) && ops.len() == 1 {
        return Ok(Vec::new());
    }
    Ok(ops)
}

/// The patch Walk Through Lines… makes of code text `node` from `state` (PLAN 2.140, ADR-0029
/// decision 6): `state` shows the first of `steps`, and a step of its slide after it shows each of
/// the rest, in order, each a set of spans as [`focusing`] takes them, none (or every line) the
/// whole block. Each step is named after `state` (`code-2`, …), in its beat, and glides in at the
/// theme's `slow`, as the editors' code steps do; the states after them are as they were. Refused
/// as [`focusing`] is, and for no steps.
pub fn walking(deck: &Deck, state: &str, node: &str, steps: &[Vec<(u64, u64)>]) -> Result<Vec<Value>, OpsError> {
    let (i, lines) = code_in(deck, state, node)?;
    let Some((first, rest)) = steps.split_first() else {
        return Err(OpsError::new("a walkthrough takes a step for each set of lines: name at least one"));
    };
    let mut walked = deck.clone();
    let mut ops = Vec::new();
    let value = focused(first, lines);
    let before = scaena_core::resolve_states(deck).map_err(|e| OpsError::new(e.to_string()))?;
    if focus_of(&before[i], node) != value.as_array().map(|_| &value) {
        put_focus(&mut walked.states[i], node, value.clone());
        ops.push(json!({ "op": "set_prop", "state": state, "node": node, "prop": "code/focus", "value": value }));
    }
    let slide = deck.states[i].slide.clone().unwrap_or_else(|| state.to_string());
    let beat = (deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats))
        .find(|b| b.states.iter().any(|s| s == state))
        .map(|b| b.id.clone());
    let base = match state.rsplit_once('-') {
        Some((stem, n)) if !stem.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => stem,
        _ => state,
    };
    let mut after = state.to_string();
    for (k, spans) in rest.iter().enumerate() {
        let id = (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !walked.states.iter().any(|s| s.id == *id))
            .expect("some number is free");
        let mut code = serde_json::Map::new();
        code.insert("focus".into(), focused(spans, lines));
        let step = json!({ "id": id, "slide": slide, "transition": "slow", "props": { node: { "code": code } } });
        let typed: scaena_core::document::State =
            serde_json::from_value(step.clone()).map_err(|e| OpsError::new(e.to_string()))?;
        walked.states.insert(i + 1 + k, typed);
        let mut op = json!({ "op": "add_state", "state": step, "after": after });
        if let Some(beat) = &beat {
            op["beat"] = json!(beat);
        }
        ops.push(op);
        after = id;
    }
    ops.extend(kept(deck, &mut walked, node, state)?);
    Ok(ops)
}

/// The number code text `node`'s gutter gives its first line in `state` (its `code.start`, else 1),
/// and how many lines it has there: what its lines in focus are said and read in
/// (`scaena_core::code::focus`). Refused as [`focusing`] is.
pub fn numbered(deck: &Deck, state: &str, node: &str) -> Result<(u64, u64), OpsError> {
    let (i, lines) = code_in(deck, state, node)?;
    let snaps = scaena_core::resolve_states(deck).map_err(|e| OpsError::new(e.to_string()))?;
    let start = snaps[i].nodes.get(node).and_then(|p| p.get("code")?.get("start")?.as_u64()).unwrap_or(1);
    Ok((start.max(1), lines))
}

/// Where `state` is among `deck`'s states, and how many lines code text `node` has there: an error
/// where it is not on screen there, is no code there, or the overrides set its lines in focus.
fn code_in(deck: &Deck, state: &str, node: &str) -> Result<(usize, u64), OpsError> {
    text_node(deck, node)?;
    let i = (deck.states.iter().position(|s| s.id == state))
        .ok_or_else(|| OpsError::new(format!("unknown state `{state}`")))?;
    let snaps = scaena_core::resolve_states(deck).map_err(|e| OpsError::new(e.to_string()))?;
    let props =
        snaps[i].nodes.get(node).ok_or_else(|| OpsError::new(format!("`{node}` is not on screen in `{state}`")))?;
    if !props.get("code").is_some_and(Value::is_object) {
        return Err(OpsError::new(format!("`{node}` is no code in `{state}`: make it code first")));
    }
    if deck.overrides.get(node).and_then(|o| o.get("code")).and_then(|c| c.get("focus")).is_some() {
        return Err(OpsError::new(format!("the deck's overrides set `{node}`'s lines in focus in every state")));
    }
    let lines = scaena_core::lists::paragraphs(&written(props).unwrap_or_default()).len() as u64;
    Ok((i, lines))
}

/// `spans` as `code.focus` writes them, held to the text's `lines`: `null`, every line shown,
/// where there are none or they name every line.
fn focused(spans: &[(u64, u64)], lines: u64) -> Value {
    let held: Vec<(u64, u64)> =
        spans.iter().filter(|&&(a, _)| a <= lines).map(|&(a, b)| (a.max(1), b.min(lines))).collect();
    match held.as_slice() {
        [(1, b)] if *b == lines => Value::Null,
        _ => scaena_core::code::focus::value(&held),
    }
}

/// `state` given `value` for `node`'s `code.focus` in its own props, as `set_prop` writes it.
fn put_focus(state: &mut scaena_core::document::State, node: &str, value: Value) {
    let props = state.props.entry(node.to_string()).or_default();
    match props.get_mut("code") {
        Some(Value::Object(code)) => drop(code.insert("focus".into(), value)),
        _ => drop(props.insert("code".into(), json!({ "focus": value }))),
    }
}

/// `node`'s lines in focus as `snap` shows them.
fn focus_of<'s>(snap: &'s scaena_core::tracking::Snapshot, node: &str) -> Option<&'s Value> {
    snap.nodes.get(node)?.get("code")?.get("focus").filter(|f| !f.is_null())
}

/// The `set_prop`s that give each of `deck`'s states but `changed` back the focus it showed of
/// `node` where `walked` shows it another, `walked` given them too: each, in order, as the one
/// before it is.
fn kept(deck: &Deck, walked: &mut Deck, node: &str, changed: &str) -> Result<Vec<Value>, OpsError> {
    let resolve = |d: &Deck| scaena_core::resolve_states(d).map_err(|e| OpsError::new(e.to_string()));
    let before = resolve(deck)?;
    let mut ops = Vec::new();
    for _ in 0..deck.states.len() {
        let after = resolve(walked)?;
        let differs = before.iter().find_map(|was| {
            let now = after.iter().find(|s| s.state_id == was.state_id)?;
            let shows = was.state_id != changed && was.nodes.contains_key(node);
            (shows && focus_of(was, node) != focus_of(now, node)).then_some(was)
        });
        let Some(was) = differs else { break };
        let value = focus_of(was, node).cloned().unwrap_or(Value::Null);
        let j = walked.states.iter().position(|s| s.id == was.state_id).expect("a state of the deck");
        put_focus(&mut walked.states[j], node, value.clone());
        ops.push(
            json!({ "op": "set_prop", "state": was.state_id, "node": node, "prop": "code/focus", "value": value }),
        );
    }
    Ok(ops)
}

/// `node`, which is a text.
fn text_node<'d>(deck: &'d Deck, node: &str) -> Result<&'d scaena_core::document::Node, OpsError> {
    let found = deck.nodes.get(node).ok_or_else(|| OpsError::new(format!("no node `{node}`")))?;
    if found.node_type != NodeType::Text {
        return Err(OpsError::new(format!("`{node}` is no text: only a text's words are code")));
    }
    Ok(found)
}

/// Where `node`'s props are written, as JSON pointers, and what each holds: its own, each state's
/// delta, then the deck's overrides.
fn written_at<'d>(deck: &'d Deck, node: &str) -> Vec<(String, &'d Props)> {
    let id = esc(node);
    let mut out = Vec::new();
    if let Some(found) = deck.nodes.get(node) {
        out.push((format!("/nodes/{id}"), &found.props));
    }
    for (j, state) in deck.states.iter().enumerate() {
        if let Some(delta) = state.props.get(node) {
            out.push((format!("/states/{j}/props/{id}"), delta));
        }
    }
    if let Some(over) = deck.overrides.get(node) {
        out.push((format!("/overrides/{id}"), over));
    }
    out
}

/// The words `props` write: their runs' texts end to end, else their `text`; none where they
/// write neither.
fn written(props: &Props) -> Option<String> {
    match props.get("runs").and_then(Value::as_array) {
        Some(runs) => Some(runs.iter().filter_map(|r| r.get("text").and_then(Value::as_str)).collect()),
        None => props.get("text").and_then(Value::as_str).map(str::to_string),
    }
}

/// A key as a JSON pointer writes it.
fn esc(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::changed_runs;

    #[test]
    fn a_run_of_lines_is_replaced_where_it_changes() {
        assert!(changed_runs("a\nb", "a\nb").is_empty());
        // Line for line: each run of changed lines, by its characters.
        assert_eq!(changed_runs("a \nb\nc \nd ", "a\nb\nc\nd"), vec![(0, 2, "a".into()), (5, 10, "c\nd".into())]);
        // Lines taken away: the lines between those both start and end with.
        assert_eq!(changed_runs("```\nx\n```", "x"), vec![(0, 9, "x".into())]);
        assert_eq!(changed_runs("a\n\tb\nc", "a\n    b\nc"), vec![(2, 4, "    b".into())]);
    }
}
