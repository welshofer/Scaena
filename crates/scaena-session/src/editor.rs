//! The source editor's engine (PLAN 2.3, SPEC §9.2): a session compiles `.scn` as it is
//! typed, lints what it compiles with the engine it already has, applies a finding's fix,
//! and inspects a state. The operations are `scaena-ops`' (ADR-0009); this module keeps
//! what they need between edits: the source, where each part of the deck came from in it,
//! and what lint found.
//!
//! Places in the source go to the page as UTF-16 offsets, as a JavaScript string counts
//! them, and as a line and a column.

use crate::{Error, Session};
use scaena_core::model::Format;
use scaena_core::validate::BundleFiles;
use scaena_core::{Deck, Finding, Severity, resolve_states};
use scaena_engine::FrameRequest;
use scaena_ops::compile::{Compiled, compile, line_col};
use scaena_ops::inspect::{Inspected, Views, inspect_deck};
use scaena_ops::lint::{layout_rules, lint_with};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The source compiled last, and what lint found in it.
pub(crate) struct Edit {
    source: String,
    compiled: Compiled,
    findings: Vec<Finding>,
    /// Whether the deck it says validated: the one shown.
    shown: bool,
}

/// What the layout rules found in every state the last time they ran on all of them, and the
/// nodes each state had then: kept for the states a lint of one state does not lay out.
#[derive(Default)]
pub(crate) struct Laid {
    findings: Vec<Finding>,
    nodes: BTreeMap<String, BTreeSet<String>>,
}

/// Each state of `deck` by its id, with the nodes it shows and those that leave in its cue:
/// what a finding in that state can be about, or owe something to.
fn nodes(deck: &Deck) -> BTreeMap<String, BTreeSet<String>> {
    let states = resolve_states(deck).unwrap_or_default();
    scaena_core::sort::map(
        states.into_iter().map(|s| (s.state_id, scaena_core::sort::set(s.nodes.into_keys().chain(s.exited)))),
    )
}

/// The files a page handed the session, by their paths in the bundle, as validation reads
/// them.
pub(crate) struct Handed<'a>(pub &'a BTreeMap<String, Vec<u8>>);

impl BundleFiles for Handed<'_> {
    fn exists(&self, path: &str) -> bool {
        self.0.contains_key(path)
    }

    fn read_text(&self, path: &str) -> Option<String> {
        String::from_utf8(self.0.get(path)?.clone()).ok()
    }
}

/// Where in the source something is: UTF-16 offsets, and the 1-based line and column (in
/// characters) it starts at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Place {
    pub from: usize,
    pub to: usize,
    pub line: usize,
    pub col: usize,
}

impl Place {
    fn of(source: &str, offset: usize, len: usize) -> Place {
        let utf16 = |at: usize| source[..source.floor_char_boundary(at)].encode_utf16().count();
        let (line, col) = line_col(source, offset);
        Place { from: utf16(offset), to: utf16(offset + len), line, col }
    }
}

/// A finding, where it is in the source, whether it has a fix, and whether it holds in the
/// format shown.
#[derive(Debug, Clone, Serialize)]
pub struct Located {
    #[serde(flatten)]
    pub finding: Finding,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<Place>,
    pub fixable: bool,
    /// Whether it holds in the format frames are laid out in, as the canvas shows it (PLAN
    /// 2.49): see [`Shown::holds`].
    pub shown: bool,
    /// The formats it holds in, as the format menu names them: `""` for the deck's own canvas,
    /// then each of the deck's `formats` (PLAN 2.62). What the formats shown side by side count
    /// on each.
    pub formats: Vec<String>,
}

/// The format frames are laid out in, as the findings shown in it are told apart (PLAN 2.49).
struct Shown {
    format: Option<String>,
    /// Whether it lays out as the deck's own canvas: it is the deck's own, or a listed format of
    /// the same canvas, which lint does not lay out again (SPEC §7.4).
    own: bool,
}

impl Shown {
    /// Whether `f` holds in the format shown: one that names a format holds in that format; one
    /// that laying the deck out found in its own canvas (`laid`), in a format laid out as it is;
    /// the rest, validation's and the document rules', in every format.
    fn holds(&self, f: &Finding, laid: bool) -> bool {
        match &f.format {
            Some(format) => self.format.as_ref() == Some(format),
            None => !laid || self.own,
        }
    }
}

/// What compiling a source says: why it does not compile, or what validation finds in the
/// deck it says, and where each state starts. A deck that validates replaces the session's.
#[derive(Debug, Clone, Serialize)]
pub struct Compiling {
    /// Why the source does not compile, and where.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Located>,
    pub findings: Vec<Located>,
    /// Each state, by id, and where the line that declares it starts in the source (UTF-16):
    /// a cursor anywhere on `state id …` or under it is in that state.
    pub states: Vec<(String, usize)>,
    /// Whether the deck validated, and so is what frames show from now on.
    pub valid: bool,
}

/// What lint finds in the deck compiled last, in its own format and each of its `formats`.
#[derive(Debug, Clone, Serialize)]
pub struct Linting {
    pub findings: Vec<Located>,
    /// Whether the layout rules ran: they run once nothing above them is an error.
    pub laid: bool,
    /// Whether they ran on every state, or on one, with the others' findings kept from the
    /// last time they did.
    pub whole: bool,
}

