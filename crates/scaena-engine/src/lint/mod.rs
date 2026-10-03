//! Layout-level lint (SPEC §7.5): what a deck shows once it is laid out. Text that does
//! not fit or lacks glyphs, content that collides, contrast against what is painted
//! behind it, lines, chart labels, and motion, in the deck's own format and in each of
//! its `formats`. The document-level rules are `scaena_core::lint`'s.
//!
//! Lint lays out what a frame refuses (text under `fit: error` that does not fit, a
//! table whose rows do not) and reports it, where a frame would stop at the first.

mod contrast;
mod motion;
mod space;
mod text;

use crate::EngineError;
use crate::cascade;
use crate::data::DataFiles;
use crate::motion as cues;
use crate::render::{Engine, project};
use crate::sample::{Scene, Timing, Transition};
use crate::theme::Theme;
use scaena_core::document::Deck;
use scaena_core::lint::{Backdrop, Finding, Severity};
use scaena_core::timeline::{Schedule, Slot};
use scaena_core::tracking::Snapshot;
use std::borrow::Cow;

/// A layout-level rule: a stable code from SPEC §7.5, and what it finds in the deck as
/// laid out in one format.
pub trait Rule {
    fn code(&self) -> &'static str;
    fn severity(&self) -> Severity;
    fn check(&self, cx: &Cx) -> Vec<Finding>;
}

/// The layout-level rule set, contrast aside (it needs a painter). Order is the report
/// order.
pub fn rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(text::E100Overflow),
        Box::new(space::E101Collision),
        Box::new(text::E120MissingGlyphs),
        Box::new(text::W200Widow),
        Box::new(text::W201Measure),
        Box::new(text::W202MaxLines),
        Box::new(text::W203ShrinkFloor),
        Box::new(text::W220MixedAlignment),
        Box::new(text::W310LabelCollision),
        Box::new(space::W311ShaderBehindData),
        Box::new(text::W312ChartTextSize),
        Box::new(motion::W320Concurrent),
        Box::new(motion::W321Build),
    ]
}

/// The deck laid out in one format: what layout rules read.
pub struct Cx<'a> {
    /// The deck and theme as projected into the format (SPEC §3.4).
    pub deck: &'a Deck,
    pub theme: &'a Theme,
    /// `None` in the deck's own format.
    pub format: Option<&'a str>,
    pub states: &'a [Laid],
}

/// One state, laid out.
pub struct Laid {
    pub index: usize,
    /// The resolved snapshot with the deck's overrides merged in: what the frame draws.
    pub snapshot: Snapshot,
    pub scene: Scene,
    /// Where it falls on the format's global timeline.
    pub slot: Slot,
    /// Its motions on its clock, with the transition into it.
    pub schedule: Schedule,
}

impl Cx<'_> {
    /// A finding about `state`, in this format.
    pub fn finding(&self, code: &str, severity: Severity, state: &Laid, message: String) -> Finding {
        let f = Finding::new(code, severity, message).state(state.snapshot.state_id.clone());
        match self.format {
            Some(format) => f.format(format),
            None => f,
        }
    }

    /// The pointer to `node` in the deck: its entry in `nodes`.
    pub fn node_path(&self, node: &str) -> String {
        format!("/nodes/{}", node.replace('~', "~0").replace('/', "~1"))
    }
}

