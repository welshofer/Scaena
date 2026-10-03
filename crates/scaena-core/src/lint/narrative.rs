//! Narrative rules (SPEC §7.5, 4xx): does the deck make its argument? They read the
//! spine and what each node is for (`semantic`), and they are warnings, never errors.
//!
//! A beat's claim is shown by a node whose `semantic` is `claim` or `takeaway`. A build
//! may show its evidence before its claim, so W421 and W422 judge each slide at its last
//! state, where the build is complete (authorability finding 12).

use super::{Context, Finding, Rule, Severity};
use crate::document::{NodeType, Props};
use crate::tracking::Snapshot;
use serde_json::Value;

pub(super) fn rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(W420ClaimNotShown),
        Box::new(W421EvidenceWithoutClaim),
        Box::new(W422EvidenceOutranksClaim),
        Box::new(W423EvidenceNotShown),
        Box::new(W424TooManyClaims),
        Box::new(W425RepeatedClaim),
        Box::new(W426SpineOutOfOrder),
    ]
}

/// What a visible node is for.
fn semantic(props: &Props) -> Option<&str> {
    props.get("semantic").and_then(Value::as_str)
}

fn is_claim(props: &Props) -> bool {
    matches!(semantic(props), Some("claim" | "takeaway"))
}

/// Each beat with its pointer.
fn beats<'a>(cx: &Context<'a>) -> Vec<(String, &'a crate::document::Beat)> {
    let Some(spine) = &cx.deck.spine else { return vec![] };
    spine
        .sections
        .iter()
        .enumerate()
        .flat_map(|(s, section)| {
            section.beats.iter().enumerate().map(move |(b, beat)| (format!("/spine/sections/{s}/beats/{b}"), beat))
        })
        .collect()
}

/// The last state of each slide, by index: where its build is complete.
fn slide_ends(cx: &Context) -> Vec<usize> {
    let slide = |i: usize| {
        let s = &cx.deck.states[i];
        s.slide.clone().unwrap_or_else(|| s.id.clone())
    };
    (0..cx.deck.states.len()).filter(|&i| i + 1 == cx.deck.states.len() || slide(i) != slide(i + 1)).collect()
}

/// W420: a beat whose states show no claim. A beat whose states show only signposts
/// (`navigation`, `decoration`, `source`: a title, a closing slide) makes no argument
/// there, and is not judged.
struct W420ClaimNotShown;
impl Rule for W420ClaimNotShown {
    fn code(&self) -> &'static str {
        "W420"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let mut out = Vec::new();
        for (pointer, beat) in beats(cx) {
            let shown: Vec<&Snapshot> = cx.snapshots.iter().filter(|s| beat.states.contains(&s.state_id)).collect();
            let signpost = |p: &Props| matches!(semantic(p), Some("navigation" | "decoration" | "source"));
            let argues = shown.iter().any(|s| s.nodes.values().any(|p| !signpost(p)));
            if !argues || shown.iter().any(|s| s.nodes.values().any(is_claim)) {
                continue;
            }
            out.push(
                Finding::new(
                    self.code(),
                    self.severity(),
                    format!("beat `{}` claims \"{}\", and none of its states shows a claim", beat.id, beat.claim),
                )
                .at(pointer)
                .hint("Mark the node that says the claim `semantic: claim` (or `takeaway`), or add one."),
            );
        }
        out
    }
}

/// W421: a slide that ends showing evidence and no claim.
struct W421EvidenceWithoutClaim;
impl Rule for W421EvidenceWithoutClaim {
    fn code(&self) -> &'static str {
        "W421"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let mut out = Vec::new();
        for i in slide_ends(cx) {
            let Some(snap) = cx.snapshots.get(i) else { continue };
            let evidence: Vec<&String> =
                snap.nodes.iter().filter(|(_, p)| semantic(p) == Some("evidence")).map(|(id, _)| id).collect();
            if evidence.is_empty() || snap.nodes.values().any(is_claim) {
                continue;
            }
            let names: Vec<String> = evidence.iter().map(|id| format!("`{id}`")).collect();
            out.push(
                Finding::new(
                    self.code(),
                    self.severity(),
                    format!(
                        "state `{}` ends its slide showing evidence ({}) and no claim",
                        snap.state_id,
                        names.join(", ")
                    ),
                )
                .at(format!("/states/{i}"))
                .state(snap.state_id.clone())
                .hint("Say what the evidence shows: a node with `semantic: claim` on the slide."),
            );
        }
        out
    }
}