impl Session {
    /// The deck as canonical `.scn` (SPEC §4): what the editor opens on.
    pub fn source(&self) -> String {
        scaena_core::dsl::decompile(&self.deck)
    }

    /// Whether the deck shown is the one compiled from `source`, nothing written over it since:
    /// what a page asks before it reads a state of the deck its source says.
    pub fn compiled_from(&self, source: &str) -> bool {
        self.edit.as_ref().is_some_and(|edit| edit.shown && edit.source == source)
    }

    /// The format frames are laid out in, as findings are shown in it.
    fn shown(&self) -> Shown {
        self.shown_in(self.format.clone())
    }

    /// `format` as findings are told apart in it: `None`, the deck's own canvas.
    fn shown_in(&self, format: Option<String>) -> Shown {
        let own = [self.deck.canvas.width, self.deck.canvas.height];
        let laid_out = |name: &String| Format::parse(name).is_some_and(|f| f.canvas(own) == own);
        Shown { own: format.as_ref().is_none_or(laid_out), format }
    }

    /// Every format the deck is laid out in, as findings are told apart in each (PLAN 2.62): its
    /// own canvas, then each of its `formats`.
    fn every(&self) -> Every {
        let named = self.deck.formats.iter().map(|f| (f.clone(), self.shown_in(Some(f.clone()))));
        Every {
            shown: self.shown(),
            each: std::iter::once((String::new(), self.shown_in(None))).chain(named).collect(),
        }
    }

    /// Compile `source` and validate it against the files handed over. A deck that
    /// validates becomes the session's: timelines and frames show it from now on.
    pub fn compile(&mut self, source: &str) -> Compiling {
        let compiled = match compile(source, &Handed(&self.files)) {
            Ok(compiled) => compiled,
            Err(e) => {
                let finding = Finding::new("E106", Severity::Error, e.message.clone());
                let at = Some(Place::of(source, e.offset, e.len));
                let formats = self.every().each.into_iter().map(|(name, _)| name).collect();
                let error = Located { finding, at, fixable: false, shown: true, formats };
                self.edit = None;
                return Compiling { error: Some(error), findings: Vec::new(), states: Vec::new(), valid: false };
            }
        };
        let states = (compiled.states().into_iter())
            .map(|(id, at)| {
                let before = &source[..source.floor_char_boundary(at)];
                let line = before.rfind('\n').map_or(0, |i| i + 1);
                (id, before[..line].encode_utf16().count())
            })
            .collect();
        let deck = compiled.deck();
        let valid = deck.is_some();
        if let Some(deck) = deck {
            self.set_deck(deck);
        }
        let every = self.every();
        let findings = (compiled.findings.iter()).map(|f| every.locate(source, &compiled, f, false)).collect();
        let found = compiled.findings.clone();
        self.edit = Some(Edit { source: source.to_string(), compiled, findings: found, shown: valid });
        Compiling { error: None, findings, states, valid }
    }

    /// Lint the deck compiled last: validation, the document rules, and the layout rules in
    /// every format, laid out by the session's engine. With `only`, the layout rules lay out
    /// that state alone, the one being edited, so the answer comes at once (PLAN 2.3); the
    /// other states keep what the layout rules found the last time they ran on every state,
    /// in the formats the deck still lists, placed again in this source. A state keeps it
    /// while it has every node it had then: a state the deck no longer has, or one an edit
    /// took a node out of, keeps nothing until they run on every state again, since what they
    /// found there can be about that node, or owe something to it (a collision, or the
    /// contrast of text over it).
    pub fn lint(&mut self, only: Option<&str>) -> Result<Linting, Error> {
        let every = self.every();
        let edit = self.edit.as_ref().ok_or(Error::NothingCompiled)?;
        let Some(deck) = edit.compiled.deck() else {
            let at = |f| every.locate(&edit.source, &edit.compiled, f, false);
            return Ok(Linting {
                findings: edit.findings.iter().map(at).collect(),
                laid: false,
                whole: only.is_none(),
            });
        };
        self.build()?;
        let Session { engine, store, data, theme_json, laid, files, .. } = self;
        let engine = engine.as_mut().expect("built above");
        let mut fresh = None;
        // What the layout rules found, this time and kept from the last: what holds only in the
        // format it was laid out in.
        let mut layout = Vec::new();
        let linted = lint_with(&deck, &Handed(files), Some(theme_json.as_str()), |theme| {
            let found = layout_rules(engine, &deck, theme, data, store, only)?;
            fresh = Some(found.clone());
            layout = match only {
                None => found,
                Some(only) => {
                    let now = nodes(&deck);
                    let keeps = |f: &&Finding| {
                        let Some(state) = f.state.as_deref() else { return false };
                        let listed = f.format.as_ref().is_none_or(|format| deck.formats.contains(format));
                        match (laid.nodes.get(state), now.get(state)) {
                            (Some(then), Some(now)) => state != only && listed && then.is_subset(now),
                            _ => false,
                        }
                    };
                    found.into_iter().chain(laid.findings.iter().filter(keeps).cloned()).collect()
                }
            };
            Ok(layout.clone())
        })
        .map_err(|e| Error::Ops(e.message))?;
        match (fresh, only) {
            (Some(findings), None) => *laid = Laid { findings, nodes: nodes(&deck) },
            // A deck the layout rules cannot run on has nothing laid out to keep.
            (None, None) => *laid = Laid::default(),
            _ => {}
        }
        let edit = self.edit.as_mut().expect("checked above");
        edit.findings = linted.findings;
        let at = |f| every.locate(&edit.source, &edit.compiled, f, layout.contains(f));
        Ok(Linting { findings: edit.findings.iter().map(at).collect(), laid: linted.laid, whole: only.is_none() })
    }

