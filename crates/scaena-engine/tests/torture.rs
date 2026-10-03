//! The typography torture deck (PLAN 0.2) through the engine (PLAN 0.4).
//!
//! - Golden display lists per state under `tests/golden/torture/` (quantized, golden
//!   JSON layout). After a reviewed change: `SCAENA_BLESS=1 cargo test -p scaena-engine
//!   --test torture`. On a mismatch the new output is written to `actual/` beside the
//!   goldens (git-ignored) for diffing.
//! - Determinism: fonts registered in a different order still give identical bytes.
//! - One test per kill case asserting that the feature it names was applied.
//! - Catalogue cases are characterized (pinned as observed, failures included), so an
//!   upstream change shows up as a test to update, not a silent shift.

use scaena_core::Deck;
use scaena_core::displaylist::{Affine, DisplayList, Glyph, Op, PathEl, quantize};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::text::{TextEngine, TextLayout, TextSpec};
use scaena_engine::theme::{Theme, Wrap};
use scaena_engine::{Engine, EngineError, FrameRequest, PlacedText};
use std::collections::BTreeSet;

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";
const GOLDEN: &str = "../../tests/golden/torture";
/// Frames inside a state's cue, as (state, fraction of its span: its transition and its
/// motions). PLAN 0.10 renders each chart transition at t = 0, 0.25, 0.5, and 1; 0 and
/// 1 are the states at rest (asserted below), so the middle two get goldens of their
/// own: the chart's values animating in, and the next quarter arriving. PLAN 1.9 adds
/// every kind's data motion mid-way, the two stages of bars that regroup, and
/// annotations that move. PLAN 1.11 adds case 40's motions on the cue clock: words
/// rising in turn, then cards, then bars on a spring, then a pulse. PLAN 1.12 adds case
/// 42's morphs: words, points, paths, uniforms, a group, an outline drawing on, and a
/// color that comes and goes. Case 44 adds charts whose marks grow in turn: stacks that
/// build member on member, a ring that sweeps open, and lines that rise series by series.
const MORPH: [(&str, f64); 17] = [
    ("chart", 0.25),
    ("chart", 0.5),
    ("chart-next", 0.25),
    ("chart-next", 0.5),
    ("chart-kinds-next", 0.5),
    ("chart-kinds-2-next", 0.5),
    ("regroup-stacked", 0.25),
    ("regroup-stacked", 0.75),
    ("annotations-next", 0.5),
    ("motion", 0.15),
    ("motion", 0.35),
    ("motion", 0.65),
    ("morph", 0.25),
    ("morph", 0.5),
    ("morph", 0.75),
    ("stagger", 0.3),
    ("stagger", 0.6),
];
/// Frames in a format of the deck's (PLAN 1.13) as (state, fraction of its span, or `None`
/// at rest), named `state~9x16` and `state@fraction~9x16`: case 43's halves stacked, and
/// containers, a chart's data, and morphs laid out again on the tall canvas.
const TALL: [(&str, Option<f64>); 4] =
    [("formats", None), ("containers", None), ("chart-next", Some(0.5)), ("morph", Some(0.75))];
const SERIF: &str = "fonts/RobotoSerif-VF.ttf";
const GARAMOND: &str = "fonts/EBGaramond-VF.ttf";
const HEBREW: &str = "fonts/NotoSansHebrew-VF.ttf";
const EMOJI: &str = "fonts/NotoColorEmoji-COLRv1.ttf";

struct Fixture {
    deck: Deck,
    theme: Theme,
    data: DataFiles,
    engine: Engine,
}

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

fn fixture_with(theme_edit: impl FnOnce(&mut serde_json::Value), reverse_fonts: bool) -> Fixture {
    let deck = Deck::from_json(&String::from_utf8(read("deck.json")).unwrap()).unwrap();
    let mut raw: serde_json::Value = serde_json::from_slice(&read("theme.json")).unwrap();
    theme_edit(&mut raw);
    let theme = Theme::from_json(&raw.to_string()).unwrap();
    let fonts = bundle_fonts(&deck, reverse_fonts);
    fonts.check_theme(&theme).unwrap();
    let mut data = DataFiles::new();
    for source in deck.data.values() {
        // Files the bundle holds; inline sources need none.
        if let Some(path) = source.source.as_str() {
            data.insert(path, read(path));
        }
    }
    let mut images = BundleImages::new();
    for path in deck.image_files() {
        images.register(&path, &read(&path)).unwrap();
    }
    Fixture { deck, theme, data, engine: Engine::new(fonts).with_images(images) }
}

fn bundle_fonts(deck: &Deck, reverse: bool) -> BundleFonts {
    let mut fonts = BundleFonts::new();
    let mut files: Vec<&str> = deck.fonts.iter().map(|f| f.file.as_str()).collect();
    if reverse {
        files.reverse();
    }
    for file in files {
        fonts.register(file, read(file)).unwrap();
    }
    fonts
}

fn fixture() -> Fixture {
    fixture_with(|_| {}, false)
}

impl Fixture {
    fn frame(&mut self, state: &str) -> Result<DisplayList, EngineError> {
        self.frame_at(state, f64::INFINITY)
    }

    fn frame_at(&mut self, state: &str, t_ms: f64) -> Result<DisplayList, EngineError> {
        self.frame_in(state, t_ms, None)
    }

    /// The frame `t_ms` into `state`'s cue, in `format` or on the deck's own canvas.
    fn frame_in(&mut self, state: &str, t_ms: f64, format: Option<&str>) -> Result<DisplayList, EngineError> {
        let req = FrameRequest { deck: &self.deck, theme: &self.theme, data: &self.data, state, t_ms, format };
        Ok(self.engine.frame(&req)?.display_list)
    }

    /// The state's span, ms: its transition and its motions (SPEC §2.4).
    fn duration(&mut self, state: &str) -> f64 {
        self.duration_in(state, None)
    }

    fn duration_in(&mut self, state: &str, format: Option<&str>) -> f64 {
        let (deck, theme) = scaena_engine::project(&self.deck, &self.theme, format).unwrap();
        let timeline = self.engine.timeline(&deck, &theme, &self.data).unwrap();
        timeline.slot(state).unwrap().span
    }

