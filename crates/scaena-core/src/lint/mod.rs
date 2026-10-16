//! Lint findings and rules (SPEC §7.4–§7.5).
//!
//! Document-level rules live here and need only the document, its theme, and its
//! resolved snapshots. Layout-level rules (overflow, contrast, collisions, …) live in
//! `scaena-engine::lint` because they need fonts and geometry.
//!
//! Every rule ships with fixtures under `tests/lint/<CODE>/` (one deck that
//! triggers it, one that must not) — see PLAN working agreement 5.

mod delta;
mod document;
mod narrative;

pub use delta::{Delta, delta};
pub(crate) use document::literal;
pub use document::words;

use crate::displaylist::DisplayList;
use crate::document::Deck;
use crate::model::theme::Theme;
use crate::tracking::Snapshot;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// One lint finding, shaped for agents: a code, a JSON pointer, and (when safe) a fix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// The format the deck was laid out in (`"9:16"`), when it is not the deck's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measure: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// A JSON Patch (RFC 6902) that resolves the finding. Only safe, non-content fixes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Vec<Value>>,
    /// The data source to fetch again, where what resolves the finding is a fetch, which no patch
    /// makes (W428, ADR-0030): the source's name, without its `@`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh: Option<String>,
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
            format: None,
            measure: None,
            hint: None,
            fix: None,
            refresh: None,
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
    pub fn format(mut self, f: impl Into<String>) -> Self {
        self.format = Some(f.into());
        self
    }
    pub fn hint(mut self, h: impl Into<String>) -> Self {
        self.hint = Some(h.into());
        self
    }
    pub fn measure(mut self, m: Value) -> Self {
        self.measure = Some(m);
        self
    }
    pub fn fix(mut self, ops: Vec<Value>) -> Self {
        self.fix = Some(ops);
        self
    }
    pub fn refresh(mut self, source: impl Into<String>) -> Self {
        self.refresh = Some(source.into());
        self
    }
}

/// What a rule gets to look at.
pub struct Context<'a> {
    pub deck: &'a Deck,
    /// The deck's theme, when it has one that parses: what thresholds and presets a rule
    /// reads. Without one, rules use the theme schema's defaults.
    pub theme: Option<&'a Theme>,
    pub snapshots: &'a [Snapshot],
}

pub trait Rule: Sync + Send {
    fn code(&self) -> &'static str;
    fn severity(&self) -> Severity;
    fn check(&self, cx: &Context) -> Vec<Finding>;
}

/// The document-level rule set. Order is the report order.
pub fn document_rules() -> Vec<Box<dyn Rule>> {
    let mut rules = document::rules();
    rules.extend(narrative::rules());
    rules
}

/// Run semantic validation (E-codes) and all document-level rules.
pub fn lint_document(deck: &Deck, theme: Option<&Theme>) -> Vec<Finding> {
    let mut findings = crate::validate::validate(deck);
    findings.extend(check(deck, theme));
    sort(&mut findings);
    findings
}

/// The document-level rules alone: what to add to a bundle's validation
/// (`validate::validate_bundle`), which already holds `validate`'s findings.
pub fn check(deck: &Deck, theme: Option<&Theme>) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Rules need snapshots; if tracking fails, validation already reported why.
    if let Ok(snapshots) = crate::tracking::resolve_states(deck) {
        let cx = Context { deck, theme, snapshots: &snapshots };
        for rule in document_rules() {
            findings.extend(rule.check(&cx));
        }
    }
    findings
}

