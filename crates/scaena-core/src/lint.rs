//! Lint findings and rules (SPEC §7.4–§7.5).
//!
//! Document-level rules live here and need only the document and its resolved
//! snapshots. Layout-level rules (overflow, contrast, collisions, …) live in
//! `scaena-engine::lint` because they need fonts and geometry.
//!
//! Every rule ships with fixtures under `tests/lint/<CODE>/` (one deck that
//! triggers it, one that must not) — see PLAN working agreement 5.

use crate::document::{Deck, NodeType};
use crate::tracking::Snapshot;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// One lint finding, shaped for agents: a code, a JSON pointer, and (when safe) a fix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    /// The bundle file `path` points into, when it is not `deck.json` (a theme file).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// JSON pointer into `deck.json`, or into `file`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measure: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// A JSON Patch (RFC 6902) that resolves the finding. Only safe, non-content fixes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Vec<Value>>,
}

impl Finding {
    pub fn new(code: &str, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            file: None,
            path: None,
            state: None,
            node: None,
            measure: None,
            hint: None,
            fix: None,
        }
    }
    pub fn file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }
    pub fn at(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
    pub fn state(mut self, s: impl Into<String>) -> Self {
        self.state = Some(s.into());
        self
    }
    pub fn node(mut self, n: impl Into<String>) -> Self {
        self.node = Some(n.into());
        self
    }
    pub fn hint(mut self, h: impl Into<String>) -> Self {
        self.hint = Some(h.into());
        self
    }
    pub fn fix(mut self, ops: Vec<Value>) -> Self {
        self.fix = Some(ops);
        self
    }
}

/// What a rule gets to look at.
pub struct Context<'a> {
    pub deck: &'a Deck,
    pub snapshots: &'a [Snapshot],
}

pub trait Rule: Sync + Send {
    fn code(&self) -> &'static str;
    fn severity(&self) -> Severity;
    fn check(&self, cx: &Context) -> Vec<Finding>;
}

/// The document-level rule set. Order is the report order.
pub fn document_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(W401StateNotInBeat),
        Box::new(W410ImageWithoutAlt),
        Box::new(I400NoopState),
        Box::new(I401NodeNeverVisible),
        Box::new(I402OverrideCount),
    ]
}

/// Run semantic validation (E-codes) and all document-level rules.
pub fn lint_document(deck: &Deck) -> Vec<Finding> {
    let mut findings = crate::validate::validate(deck);
    // Rules need snapshots; if tracking fails, validation already reported why.
    if let Ok(snapshots) = crate::tracking::resolve_states(deck) {
        let cx = Context { deck, snapshots: &snapshots };
        for rule in document_rules() {
            findings.extend(rule.check(&cx));
        }
    }
    findings.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.code.cmp(&b.code)));
    findings
}

// ---------------------------------------------------------------------------

struct W401StateNotInBeat;
impl Rule for W401StateNotInBeat {
    fn code(&self) -> &'static str {
        "W401"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let Some(spine) = &cx.deck.spine else { return vec![] };
        let referenced: Vec<&String> =
            spine.sections.iter().flat_map(|s| s.beats.iter()).flat_map(|b| b.states.iter()).collect();
        cx.deck.states.iter().enumerate().filter(|(_, s)| !referenced.contains(&&s.id)).map(|(i, s)| {
            Finding::new(self.code(), self.severity(), format!("state `{}` is not referenced by any beat", s.id))
                .at(format!("/states/{i}")).state(s.id.clone())
                .hint("Add it to a beat's `states` so projections (PDF order, podcast, infographic) know where it belongs.")
        }).collect()
    }
}

struct W410ImageWithoutAlt;
impl Rule for W410ImageWithoutAlt {
    fn code(&self) -> &'static str {
        "W410"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        cx.deck
            .nodes
            .iter()
            .filter(|(_, n)| n.node_type == NodeType::Image && !n.props.contains_key("alt"))
            .map(|(id, _)| {
                Finding::new(self.code(), self.severity(), format!("image `{id}` has no `alt` text"))
                    .at(format!("/nodes/{id}"))
                    .node(id.clone())
                    .hint("Describe the image for screen readers and tagged PDF, or set alt to \"\" if decorative.")
            })
            .collect()
    }
}

struct I400NoopState;
impl Rule for I400NoopState {
    fn code(&self) -> &'static str {
        "I400"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        cx.snapshots
            .windows(2)
            .enumerate()
            .filter_map(|(i, w)| {
                let (prev, cur) = (&w[0], &w[1]);
                let st = &cx.deck.states[i + 1];
                let same = prev.nodes == cur.nodes && st.choreography.is_empty() && prev.layout == cur.layout;
                same.then(|| {
                    Finding::new(
                        self.code(),
                        self.severity(),
                        format!("state `{}` is identical to `{}`", cur.state_id, prev.state_id),
                    )
                    .at(format!("/states/{}", i + 1))
                    .state(cur.state_id.clone())
                    .hint("Remove it, or give it a change worth a click.")
                })
            })
            .collect()
    }
}

struct I401NodeNeverVisible;
impl Rule for I401NodeNeverVisible {
    fn code(&self) -> &'static str {
        "I401"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        cx.deck
            .nodes
            .keys()
            .filter(|id| !cx.snapshots.iter().any(|s| s.nodes.contains_key(*id)))
            .map(|id| {
                Finding::new(self.code(), self.severity(), format!("node `{id}` is never visible in any state"))
                    .at(format!("/nodes/{id}"))
                    .node(id.clone())
            })
            .collect()
    }
}

struct I402OverrideCount;
impl Rule for I402OverrideCount {
    fn code(&self) -> &'static str {
        "I402"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        cx.deck
            .overrides
            .iter()
            .filter(|(_, o)| !o.is_empty())
            .map(|(id, o)| {
                Finding::new(
                    self.code(),
                    self.severity(),
                    format!("node `{id}` has {} override(s); it is not theme-safe", o.len()),
                )
                .at(format!("/overrides/{id}"))
                .node(id.clone())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> Deck {
        Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap()
    }

    #[test]
    fn example_deck_is_clean() {
        let findings = lint_document(&example());
        assert!(findings.is_empty(), "{findings:#?}");
    }

    #[test]
    fn flags_orphan_state_and_noop_state() {
        let mut deck = example();
        deck.states.push(crate::document::State {
            id: "extra".into(),
            name: None,
            slide: None,
            from: None,
            mode: Default::default(),
            layout: Some("title".into()),
            transition: None,
            props: Default::default(),
            remove: vec![],
            choreography: vec![],
            hold: None,
            notes: None,
            comment: None,
        });
        let codes: Vec<String> = lint_document(&deck).into_iter().map(|f| f.code).collect();
        assert!(codes.contains(&"W401".to_string()), "{codes:?}");
        assert!(codes.contains(&"I400".to_string()), "{codes:?}");
    }
}
