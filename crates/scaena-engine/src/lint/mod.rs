//! Layout-level lint (SPEC §7.5): what a deck shows once it is laid out. Text that does
//! not fit or lacks glyphs, content that collides, contrast against what is painted
//! behind it, lines, chart labels, and motion, in the deck's own format and in each of
//! its `formats`; and, once, fonts whose licenses do not allow what a bundle does with
//! them. The document-level rules are `scaena_core::lint`'s.
//!
//! Lint lays out what a frame refuses (text under `fit: error` that does not fit, a
//! table whose rows do not) and reports it, where a frame would stop at the first.

mod contrast;
mod fonts;
mod motion;
mod pictures;
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
use scaena_core::timeline::{Schedule, Slot, Timeline};
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
        Box::new(text::W205CodeSize),
        Box::new(text::W220MixedAlignment),
        Box::new(text::W231Upright),
        Box::new(text::W232Lighter),
        Box::new(space::W303UnderFurniture),
        Box::new(space::W304EmptyPlace),
        Box::new(text::W310LabelCollision),
        Box::new(space::W311ShaderBehindData),
        Box::new(text::W312ChartTextSize),
        Box::new(space::W313ChartSquashed),
        Box::new(space::W314Credit),
        Box::new(motion::W320Concurrent),
        Box::new(motion::W321Build),
        Box::new(motion::W323Skipped),
        Box::new(space::I405Crowded),
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

    /// State `index` of the deck, as laid out: what a rule reports a finding at, where it
    /// may have laid out only some states.
    pub fn laid(&self, index: usize) -> &Laid {
        self.states.iter().find(|s| s.index == index).expect("a rule reports at a state it was given")
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
    backdrop: Option<&mut dyn Backdrop>,
) -> Result<Vec<Finding>, EngineError> {
    lint_in(engine, deck, theme, data, backdrop, None, true)
}

/// What the layout-level rules find in one state of `deck`, in every format, laid out
/// after the state before it: what an editor answers at once for the state being edited
/// (PLAN 2.3), where [`lint`] lays out every state. A finding that holds from an earlier
/// state on, which [`lint`] reports where it starts, is reported at this one.
pub fn lint_state(
    engine: &mut Engine,
    deck: &Deck,
    theme: &Theme,
    data: &DataFiles,
    backdrop: Option<&mut dyn Backdrop>,
    state: &str,
) -> Result<Vec<Finding>, EngineError> {
    lint_in(engine, deck, theme, data, backdrop, Some(&[state]), false)
}

/// What the layout-level rules find in `states` of `deck`, in every format, each laid out after
/// the state before it, as [`lint_state`] lays one out; and what is judged once for the deck,
/// the figures its texts quote: what a patch changes, judged before and after it (SPEC §7.3).
/// A finding that holds across several of them is reported at the first, as [`lint`] reports
/// it.
pub fn lint_states(
    engine: &mut Engine,
    deck: &Deck,
    theme: &Theme,
    data: &DataFiles,
    backdrop: Option<&mut dyn Backdrop>,
    states: &[&str],
) -> Result<Vec<Finding>, EngineError> {
    lint_in(engine, deck, theme, data, backdrop, Some(states), true)
}

fn lint_in(
    engine: &mut Engine,
    deck: &Deck,
    theme: &Theme,
    data: &DataFiles,
    mut backdrop: Option<&mut dyn Backdrop>,
    only: Option<&[&str]>,
    quotes: bool,
) -> Result<Vec<Finding>, EngineError> {
    let mut linting = Linting::new(deck, only, quotes);
    while !linting.step(engine, deck, theme, data, backdrop.as_deref_mut())? {}
    Ok(linting.found())
}

/// The layout rules on a deck, as [`lint`] runs them, a step at a time (lift-up 20261110
/// PERF-1): each state laid out, and each state's contrast judged, in a step of its own, so a
/// client with one thread lays a large deck out between the other work it answers, and drops
/// it for a newer deck. [`Linting::step`] each time with the same deck, theme, data, and
/// backdrop, until it says it is done; what it found is then [`Linting::found`], what [`lint`]
/// finds, in the same order.
pub struct Linting {
    /// The states asked for, by id; every state where none are.
    only: Option<Vec<String>>,
    quotes: bool,
    /// The deck's own format first, then each it lists.
    formats: Vec<Option<String>>,
    /// The format being laid out, by its place in `formats`.
    format: usize,
    phase: Phase,
    out: Vec<Finding>,
    /// Each format's timeline, as laid out, by its place in `formats`: where a shader's clock
    /// puts each state.
    timelines: Vec<(usize, Timeline)>,
    /// Each run of a node's own text that read too faintly, in any format: what a color that
    /// fixes the node must read over (PLAN 2.82).
    faint: std::collections::BTreeMap<String, Vec<contrast::Faint>>,
}