    /// Every golden frame as (name, state, t_ms, format): each state at rest under its own
    /// name, then the frames in [`MORPH`] as `state@fraction`, then those in [`TALL`] in
    /// `9:16`, with `~9x16` after their names.
    fn golden_frames(&mut self) -> Vec<(String, String, f64, Option<&'static str>)> {
        let mut out: Vec<(String, String, f64, Option<&'static str>)> =
            self.states().into_iter().map(|s| (s.clone(), s, f64::INFINITY, None)).collect();
        for (state, at) in MORPH {
            out.push((format!("{state}@{at}"), state.to_string(), at * self.duration(state), None));
        }
        let tall = Some("9:16");
        for (state, at) in TALL {
            let (name, t) = match at {
                Some(at) => (format!("{state}@{at}~9x16"), at * self.duration_in(state, tall)),
                None => (format!("{state}~9x16"), f64::INFINITY),
            };
            out.push((name, state.to_string(), t, tall));
        }
        out
    }

    fn placed(&mut self, state: &str, node: &str) -> PlacedText {
        let req = FrameRequest {
            deck: &self.deck,
            theme: &self.theme,
            data: &self.data,
            state,
            t_ms: f64::INFINITY,
            format: None,
        };
        self.engine.text_layout(&req, node).unwrap()
    }

    fn text(&mut self, state: &str, node: &str) -> TextLayout {
        self.placed(state, node).text
    }

    fn states(&self) -> Vec<String> {
        self.deck.states.iter().map(|s| s.id.clone()).collect()
    }
}

/// Glyph runs a node's layer draws: (font id, size, coords, glyphs).
fn runs<'a>(dl: &'a DisplayList, node: &str) -> Vec<(&'a str, f32, &'a [i16], &'a [Glyph])> {
    let ops = dl
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Layer { node: Some(n), ops, .. } if n == node => Some(ops),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no layer for `{node}`"));
    ops.iter()
        .map(|op| match op {
            Op::Glyphs { font, size, coords, glyphs, .. } => {
                (dl.fonts[*font as usize].id.as_str(), *size, coords.as_slice(), glyphs.as_slice())
            }
            other => panic!("unexpected op in a text layer: {other:?}"),
        })
        .collect()
}

fn glyphs(dl: &DisplayList, node: &str) -> Vec<Glyph> {
    runs(dl, node).into_iter().flat_map(|(_, _, _, g)| g.iter().copied()).collect()
}

fn fonts_used<'a>(dl: &'a DisplayList, node: &str) -> BTreeSet<&'a str> {
    runs(dl, node).into_iter().map(|(font, ..)| font).collect()
}

fn line_words(layout: &TextLayout, text: &str, line: usize) -> usize {
    text[layout.lines[line].text.clone()].split_whitespace().count()
}

fn line_texts<'a>(layout: &TextLayout, text: &'a str) -> Vec<&'a str> {
    layout.lines.iter().map(|l| &text[l.text.clone()]).collect()
}

/// `text` in the `specimen` role, which sets neither `hangingPunctuation` nor
/// `opticalMargins`, broken by `wrap` at `width`, outside any deck.
fn set(text: &str, wrap: Wrap, width: f32) -> TextLayout {
    let fx = fixture();
    let mut fonts = bundle_fonts(&fx.deck, false);
    let spec = TextSpec { wrap: Some(wrap), ..TextSpec::plain(fx.theme.text_role("specimen").unwrap(), text) };
    TextEngine::new().layout(&mut fonts, &fx.theme, &spec, width).unwrap()
}

// --- goldens and determinism -----------------------------------------------------

#[test]
fn display_lists_match_goldens() {
    let mut fx = fixture();
    let bless = std::env::var_os("SCAENA_BLESS").is_some();
    let mut changed = Vec::new();
    for (name, state, t_ms, format) in fx.golden_frames() {
        let mut dl = fx.frame_in(&state, t_ms, format).unwrap_or_else(|e| panic!("{name}: {e}"));
        quantize(&mut dl);
        let json = dl.to_golden_json().unwrap();
        let path = format!("{GOLDEN}/{name}.dl.json");
        if bless {
            std::fs::create_dir_all(GOLDEN).unwrap();
            std::fs::write(&path, &json).unwrap();
        } else if std::fs::read_to_string(&path).ok().as_deref() != Some(json.as_str()) {
            std::fs::create_dir_all(format!("{GOLDEN}/actual")).unwrap();
            std::fs::write(format!("{GOLDEN}/actual/{name}.dl.json"), &json).unwrap();
            changed.push(name);
        }
    }
    assert!(
        changed.is_empty(),
        "display lists differ from tests/golden/torture for {changed:?}; new output is in tests/golden/torture/actual/. \
         Review the diff, then bless with SCAENA_BLESS=1."
    );
}

/// Stricter than the contract: SPEC §13.4 compares display lists after quantizing,
/// but if unquantized output is already bit-identical across platforms (CI runs
/// x86-64 Linux and arm64 macOS), quantization is pure margin, and any change that
/// makes float results platform-dependent fails here before it happens to cross a
/// rounding boundary.
#[test]
fn raw_display_lists_are_bit_identical_across_platforms() {
    let mut fx = fixture();
    let mut digests = String::new();
    for (name, state, t_ms, format) in fx.golden_frames() {
        // FNV-1a, 64-bit, over the postcard bytes: change detection, not security.
        let digest = fx.frame_in(&state, t_ms, format).unwrap().digest().unwrap();
        digests.push_str(&format!("{name} {digest}\n"));
    }
    let path = format!("{GOLDEN}/raw.fnv1a");
    if std::env::var_os("SCAENA_BLESS").is_some() {
        std::fs::write(&path, &digests).unwrap();
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        digests, expected,
        "unquantized display lists differ from {path}. If only this test fails, float results now depend on \
         the platform: quantized goldens still hold the contract, but find the cause before re-blessing."
    );
}

#[test]
fn frames_are_deterministic_across_engines_and_font_order() {
    let (mut a, mut b) = (fixture(), fixture_with(|_| {}, true));
    for (name, state, t_ms, format) in a.golden_frames() {
        let first = a.frame_in(&state, t_ms, format).unwrap().to_postcard().unwrap();
        assert_eq!(
            a.frame_in(&state, t_ms, format).unwrap().to_postcard().unwrap(),
            first,
            "{name}: same engine, second frame"
        );
        assert_eq!(
            b.frame_in(&state, t_ms, format).unwrap().to_postcard().unwrap(),
            first,
            "{name}: fonts registered in reverse order"
        );
    }
}

/// Every glyph op in `ops`, depth first: (its text, each glyph's cluster start).
fn said(ops: &[Op], out: &mut Vec<(String, Vec<u32>, usize)>) {
    for op in ops {
        match op {
            Op::Layer { ops, .. } => said(ops, out),
            Op::Glyphs { text, clusters, glyphs, .. } => out.push((text.clone(), clusters.clone(), glyphs.len())),
            _ => {}
        }
    }
}

