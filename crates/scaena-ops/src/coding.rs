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
    let snap = snaps.iter().find(|s| s.state_id == state).ok_or_else(|| OpsError::unknown_state(deck, state))?;
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