/// W422: at the end of a slide, evidence set larger than the claim it supports. Text
/// only, by the size its role (or its `style.size`) gives it; a stat's numeral (role
/// `numeral`) is evidence meant to be big, and does not count.
struct W422EvidenceOutranksClaim;
impl Rule for W422EvidenceOutranksClaim {
    fn code(&self) -> &'static str {
        "W422"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let Some(theme) = cx.theme else { return vec![] };
        let mut out = Vec::new();
        for i in slide_ends(cx) {
            let Some(snap) = cx.snapshots.get(i) else { continue };
            let size = |props: &Props| -> Option<(String, f64)> {
                let role = role(cx, snap, props)?;
                let own = props.get("style").and_then(|s| s.get("size")).and_then(Value::as_f64);
                Some((role.clone(), own.or(theme.typography.roles.get(&role).map(|r| r.size))?))
            };
            let text = |id: &String| cx.deck.nodes.get(id).is_some_and(|n| n.node_type == NodeType::Text);
            let largest = |pick: &dyn Fn(&Props) -> bool| {
                snap.nodes
                    .iter()
                    .filter(|(id, p)| text(id) && pick(p))
                    .filter_map(|(id, p)| size(p).map(|(role, s)| (id, role, s)))
                    .filter(|(_, role, _)| role != "numeral")
                    .max_by(|a, b| a.2.total_cmp(&b.2))
            };
            let (Some((e, _, es)), Some((c, _, cs))) =
                (largest(&|p| semantic(p) == Some("evidence")), largest(&|p| is_claim(p)))
            else {
                continue;
            };
            if es > cs {
                out.push(
                    Finding::new(
                        self.code(),
                        self.severity(),
                        format!(
                            "state `{}`: evidence `{e}` ({es} cu) is set larger than the claim `{c}` ({cs} cu)",
                            snap.state_id
                        ),
                    )
                    .at(format!("/states/{i}"))
                    .state(snap.state_id.clone())
                    .node(e.clone())
                    .measure(serde_json::json!({ "evidence": es, "claim": cs }))
                    .hint("Give the claim the larger role; the evidence supports it."),
                );
            }
        }
        out
    }
}

/// A text node's role: its own, else its slot's in the state's layout, else `body`.
fn role(cx: &Context, snap: &Snapshot, props: &Props) -> Option<String> {
    if let Some(role) = props.get("role").and_then(Value::as_str) {
        return Some(role.to_string());
    }
    let slot = props.get("at").and_then(|at| at.get("in")).and_then(Value::as_str);
    let from_slot = (|| {
        let layout = cx.theme?.layouts.get(snap.layout.as_deref()?)?;
        layout.slots.get(slot?)?.role.clone()
    })();
    Some(from_slot.unwrap_or_else(|| "body".to_string()))
}

/// W423: a beat whose `evidence` names a data source or an asset that none of its states
/// shows (a chart or table reading the source, an image of the asset). A figure from the
/// data set as text is a literal that lint cannot trace to its source (finding 6), so a
/// node marked `semantic: evidence` shows a data source too. URLs are cited, not shown,
/// and are not checked.
struct W423EvidenceNotShown;
impl Rule for W423EvidenceNotShown {
    fn code(&self) -> &'static str {
        "W423"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let mut out = Vec::new();
        for (pointer, beat) in beats(cx) {
            let shown: Vec<&Snapshot> = cx.snapshots.iter().filter(|s| beat.states.contains(&s.state_id)).collect();
            if shown.is_empty() {
                continue;
            }
            for (k, cited) in beat.evidence.iter().enumerate() {
                let key = if cited.starts_with('@') {
                    "data"
                } else if cited.contains("://") {
                    continue;
                } else {
                    "src"
                };
                let reads = |p: &Props| p.get(key).and_then(Value::as_str) == Some(cited);
                let stands_in = |p: &Props| key == "data" && semantic(p) == Some("evidence");
                let seen = shown.iter().any(|s| s.nodes.values().any(|p| reads(p) || stands_in(p)));
                if !seen {
                    out.push(
                        Finding::new(
                            self.code(),
                            self.severity(),
                            format!("beat `{}` cites `{cited}`, and none of its states shows it", beat.id),
                        )
                        .at(format!("{pointer}/evidence/{k}"))
                        .hint("Show it in one of the beat's states (a chart or table on the source, an image of the asset), or drop the citation."),
                    );
                }
            }
        }
        out
    }
}

