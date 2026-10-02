//! When each motion of a state runs (SPEC §3.9), and the deck's states end to end
//! (SPEC §2.4).
//!
//! A state's cue starts with the transition into it (`t = 0`). Its motions are
//! [`Item`]s: the [`Cue`]s a state's choreography lists (in `sequence` and `parallel`
//! groups), and the ones its nodes ask for themselves (an `enter` or `exit` preset,
//! an `emphasis`, `anim` tracks). [`schedule`] places each on the state's clock:
//! - A top-level item starts with the transition (`timing: with`) or once it ends
//!   (`after`), then waits its `delay`.
//! - In a `sequence`, each item starts when the one before it ends, then waits its own
//!   `delay`; in a `parallel`, every item starts with the group, then waits its own.
//!   The group's `timing` and `delay` place the group; its items' own `timing` does not
//!   apply.
//! - A cue moves its targets' units, in target order, `stagger` ms apart: the whole node
//!   is one unit, or its cue splits it into lines, words, glyphs, children, or marks,
//!   which the caller counts after layout.
//!
//! A state's span is when its transition and its last motion have both ended: from then
//! on, it is at rest.

use super::look::{Clock, Curve, Motion};
use crate::model::states::Timing;
use crate::model::values::SplitUnit;

/// One motion over the units of one or more nodes, resolved against the theme.
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub targets: Vec<String>,
    /// What each target splits into; `None` moves each whole.
    pub split: Option<SplitUnit>,
    pub motion: Motion,
    pub timing: Timing,
    /// Before the first unit starts, ms.
    pub delay: f64,
    /// Between one unit's start and the next's, ms.
    pub stagger: f64,
    /// Each unit's time, ms: for a spring, its settle time.
    pub duration: f64,
    pub curve: Curve,
}

/// A state's motions, as its choreography groups them.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Cue(Cue),
    Sequence { items: Vec<Item>, delay: f64, timing: Timing },
    Parallel { items: Vec<Item>, delay: f64, timing: Timing },
}

impl Item {
    fn timing(&self) -> Timing {
        match self {
            Item::Cue(c) => c.timing,
            Item::Sequence { timing, .. } | Item::Parallel { timing, .. } => *timing,
        }
    }

    fn delay(&self) -> f64 {
        match self {
            Item::Cue(c) => c.delay,
            Item::Sequence { delay, .. } | Item::Parallel { delay, .. } => *delay,
        }
    }
}

/// One node's part of a cue, placed on its state's clock: `units` units, the `k`th
/// starting `k × stagger` after `start`.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub node: String,
    pub split: Option<SplitUnit>,
    pub motion: Motion,
    pub start: f64,
    pub stagger: f64,
    pub duration: f64,
    pub curve: Curve,
    pub units: usize,
}

impl Placed {
    /// Unit `k`'s clock.
    pub fn clock(&self, k: usize) -> Clock {
        Clock { start: self.start + k as f64 * self.stagger, duration: self.duration, curve: self.curve }
    }

    /// When the last unit comes to rest, ms into the cue.
    pub fn end(&self) -> f64 {
        self.clock(self.units.saturating_sub(1)).end()
    }
}

/// A state's cue: the transition's clock, its motions placed on the same clock, and
/// its span.
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    pub transition: Clock,
    pub cues: Vec<Placed>,
    pub span: f64,
}

impl Schedule {
    /// The motions on `node`, in the order the state lists them.
    pub fn of<'a>(&'a self, node: &'a str) -> impl Iterator<Item = &'a Placed> + 'a {
        self.cues.iter().filter(move |c| c.node == node)
    }
}

/// Counts what a cue's target splits into, for the cue's motion: an entrance counts
/// what the node is in the state, an exit what it was in the state before.
pub type Units<'a> = dyn FnMut(&str, SplitUnit, &Motion) -> usize + 'a;