enum Phase {
    /// Nothing done yet.
    Begin,
    /// A format being laid out and judged.
    Format(Box<Pass>),
    /// Every format done: each fix checked by laying its state out again.
    Verify,
    /// Text too faint given the color that reads, and the quoted figures.
    Close,
    Done,
}

/// One format, as far as it has been laid out and judged.
struct Pass {
    /// The deck and theme as projected into the format, where that is not the deck's own.
    deck: Option<Deck>,
    theme: Option<Theme>,
    snapshots: Vec<Snapshot>,
    timeline: Option<Timeline>,
    /// The states to lay out, by index, and how many are laid out.
    every: Vec<usize>,
    laid: Vec<Laid>,
    /// Whether the rules have judged the states laid out, and, as contrast is judged state by
    /// state after, what it has found.
    ruled: bool,
    contrast: Option<(contrast::Pass, usize)>,
}

impl Linting {
    /// The layout rules on `deck`, on every state or on `only`, the figures its texts quote
    /// judged too where `quotes` (or every state is), not yet begun.
    pub fn new(deck: &Deck, only: Option<&[&str]>, quotes: bool) -> Self {
        let mut formats: Vec<Option<String>> = vec![None];
        formats.extend(deck.formats.iter().map(|f| Some(f.clone())));
        Self {
            only: only.map(|ids| ids.iter().map(|id| (*id).to_string()).collect()),
            quotes,
            formats,
            format: 0,
            phase: Phase::Begin,
            out: Vec::new(),
            timelines: Vec::new(),
            faint: std::collections::BTreeMap::new(),
        }
    }

    /// Whether every step is done.
    pub fn done(&self) -> bool {
        matches!(self.phase, Phase::Done)
    }

    /// What the rules found, once done.
    pub fn found(self) -> Vec<Finding> {
        self.out
    }