    /// Whether the deck with `patch` made lays `state` out in its own format and in each it
    /// lists: a table too wide for its cell in one does not (PLAN 2.96). Nothing is made.
    pub(crate) fn lays_out_with(&mut self, patch: &[Value], state: &str) -> Result<bool, Error> {
        let doc = self.deck.to_value().map_err(|e| Error::Deck(e.to_string()))?;
        let made = scaena_core::patch::compile(&doc, patch, &Handed(&self.files))
            .ok()
            .and_then(|compiled| Deck::from_json(&compiled.doc.to_string()).ok());
        let Some(made) = made else { return Ok(false) };
        self.build()?;
        let Session { theme_json, files, data, engine, store, .. } = self;
        let engine = engine.as_mut().expect("built above");
        let linted = lint_with(&made, &Handed(files), Some(theme_json.as_str()), |theme| {
            layout_rules(engine, &made, theme, data, store, Some(state))
        });
        Ok(linted.is_ok())
    }

    /// Begin judging the layouts `state` may take (PLAN 2.92), as `scaena inspect --layouts`
    /// judges them: how many there are. Each is judged by a [`Session::layouts_step`], so that
    /// a page's worker answers what else it is asked between them.
    pub fn layouts_begin(&mut self, state: &str) -> Result<usize, Error> {
        // Built now: building lets go of what was laid out, a round too.
        self.build()?;
        let found = scaena_ops::layouts::candidates(&self.deck, &self.theme, &Handed(&self.files), state)
            .map_err(|e| Error::Ops(e.message))?;
        let count = found.len();
        self.suggesting = Some(Suggesting { state: state.to_string(), left: found.into_iter(), judged: Vec::new() });
        Ok(count)
    }

    /// Judge the next layout: the state laid out in it by the session's engine, linted in every
    /// format, and drawn at rest in the format shown. Whether any is left. Nothing is made, and
    /// the canvas's own layout of the state is kept; a deck, its files, or the format changed
    /// since [`Session::layouts_begin`] ends the round, an error.
    pub fn layouts_step(&mut self) -> Result<bool, Error> {
        self.build()?;
        let Session { theme, theme_json, files, data, engine, store, format, suggesting, .. } = self;
        let round = suggesting.as_mut().ok_or_else(|| Error::Ops("no layouts are being judged".into()))?;
        let Some(candidate) = round.left.next() else { return Ok(false) };
        let engine = engine.as_mut().expect("built above");
        let (made, state) = (&candidate.deck, round.state.as_str());
        let linted = lint_with(made, &Handed(files), Some(theme_json.as_str()), |theme| {
            layout_rules(engine, made, theme, data, store, Some(state))
        })
        .map_err(|e| Error::Ops(e.message))?;
        let req = FrameRequest { deck: made, theme, data, state, t_ms: f64::INFINITY, format: format.as_deref() };
        let drawn = engine.frame(&req)?.display_list;
        let counted = scaena_ops::layouts::counted(candidate.suggestion, state, &linted.findings);
        round.judged.push((counted, drawn));
        Ok(round.left.len() > 0)
    }

    /// The layouts judged, best first, those drawn alike folded into one, each painted at rest
    /// `height` pixels high by a CPU painter of its own; the round ends. Any not judged yet is
    /// judged first.
    pub fn layouts_end(
        &mut self,
        height: u32,
    ) -> Result<Vec<(scaena_ops::layouts::Suggestion, scaena_paint::Raster)>, Error> {
        use scaena_paint::Painter;
        while self.layouts_step()? {}
        let round = self.suggesting.take().ok_or_else(|| Error::Ops("no layouts are being judged".into()))?;
        let mut painter = scaena_paint::cpu::CpuPainter::default();
        let mut painted = Vec::with_capacity(round.judged.len());
        for (suggestion, list) in scaena_ops::layouts::ranked(round.judged) {
            let scale = height as f32 / list.viewport[1];
            painted.push((suggestion, painter.paint(&list, &self.store, scale)?));
        }
        Ok(painted)
    }

    /// [`Session::layouts_end`] as a client lists it: each suggestion as JSON with its picture's
    /// `width` and `height`, the pictures kept until [`Session::layout_pixels`] takes each.
    pub fn layouts_painted(&mut self, height: u32) -> Result<Vec<serde_json::Value>, Error> {
        let painted = self.layouts_end(height)?;
        self.suggested.clear();
        let mut listed = Vec::with_capacity(painted.len());
        for (suggestion, picture) in painted {
            let mut value = serde_json::to_value(&suggestion).map_err(|e| Error::Ops(e.to_string()))?;
            value["width"] = picture.width.into();
            value["height"] = picture.height.into();
            listed.push(value);
            self.suggested.push(picture);
        }
        Ok(listed)
    }

    /// The pixels of the `i`th picture [`Session::layouts_painted`] painted last, taken: a
    /// second call gives none.
    pub fn layout_pixels(&mut self, i: usize) -> Vec<u8> {
        self.layout_picture(i).map(|picture| picture.rgba).unwrap_or_default()
    }