/// What each glyph of each run in `dl` says: its cluster, up to the next larger start.
fn glyph_texts(dl: &DisplayList) -> Vec<Vec<String>> {
    let mut ops = Vec::new();
    said(&dl.ops, &mut ops);
    ops.into_iter()
        .map(|(text, clusters, _)| {
            let mut starts = clusters.clone();
            starts.sort_unstable();
            let end = |c: u32| starts.iter().copied().find(|&s| s > c).map_or(text.len(), |s| s as usize);
            clusters.iter().map(|&c| text[c as usize..end(c)].to_string()).collect()
        })
        .collect()
}

#[test]
fn glyph_runs_say_what_they_set() {
    let mut fx = fixture();
    // Every run of every golden frame says something, cluster by cluster.
    for (name, state, t, format) in fx.golden_frames() {
        let dl = fx.frame_in(&state, t, format).unwrap();
        let mut ops = Vec::new();
        said(&dl.ops, &mut ops);
        for (text, clusters, glyphs) in ops {
            assert!(!text.is_empty() && clusters.len() == glyphs, "{name}: {text:?} {clusters:?}");
            let fits = |c: &u32| (*c as usize) < text.len() && text.is_char_boundary(*c as usize);
            assert!(clusters.iter().all(fits), "{name}: {text:?} {clusters:?}");
        }
    }
    // A ligature says its letters.
    let liga = glyph_texts(&fx.frame("liga").unwrap());
    assert!(liga.iter().any(|run| ["ffi", "ffl", "fj"].iter().all(|l| run.iter().any(|g| g == l))), "{liga:?}");
    // A base and its marks, glyphs of one cluster, say it together.
    let combining = glyph_texts(&fx.frame("combining").unwrap()).concat();
    assert!(combining.windows(2).any(|w| w[0] == w[1] && w[0].chars().count() > 1), "{combining:?}");
    // The hyphen drawn at a break says the soft hyphen it draws.
    let hyphenation = glyph_texts(&fx.frame("hyphenation").unwrap());
    assert!(hyphenation.iter().any(|run| run == &["\u{AD}"]), "{hyphenation:?}");
    // In motion: a split unit says its word, a morphing word its own, a count its number.
    let texts = |fx: &mut Fixture, state: &str, at: f64| {
        let t = at * fx.duration(state);
        let mut ops = Vec::new();
        said(&fx.frame_at(state, t).unwrap().ops, &mut ops);
        ops.into_iter().map(|(text, _, _)| text).collect::<Vec<_>>()
    };
    let motion = texts(&mut fx, "motion", 0.35);
    assert!(motion.iter().any(|t| t == "word "), "{motion:?}");
    let morph = texts(&mut fx, "morph", 0.5);
    assert!(morph.iter().any(|t| t == "grew"), "{morph:?}");
    let at_rest = texts(&mut fx, "chart", f64::INFINITY);
    let counting = texts(&mut fx, "chart", 0.5);
    let number = |t: &&String| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit());
    assert!(counting.iter().filter(number).any(|t| !at_rest.contains(t)), "{counting:?}");
}

#[test]
fn every_shader_kind_draws_as_its_shader_op() {
    for kind in ["gradient", "noise", "grain", "particles"] {
        let mut fx = fixture();
        let node = &mut fx.deck.nodes["mesh-bg"].props;
        node.insert("kind".into(), kind.into());
        node.shift_remove("params");
        let dl = fx.frame("mesh").unwrap();
        let found = (dl.ops.iter())
            .flat_map(|op| match op {
                Op::Layer { ops, .. } => ops.iter().collect::<Vec<_>>(),
                other => vec![other],
            })
            .any(|op| matches!(op, Op::Shader { kind: k, .. } if serde_json::to_value(k).unwrap() == kind));
        assert!(found, "{kind}");
    }
}

// --- shaders (PLAN 0.11, gate 0 criterion 4) ----------------------------------------

/// The `mesh-bg` layer's shader op.
fn mesh_op(dl: &DisplayList) -> (Affine, f32, [f32; 4]) {
    let (transform, ops) = dl
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Layer { node: Some(n), transform, ops, .. } if n == "mesh-bg" => Some((*transform, ops)),
            _ => None,
        })
        .expect("a `mesh-bg` layer");
    match ops.as_slice() {
        [Op::Shader { t, rect, .. }] => (transform, *t, *rect),
        other => panic!("expected one shader op, got {other:?}"),
    }
}

/// The mesh draws under everything else, full bleed, with the theme palette and its
/// typed params, at the time the global timeline rests in its state: the two chart
/// transitions before it, 420 ms each, end to end.
#[test]
fn the_mesh_background_rests_at_its_global_time() {
    let mut fx = fixture();
    let dl = fx.frame("mesh").unwrap();
    let first = dl.ops.iter().position(|op| matches!(op, Op::Layer { .. })).unwrap();
    assert!(matches!(&dl.ops[first], Op::Layer { node: Some(n), .. } if n == "mesh-bg"), "z = -100 paints first");
    let (transform, t, rect) = mesh_op(&dl);
    assert_eq!((transform, rect), ([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1920.0, 1080.0]));
    assert_eq!(t, 0.84);
    let Some(Op::Layer { ops, .. }) = dl.ops.get(first) else { unreachable!() };
    let Op::Shader { seed, palette, params, .. } = &ops[0] else { unreachable!() };
    assert_eq!((*seed, palette.len()), (7, 4));
    let typed: Vec<(&str, f32)> = params.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    assert_eq!(typed, [("drift", 0.12), ("grain", 0.035), ("points", 5.0), ("softness", 0.85)]);
}

/// Shader time is the global timeline's, so a background keeps drifting through a
/// transition and comes to rest where the next state's transition picks it up.
#[test]
fn a_shader_drifts_on_through_a_transition() {
    let mut fx = fixture();
    fx.deck.states.iter_mut().find(|s| s.id == "mesh").unwrap().transition = Some("standard".into());
    let d = fx.duration("mesh");
    let start = 0.84_f32;
    for at in [0.25, 0.5] {
        let (_, t, _) = mesh_op(&fx.frame_at("mesh", at * d).unwrap());
        assert!((t - (start + (at * d / 1000.0) as f32)).abs() < 1e-6, "{at}: t = {t}");
    }
    let (_, t, _) = mesh_op(&fx.frame("mesh").unwrap());
    assert!((t - (start + (d / 1000.0) as f32)).abs() < 1e-6, "at rest: t = {t}");
}

#[test]
fn no_kill_case_draws_notdef() {
    let mut fx = fixture();
    let kill = [
        "axes", "liga", "kern", "numerals", "accents", "mixed", "tracking", "hanging", "balance", "pretty", "fallback",
    ];
    for state in kill {
        let dl = fx.frame(state).unwrap();
        for op in &dl.ops {
            if let Op::Layer { node: Some(node), .. } = op {
                assert!(glyphs(&dl, node).iter().all(|g| g.id != 0), "{state}/{node} draws .notdef");
            }
        }
    }
}