    /// Take the next step on `deck` as `engine` lays it out, with `backdrop` painting what is
    /// behind its text: whether it is done. An error ends it; nothing found stands.
    pub fn step(
        &mut self,
        engine: &mut Engine,
        deck: &Deck,
        theme: &Theme,
        data: &DataFiles,
        backdrop: Option<&mut (dyn Backdrop + '_)>,
    ) -> Result<bool, EngineError> {
        engine.lenient = true;
        let stepped = self.stepping(engine, deck, theme, data, backdrop);
        engine.lenient = false;
        if stepped.is_err() {
            self.phase = Phase::Done;
            self.out.clear();
        }
        stepped.map(|()| self.done())
    }

    fn stepping(
        &mut self,
        engine: &mut Engine,
        deck: &Deck,
        theme: &Theme,
        data: &DataFiles,
        backdrop: Option<&mut (dyn Backdrop + '_)>,
    ) -> Result<(), EngineError> {
        match std::mem::replace(&mut self.phase, Phase::Done) {
            Phase::Begin => {
                // What a font's license allows is the same in every format: judged once.
                self.out = fonts::W230FontLicense::check(deck, engine.fonts());
                self.phase = self.begin(engine, deck, theme, data)?;
            }
            Phase::Format(mut pass) => {
                if self.advance(&mut pass, engine, deck, theme, data, backdrop)? {
                    self.format += 1;
                    self.phase = self.begin(engine, deck, theme, data)?;
                } else {
                    self.phase = Phase::Format(pass);
                }
            }
            Phase::Verify => {
                verify(engine, deck, theme, data, &mut self.out)?;
                self.phase = Phase::Close;
            }
            Phase::Close => {
                // Text that reads too faintly, with the color that reads (PLAN 2.82): judged
                // once lint has found it in every format.
                if let Some(b) = backdrop {
                    let formats = &self.formats;
                    let timelines: Vec<(Option<&str>, Timeline)> = std::mem::take(&mut self.timelines)
                        .into_iter()
                        .map(|(at, timeline)| (formats[at].as_deref(), timeline))
                        .collect();
                    contrast::recolor(engine, (deck, theme, data), &timelines, &self.faint, b, &mut self.out)?;
                }
                // Quoted figures are the same in every format: judged once, and each fix is the
                // figure.
                if self.only.is_none() || self.quotes {
                    self.out.extend(w427(deck, data));
                }
                self.phase = Phase::Done;
            }
            Phase::Done => {}
        }
        Ok(())
    }

    /// The pass over the first format from `self.format` on that lays out anew, `self.format`
    /// set to it, else [`Phase::Verify`] once there is none.
    fn begin(
        &mut self,
        engine: &mut Engine,
        deck: &Deck,
        theme: &Theme,
        data: &DataFiles,
    ) -> Result<Phase, EngineError> {
        while let Some(format) = self.formats.get(self.format) {
            let (d, t) = project(deck, theme, format.as_deref())?;
            // A listed format of the deck's own shape lays out the same.
            if format.is_some() && matches!(d, Cow::Borrowed(_)) {
                self.format += 1;
                continue;
            }
            let every = match &self.only {
                Some(ids) => {
                    let mut at = Vec::with_capacity(ids.len());
                    for id in ids {
                        match d.states.iter().position(|s| s.id == *id) {
                            Some(i) => at.push(i),
                            None => return Err(EngineError::Layout(format!("no state `{id}`"))),
                        }
                    }
                    at.sort_unstable();
                    at.dedup();
                    at
                }
                None => (0..d.states.len()).collect(),
            };
            let snapshots = scaena_core::resolve_states(&d)?;
            let timeline = engine.timeline(&d, &t, data)?;
            let laid = Vec::with_capacity(every.len());
            let owned = |d: Cow<'_, Deck>| match d {
                Cow::Borrowed(_) => None,
                Cow::Owned(d) => Some(d),
            };
            let theme = match t {
                Cow::Borrowed(_) => None,
                Cow::Owned(t) => Some(t),
            };
            return Ok(Phase::Format(Box::new(Pass {
                deck: owned(d),
                theme,
                snapshots,
                timeline: Some(timeline),
                every,
                laid,
                ruled: false,
                contrast: None,
            })));
        }
        Ok(Phase::Verify)
    }

    /// One step of `pass`: a state laid out, the rules on them all, or a state's contrast
    /// judged. Whether the format is done.
    fn advance(
        &mut self,
        pass: &mut Pass,
        engine: &mut Engine,
        deck: &Deck,
        theme: &Theme,
        data: &DataFiles,
        backdrop: Option<&mut (dyn Backdrop + '_)>,
    ) -> Result<bool, EngineError> {
        let format = self.formats[self.format].as_deref();
        let Pass { deck: projected, theme: themed, snapshots, timeline, every, laid, ruled, contrast } = pass;
        let d = projected.as_ref().unwrap_or(deck);
        let t = themed.as_ref().unwrap_or(theme);
        if laid.len() < every.len() {
            let slots = &timeline.as_ref().expect("kept until every state is laid out").slots;
            laid.push(lay_one(engine, d, t, data, snapshots, slots, every[laid.len()], laid.last())?);
            return Ok(false);
        }
        let cx = Cx { deck: d, theme: t, format, states: laid };
        if !*ruled {
            if let Some(timeline) = timeline.take() {
                self.timelines.push((self.format, timeline));
            }
            for rule in rules() {
                // Motion is the same in every format but for what layout counts, and so is
                // the timeline; they are judged once, in the deck's own.
                if format.is_some() && matches!(rule.code(), "W320" | "W321" | "W323") {
                    continue;
                }
                self.out.extend(rule.check(&cx));
            }
            *ruled = true;
            let Some(b) = backdrop else { return Ok(true) };
            // A picture is the same file in every format: drawn once, in the deck's own (E107).
            if format.is_none() {
                self.out.extend(pictures::e107(&cx, &pictures::undrawable(laid, b)));
            }
            *contrast = Some((contrast::Pass::new(&cx), 0));
            return Ok(false);
        }
        let Some(b) = backdrop else { return Ok(true) };
        let Some((judging, next)) = contrast.as_mut() else { return Ok(true) };
        if *next < laid.len() {
            judging.state(&cx, b, &mut self.faint, &laid[*next])?;
            *next += 1;
            return Ok(false);
        }
        let (judging, _) = contrast.take().expect("judged above");
        self.out.extend(judging.finish(&cx));
        Ok(true)
    }
}

/// W427 (ADR-0019): each figure a run quotes that its data no longer gives, a file changed
/// behind the deck's back, with the patch that sets it again; and each word that holds the old
/// figure where the text shows (a beat's claim or notes, a state's notes, a node's description),
/// written again (PLAN 2.90).
fn w427(deck: &Deck, data: &DataFiles) -> Vec<Finding> {
    let Ok(doc) = serde_json::to_value(deck) else { return Vec::new() };
    let mut out = Vec::new();
    for s in scaena_core::quotes::stale(deck, &doc, data) {
        let text = format!("{}/text", s.path);
        out.push(
            Finding::new(
                "W427",
                Severity::Warning,
                format!("`{}` quotes {} where its data now gives {}", s.node, s.was, s.now),
            )
            .at(text.clone())
            .node(s.node.clone())
            .fix(vec![serde_json::json!({ "op": "replace", "path": text, "value": s.now })]),
        );
        for said in &s.words {
            out.push(
                Finding::new(
                    "W427",
                    Severity::Warning,
                    format!(
                        "{} says {}, which `{}` quotes, where its data now gives {}",
                        said.what, s.was, s.node, s.now
                    ),
                )
                .at(said.path.clone())
                .fix(vec![serde_json::json!({ "op": "replace", "path": said.path, "value": said.text })]),
            );
        }
    }
    out
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
        // A crowded text's step, or one whose tail reaches the text under it, is taken again
        // until it comes clear, never under half its size.
        let floor = (f.code == "I405" || f.code == "E101")
            .then(|| f.measure.as_ref().and_then(|m| m["own"].as_f64()).map(|own| own * 0.5))
            .flatten();
        loop {
            if works(engine, &json, theme, data, f) == Some(true) {
                break;
            }
            let next = floor.and_then(|floor| {
                let fix = f.fix.as_mut()?;
                let op = fix.iter_mut().find(|op| op["path"].as_str().is_some_and(|p| p.ends_with("/maxSize")))?;
                let next = (op["value"].as_f64()? * 0.95 * 4.0).floor() / 4.0;
                (next >= floor).then(|| op["value"] = serde_json::json!(next))
            });
            if next.is_none() {
                f.fix = None;
                break;
            }
        }
    }
    Ok(())
}

/// Whether laying `f`'s state out again with its fix applied to `json` shows that it works.
fn works(engine: &mut Engine, json: &serde_json::Value, theme: &Theme, data: &DataFiles, f: &Finding) -> Option<bool> {
    let mut doc = json.clone();
    scaena_core::patch::apply(&mut doc, f.fix.as_deref()?).ok()?;
    let patched = Deck::from_value(&doc).ok()?;
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
    // A credit set below must read there (ADR-0030); text set smaller must fit, and
    // a text crowding the one under it come clear of it.
    let rules: Vec<&dyn Rule> = match f.code.as_str() {
        "W314" => vec![&space::W314Credit],
        "I405" => vec![&space::I405Crowded, &text::E100Overflow, &text::W203ShrinkFloor],
        "E101" => vec![&space::E101Collision, &text::E100Overflow, &text::W203ShrinkFloor],
        _ => vec![&text::E100Overflow, &text::W202MaxLines, &text::W203ShrinkFloor],
    };
    Some(rules.iter().flat_map(|r| r.check(&cx)).all(|g| g.node != f.node))
}

/// State `i` of `deck` laid out, its cue from the state before it: `last`, where that is the
/// state laid out before it, else laid out for the cue.
#[allow(clippy::too_many_arguments)]
fn lay_one(
    engine: &mut Engine,
    deck: &Deck,
    theme: &Theme,
    data: &DataFiles,
    snapshots: &[Snapshot],
    slots: &[Slot],
    i: usize,
    last: Option<&Laid>,
) -> Result<Laid, EngineError> {
    let snap = &snapshots[i];
    let scene = engine.scene(deck, theme, data, snap)?;
    let state = &deck.states[i];
    let timing = Timing::parse(theme, state.transition.as_ref())?;
    let before = i.checked_sub(1).map(|p| &snapshots[p]);
    let items = cues::items(deck, theme, state, before, snap, timing.matched, timing.slide.is_some())?;
    // The state before, as laid out already, or laid out for the cue into this one.
    let from = match (i.checked_sub(1), last) {
        (Some(p), Some(laid)) if laid.index == p => Some(laid.scene.clone()),
        (Some(p), _) => Some(engine.scene(deck, theme, data, &snapshots[p])?),
        (None, _) => None,
    };
    let schedule = Transition::new(from, scene.clone(), timing, &items, 0.0)?.schedule().clone();
    Ok(Laid { index: i, snapshot: cascade::with_overrides(deck, snap), scene, slot: slots[i].clone(), schedule })
}