    /// The `i`th picture [`Session::layouts_painted`] painted last, taken, with its size: none for
    /// one taken already, or past the last (PLAN 3.26).
    pub fn layout_picture(&mut self, i: usize) -> Option<scaena_paint::Raster> {
        let kept = self.suggested.get_mut(i).filter(|kept| !kept.rgba.is_empty())?;
        Some(std::mem::replace(kept, scaena_paint::Raster { width: 0, height: 0, rgba: Vec::new() }))
    }

    /// The slides a person may start after the slide of `state`, the state shown (PLAN 3.30): one
    /// in each of the theme's layouts, in its order, then a blank one, each as
    /// [`Session::starting`] makes it, laid out in the format shown and painted at rest `height`
    /// pixels high by a CPU painter of its own, a box of the theme's second surface (`surface-2`)
    /// in each slot that waits for a picture or a figure. Each as JSON: its `layout` (none for the
    /// blank one), the layout's `description` and `group`, and its picture's `width` and `height`,
    /// the pictures kept until [`Session::starter_picture`] takes each. Nothing is made.
    pub fn starters_painted(&mut self, state: &str, height: u32) -> Result<Vec<Value>, Error> {
        use scaena_paint::Painter;
        self.build()?;
        let base = self.deck.to_value().map_err(|e| Error::Ops(e.to_string()))?;
        let fill = ["surface-2", "line"].into_iter().find(|c| self.theme.tokens.roles.contains_key(*c));
        let mut layouts: Vec<Option<String>> = self.theme.layouts.keys().cloned().map(Some).collect();
        layouts.push(None);
        let mut painter = scaena_paint::cpu::CpuPainter::default();
        let (mut listed, mut painted) = (Vec::with_capacity(layouts.len()), Vec::with_capacity(layouts.len()));
        for layout in layouts {
            let started = scaena_ops::states::starting(&self.deck, &self.theme, state, layout.as_deref())
                .map_err(|e| Error::Ops(e.message))?;
            let mut patch = started.patch;
            // A box where a picture or a figure goes, drawn under the words: in the picture only.
            let template = layout.as_deref().and_then(|name| self.theme.layouts.get(name));
            let (description, group) =
                (template.and_then(|t| t.description.clone()), template.and_then(|t| t.group.clone()));
            let waits = |s: &scaena_core::model::theme::Slot| {
                s.role.is_none() && matches!(s.prompt, Some(scaena_core::model::theme::Prompt::Words(_)))
            };
            let waiting = template.into_iter().flat_map(|t| &t.slots).filter(|(_, s)| waits(s));
            for (n, (slot, _)) in waiting.enumerate() {
                let Some(fill) = fill else { break };
                let node =
                    serde_json::json!({ "type": "shape", "kind": "rect", "fill": fill, "at": { "in": slot }, "z": -1 });
                let id = format!("{}-waiting-{n}", started.id);
                patch.push(serde_json::json!({ "op": "add_node", "id": id, "node": node, "state": started.id }));
            }
            let doc = scaena_core::patch::compile(&base, &patch, &Handed(&self.files))
                .map_err(|e| Error::Ops(format!("{e:?}")))?
                .doc;
            let made = Deck::from_value(&doc).map_err(|e| Error::Deck(e.to_string()))?;
            let Session { theme, data, engine, store, format, .. } = &mut *self;
            let engine = engine.as_mut().expect("built above");
            let req = FrameRequest {
                deck: &made,
                theme,
                data,
                state: &started.id,
                t_ms: f64::INFINITY,
                format: format.as_deref(),
            };
            let list = engine.frame(&req)?.display_list;
            let picture = painter.paint(&list, store, height as f32 / list.viewport[1])?;
            listed.push(serde_json::json!({
                "layout": layout,
                "description": description,
                "group": group,
                "width": picture.width,
                "height": picture.height,
            }));
            painted.push(picture);
        }
        self.started = painted;
        Ok(listed)
    }

    /// The `i`th picture [`Session::starters_painted`] painted last, taken, with its size: none
    /// for one taken already, or past the last (PLAN 3.30).
    pub fn starter_picture(&mut self, i: usize) -> Option<scaena_paint::Raster> {
        let kept = self.started.get_mut(i).filter(|kept| !kept.rgba.is_empty())?;
        Some(std::mem::replace(kept, scaena_paint::Raster { width: 0, height: 0, rgba: Vec::new() }))
    }

    /// The layouts `state` may take, best first, judged and painted in one go
    /// ([`Session::layouts_begin`], each step, then [`Session::layouts_end`]).
    pub fn layout_suggestions(
        &mut self,
        state: &str,
        height: u32,
    ) -> Result<Vec<(scaena_ops::layouts::Suggestion, scaena_paint::Raster)>, Error> {
        self.layouts_begin(state)?;
        self.layouts_end(height)
    }

    /// The source compiled last with `patch`, a finding's fix, applied: the fixed deck as
    /// canonical `.scn`. The fix names what it changes by pointer, so it applies to a
    /// later edit too.
    pub fn fix(&self, patch: &[Value]) -> Result<String, Error> {
        let edit = self.edit.as_ref().ok_or(Error::NothingCompiled)?;
        let mut doc = edit.compiled.json.clone();
        scaena_core::patch::apply(&mut doc, patch).map_err(|e| Error::Ops(e.to_string()))?;
        let deck = Deck::from_json(&doc.to_string()).map_err(|e| Error::Deck(e.to_string()))?;
        Ok(scaena_core::dsl::decompile(&deck))
    }