// --- kill cases ------------------------------------------------------------------

/// Roboto Serif's fvar order is wdth, opsz, wght, GRAD.
#[test]
fn kill_variable_axes_reach_the_instance() {
    let mut fx = fixture();
    let dl = fx.frame("axes").unwrap();
    let coords = |node: &str| runs(&dl, node)[0].2.to_vec();
    assert_eq!((coords("axes-wght-100")[2], coords("axes-wght-900")[2]), (-16384, 16384), "wght 100 / 900");
    assert_eq!((coords("axes-wdth-50")[0], coords("axes-wdth-150")[0]), (-16384, 16384), "wdth 50 / 150");
    assert_eq!((coords("axes-opsz-8")[1], coords("axes-opsz-144")[1]), (-16384, 16384), "opsz 8 / 144");
    // Width follows wdth; opsz alone changes advances at a fixed size.
    let mut width = |node: &str| fx.text("axes", node).width;
    assert!(width("axes-wdth-50") < 0.75 * width("axes-wdth-150"));
    assert_ne!(width("axes-opsz-8"), width("axes-opsz-144"));
}

#[test]
fn kill_standard_ligatures_form_and_switch_off() {
    let mut fx = fixture();
    let dl = fx.frame("liga").unwrap();
    let (on, off) = (glyphs(&dl, "liga-on"), glyphs(&dl, "liga-off"));
    // office affluent fjord flight: ffi, ffl, fj, fl save 2 + 2 + 1 + 1 glyphs.
    assert_eq!(off.len() - on.len(), 6, "on: {}, off: {}", on.len(), off.len());
    let off_ids: BTreeSet<u32> = off.iter().map(|g| g.id).collect();
    assert!(on.iter().any(|g| !off_ids.contains(&g.id)), "ligature glyphs appear only with liga on");
}

#[test]
fn kill_kerning_closes_pairs_and_switches_off() {
    let mut fx = fixture();
    let dl = fx.frame("kern").unwrap();
    let (on, off) = (glyphs(&dl, "kern-on"), glyphs(&dl, "kern-off"));
    assert_eq!(on.iter().map(|g| g.id).collect::<Vec<_>>(), off.iter().map(|g| g.id).collect::<Vec<_>>());
    // "AVATAR": A→V is the first kerned pair.
    assert!(on[1].x - on[0].x < off[1].x - off[0].x - 3.0, "AV kerns tighter with kern on");
    assert!(on.last().unwrap().x < off.last().unwrap().x - 20.0, "the whole line closes up");
}

#[test]
fn kill_numeral_styles_switch_widths_and_forms() {
    let mut fx = fixture();
    let dl = fx.frame("numerals").unwrap();
    // "1111 · 0000 · …": glyphs 0–3 are 1s, 7–10 are 0s, one glyph per character.
    let widths = |node: &str| {
        let g = glyphs(&dl, node);
        (g[4].x - g[0].x, g[11].x - g[7].x, g[0].id)
    };
    let (tl, to, pl, po) = (widths("num-tab-lin"), widths("num-tab-old"), widths("num-pro-lin"), widths("num-pro-old"));
    for (name, (ones, zeros, _)) in [("tabular lining", tl), ("tabular oldstyle", to)] {
        assert!((ones - zeros).abs() < 1e-3, "{name}: 1111 = {ones}, 0000 = {zeros}");
    }
    for (name, (ones, zeros, _)) in [("proportional lining", pl), ("proportional oldstyle", po)] {
        assert!(ones < zeros - 10.0, "{name}: 1111 = {ones} should be narrower than 0000 = {zeros}");
    }
    assert_ne!(tl.2, to.2, "oldstyle and lining 1 are different glyphs");
    assert_ne!(pl.2, po.2, "oldstyle and lining 1 are different glyphs (proportional)");
}

#[test]
fn kill_accented_latin_stays_in_the_primary_font() {
    let mut fx = fixture();
    let dl = fx.frame("accents").unwrap();
    for node in ["accents-1", "accents-2", "accents-3"] {
        assert_eq!(fonts_used(&dl, node), BTreeSet::from([SERIF]), "{node}");
    }
}

#[test]
fn kill_mixed_weights_and_sizes_share_one_baseline() {
    let mut fx = fixture();
    let dl = fx.frame("mixed").unwrap();
    let all = runs(&dl, "mixed-line");
    let baselines: BTreeSet<u32> = all.iter().flat_map(|r| r.3.iter().map(|g| g.y.to_bits())).collect();
    assert_eq!(baselines.len(), 1, "one line, one baseline");
    let sizes: BTreeSet<u32> = all.iter().map(|r| r.1 as u32).collect();
    assert_eq!(sizes, BTreeSet::from([28, 56, 112]));
    let weights: BTreeSet<i16> = all.iter().filter(|r| r.1 == 56.0).map(|r| r.2[2]).collect();
    assert_eq!(weights.len(), 3, "thin, regular, and black instances: {weights:?}");
}

#[test]
fn kill_tracking_extremes_space_every_glyph_without_reshaping() {
    // Same deck with tracking zeroed in both roles is the control.
    let mut fx = fixture();
    let mut control = fixture_with(
        |raw| {
            raw["type"]["roles"]["tight"]["tracking"] = 0.0.into();
            raw["type"]["roles"]["loose"]["tracking"] = 0.0.into();
        },
        false,
    );
    let (dl, base) = (fx.frame("tracking").unwrap(), control.frame("tracking").unwrap());
    for (node, per_glyph) in [("track-tight", -0.08 * 96.0), ("track-loose", 0.4 * 56.0)] {
        let (g, c) = (glyphs(&dl, node), glyphs(&base, node));
        assert_eq!(g.iter().map(|g| g.id).collect::<Vec<_>>(), c.iter().map(|g| g.id).collect::<Vec<_>>(), "{node}");
        let shift = g.last().unwrap().x - c.last().unwrap().x;
        let expected = per_glyph * (g.len() - 1) as f32;
        assert!((shift - expected).abs() < 0.05, "{node}: last glyph moved {shift}, expected {expected}");
    }
}

#[test]
fn kill_balance_evens_the_headline_that_greedy_leaves_ragged() {
    let mut fx = fixture();
    let balanced = fx.text("balance", "balance-head");
    assert_eq!((balanced.wrap, balanced.lines.len()), (Wrap::Balance, 2));
    let (a, b) = (balanced.lines[0].width, balanced.lines[1].width);
    assert!(a.min(b) / a.max(b) > 0.9, "balanced lines {a} / {b}");
    // The bait still bites: greedy at the same width leaves a short second line.
    let mut greedy = fixture_with(|raw| raw["type"]["roles"]["headline"]["wrap"] = "greedy".into(), false);
    let ragged = greedy.text("balance", "balance-head");
    assert!(ragged.lines[1].width / ragged.lines[0].width < 0.4, "greedy {:?}", ragged.lines);
}