/// Everything the layout-level rules find in `deck`, in its own format and each of its
/// `formats`. `backdrop` paints for the contrast rules (E110, E111); without one they
/// do not run.
pub fn lint(
    engine: &mut Engine,
    deck: &Deck,
    theme: &Theme,
    data: &DataFiles,
    mut backdrop: Option<&mut dyn Backdrop>,
) -> Result<Vec<Finding>, EngineError> {
    engine.lenient = true;
    let mut out = Vec::new();
    let mut formats: Vec<Option<&str>> = vec![None];
    formats.extend(deck.formats.iter().map(|f| Some(f.as_str())));
    let result = (|| {
        for format in formats {
            let (d, t) = project(deck, theme, format)?;
            // A listed format of the deck's own shape lays out the same.
            if format.is_some() && matches!(d, Cow::Borrowed(_)) {
                continue;
            }
            let states = lay_out(engine, &d, &t, data)?;
            let cx = Cx { deck: &d, theme: &t, format, states: &states };
            for rule in rules() {
                // Motion is the same in every format but for what layout counts; it is
                // judged once, in the deck's own.
                if format.is_some() && matches!(rule.code(), "W320" | "W321") {
                    continue;
                }
                out.extend(rule.check(&cx));
            }
            if let Some(b) = backdrop.as_deref_mut() {
                out.extend(contrast::check(&cx, b)?);
            }
        }
        Ok(())
    })();
    let result = result.and_then(|()| verify(engine, deck, theme, data, &mut out));
    engine.lenient = false;
    result.map(|()| out)
}

/// Keep a finding's fix only where laying its state out again with the fix applied
/// shows that it works: the text fits, within its lines, at a size its bounds allow.
fn verify(
    engine: &mut Engine,
    deck: &Deck,
    theme: &Theme,
    data: &DataFiles,
    findings: &mut [Finding],
) -> Result<(), EngineError> {
    let json = serde_json::to_value(deck).map_err(|e| EngineError::Layout(e.to_string()))?;
    for f in findings.iter_mut().filter(|f| f.fix.is_some()) {
        let works = (|| -> Option<bool> {
            let mut doc = json.clone();
            scaena_core::patch::apply(&mut doc, f.fix.as_deref()?).ok()?;
            let patched: Deck = serde_json::from_value(doc).ok()?;
            let (d, t) = project(&patched, theme, f.format.as_deref()).ok()?;
            let snaps = scaena_core::resolve_states(&d).ok()?;
            let index = snaps.iter().position(|s| Some(&s.state_id) == f.state.as_ref())?;
            let scene = engine.scene(&d, &t, data, &snaps[index]).ok()?;
            let state = Laid {
                index,
                snapshot: cascade::with_overrides(&d, &snaps[index]),
                scene,
                slot: Slot { state: snaps[index].state_id.clone(), start: 0.0, span: 0.0, hold: 0.0 },
                schedule: Schedule { transition: Timing::CUT.clock(), cues: vec![], span: 0.0 },
            };
            let cx = Cx { deck: &d, theme: &t, format: f.format.as_deref(), states: std::slice::from_ref(&state) };
            let rules: [&dyn Rule; 3] = [&text::E100Overflow, &text::W202MaxLines, &text::W203ShrinkFloor];
            Some(rules.iter().flat_map(|r| r.check(&cx)).all(|g| g.node != f.node))
        })();
        if works != Some(true) {
            f.fix = None;
        }
    }
    Ok(())
}

/// Every state of the (projected) deck, laid out, with its slot and its cue.
fn lay_out(engine: &mut Engine, deck: &Deck, theme: &Theme, data: &DataFiles) -> Result<Vec<Laid>, EngineError> {
    let snapshots = scaena_core::resolve_states(deck)?;
    let timeline = engine.timeline(deck, theme, data)?;
    let mut out: Vec<Laid> = Vec::with_capacity(snapshots.len());
    for (i, snap) in snapshots.iter().enumerate() {
        let scene = engine.scene(deck, theme, data, snap)?;
        let state = &deck.states[i];
        let timing = Timing::parse(theme, state.transition.as_ref())?;
        let before = i.checked_sub(1).map(|p| &snapshots[p]);
        let items = cues::items(deck, theme, state, before, snap, timing.matched)?;
        let from = i.checked_sub(1).map(|p| out[p].scene.clone());
        let schedule = Transition::new(from, scene.clone(), timing, &items, 0.0)?.schedule().clone();
        out.push(Laid {
            index: i,
            snapshot: cascade::with_overrides(deck, snap),
            scene,
            slot: timeline.slots[i].clone(),
            schedule,
        });
    }
    Ok(out)
}
