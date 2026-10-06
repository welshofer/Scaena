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
use scaena_core::{Deck, Finding, Severity};
use scaena_ops::compile::{Compiled, compile, line_col};
use scaena_ops::inspect::{Inspected, Views, inspect_deck};
use scaena_ops::lint::{layout_rules, lint_with};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// The source compiled last, and what lint found in it.
pub(crate) struct Edit {
    source: String,
    compiled: Compiled,
    findings: Vec<Finding>,
    /// Whether the deck it says validated: the one shown.
    shown: bool,
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
    /// placed again in this source.
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
                    let kept = (laid.iter())
                        .filter(|f| f.state.as_deref() != Some(only))
                        .filter(|f| deck.states.iter().any(|s| f.state.as_ref() == Some(&s.id)));
                    found.into_iter().chain(kept.cloned()).collect()
                }
            };
            Ok(layout.clone())
        })
        .map_err(|e| Error::Ops(e.message))?;
        match (fresh, only) {
            (Some(found), None) => *laid = found,
            // A deck the layout rules cannot run on has nothing laid out to keep.
            (None, None) => laid.clear(),
            _ => {}
        }
        let edit = self.edit.as_mut().expect("checked above");
        edit.findings = linted.findings;
        let at = |f| every.locate(&edit.source, &edit.compiled, f, layout.contains(f));
        Ok(Linting { findings: edit.findings.iter().map(at).collect(), laid: linted.laid, whole: only.is_none() })
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
