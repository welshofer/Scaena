//! Semantic validation (SPEC §7.5 E-codes that need no layout): ids, references,
//! tracking errors. Schema validation (shape) happens before this, against
//! `docs/schema/deck.schema.json`; this catches what a schema cannot.

use crate::document::Deck;
use crate::ids::is_valid_id;
use crate::lint::{Finding, Severity};
use serde_json::Value;
use std::collections::HashSet;

/// Validate `deck` and return every problem as an E-finding.
pub fn validate(deck: &Deck) -> Vec<Finding> {
    let mut out = Vec::new();
    let err = |code: &str, msg: String| Finding::new(code, Severity::Error, msg);

    // --- ids -------------------------------------------------------------
    for id in deck.nodes.keys() {
        if !is_valid_id(id) {
            out.push(err("E105", format!("invalid node id `{id}`")).at(format!("/nodes/{id}")).node(id.clone()));
        }
    }
    let mut seen = HashSet::new();
    for (i, s) in deck.states.iter().enumerate() {
        if !is_valid_id(&s.id) {
            out.push(
                err("E105", format!("invalid state id `{}`", s.id)).at(format!("/states/{i}/id")).state(s.id.clone()),
            );
        }
        if !seen.insert(&s.id) {
            out.push(
                err("E105", format!("duplicate state id `{}`", s.id)).at(format!("/states/{i}/id")).state(s.id.clone()),
            );
        }
    }

    // --- references ------------------------------------------------------
    let state_ids: HashSet<&str> = deck.states.iter().map(|s| s.id.as_str()).collect();
    for (i, s) in deck.states.iter().enumerate() {
        for id in s.props.keys() {
            if !deck.nodes.contains_key(id) {
                out.push(
                    err("E102", format!("state `{}` sets props on unknown node `{id}`", s.id))
                        .at(format!("/states/{i}/props/{id}"))
                        .state(s.id.clone())
                        .node(id.clone()),
                );
            }
        }
        for id in &s.remove {
            if !deck.nodes.contains_key(id) {
                out.push(
                    err("E102", format!("state `{}` removes unknown node `{id}`", s.id))
                        .at(format!("/states/{i}/remove"))
                        .state(s.id.clone())
                        .node(id.clone()),
                );
            }
        }
        if let Some(from) = &s.from
            && !state_ids.contains(from.as_str())
        {
            out.push(
                err("E102", format!("state `{}` tracks from unknown state `{from}`", s.id))
                    .at(format!("/states/{i}/from"))
                    .state(s.id.clone()),
            );
        }
        if let Some(slide) = &s.slide
            && !state_ids.contains(slide.as_str())
        {
            out.push(
                err("E102", format!("state `{}` belongs to unknown slide `{slide}`", s.id))
                    .at(format!("/states/{i}/slide"))
                    .state(s.id.clone()),
            );
        }
        for (j, item) in s.choreography.iter().enumerate() {
            for target in choreo_targets(item) {
                if !deck.nodes.contains_key(&target) {
                    out.push(
                        err("E102", format!("choreography in `{}` targets unknown node `{target}`", s.id))
                            .at(format!("/states/{i}/choreography/{j}"))
                            .state(s.id.clone())
                            .node(target),
                    );
                }
            }
        }
    }
    for (id, node) in &deck.nodes {
        if let Some(Value::String(parent)) = node.props.get("parent")
            && !deck.nodes.contains_key(parent)
        {
            out.push(
                err("E102", format!("node `{id}` has unknown parent `{parent}`"))
                    .at(format!("/nodes/{id}/parent"))
                    .node(id.clone()),
            );
        }
        if let Some(Value::String(data)) = node.props.get("data") {
            let name = data.trim_start_matches('@');
            if !deck.data.contains_key(name) {
                out.push(
                    err("E102", format!("node `{id}` references unknown data source `{data}`"))
                        .at(format!("/nodes/{id}/data"))
                        .node(id.clone())
                        .hint(format!(
                            "Declare it under /data/{name} or attach it with `scaena patch` / `data_attach`."
                        )),
                );
            }
        }
    }
    if let Some(spine) = &deck.spine {
        let mut beat_ids = HashSet::new();
        for (si, sec) in spine.sections.iter().enumerate() {
            for (bi, beat) in sec.beats.iter().enumerate() {
                if !beat_ids.insert(&beat.id) {
                    out.push(
                        err("E105", format!("duplicate beat id `{}`", beat.id))
                            .at(format!("/spine/sections/{si}/beats/{bi}/id")),
                    );
                }
                for st in &beat.states {
                    if !state_ids.contains(st.as_str()) {
                        out.push(
                            err("E102", format!("beat `{}` references unknown state `{st}`", beat.id))
                                .at(format!("/spine/sections/{si}/beats/{bi}/states")),
                        );
                    }
                }
            }
        }
    }

    // --- tracking --------------------------------------------------------
    // Unknown-node / unknown-from errors were reported above; only the ordering rule is new here.
    if let Err(e @ crate::tracking::TrackingError::ForwardFrom { .. }) = crate::tracking::resolve_states(deck) {
        out.push(err("E102", e.to_string()));
    }
    out
}

fn choreo_targets(item: &Value) -> Vec<String> {
    let mut v = Vec::new();
    match item.get("target") {
        Some(Value::String(s)) => v.push(s.clone()),
        Some(Value::Array(a)) => v.extend(a.iter().filter_map(|x| x.as_str().map(String::from))),
        _ => {}
    }
    for key in ["sequence", "parallel"] {
        if let Some(Value::Array(items)) = item.get(key) {
            v.extend(items.iter().flat_map(choreo_targets));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn example() -> Deck {
        Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap()
    }

    #[test]
    fn example_validates() {
        assert!(validate(&example()).is_empty());
    }

    #[test]
    fn unknown_references_are_e102() {
        let mut deck = example();
        deck.states[1].props.insert("ghost".into(), Default::default());
        deck.nodes.get_mut("rev").unwrap().props.insert("data".into(), json!("@nope"));
        let f = validate(&deck);
        assert_eq!(f.iter().filter(|x| x.code == "E102").count(), 2, "{f:#?}");
    }

    #[test]
    fn duplicate_state_ids_are_e105() {
        let mut deck = example();
        let dup = deck.states[0].clone();
        deck.states.push(dup);
        assert!(validate(&deck).iter().any(|f| f.code == "E105"));
    }
}
