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
use scaena_core::displaylist::{DisplayList, Glyph, Op, Paint, PathEl, quantize};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::text::{Span, TextEngine, TextLayout, TextSpec};
use scaena_engine::theme::{Theme, Wrap};
use scaena_engine::{Engine, EngineError, FrameRequest, PlacedText};
use std::collections::BTreeSet;

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";
const GOLDEN: &str = "../../tests/golden/torture";
/// States whose node types arrive later: shaders (PLAN 0.11).
const LATER: [(&str, &str); 1] = [("mesh", "PLAN 0.11")];
/// Frames inside a transition, as (state, fraction of its duration). PLAN 0.10 renders
/// the bar → line morph at t = 0, 0.25, 0.5, and 1 of the transition; 0 and 1 are the
/// two states at rest (asserted below), so the middle two get goldens of their own.
const MORPH: [(&str, f64); 2] = [("chart-line", 0.25), ("chart-line", 0.5)];
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
    let mut theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    theme_edit(&mut theme.raw);
    let fonts = bundle_fonts(&deck, reverse_fonts);
    fonts.check_theme(&theme).unwrap();
    let mut data = DataFiles::new();
    for source in deck.data.values() {
        let path = source.source.as_str().unwrap();
        data.insert(path, read(path));
    }
    Fixture { deck, theme, data, engine: Engine::new(fonts) }
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
        let req = FrameRequest { deck: &self.deck, theme: &self.theme, data: &self.data, state, t_ms };
        Ok(self.engine.frame(&req)?.display_list)
    }

    fn duration(&self, state: &str) -> f64 {
        scaena_engine::render::timing(&self.deck, &self.theme, state).unwrap().duration_ms
    }

    /// Every golden frame as (name, state, t_ms): each state at rest under its own
    /// name, then the transition frames in [`MORPH`] as `state@fraction`.
    fn golden_frames(&self) -> Vec<(String, String, f64)> {
        let mut out: Vec<(String, String, f64)> = self
            .states()
            .into_iter()
            .filter(|s| !LATER.iter().any(|(later, _)| later == s))
            .map(|s| (s.clone(), s, f64::INFINITY))
            .collect();
        for (state, at) in MORPH {
            out.push((format!("{state}@{at}"), state.to_string(), at * self.duration(state)));
        }
        out
    }

    fn placed(&mut self, state: &str, node: &str) -> PlacedText {
        let req = FrameRequest { deck: &self.deck, theme: &self.theme, data: &self.data, state, t_ms: f64::INFINITY };
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
    let span = Span { text: text.into(), role: "specimen".into() };
    let spec = TextSpec { spans: vec![span], role: "specimen".into(), wrap: Some(wrap), ..TextSpec::default() };
    TextEngine::new().layout(&mut fonts, &fx.theme, &spec, width).unwrap()
}

// --- goldens and determinism -----------------------------------------------------

#[test]
fn display_lists_match_goldens() {
    let mut fx = fixture();
    let bless = std::env::var_os("SCAENA_BLESS").is_some();
    let mut changed = Vec::new();
    for (name, state, t_ms) in fx.golden_frames() {
        let mut dl = fx.frame_at(&state, t_ms).unwrap_or_else(|e| panic!("{name}: {e}"));
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

/// FNV-1a, 64-bit: a dependency-free digest for change detection (not security).
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
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
    for (name, state, t_ms) in fx.golden_frames() {
        let bytes = fx.frame_at(&state, t_ms).unwrap().to_postcard().unwrap();
        digests.push_str(&format!("{name} {:016x}\n", fnv1a(&bytes)));
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
    for (name, state, t_ms) in a.golden_frames() {
        let first = a.frame_at(&state, t_ms).unwrap().to_postcard().unwrap();
        assert_eq!(
            a.frame_at(&state, t_ms).unwrap().to_postcard().unwrap(),
            first,
            "{name}: same engine, second frame"
        );
        assert_eq!(
            b.frame_at(&state, t_ms).unwrap().to_postcard().unwrap(),
            first,
            "{name}: fonts registered in reverse order"
        );
    }
}

#[test]
fn later_node_types_say_which_plan_task_adds_them() {
    let mut fx = fixture();
    for (state, task) in LATER {
        let err = fx.frame(state).unwrap_err();
        assert!(matches!(err, EngineError::NotImplemented(m) if m.contains(task)), "{state}: {err}");
    }
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
    let mut greedy = fixture_with(|raw| raw["type"]["roles"]["body"]["wrap"] = "greedy".into(), false);
    let stranded = greedy.text("pretty", "pretty-para");
    assert_eq!(line_words(&stranded, &text, stranded.lines.len() - 1), 1, "the bait still bites under greedy");
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
    // A hung quote sits at -hang with the next letter on the edge; other lines start on it.
    for (k, line) in p.text.lines.iter().enumerate() {
        let mut xs: Vec<f32> =
            p.text.runs.iter().filter(|r| r.line == k).flat_map(|r| r.glyphs.iter().map(|g| g.x)).collect();
        xs.sort_by(f32::total_cmp);
        let starts = if line.hang > 0.0 { vec![-line.hang, 0.0] } else { vec![0.0] };
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

// --- the bar → line morph (PLAN 0.10, gate 0 criterion 3) ------------------------

/// The `bars` layer's fills as boxes `[x0, y0, x1, y1]`, and its strokes as
/// (path elements, paint alpha), in paint order.
fn chart_parts(dl: &DisplayList) -> (Vec<[f32; 4]>, Vec<(usize, u8)>) {
    let ops = dl
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Layer { node: Some(n), ops, .. } if n == "bars" => Some(ops),
            _ => None,
        })
        .expect("a `bars` layer");
    let (mut fills, mut strokes) = (Vec::new(), Vec::new());
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
            Op::Stroke { path, paint: Paint::Solid(c), .. } => strokes.push((path.0.len(), c.0[3])),
            _ => {}
        }
    }
    (fills, strokes)
}