#[test]
fn kill_pretty_keeps_two_words_on_the_last_line_where_greedy_strands_one() {
    let mut fx = fixture();
    let text = fx.deck.nodes["pretty-para"].props["text"].as_str().unwrap().to_string();
    let pretty = fx.text("pretty", "pretty-para");
    assert_eq!((pretty.wrap, pretty.fallback), (Wrap::Pretty, None));
    assert!(line_words(&pretty, &text, pretty.lines.len() - 1) >= 2, "{:?}", pretty.lines);
    assert!(!pretty.widow);
    let mut bare = fixture_with(
        |raw| {
            raw["type"]["roles"]["body"]["wrap"] = "greedy".into();
            raw["type"]["roles"]["body"]["minLastLineWords"] = 1.into();
        },
        false,
    );
    let stranded = bare.text("pretty", "pretty-para");
    assert_eq!(line_words(&stranded, &text, stranded.lines.len() - 1), 1, "the bait still bites under bare greedy");
    // Greedy breaking holds `minLastLineWords` too: it pulls a word down to the last line.
    let mut greedy = fixture_with(|raw| raw["type"]["roles"]["body"]["wrap"] = "greedy".into(), false);
    let held = greedy.text("pretty", "pretty-para");
    assert_eq!((held.wrap, held.lines.len()), (Wrap::Greedy, stranded.lines.len()));
    assert!(line_words(&held, &text, held.lines.len() - 1) >= 2 && !held.widow, "{:?}", held.lines);
}

/// SPEC §3.5: a quotation mark that opens a line hangs outside the text edge, in every
/// role, and the line is measured without it.
#[test]
fn kill_quotes_that_open_a_line_hang_outside_the_text_edge() {
    let mut fx = fixture();
    let text = fx.deck.nodes["hanging-quote"].props["text"].as_str().unwrap().to_string();
    let p = fx.placed("hanging", "hanging-quote");
    let lines = line_texts(&p.text, &text);
    let hung: Vec<bool> = p.text.lines.iter().map(|l| l.hang > 0.0).collect();
    assert_eq!(hung, [true, false, true, false, false], "{lines:?}");
    let first = &p.text.lines[0];
    assert!(
        first.width <= p.cell[2] && first.width + first.hang > p.cell[2],
        "fits only with its quote hung: {first:?}"
    );
    // A hung quote sits at -hang with the next letter on the edge; other lines start on it,
    // or, with optical margins (the role sets them), a little past it: the V of line 2 and
    // the W of line 5 move out 5% of their advance (PLAN 1.8).
    let optical: Vec<bool> = p.text.lines.iter().map(|l| l.optical > 0.0).collect();
    assert_eq!(optical, [false, true, false, false, true], "{lines:?}");
    for (k, line) in p.text.lines.iter().enumerate() {
        let mut xs: Vec<f32> =
            p.text.runs.iter().filter(|r| r.line == k).flat_map(|r| r.glyphs.iter().map(|g| g.x)).collect();
        xs.sort_by(f32::total_cmp);
        let starts = if line.hang > 0.0 { vec![-line.hang, 0.0] } else { vec![-line.optical] };
        assert_eq!(xs[..starts.len()], starts, "line {k}: {}", lines[k]);
    }
    // Not a role option: `specimen` sets neither `hangingPunctuation` nor `opticalMargins`.
    let punct = fx.text("punctuation", "punct-1");
    let left = punct.runs.iter().flat_map(|r| &r.glyphs).map(|g| g.x).fold(f32::INFINITY, f32::min);
    assert!(punct.lines[0].hang > 0.0 && left == -punct.lines[0].hang, "{:?}", punct.lines[0]);
}

#[test]
fn kill_fallback_within_one_run_uses_three_bundle_fonts() {
    let mut fx = fixture();
    let dl = fx.frame("fallback").unwrap();
    assert_eq!(fonts_used(&dl, "fallback-run"), BTreeSet::from([SERIF, GARAMOND, HEBREW]));
}

// --- hanging quotes (SPEC §3.5) -------------------------------------------------------

/// Each breaking gives a line opened by a quote the measure plus the quote. At a measure
/// the second line fits only that way, all three keep it whole; counting the quote would
/// break it after `‘quoted’`.
#[test]
fn every_breaking_measures_a_line_without_the_quote_it_hangs() {
    let alone = set("‘quoted’ words", Wrap::Greedy, 1e4);
    let (inside, hang) = (alone.lines[0].width, alone.lines[0].hang);
    assert!(hang > 0.0, "{:?}", alone.lines);
    let measure = inside + 0.5 * hang;
    let text = "Typography ‘quoted’ words";
    for wrap in [Wrap::Greedy, Wrap::Pretty, Wrap::Balance] {
        let layout = set(text, wrap, measure);
        assert_eq!((layout.wrap, layout.fallback), (wrap, None));
        assert_eq!(line_texts(&layout, text), ["Typography ", "‘quoted’ words"], "{wrap:?}");
        assert_eq!(layout.lines[1].hang, hang, "{wrap:?}");
        assert!(layout.lines.iter().all(|l| l.width <= measure), "{wrap:?}: {:?}", layout.lines);
    }
}

/// Right to left, the start edge is the right one: the quote that opens the line starts
/// where the measure ends.
#[test]
fn a_right_to_left_line_hangs_its_opening_quote_past_the_right_edge() {
    let measure = 1000.0;
    let layout = set("“שלום הגרסה מוכנה”", Wrap::Greedy, measure);
    assert!(layout.rtl && layout.lines.len() == 1, "{:?}", layout.lines);
    assert!(layout.lines[0].hang > 0.0);
    let right = layout.runs.iter().flat_map(|r| &r.glyphs).map(|g| g.x).fold(f32::MIN, f32::max);
    assert!((right - measure).abs() < 1e-3, "rightmost glyph at {right}, measure {measure}");
}

// --- chart motion (PLAN 0.10, gate 0 criterion 3) ---------------------------------

/// Fills as boxes `[x0, y0, x1, y1]`, and nested layers as (opacity, glyph ids).
type ChartParts = (Vec<[f32; 4]>, Vec<(f32, Vec<u32>)>);