    /// `state` inspected in the format frames are laid out in (SPEC §7.1): each node with
    /// the deck's overrides merged in, each text node's look, what its overrides set, and
    /// its cue on the timeline.
    pub fn inspect(&mut self, state: &str) -> Result<Inspected, Error> {
        self.build()?;
        let (deck, theme) = scaena_engine::project(&self.deck, &self.theme, self.format.as_deref())?;
        let engine = self.engine.as_mut().expect("built above");
        // The deck is in the format shown already.
        let views = Views { resolved: true, timeline: true, ..Views::default() };
        let mut found = inspect_deck(&deck, Some(&theme), &self.data, Some(engine), Some(state), &views)
            .map_err(|e| Error::Ops(e.message))?;
        Ok(found.remove(0))
    }
}

/// A round of the layouts a state may take, judged a step at a time (PLAN 2.92): those left,
/// and those judged, each with its drawing.
pub struct Suggesting {
    state: String,
    left: std::vec::IntoIter<scaena_ops::layouts::Candidate>,
    judged: Vec<(scaena_ops::layouts::Suggestion, scaena_core::displaylist::DisplayList)>,
}

/// The format shown, and every format the deck is laid out in, by the name the format menu gives
/// it, as findings are told apart in each (PLAN 2.49, 2.62).
struct Every {
    shown: Shown,
    each: Vec<(String, Shown)>,
}