#[test]
fn bar_to_line_morph_starts_and_ends_exactly_at_rest() {
    let mut fx = fixture();
    let d = fx.duration("chart-line");
    assert_eq!(d, 420.0, "theme `standard`");
    let bars = fx.frame("chart").unwrap().to_postcard().unwrap();
    let line = fx.frame("chart-line").unwrap().to_postcard().unwrap();
    assert_eq!(fx.frame_at("chart-line", 0.0).unwrap().to_postcard().unwrap(), bars, "t = 0 is `chart` at rest");
    assert_eq!(fx.frame_at("chart-line", d).unwrap().to_postcard().unwrap(), line, "t = d is `chart-line` at rest");
    for t in [1.0, 0.5 * d, d - 1.0] {
        let mid = fx.frame_at("chart-line", t).unwrap().to_postcard().unwrap();
        assert!(mid != bars && mid != line, "t = {t} lies between the two states");
    }
    // A state without `transition` cuts: at rest from its first frame.
    assert_eq!(fx.frame_at("liga", 0.0).unwrap(), fx.frame("liga").unwrap());
}

#[test]
fn bars_morph_into_points_by_key_while_the_line_fades_in() {
    let mut fx = fixture();
    let timing = scaena_engine::render::timing(&fx.deck, &fx.theme, "chart-line").unwrap();
    let (bars, _) = chart_parts(&fx.frame("chart").unwrap());
    let (points, _) = chart_parts(&fx.frame("chart-line").unwrap());
    assert_eq!((bars.len(), points.len()), (6, 6));
    for at in [0.25, 0.5] {
        let t = at * timing.duration_ms;
        let p = timing.progress(t) as f32;
        let dl = fx.frame_at("chart-line", t).unwrap();
        let (marks, strokes) = chart_parts(&dl);
        for (k, mark) in marks.iter().enumerate() {
            for c in 0..4 {
                let want = bars[k][c] * (1.0 - p) + points[k][c] * p;
                assert!((mark[c] - want).abs() < 1e-3, "{at}: mark {k} edge {c}: {} vs {want}", mark[c]);
            }
        }
        // The baseline stays; the line through the six points fades in with the progress.
        assert_eq!(strokes, [(2, 255), (6, (255.0 * p).round() as u8)], "{at}");
        // The case label changed, so it cross-fades: two layers for one node.
        let case = dl.ops.iter().filter(|op| matches!(op, Op::Layer { node: Some(n), .. } if n == "case")).count();
        assert_eq!(case, 2, "{at}");
    }
}

/// Gate 0 criterion 3: frames sample. One `Transition` is laid out once and draws
/// every frame; `Transition::frame` takes `&self` and holds no fonts or layout engine,
/// so it cannot lay out. Its frames are the ones `Engine::frame` returns.
#[test]
fn one_transition_samples_every_frame_engine_frame_returns() {
    let mut fx = fixture();
    let transition = fx.engine.transition(&fx.deck, &fx.theme, &fx.data, "chart-line").unwrap();
    let d = transition.duration_ms();
    for at in [0.0, 0.25, 0.5, 1.0] {
        assert_eq!(transition.frame(at * d), fx.frame_at("chart-line", at * d).unwrap(), "t = {at} of {d} ms");
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