/// The `bars` layer's fills and its nested layers (the category and value labels), in
/// paint order.
fn chart_parts(dl: &DisplayList) -> ChartParts {
    let ops = dl
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Layer { node: Some(n), ops, .. } if n == "bars" => Some(ops),
            _ => None,
        })
        .expect("a `bars` layer");
    let (mut fills, mut labels) = (Vec::new(), Vec::new());
    for op in ops {
        match op {
            Op::Fill { path, .. } => {
                let pts = path.0.iter().flat_map(|el| match *el {
                    PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
                    PathEl::QuadTo(a, b) => vec![a, b],
                    PathEl::CurveTo(a, b, c) => vec![a, b, c],
                    PathEl::Close => vec![],
                });
                fills.push(pts.fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, [x, y]| {
                    [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)]
                }));
            }
            Op::Layer { opacity, ops, .. } => {
                let ids = ops.iter().flat_map(|op| match op {
                    Op::Glyphs { glyphs, .. } => glyphs.iter().map(|g| g.id).collect(),
                    _ => Vec::new(),
                });
                labels.push((*opacity, ids.collect()));
            }
            _ => {}
        }
    }
    (fills, labels)
}

/// The bounding box `[x0, y0, x1, y1]` of the `bars` layer's clip, in the layer's space.
fn chart_clip(dl: &DisplayList) -> [f32; 4] {
    let clip = dl
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Layer { node: Some(n), clip, .. } if n == "bars" => clip.as_ref(),
            _ => None,
        })
        .expect("the `bars` layer clips");
    clip.0.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, el| match *el {
        PathEl::MoveTo([x, y]) | PathEl::LineTo([x, y]) => [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)],
        _ => b,
    })
}

fn lerp_box(a: [f32; 4], b: [f32; 4], p: f32) -> [f32; 4] {
    [0, 1, 2, 3].map(|c| a[c] * (1.0 - p) + b[c] * p)
}

fn assert_box(got: [f32; 4], want: [f32; 4], what: &str) {
    for c in 0..4 {
        assert!((got[c] - want[c]).abs() < 1e-3, "{what}: edge {c}: {} vs {}", got[c], want[c]);
    }
}

/// The figures `state`'s chart counts with, as laid out for that state.
fn numerals(fx: &mut Fixture, state: &str) -> scaena_engine::charts::Numerals {
    let snapshots = scaena_core::resolve_states(&fx.deck).unwrap();
    let snap = snapshots.iter().find(|s| s.state_id == state).unwrap();
    let scene = fx.engine.scene(&fx.deck, &fx.theme, &fx.data, snap).unwrap();
    scene
        .nodes
        .into_iter()
        .find_map(|n| match n.content {
            scaena_engine::sample::Content::Chart { chart, .. } => chart.numerals,
            _ => None,
        })
        .expect("the chart shows values")
}

fn ids(runs: &[scaena_engine::text::GlyphRun]) -> Vec<u32> {
    runs.iter().flat_map(|r| r.glyphs.iter().map(|g| g.id)).collect()
}

#[test]
fn chart_transitions_start_and_end_exactly_at_rest() {
    let mut fx = fixture();
    for (state, before) in [("chart", "chart-intro"), ("chart-next", "chart")] {
        let d = fx.duration(state);
        assert_eq!(d, 420.0, "theme `standard`");
        let from = fx.frame(before).unwrap().to_postcard().unwrap();
        let to = fx.frame(state).unwrap().to_postcard().unwrap();
        assert_eq!(fx.frame_at(state, 0.0).unwrap().to_postcard().unwrap(), from, "{state}: t = 0 is `{before}`");
        assert_eq!(fx.frame_at(state, d).unwrap().to_postcard().unwrap(), to, "{state}: t = d is `{state}` at rest");
        for t in [1.0, 0.5 * d, d - 1.0] {
            let mid = fx.frame_at(state, t).unwrap().to_postcard().unwrap();
            assert!(mid != from && mid != to, "{state}: t = {t} lies between");
        }
    }
    // A state without `transition` cuts: at rest from its first frame.
    assert_eq!(fx.frame_at("liga", 0.0).unwrap(), fx.frame("liga").unwrap());
}

/// "Animating in values": the chart enters, every bar grows from the baseline, and
/// every value label counts up from 0 on top of its bar as it fades in.
#[test]
fn values_animate_in_bars_grow_and_labels_count_up() {
    let mut fx = fixture();
    let timing = scaena_engine::render::timing(&fx.deck, &fx.theme, "chart").unwrap();
    let (bars, _) = chart_parts(&fx.frame("chart").unwrap());
    let base = bars[0][3];
    let numerals = numerals(&mut fx, "chart");
    let values = [12.0, 19.0, 7.0, 24.0, 16.0, 31.0];
    for at in [0.25, 0.5] {
        let p = timing.progress(at * timing.duration_ms) as f32;
        let (fills, labels) = chart_parts(&fx.frame_at("chart", at * timing.duration_ms).unwrap());
        for (k, (fill, bar)) in fills.iter().zip(&bars).enumerate() {
            assert_box(*fill, lerp_box([bar[0], base, bar[2], base], *bar, p), &format!("{at}: bar {k}"));
        }
        // Six quarter labels fading in, then six value labels counting.
        assert!(labels[..6].iter().all(|(o, _)| *o == p), "{at}: {:?}", &labels[..6]);
        for (k, v) in values.iter().enumerate() {
            let text = format!("{}", (v * f64::from(p)).round() as i64);
            let (runs, _) = numerals.compose(&text).unwrap();
            assert_eq!(labels[6 + k], (p, ids(&runs)), "{at}: value {k} reads {text}");
        }
    }
}

