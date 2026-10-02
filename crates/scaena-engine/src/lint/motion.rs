//! Motion as scheduled (SPEC §3.9): how many nodes move at once (W320), and how long a
//! state's motions run (W321), against the theme's `motion.maxConcurrent` (12) and
//! `motion.maxBuild` (2500 ms).

use super::{Cx, Rule};
use scaena_core::lint::{Finding, Severity};
use serde_json::json;

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