/// Places a state's motions on its clock, after `transition`. `units` counts what a
/// split cue's targets split into; a target with none takes no time.
pub fn schedule(transition: Clock, items: &[Item], units: &mut Units) -> Schedule {
    let mut cues = Vec::new();
    let mut span = transition.end();
    for item in items {
        let at = match item.timing() {
            Timing::With => transition.start,
            Timing::After => transition.end(),
        };
        span = span.max(place(item, at, &mut cues, units));
    }
    Schedule { transition, cues, span }
}

/// Places `item` from `at` (before its own delay); returns when it ends.
fn place(item: &Item, at: f64, out: &mut Vec<Placed>, units: &mut Units) -> f64 {
    let start = at + item.delay();
    match item {
        Item::Cue(cue) => {
            let (mut k, mut end) = (0, start);
            for node in &cue.targets {
                let n = cue.split.map_or(1, |split| units(node, split, &cue.motion));
                if n == 0 {
                    continue;
                }
                let placed = Placed {
                    node: node.clone(),
                    split: cue.split,
                    motion: cue.motion.clone(),
                    start: start + k as f64 * cue.stagger,
                    stagger: cue.stagger,
                    duration: cue.duration,
                    curve: cue.curve,
                    units: n,
                };
                end = end.max(placed.end());
                out.push(placed);
                k += n;
            }
            end
        }
        Item::Sequence { items, .. } => items.iter().fold(start, |cursor, item| place(item, cursor, out, units)),
        Item::Parallel { items, .. } => items.iter().fold(start, |end, item| end.max(place(item, start, out, units))),
    }
}

/// One state on the global timeline, ms.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub state: String,
    /// When the transition into it starts.
    pub start: f64,
    /// Its transition and motions, from `start`.
    pub span: f64,
    /// Its dwell at rest before the next state starts.
    pub hold: f64,
}

impl Slot {
    /// When the next state starts.
    pub fn end(&self) -> f64 {
        self.start + self.span + self.hold
    }
}

/// The deck's states end to end, each its span and then its hold (SPEC §2.4): what a
/// video samples, and what shaders keep time by.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Timeline {
    pub slots: Vec<Slot>,
}

impl Timeline {
    /// The states in cue-list order, each `(state, span, hold)`.
    pub fn new(states: impl IntoIterator<Item = (String, f64, f64)>) -> Timeline {
        let mut start = 0.0;
        let slots = states
            .into_iter()
            .map(|(state, span, hold)| {
                let slot = Slot { state, start, span, hold };
                start = slot.end();
                slot
            })
            .collect();
        Timeline { slots }
    }

    pub fn slot(&self, state: &str) -> Option<&Slot> {
        self.slots.iter().find(|s| s.state == state)
    }

    /// The whole timeline, ms.
    pub fn duration(&self) -> f64 {
        self.slots.last().map_or(0.0, Slot::end)
    }