impl Every {
    /// `f` in `source`, where `compiled` says it is, with the formats it holds in; `laid`,
    /// whether laying the deck out found it.
    fn locate(&self, source: &str, compiled: &Compiled, f: &Finding, laid: bool) -> Located {
        let at = compiled.span(f).map(|(offset, len)| Place::of(source, offset, len.max(1)));
        let formats = (self.each.iter()).filter(|(_, shown)| shown.holds(f, laid)).map(|(name, _)| name.clone());
        Located {
            finding: f.clone(),
            at,
            fixable: f.fix.is_some(),
            shown: self.shown.holds(f, laid),
            formats: formats.collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revenue() -> Session {
        let dir = "../../docs/examples";
        let read = |p: &str| std::fs::read_to_string(format!("{dir}/{p}")).unwrap();
        let deck = read("revenue.deck.json");
        let mut s = Session::new(&deck, &read("themes/dusk.theme.json")).unwrap();
        let parsed = Deck::from_json(&deck).unwrap();
        for font in &parsed.fonts {
            s.add_file(&font.file, std::fs::read(format!("{dir}/{}", font.file)).unwrap());
        }
        for source in parsed.data.values() {
            if let serde_json::Value::String(path) = &source.source {
                s.add_file(path, std::fs::read(format!("{dir}/{path}")).unwrap());
            }
        }
        s
    }

    /// The layouts a state may take (PLAN 2.92), judged as `scaena inspect --layouts` judges
    /// them and painted small, with nothing made: the canvas draws the deck as it was.
    #[test]
    fn a_states_layouts_come_judged_and_painted_best_first() {
        let mut s = revenue();
        let before = s.pixels("revenue", f64::INFINITY, 160).unwrap();
        let suggested = s.layout_suggestions("revenue", 90).unwrap();
        let judged: Vec<(&str, bool, usize)> =
            suggested.iter().map(|(x, _)| (x.layout.as_str(), x.current, x.errors)).collect();
        assert_eq!(judged[0], ("figure", true, 0), "{judged:?}");
        assert_eq!(judged.len(), 3, "{judged:?}");
        assert!(judged[1..].iter().all(|&(_, current, errors)| !current && errors > 0), "{judged:?}");
        // As the bundle's lint judges them, the CLI's.
        let b = scaena_ops::open(std::path::Path::new("../../docs/examples/revenue.deck.json")).unwrap();
        let cli = scaena_ops::layouts::suggest(&b, "revenue", None).unwrap();
        assert_eq!(cli, suggested.iter().map(|(x, _)| x.clone()).collect::<Vec<_>>());
        // Each painted 90 pixels high on the deck's 16:9 canvas, and no two alike.
        for (i, (x, picture)) in suggested.iter().enumerate() {
            assert_eq!((picture.width, picture.height), (160, 90), "{}", x.layout);
            assert!(
                suggested[..i].iter().all(|(_, other)| other.rgba != picture.rgba),
                "{} draws as another",
                x.layout
            );
        }
        // The canvas draws the deck as it was.
        assert!(s.pixels("revenue", f64::INFINITY, 160).unwrap().rgba == before.rgba);
    }

    /// The slides a person may start (PLAN 3.30): one in each of the theme's layouts, in its
    /// order, then a blank one, each painted small and none alike, with nothing made.
    #[test]
    fn the_slides_to_start_come_painted_one_for_each_layout_then_a_blank_one() {
        let mut s = revenue();
        let before = s.pixels("revenue", f64::INFINITY, 160).unwrap();
        let listed = s.starters_painted("revenue", 90).unwrap();
        let layouts: Vec<&str> = listed.iter().map(|one| one["layout"].as_str().unwrap_or("blank")).collect();
        assert_eq!(layouts.len(), 48, "Dusk's 47 layouts, then a blank slide: {layouts:?}");
        assert_eq!((layouts[0], layouts[47]), ("title", "blank"));
        let bullets = &listed[layouts.iter().position(|l| *l == "bullets").unwrap()];
        assert_eq!(bullets["description"], "A headline, and the points that make its case.");
        assert_eq!(bullets["group"], "Words");
        assert!(listed[47]["description"].is_null() && listed[47]["group"].is_null());
        let mut pictures: Vec<scaena_paint::Raster> = Vec::new();
        for (i, one) in listed.iter().enumerate() {
            let picture = s.starter_picture(i).unwrap();
            assert_eq!((one["width"].as_u64(), one["height"].as_u64()), (Some(160), Some(90)));
            assert_eq!((picture.width, picture.height), (160, 90), "{}", layouts[i]);
            assert!(s.starter_picture(i).is_none(), "taken");
            assert!(pictures.iter().all(|p| p.rgba != picture.rgba), "{} draws as another", layouts[i]);
            pictures.push(picture);
        }
        // Nothing is made: the deck draws as it was, and a slide started is a patch to make.
        assert!(s.pixels("revenue", f64::INFINITY, 160).unwrap().rgba == before.rgba);
        let started = s.starting("revenue", Some("bullets")).unwrap();
        assert_eq!((started.id.as_str(), started.patch.len()), ("bullets", 5));
        assert!(s.states().iter().all(|id| id != "bullets"));
    }

    #[test]
    fn a_round_of_layouts_is_judged_a_step_at_a_time_and_ends_with_an_edit() {
        let mut s = revenue();
        let whole = s.layout_suggestions("revenue", 90).unwrap();
        // A step judges one layout and says whether any is left: what a page's worker answers
        // between, the same in the end.
        let count = s.layouts_begin("revenue").unwrap();
        assert!(count >= whole.len(), "{count} judged for {} suggested", whole.len());
        let mut left = 0;
        while s.layouts_step().unwrap() {
            left += 1;
        }
        assert_eq!(left + 1, count);
        let stepped = s.layouts_end(90).unwrap();
        assert_eq!(stepped.len(), whole.len());
        for ((a, x), (b, y)) in stepped.iter().zip(&whole) {
            assert!(a == b && x.rgba == y.rgba, "{} against {}", a.layout, b.layout);
        }
        // An edit between two steps ends the round: what is left was made from the deck before.
        s.layouts_begin("revenue").unwrap();
        assert!(s.layouts_step().unwrap());
        let source = s.source().replace("Same bars, stacked.", "The same bars, stacked.");
        assert!(s.compile(&source).valid);
        assert!(s.layouts_step().is_err() && s.layouts_end(90).is_err());
    }

    #[test]
    fn the_source_is_the_decks_canonical_source() {
        let s = revenue();
        let expected = std::fs::read_to_string("../../docs/examples/revenue.deck.scn").unwrap();
        assert_eq!(s.source(), expected);
    }

    #[test]
    fn an_edit_that_compiles_becomes_the_deck_and_lints() {
        let mut s = revenue();
        let source = s.source().replace("Same bars, stacked.", "The same bars, stacked.");
        let compiled = s.compile(&source);
        assert!(compiled.error.is_none() && compiled.valid, "{compiled:?}");
        assert_eq!(
            compiled.states.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            ["intro", "revenue", "mix", "close"]
        );
        let utf16: Vec<u16> = source.encode_utf16().collect();
        for (id, at) in &compiled.states {
            let line = String::from_utf16_lossy(&utf16[*at..]);
            assert!(line.starts_with(&format!("state {id} ")), "{id} at {at}: {}", line.lines().next().unwrap_or(""));
        }
        assert!(s.source().contains("The same bars, stacked."));
        let linted = s.lint(None).unwrap();
        assert!(linted.laid);
        assert!(!linted.findings.iter().any(|f| f.finding.severity == Severity::Error), "{:?}", linted.findings);
    }

    #[test]
    fn a_source_that_does_not_compile_says_where() {
        let mut s = revenue();
        let source = s.source().replace("state revenue layout:figure", "state revenue layout:");
        let compiled = s.compile(&source);
        let error = compiled.error.expect("an error");
        let line = source.lines().position(|l| l.starts_with("state revenue")).unwrap() + 1;
        assert_eq!(error.at.unwrap().line, line, "{}", error.finding.message);
        assert!(!compiled.valid);
        // The deck the session had stays.
        assert!(s.source().contains("state revenue layout:figure"));
    }

    /// A letter typed where none goes, in any script, is an error placed around it in the
    /// page's UTF-16 offsets, not a panic mid-keystroke.
    #[test]
    fn a_letter_typed_where_none_goes_is_placed_around_it() {
        let mut s = revenue();
        for (c, units) in [('é', 1), ('漢', 1), ('🇺', 2)] {
            let source = s.source().replace("layout:figure", &format!("layout:{c}figure"));
            let at = s.compile(&source).error.expect("an error").at.expect("placed");
            let offset = source.find(c).unwrap();
            assert_eq!((at.from, at.to), (source[..offset].encode_utf16().count(), at.from + units), "{c}");
        }
    }

    #[test]
    fn a_fix_comes_back_as_source() {
        let mut s = revenue();
        // A headline too long for its slot at its size overflows: E100, fixed by shrinking.
        let long = "Revenue doubled, and then some";
        let source = s.source().replace("\"Revenue doubled\"", &format!("\"{long}\""));
        assert!(s.compile(&source).valid);
        let linted = s.lint(None).unwrap();
        let overflow =
            (linted.findings.iter()).find(|f| f.finding.code == "E100" && f.fixable).expect("an E100 with a fix");
        let at = overflow.at.expect("placed in the source");
        assert_eq!(
            &source[..].encode_utf16().skip(at.from).take(5).map(|u| u as u8 as char).collect::<String>(),
            "title"
        );
        let fixed = s.fix(overflow.finding.fix.as_deref().unwrap()).unwrap();
        assert!(fixed.contains(long));
        assert!(s.compile(&fixed).valid);
        let after = s.lint(None).unwrap();
        assert!(!after.findings.iter().any(|f| f.finding.code == "E100"), "{:?}", after.findings);
    }

    #[test]
    fn a_lint_of_one_state_answers_for_it_and_keeps_the_rest() {
        let mut s = revenue();
        let long = "Revenue doubled, and then some";
        let source = s.source().replace("\"Revenue doubled\"", &format!("\"{long}\""));
        assert!(s.compile(&source).valid);
        let whole = s.lint(None).unwrap();
        assert!(whole.whole && whole.laid);
        let said = |l: &Linting, state: &str| {
            let mut found: Vec<String> = (l.findings.iter())
                .filter(|f| f.finding.state.as_deref() == Some(state))
                .map(|f| {
                    format!("{} {:?} {:?} {}", f.finding.code, f.finding.node, f.finding.format, f.finding.message)
                })
                .collect();
            found.sort();
            found
        };
        // Each state alone finds what the whole lint finds in it, and keeps the rest.
        for state in ["intro", "revenue", "mix", "close"] {
            let one = s.lint(Some(state)).unwrap();
            assert!(!one.whole);
            for other in ["intro", "revenue", "mix", "close"] {
                assert_eq!(said(&one, other), said(&whole, other), "{state}: {other}");
            }
        }
        assert!(said(&whole, "revenue").iter().any(|f| f.starts_with("E100")));
        // The headline put right: linting its state drops what the whole lint found there.
        assert!(s.compile(&s.source().replace(long, "Revenue doubled")).valid);
        let one = s.lint(Some("revenue")).unwrap();
        assert!(!said(&one, "revenue").iter().any(|f| f.starts_with("E100")), "{:?}", said(&one, "revenue"));
    }

    /// Each finding, written out as the page reads it.
    fn written(l: &Linting) -> Vec<String> {
        let mut found: Vec<String> = l.findings.iter().map(|f| serde_json::to_string(&f.finding).unwrap()).collect();
        found.sort();
        found
    }

    /// The revenue example's `source` with a copy of its chart, `rev-2`, low on the slide in
    /// `revenue`: `mix` keeps it, and so does `close`, over the shader it brings back.
    fn with_a_copy(source: &str) -> String {
        let (from, to) = (source.find("  rev chart:").unwrap(), source.find("  note text").unwrap());
        let copy = (source[from..to].replacen("  rev ", "  rev-2 ", 1))
            .replace("at:in(main)", "at:{rect: [600, 820, 1100, 600]}");
        format!("{}{copy}{}", &source[..to], &source[to..])
    }

    /// A node an edit put in and an undo took out again, as a paste and its undo do: what the
    /// lint of every state found while it was there goes with it, from every state it was in,
    /// though the lint of one state lays out only that one. Some of that names the node only in
    /// what it measures, and some not at all: a note it was over reads badly on its bars.
    #[test]
    fn a_lint_of_one_state_keeps_nothing_from_a_state_an_edit_took_a_node_out_of() {
        let mut s = revenue();
        let original = s.source();
        assert!(s.compile(&with_a_copy(&original)).valid);
        let whole = s.lint(None).unwrap();
        let found = |code: &str, state: &str, node: &str| {
            (whole.findings.iter())
                .map(|f| &f.finding)
                .any(|f| f.code == code && f.state.as_deref() == Some(state) && f.node.as_deref() == Some(node))
        };
        assert!(found("W311", "close", "bg"), "the shader behind the copy: {:#?}", written(&whole));
        assert!(found("E110", "revenue", "note"), "the note over the copy: {:#?}", written(&whole));
        // Undone, in `mix`: the lint of that state alone finds what the lint of every state does.
        assert!(s.compile(&original).valid);
        let one = s.lint(Some("mix")).unwrap();
        assert!(!one.whole);
        assert!(!written(&one).iter().any(|f| f.contains("rev-2")), "{:#?}", written(&one));
        assert_eq!(written(&one), written(&s.lint(None).unwrap()));
    }

    /// A state an edit took out, with what the lint of every state found in it.
    #[test]
    fn a_lint_of_one_state_keeps_nothing_from_a_state_the_deck_no_longer_has() {
        let mut s = revenue();
        let original = s.source();
        let long = "Thank you, every one of you, for coming tonight".repeat(4);
        assert!(s.compile(&format!("{original}\nstate encore\n  title \"{long}\"\n")).valid);
        let whole = s.lint(None).unwrap();
        let encore = |l: &Linting| l.findings.iter().any(|f| f.finding.state.as_deref() == Some("encore"));
        let overflow = |f: &Located| f.finding.code == "E100" && f.finding.state.as_deref() == Some("encore");
        assert!(whole.findings.iter().any(overflow), "{:#?}", written(&whole));
        assert!(s.compile(&original).valid);
        let one = s.lint(Some("revenue")).unwrap();
        assert!(!encore(&one), "{:#?}", written(&one));
    }

    /// A format an edit took out of the deck's list, with what the lint of every state found
    /// laid out in it: every state keeps its nodes, and nothing found in that format stays.
    #[test]
    fn a_lint_of_one_state_keeps_nothing_from_a_format_the_deck_no_longer_lists() {
        let mut s = revenue();
        let copied = with_a_copy(&s.source());
        assert!(s.compile(&copied).valid);
        let whole = s.lint(None).unwrap();
        let tall = |l: &Linting| l.findings.iter().any(|f| f.finding.format.as_deref() == Some("9:16"));
        assert!(tall(&whole), "{:#?}", written(&whole));
        assert!(s.compile(&copied.replace("formats:[16:9, 9:16]", "formats:[16:9]")).valid);
        let one = s.lint(Some("intro")).unwrap();
        assert!(!tall(&one), "{:#?}", written(&one));
    }

    /// The canvas shows the findings that hold in the format shown (PLAN 2.49): one laying the
    /// deck out found in a format holds there; one it found on the deck's own canvas, there and
    /// in a listed format of the same canvas (`16:9` on revenue's 1920 × 1080); validation's and
    /// the document rules', in every format.
    #[test]
    fn a_finding_is_shown_in_the_formats_it_holds_in() {
        let mut s = revenue();
        // A headline too long for its header in every format (E100, laid out), and a note placed
        // by `rect` on a slide with a layout in a deck with formats (W301 and W302, document rules').
        let source = (s.source().replace("\"Revenue doubled\"", "\"Revenue doubled, and then some, and more\""))
            .replace("semantic:source\n    at:in(note)", "semantic:source\n    at:rect(1200, 980, 600, 60)");
        assert!(s.compile(&source).valid);
        // Each finding about `revenue`: its code, the format it names, and whether it is shown.
        let found = |l: &Linting| -> Vec<(String, Option<String>, bool)> {
            (l.findings.iter())
                .filter(|f| f.finding.state.as_deref() == Some("revenue"))
                .map(|f| (f.finding.code.clone(), f.finding.format.clone(), f.shown))
                .collect()
        };
        let document = |code: &str| matches!(code, "W301" | "W302");
        // On the deck's own canvas: what names no format.
        let own = found(&s.lint(None).unwrap());
        assert!(own.contains(&("E100".into(), None, true)) && own.contains(&("W301".into(), None, true)), "{own:?}");
        assert!(own.contains(&("E100".into(), Some("9:16".into()), false)), "{own:?}");
        assert!(own.iter().all(|(_, format, shown)| *shown == format.is_none()), "{own:?}");
        // Each finding lists the formats it holds in, as the format menu names them (PLAN 2.62):
        // the one it names, else the deck's own canvas and `16:9` for what laying it out there
        // found, and every format for the document rules'.
        let listed = |l: &Linting| -> Vec<(String, Option<String>, Vec<String>)> {
            (l.findings.iter())
                .filter(|f| f.finding.state.as_deref() == Some("revenue"))
                .map(|f| (f.finding.code.clone(), f.finding.format.clone(), f.formats.clone()))
                .collect()
        };
        let every = listed(&s.lint(None).unwrap());
        let names = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert!(every.contains(&("E100".into(), None, names(&["", "16:9"]))), "{every:?}");
        assert!(every.contains(&("E100".into(), Some("9:16".into()), names(&["9:16"]))), "{every:?}");
        assert!(every.contains(&("W301".into(), None, names(&["", "16:9", "9:16"]))), "{every:?}");
        // In `9:16`: what laying it out there found, and the document rules'.
        s.set_format(Some("9:16")).unwrap();
        let tall = found(&s.lint(Some("revenue")).unwrap());
        assert!(
            tall.contains(&("E100".into(), None, false)) && tall.contains(&("W301".into(), None, true)),
            "{tall:?}"
        );
        let held = |(code, format, _): &(String, Option<String>, bool)| match format {
            Some(format) => format == "9:16",
            None => document(code),
        };
        assert!(tall.iter().all(|f| f.2 == held(f)), "{tall:?}");
        // `16:9` lays out as the deck's own canvas, and lint does not lay it out again.
        s.set_format(Some("16:9")).unwrap();
        let wide = found(&s.lint(None).unwrap());
        assert_eq!(wide, own);
        // What compiling finds holds in every format.
        let found = s.compile(&source.replace("@q3", "@q4")).findings;
        assert!(!found.is_empty() && found.iter().all(|f| f.shown), "{found:?}");
        assert!(found.iter().all(|f| f.formats == names(&["", "16:9", "9:16"])), "{found:?}");
    }

    #[test]
    fn a_state_inspects_with_its_looks_and_its_cue() {
        let mut s = revenue();
        let inspected = serde_json::to_value(s.inspect("revenue").unwrap()).unwrap();
        assert_eq!(inspected["looks"]["title"]["role"], "headline");
        assert!(inspected["timeline"]["span"].as_f64().unwrap() > 0.0, "{inspected}");
    }
}