/// W428 (ADR-0030, decisions 21 and 25): each fetched forecast whose first period's day has passed
/// at its place at `now`, seconds since the epoch, and each price series older than its range
/// allows, unless it is `kept`: one finding at each node that
/// shows it, in the first state that does, else one at the source. Each names the source to
/// fetch again in `refresh`, since a fetch is no patch. Lint judges no date without `now`.
pub fn dated(deck: &Deck, now: i64) -> Vec<Finding> {
    let mut out = Vec::new();
    let snapshots = crate::tracking::resolve_states(deck).unwrap_or_default();
    for (name, source) in &deck.data {
        let Some(fetched) = &source.fetched() else { continue };
        let rows = source.source.get("inline").and_then(Value::as_array);
        let first = rows.and_then(|r| r.first()).and_then(|row| row.get("date").or_else(|| row.get("time")));
        let first = first.and_then(Value::as_str);
        let message: Box<dyn Fn(&str) -> String> = if crate::fetched::passed(fetched, first, now) {
            let Some(forecast) = &fetched.forecast else { continue };
            let day: String =
                first.map_or_else(|| crate::fetched::date(&fetched.at).unwrap_or("").into(), |d| d[..10].into());
            let place = forecast.place.name.clone();
            Box::new(move |what: &str| {
                format!("{what} forecast `@{name}` starts on {day}, a day that has passed in {place}")
            })
        } else if crate::fetched::stale(fetched, now) {
            // A price series older than its range allows (ADR-0030, decision 25).
            let Some(stock) = &fetched.stock else { continue };
            let (symbol, through) = (stock.symbol.clone(), stock.through.clone());
            Box::new(move |what: &str| {
                format!("{what} prices of {symbol} `@{name}` run to {through}, older than their range allows")
            })
        } else {
            continue;
        };
        let hint = "Fetch it again, or keep it as it is (`fetched.kept`).";
        let reads = format!("@{name}");
        let mut shown = BTreeSet::new();
        for snap in &snapshots {
            for (id, props) in &snap.nodes {
                if props.get("data").and_then(Value::as_str) == Some(reads.as_str()) && shown.insert(id.clone()) {
                    out.push(
                        Finding::new("W428", Severity::Warning, message(&format!("`{id}` shows the")))
                            .at(format!("/nodes/{}", document::token(id)))
                            .state(snap.state_id.clone())
                            .node(id.clone())
                            .hint(hint)
                            .refresh(name.clone()),
                    );
                }
            }
        }
        if shown.is_empty() {
            out.push(
                Finding::new("W428", Severity::Warning, message("the"))
                    .at(format!("/data/{}", document::token(name)))
                    .hint(hint)
                    .refresh(name.clone()),
            );
        }
    }
    out
}

/// Findings in report order: errors first, then by code, then where they are.
pub fn sort(findings: &mut [Finding]) {
    crate::sort::by(findings, |a, b| {
        b.severity.cmp(&a.severity).then_with(|| a.code.cmp(&b.code)).then_with(|| a.format.cmp(&b.format))
    });
}

/// A painter lent to lint, for what only pixels say: the background text sits on (E110,
/// E111). The engine never paints; the client that holds a painter lends it.
pub trait Backdrop {
    /// `dl` painted at `scale` pixels per canvas unit.
    fn paint(&mut self, dl: &DisplayList, scale: f32) -> Result<Pixels, String>;
}

/// Straight-alpha sRGB RGBA8 pixels, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Pixels {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> Deck {
        Deck::from_json(include_str!("../../../../docs/examples/revenue.deck.json")).unwrap()
    }

    fn dusk() -> Theme {
        serde_json::from_str(include_str!("../../../../docs/examples/themes/dusk.theme.json")).unwrap()
    }

    #[test]
    fn example_deck_is_clean() {
        let findings = lint_document(&example(), Some(&dusk()));
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
            furniture: None,
            transition: None,
            props: Default::default(),
            remove: vec![],
            choreography: vec![],
            hold: None,
            notes: None,
            comment: None,
        });
        let codes: Vec<String> = lint_document(&deck, Some(&dusk())).into_iter().map(|f| f.code).collect();
        assert!(codes.contains(&"W401".to_string()), "{codes:?}");
        assert!(codes.contains(&"I400".to_string()), "{codes:?}");
    }
}
