//! Motion as scheduled (SPEC §3.9): how many nodes move at once (W320), and how long a
//! state's motions run (W321), against the theme's `motion.maxConcurrent` (12) and
//! `motion.maxBuild` (2500 ms); and a state the global timeline gives no time, in a deck
//! that runs on its own (W323, SPEC §2.4).

use super::{Cx, Laid, Rule};
use scaena_core::document::NodeType;
use scaena_core::lint::{Finding, Severity, words};
use serde_json::{Value, json};

/// W320: more nodes moving at one time than the theme's `maxConcurrent`.
pub struct W320Concurrent;
impl Rule for W320Concurrent {
    fn code(&self) -> &'static str {
        "W320"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let max = cx.theme.motion.max_concurrent.unwrap_or(12) as usize;
        let mut out = Vec::new();
        for state in cx.states {
            // Each node's time in motion, then the most nodes at any one time: a node
            // moves from its first motion's start to its last one's end.
            let mut spans: Vec<(&str, f64, f64)> = Vec::new();
            for cue in &state.schedule.cues {
                match spans.iter_mut().find(|(n, _, _)| *n == cue.node) {
                    Some(span) => (span.1, span.2) = (span.1.min(cue.start), span.2.max(cue.end())),
                    None => spans.push((&cue.node, cue.start, cue.end())),
                }
            }
            let (mut most, mut at) = (0, 0.0);
            for &(_, t, _) in &spans {
                let moving = spans.iter().filter(|(_, s, e)| *s <= t && t < *e).count();
                if moving > most {
                    (most, at) = (moving, t);
                }
            }
            if most > max {
                out.push(
                    cx.finding(
                        self.code(),
                        self.severity(),
                        state,
                        format!(
                            "state `{}` moves {most} nodes at once ({at:.0} ms in); the theme allows {max}",
                            state.snapshot.state_id
                        ),
                    )
                    .at(format!("/states/{}", state.index))
                    .measure(json!({ "nodes": most, "at": at, "maxConcurrent": max }))
                    .hint(
                        "Stagger them, put them in a `sequence`, or move a group as one (choreograph its container).",
                    ),
                );
            }
        }
        out
    }
}

/// W321: a state whose motions run past the theme's `maxBuild`.
pub struct W321Build;
impl Rule for W321Build {
    fn code(&self) -> &'static str {
        "W321"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        let max = cx.theme.motion.max_build.unwrap_or(2500.0);
        let mut out = Vec::new();
        for state in cx.states {
            let end = state.schedule.cues.iter().map(|c| c.end()).fold(0.0, f64::max);
            if end > max {
                out.push(
                    cx.finding(
                        self.code(),
                        self.severity(),
                        state,
                        format!(
                            "state `{}`'s motions run {end:.0} ms; the theme's `maxBuild` is {max:.0}",
                            state.snapshot.state_id
                        ),
                    )
                    .at(format!("/states/{}/choreography", state.index))
                    .measure(json!({ "ms": end, "maxBuild": max }))
                    .hint("Shorten the staggers and delays, or split the build into another state."),
                );
            }
        }
        out
    }
}

/// W323: a state that shows for no time in a deck that runs on its own. On the global
/// timeline a state lasts its span, then its hold (SPEC §2.4): with both 0, a video has
/// no frame of it, and a player's auto-advance passes over it. A deck with no hold is
/// presented live, where each state shows until the next click, and is not judged; nor
/// is a state with a cue, which plays it on its way to the next.
pub struct W323Skipped;
impl Rule for W323Skipped {
    fn code(&self) -> &'static str {
        "W323"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, cx: &Cx) -> Vec<Finding> {
        if !cx.states.iter().any(|s| s.slot.hold > 0.0) {
            return vec![];
        }
        let mut out = Vec::new();
        for state in cx.states.iter().filter(|s| s.slot.span + s.slot.hold <= 0.0) {
            let (words, figures) = reading(cx, state);
            let hold = hold_for(words, figures);
            out.push(
                cx.finding(
                    self.code(),
                    self.severity(),
                    state,
                    format!(
                        "state `{}` shows for 0 ms: it cuts in, moves nothing, and has no `hold`, so a video has no frame of it; give it a `hold` of about {hold:.0} ms",
                        state.snapshot.state_id
                    ),
                )
                .at(format!("/states/{}", state.index))
                .measure(json!({ "words": words, "figures": figures, "suggestedHold": hold }))
                .hint(
                    "The deck has holds, so it runs on its own. Hold each state for its reading: about four words a second, plus 2 s for each chart, table, or image.",
                ),
            );
        }
        out
    }
}

/// What a state gives to read, decoration aside: the words of its text, and its figures
/// (charts, tables, and images).
fn reading(cx: &Cx, state: &Laid) -> (usize, usize) {
    let (mut text, mut figures) = (0, 0);
    for (id, props) in &state.snapshot.nodes {
        let said = |key: &str| props.get(key).and_then(Value::as_str);
        if said("semantic") == Some("decoration") || said("alt") == Some("") {
            continue;
        }
        match cx.deck.nodes.get(id).map(|n| n.node_type) {
            Some(NodeType::Text) => text += words(props),
            Some(NodeType::Chart | NodeType::Table | NodeType::Image) => figures += 1,
            _ => {}
        }
    }
    (text, figures)
}

/// A hold to read `words` words and look at `figures` figures by, as the motion-pass
/// skill sizes one: four words a second and 2 s a figure, rounded up to the half second,
/// and never under 2 s.
fn hold_for(words: usize, figures: usize) -> f64 {
    let ms = words as f64 * 250.0 + figures as f64 * 2000.0;
    ((ms / 500.0).ceil() * 500.0).max(2000.0)
}

#[cfg(test)]
mod tests {
    use super::hold_for;

    #[test]
    fn a_hold_reads_four_words_a_second_and_two_seconds_a_figure() {
        assert_eq!(hold_for(14, 0), 3500.0);
        assert_eq!(hold_for(13, 0), 3500.0, "rounded up to the half second");
        assert_eq!(hold_for(9, 1), 4500.0);
        assert_eq!(hold_for(40, 2), 14000.0);
        assert_eq!(hold_for(2, 0), 2000.0, "never under 2 s");
        assert_eq!(hold_for(0, 0), 2000.0);
    }
}