    /// The state on screen `ms` into the timeline, and how far into its cue: the first
    /// state before 0, the last one past the end.
    pub fn locate(&self, ms: f64) -> Option<(&Slot, f64)> {
        let slot = self.slots.iter().rev().find(|s| s.start <= ms).or(self.slots.first())?;
        Some((slot, ms - slot.start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{CubicBezier, Look};

    const LINEAR: Curve = Curve::Ease(CubicBezier::LINEAR);

    fn cue(targets: &[&str], timing: Timing, delay: f64, stagger: f64, duration: f64) -> Cue {
        Cue {
            targets: targets.iter().map(|s| s.to_string()).collect(),
            split: None,
            motion: Motion::Enter(Look { opacity: 0.0, ..Look::REST }),
            timing,
            delay,
            stagger,
            duration,
            curve: LINEAR,
        }
    }

    fn transition(ms: f64) -> Clock {
        Clock { start: 0.0, duration: ms, curve: LINEAR }
    }

    fn starts(s: &Schedule) -> Vec<(&str, f64)> {
        s.cues.iter().map(|c| (c.node.as_str(), c.start)).collect()
    }

    #[test]
    fn with_starts_at_the_transition_and_after_once_it_ends() {
        let items = [
            Item::Cue(cue(&["a"], Timing::With, 0.0, 0.0, 100.0)),
            Item::Cue(cue(&["b"], Timing::After, 50.0, 0.0, 100.0)),
        ];
        let s = schedule(transition(400.0), &items, &mut |_, _, _| 1);
        assert_eq!(starts(&s), [("a", 0.0), ("b", 450.0)]);
        assert_eq!(s.span, 550.0);
        let quiet = schedule(transition(400.0), &[], &mut |_, _, _| 1);
        assert_eq!(quiet.span, 400.0, "with no motions, the transition is the span");
    }

    #[test]
    fn units_stagger_across_targets_in_order() {
        let mut split = cue(&["title", "subtitle"], Timing::With, 0.0, 30.0, 200.0);
        split.split = Some(SplitUnit::Words);
        let s = schedule(transition(0.0), &[Item::Cue(split)], &mut |node, _, _| if node == "title" { 3 } else { 2 });
        assert_eq!(starts(&s), [("title", 0.0), ("subtitle", 90.0)]);
        assert_eq!((s.cues[0].units, s.cues[1].clock(1).start), (3, 120.0));
        assert_eq!(s.span, 120.0 + 200.0);
        let mut empty = cue(&["blank", "title"], Timing::With, 0.0, 30.0, 200.0);
        empty.split = Some(SplitUnit::Lines);
        let s = schedule(transition(0.0), &[Item::Cue(empty)], &mut |node, _, _| usize::from(node == "title"));
        assert_eq!(starts(&s), [("title", 0.0)], "a target with no units takes no time");
    }

    #[test]
    fn sequences_follow_on_and_parallels_start_together() {
        let sequence = Item::Sequence {
            items: vec![
                Item::Cue(cue(&["note"], Timing::With, 0.0, 0.0, 300.0)),
                Item::Cue(cue(&["arrow"], Timing::With, 20.0, 0.0, 100.0)),
                Item::Parallel {
                    items: vec![
                        Item::Cue(cue(&["x"], Timing::After, 0.0, 0.0, 50.0)),
                        Item::Cue(cue(&["y"], Timing::After, 10.0, 0.0, 200.0)),
                    ],
                    delay: 5.0,
                    timing: Timing::With,
                },
            ],
            delay: 200.0,
            timing: Timing::After,
        };
        let s = schedule(transition(400.0), &[sequence], &mut |_, _, _| 1);
        // The group starts 200 ms after the transition; its items' own timing is moot.
        assert_eq!(starts(&s), [("note", 600.0), ("arrow", 920.0), ("x", 1025.0), ("y", 1035.0)]);
        assert_eq!(s.span, 1235.0);
        assert_eq!(s.of("arrow").count(), 1);
    }

    #[test]
    fn states_lie_end_to_end_with_their_holds() {
        let t = Timeline::new([("a".to_string(), 400.0, 1000.0), ("b".into(), 600.0, 0.0), ("c".into(), 0.0, 500.0)]);
        let starts: Vec<f64> = t.slots.iter().map(|s| s.start).collect();
        assert_eq!(starts, [0.0, 1400.0, 2000.0]);
        assert_eq!(t.duration(), 2500.0);
        let at = |ms: f64| t.locate(ms).map(|(s, t)| (s.state.clone(), t)).unwrap();
        assert_eq!(at(0.0), ("a".into(), 0.0));
        assert_eq!(at(1399.0), ("a".into(), 1399.0), "a holds at rest");
        assert_eq!(at(1500.0), ("b".into(), 100.0));
        assert_eq!(at(9000.0), ("c".into(), 7000.0));
        assert_eq!(at(-5.0), ("a".into(), -5.0));
        assert_eq!(t.slot("b").unwrap().end(), 2000.0);
        assert!(Timeline::default().locate(0.0).is_none());
    }
}