/// "One month to the next": the next quarter arrives and the window scrolls. Matched
/// by key, the five quarters that stay slide one band left. 2025-Q1 rides out with its
/// neighbor under the cell's left edge, shrinking onto the baseline as its label
/// counts down and fades; 2026-Q3 rides in from the right, growing from the baseline
/// as its label counts up and fades in. The chart clips at its cell's sides only.
#[test]
fn the_next_quarter_scrolls_the_window_by_key() {
    let mut fx = fixture();
    let timing = scaena_engine::render::timing(&fx.deck, &fx.theme, "chart-next").unwrap();
    let rest = fx.frame("chart").unwrap();
    let (old, _) = chart_parts(&rest);
    let (new, _) = chart_parts(&fx.frame("chart-next").unwrap());
    let numerals = numerals(&mut fx, "chart-next");
    let clip = chart_clip(&rest);
    assert_eq!((clip[0], clip[3] - clip[1]), (0.0, 1080.0), "the cell's left side; the canvas top to bottom");
    assert!((clip[2] - old[5][2] - old[0][0]).abs() < 1e-3, "the cell's right side: {clip:?}");
    let center = |b: [f32; 4]| 0.5 * (b[0] + b[2]);
    for at in [0.25, 0.5] {
        let p = timing.progress(at * timing.duration_ms) as f32;
        let dl = fx.frame_at("chart-next", at * timing.duration_ms).unwrap();
        assert_eq!(chart_clip(&dl), clip, "{at}: the same cell, the same clip");
        let (fills, labels) = chart_parts(&dl);
        assert_eq!(fills.len(), 7, "the leaving quarter, the five that stay, the arriving one");
        let (gone, dx) = (old[0], center(new[0]) - center(old[1]));
        assert!(gone[2] + dx <= clip[0], "2025-Q1 ends wholly past the left edge");
        let base = new[0][3];
        assert_box(fills[0], lerp_box(gone, [gone[0] + dx, base, gone[2] + dx, base], p), "2025-Q1 rides out");
        for k in 0..5 {
            assert_box(fills[1 + k], lerp_box(old[1 + k], new[k], p), &format!("quarter {k} moves by key"));
        }
        let (born, dx) = (new[5], center(new[4]) - center(old[5]));
        assert!(born[0] - dx >= clip[2], "2026-Q3 starts wholly past the right edge");
        let base = old[0][3];
        assert_box(fills[6], lerp_box([born[0] - dx, base, born[2] - dx, base], born, p), "2026-Q3 rides in");
        // Seven quarter labels (one leaving, one arriving), then seven value labels; the
        // five that keep their value ride along unchanged.
        let alphas: Vec<f32> = labels.iter().map(|l| l.0).collect();
        assert_eq!(alphas[..7], [1.0 - p, 1.0, 1.0, 1.0, 1.0, 1.0, p], "{at}: quarter labels");
        assert_eq!(alphas[7..], [1.0 - p, 1.0, 1.0, 1.0, 1.0, 1.0, p], "{at}: value labels");
        let down = format!("{}", (12.0 + (0.0 - 12.0) * f64::from(p)).round() as i64);
        let up = format!("{}", (38.0 * f64::from(p)).round() as i64);
        assert_eq!(labels[7].1, ids(&numerals.compose(&down).unwrap().0), "{at}: 2025-Q1 counts down to {down}");
        assert_eq!(labels[13].1, ids(&numerals.compose(&up).unwrap().0), "{at}: 2026-Q3 counts up to {up}");
    }
}

/// Counting composes figures shaped once instead of shaping per frame. That holds only
/// if tabular figures neither kern nor change in context beyond what the figures were
/// shaped in: every number the deck could count through must spell with the glyphs
/// shaping picks, within 1/1000 cu (advances measured between glyphs carry float
/// noise; only frames inside a transition compose).
#[test]
fn counting_figures_spell_numbers_as_shaping_does() {
    let mut fx = fixture();
    let numerals = numerals(&mut fx, "chart");
    let mut fonts = bundle_fonts(&fx.deck, false);
    let mut engine = TextEngine::new();
    // The chart has no `format`, so labels count in the default form: no grouping, and a
    // hyphen-minus, since these fonts have no U+2212.
    let samples = ["-7", "0.5", "12.25", "1024", "-0.75", "100.5", "-1000.25", "38.0"];
    for text in (0..=200).map(|n| n.to_string()).chain(samples.map(String::from)) {
        let spec = TextSpec {
            numeric: Some(scaena_engine::theme::Numeric::TabularLining),
            ..TextSpec::plain(fx.theme.text_role("label").unwrap(), text.clone())
        };
        let shaped = engine.layout(&mut fonts, &fx.theme, &spec, f32::INFINITY).unwrap();
        let (runs, width) = numerals.compose(&text).unwrap();
        let glyphs = |runs: &[scaena_engine::text::GlyphRun]| -> Vec<Glyph> {
            runs.iter().flat_map(|r| r.glyphs.iter().copied()).collect()
        };
        let (composed, want) = (glyphs(&runs), glyphs(&shaped.runs));
        assert_eq!(
            composed.iter().map(|g| g.id).collect::<Vec<_>>(),
            want.iter().map(|g| g.id).collect::<Vec<_>>(),
            "{text}"
        );
        for (c, w) in composed.iter().zip(&want) {
            assert!((c.x - w.x).abs() < 1e-3 && c.y == w.y, "{text}: {c:?} vs {w:?}");
        }
        assert!((width - shaped.width).abs() < 1e-3, "{text}: {width} vs {}", shaped.width);
    }
}

/// Gate 0 criterion 3: frames sample. One `Transition` is laid out once and draws
/// every frame; `Transition::frame` takes `&self` and holds no fonts or layout engine,
/// so it cannot lay out. Its frames are the ones `Engine::frame` returns.
#[test]
fn one_transition_samples_every_frame_engine_frame_returns() {
    let mut fx = fixture();
    let transition = fx.engine.transition(&fx.deck, &fx.theme, &fx.data, "chart-next").unwrap();
    let d = transition.duration_ms();
    for at in [0.0, 0.25, 0.5, 1.0] {
        assert_eq!(transition.frame(at * d), fx.frame_at("chart-next", at * d).unwrap(), "t = {at} of {d} ms");
    }
}

// --- alignment (PLAN 0.5) -----------------------------------------------------------

/// Canvas y of a placed text's first cap top, first x-height top, and last baseline.
fn anchors(p: &PlacedText) -> (f32, f32, f32) {
    let (first, last) = (&p.text.lines[0], p.text.lines.last().unwrap());
    let y = |local: f32| p.origin[1] + local;
    (y(first.baseline - first.cap_height.unwrap()), y(first.baseline - first.x_height.unwrap()), y(last.baseline))
}

#[test]
fn anchors_line_up_cap_heights_baselines_and_x_heights_across_sizes() {
    let mut fx = fixture();
    // Canvas y extent of each row's line boxes: (min top, max bottom).
    let mut rows = [(f32::INFINITY, f32::NEG_INFINITY); 3];
    for size in [112, 64, 28] {
        let placed = ["cap", "base", "x"].map(|row| fx.placed("anchors", &format!("anchor-{row}-{size}")));
        let (cap, base, x) = (anchors(&placed[0]).0, anchors(&placed[1]).2, anchors(&placed[2]).1);
        assert!((cap - 210.0).abs() < 1e-3, "{size} cu cap top at {cap}");
        assert!((base - 642.0).abs() < 1e-3, "{size} cu baseline at {base}");
        assert!((x - 780.0).abs() < 1e-3, "{size} cu x-height at {x}");
        for (row, p) in rows.iter_mut().zip(&placed) {
            // One line each, or the first-line and last-line anchors are different lines.
            assert_eq!(p.text.lines.len(), 1, "{:?} wraps in its cell", p.text.lines);
            let line = &p.text.lines[0];
            *row = (row.0.min(p.origin[1] + line.top), row.1.max(p.origin[1] + line.top + line.height));
        }
    }
    // The rows do not touch, so the raster reads as three separate rows.
    assert!(rows[0].1 < rows[1].0 && rows[1].1 < rows[2].0, "row line boxes overlap: {rows:?}");
}