/// W424: more than one claim on screen at once. A slide makes one point; a comparison is
/// `semantic: comparison`.
struct W424TooManyClaims;
impl Rule for W424TooManyClaims {
    fn code(&self) -> &'static str {
        "W424"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let mut out = Vec::new();
        for (i, snap) in cx.snapshots.iter().enumerate() {
            let claims: Vec<String> = snap
                .nodes
                .iter()
                .filter(|(_, p)| semantic(p) == Some("claim"))
                .map(|(id, _)| format!("`{id}`"))
                .collect();
            if claims.len() > 1 {
                out.push(
                    Finding::new(
                        self.code(),
                        self.severity(),
                        format!(
                            "state `{}` shows {} claims at once ({})",
                            snap.state_id,
                            claims.len(),
                            claims.join(", ")
                        ),
                    )
                    .at(format!("/states/{i}"))
                    .state(snap.state_id.clone())
                    .measure(serde_json::json!({ "claims": claims.len(), "max": 1 }))
                    .hint("One claim per state: split the slide, or mark the others `evidence` or `comparison`."),
                );
            }
        }
        out
    }
}

/// W425: two beats in a row that claim the same thing, ignoring case, spacing, and the
/// closing stop.
struct W425RepeatedClaim;
impl Rule for W425RepeatedClaim {
    fn code(&self) -> &'static str {
        "W425"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let norm = |s: &str| {
            let words: Vec<String> = s.split_whitespace().map(str::to_lowercase).collect();
            words.join(" ").trim_end_matches(['.', '!', '?']).to_string()
        };
        let all = beats(cx);
        all.windows(2)
            .filter(|w| norm(&w[0].1.claim) == norm(&w[1].1.claim))
            .map(|w| {
                Finding::new(
                    self.code(),
                    self.severity(),
                    format!("beats `{}` and `{}` make the same claim: \"{}\"", w[0].1.id, w[1].1.id, w[1].1.claim),
                )
                .at(format!("{}/claim", w[1].0))
                .hint("Merge the beats, or say what the second one adds.")
            })
            .collect()
    }
}

/// W426: a beat that comes after another in the spine while its states play before that
/// one's. The PDF reads a deck in spine order and the video in state order, so the two
/// would tell the story in different orders. A beat stands where its first state plays;
/// one with no states, or none the deck has, is not judged.
struct W426SpineOutOfOrder;
impl Rule for W426SpineOutOfOrder {
    fn code(&self) -> &'static str {
        "W426"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Context) -> Vec<Finding> {
        let plays = |id: &str| cx.deck.states.iter().position(|s| s.id == id);
        // The beat whose states play last so far, and where.
        let mut latest: Option<(usize, &str)> = None;
        let mut out = Vec::new();
        for (path, beat) in beats(cx) {
            let Some(first) = beat.states.iter().filter_map(|s| plays(s)).min() else { continue };
            match latest {
                Some((at, before)) if first < at => out.push(
                    Finding::new(
                        self.code(),
                        self.severity(),
                        format!(
                            "beat `{}` comes after `{before}` in the spine, and its states play before `{before}`'s: \
                             the PDF and the video would tell them in different orders",
                            beat.id
                        ),
                    )
                    .at(path)
                    .hint("Move the beat to where its states play, or move its states to where the beat stands."),
                ),
                _ => latest = Some((first, beat.id.as_str())),
            }
        }
        out
    }
}