#[test]
fn box_cap_trims_to_the_cap_height_and_slots_align_by_it() {
    let mut fx = fixture();
    // `headline` is `box: cap`, aligned `start`: its cap top is the cell top.
    let head = fx.placed("balance", "balance-head");
    assert!((anchors(&head).0 - head.cell[1]).abs() < 1e-3, "cap top {} vs cell {}", anchors(&head).0, head.cell[1]);
    // `body` is `box: line`: its line box starts at the cell top.
    let body = fx.placed("pretty", "pretty-para");
    assert_eq!(body.origin[1], body.cell[1]);
    // The `case` slot aligns `y: cap`.
    let label = fx.placed("axes", "case");
    assert!((anchors(&label).0 - 96.0).abs() < 1e-3);
}

// --- baseline grid (PLAN 1.25) ------------------------------------------------------

/// How far `y` is from the torture grid's nearest baseline-grid line (every 8 cu from the
/// 96 cu top margin), in grid lines.
fn off_grid(y: f32) -> f32 {
    let lines = (y - 96.0) / 8.0;
    (lines - lines.round()).abs()
}

fn baselines(p: &PlacedText) -> Vec<f32> {
    p.text.lines.iter().map(|l| p.origin[1] + l.baseline).collect()
}

#[test]
fn the_grid_roles_set_their_baselines_and_cap_heights_on_the_baseline_grid() {
    let mut fx = fixture();
    // Body (40 cu: five lines), the off-grid leading (43.2 cu, set at six), the caption (32).
    for (node, apart) in [("grid-body", 40.0), ("grid-loose", 48.0), ("grid-foot", 32.0)] {
        let on = baselines(&fx.placed("baseline-grid", node));
        assert!(on.len() >= 2, "`{node}` sets one line: {on:?}");
        assert!(on.iter().all(|y| off_grid(*y) < 1e-3), "`{node}`: {on:?}");
        assert!(on.windows(2).all(|p| (p[1] - p[0] - apart).abs() < 1e-3), "`{node}`: {on:?}");
    }
    // Both paragraphs start on row 3, at 324 cu, off the grid: their first baselines meet.
    let (body, loose) = (fx.placed("baseline-grid", "grid-body"), fx.placed("baseline-grid", "grid-loose"));
    assert!((baselines(&body)[0] - baselines(&loose)[0]).abs() < 1e-3);
    // The headline's cap top sits on a grid line.
    let head = fx.placed("baseline-grid", "grid-head");
    assert!(off_grid(anchors(&head).0) < 1e-3, "cap top at {}", anchors(&head).0);
    // The caption aligned to its box's foot moved up, so it stays in the box.
    let foot = fx.placed("baseline-grid", "grid-foot");
    let (bottom, cell_bottom) = (foot.origin[1] + foot.text.height, foot.cell[1] + foot.cell[3]);
    assert!(bottom <= cell_bottom + 1e-3 && cell_bottom - bottom < 8.0, "{bottom} {cell_bottom}");
    // The row's body text and caption share one baseline, on the grid.
    let figure = anchors(&fx.placed("baseline-grid", "grid-row-figure")).2;
    let label = anchors(&fx.placed("baseline-grid", "grid-row-label")).2;
    assert!((figure - label).abs() < 1e-3 && off_grid(figure) < 1e-3, "{figure} {label}");
}

// --- catalogue (characterized as observed) ----------------------------------------

/// Known failure: EB Garamond maps the regional indicators and precedes the emoji font
/// in the stack, and parley appends the emoji family last for emoji clusters, so the
/// flag is drawn as two monochrome Garamond letters. Everything else reaches the
/// color font. See docs/spike-report.md (PLAN 0.4, catalogue).
#[test]
fn catalogue_emoji_flag_falls_to_garamond_everything_else_is_color() {
    let mut fx = fixture();
    let dl = fx.frame("emoji").unwrap();
    let by_font: Vec<(&str, usize)> = runs(&dl, "emoji-line").iter().map(|r| (r.0, r.3.len())).collect();
    let emoji_glyphs: usize = by_font.iter().filter(|(f, _)| *f == EMOJI).map(|(_, n)| n).sum();
    assert_eq!(emoji_glyphs, 4, "🚀 🙂 👩🏽‍💻 ❤️ as one color glyph each: {by_font:?}");
    assert!(by_font.contains(&(GARAMOND, 2)), "🇺🇸 as two Garamond regional-indicator glyphs: {by_font:?}");
}

#[test]
fn catalogue_bidi_paragraphs_are_rtl_and_right_aligned() {
    let mut fx = fixture();
    for (state, node) in [("bidi-hebrew", "bidi-he-line"), ("bidi-arabic", "bidi-ar-line")] {
        let layout = fx.text(state, node);
        assert!(layout.rtl, "{node}");
        assert_eq!(layout.lines.len(), 1, "{node}");
        let dl = fx.frame(state).unwrap();
        let xs: Vec<f32> = glyphs(&dl, node).iter().map(|g| g.x).collect();
        assert!(xs.iter().copied().fold(f32::MAX, f32::min) > 800.0, "{node} starts right of centre: {xs:?}");
    }
}

/// Known flaw: `j` + U+030C keeps the j's dot under the caron. Roboto Serif has no
/// precomposed ǰ (U+01F0) for the shaper to compose to, and no substitution of its
/// dotless j before a top mark, so the mark lands on the dot. Both painters draw it the
/// same way. See docs/spike-report.md (PLAN 0.4 catalogue, found in 0.7).
#[test]
fn catalogue_j_with_caron_keeps_its_dot_because_the_font_cannot_drop_it() {
    use skrifa::MetadataProvider;
    let mut fx = fixture();
    let dl = fx.frame("combining").unwrap();
    let serif = std::fs::read(format!("{BUNDLE}/{SERIF}")).unwrap();
    let charmap = skrifa::FontRef::new(&serif).unwrap().charmap();
    let gid = |c: char| charmap.map(c).unwrap().to_u32();
    assert_eq!(charmap.map('\u{1F0}'), None, "the font now has a precomposed ǰ");
    let ids: Vec<u32> = glyphs(&dl, "combining-2").iter().map(|g| g.id).collect();
    assert!(ids.windows(2).any(|w| w == [gid('j'), gid('\u{30C}')]), "{ids:?}");
}

#[test]
fn catalogue_combining_marks_shape_to_single_clusters() {
    let mut fx = fixture();
    let dl = fx.frame("combining").unwrap();
    assert_eq!(fonts_used(&dl, "combining-1"), BTreeSet::from([SERIF]));
    assert!(glyphs(&dl, "combining-1").iter().all(|g| g.id != 0));
}
