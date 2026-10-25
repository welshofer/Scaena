//! Post-layout interpolation (SPEC §2.3, §5; PLAN 1.12): what a node present in two
//! states does between them, sampled from the two layouts without laying anything out.

use scaena_core::Deck;
use scaena_core::displaylist::{DisplayList, Op, Paint};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The torture deck's fonts, theme, and canvas, with these nodes and states.
struct Fx {
    deck: Deck,
    theme: Theme,
    data: DataFiles,
    engine: Engine,
}

impl Fx {
    fn new(nodes: Value, states: Value) -> Fx {
        let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
        d["nodes"] = nodes;
        d["states"] = states;
        let deck: Deck = serde_json::from_value(d).unwrap();
        let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
        let mut fonts = BundleFonts::new();
        for font in &deck.fonts {
            fonts.register(&font.file, read(&font.file)).unwrap();
        }
        Fx { deck, theme, data: DataFiles::new(), engine: Engine::new(fonts) }
    }

    fn dl(&mut self, state: &str, t_ms: f64) -> DisplayList {
        let req = FrameRequest { deck: &self.deck, theme: &self.theme, data: &self.data, state, t_ms, format: None };
        self.engine.frame(&req).unwrap_or_else(|e| panic!("{state} at {t_ms}: {e}")).display_list
    }
}

/// A word's layer: its transform, its opacity, its glyphs, and their paints.
type WordLayer = ([f32; 6], f32, Vec<u32>, Vec<Paint>);

/// The layers inside `node`'s layer: its words while it morphs.
fn words(dl: &DisplayList, node: &str) -> Vec<WordLayer> {
    let Some(Op::Layer { ops, .. }) =
        dl.ops.iter().find(|op| matches!(op, Op::Layer { node: Some(n), .. } if n == node))
    else {
        panic!("no layer for `{node}`")
    };
    ops.iter()
        .filter_map(|op| match op {
            Op::Layer { transform, opacity, ops, .. } => {
                let glyphs = ops.iter().filter_map(|g| match g {
                    Op::Glyphs { glyphs, paint, .. } => Some((glyphs.iter().map(|g| g.id).collect::<Vec<_>>(), paint)),
                    _ => None,
                });
                let (ids, paints): (Vec<Vec<u32>>, Vec<&Paint>) = glyphs.unzip();
                Some((*transform, *opacity, ids.concat(), paints.into_iter().cloned().collect()))
            }
            _ => None,
        })
        .collect()
}

fn two_states(a: Value, b: Value) -> Fx {
    let nodes = json!({ "t": { "type": "text", "role": "body", "text": "", "at": { "in": "main" } } });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "t": a } },
        { "id": "two", "transition": "standard", "props": { "t": b } }
    ]);
    Fx::new(nodes, states)
}

#[test]
fn changed_text_moves_its_shared_words_and_fades_the_rest() {
    let mut fx =
        two_states(json!({ "text": "Revenue grew this quarter" }), json!({ "text": "Revenue grew fast this quarter" }));
    let mid = words(&fx.dl("two", 210.0), "t");
    // Four words on both sides, at full opacity; `fast` fading in.
    assert_eq!(mid.len(), 5, "{mid:?}");
    let opacities: Vec<f32> = mid.iter().map(|w| w.1).collect();
    assert_eq!(opacities.iter().filter(|o| **o == 1.0).count(), 4, "{opacities:?}");
    assert!(opacities.iter().any(|o| *o > 0.0 && *o < 1.0), "{opacities:?}");
    // `this` and `quarter` slide right to make room: their offset is positive and
    // shrinks to nothing as the transition ends.
    let shifts: Vec<f32> = mid.iter().filter(|w| w.1 == 1.0).map(|w| w.0[4]).collect();
    assert_eq!(shifts.iter().filter(|dx| **dx < -1.0).count(), 2, "two words still left of where they end: {shifts:?}");
    assert_eq!(shifts.iter().filter(|dx| dx.abs() < 1e-3).count(), 2, "two words stay put: {shifts:?}");
    let late = words(&fx.dl("two", 400.0), "t");
    let late_shifts: Vec<f32> = late.iter().filter(|w| w.1 == 1.0).map(|w| w.0[4]).collect();
    assert!(late_shifts.iter().zip(&shifts).all(|(l, m)| l.abs() <= m.abs()), "{late_shifts:?} {shifts:?}");
    // At rest, no words: the text as it is laid out.
    let rest = fx.dl("two", f64::INFINITY);
    assert!(words(&rest, "t").is_empty());
}

/// Every glyph `node`'s layers draw.
fn glyphs(dl: &DisplayList, node: &str) -> usize {
    fn count(ops: &[Op]) -> usize {
        (ops.iter())
            .map(|op| match op {
                Op::Glyphs { glyphs, .. } => glyphs.len(),
                Op::Layer { ops, .. } => count(ops),
                _ => 0,
            })
            .sum()
    }
    layers(dl, node).iter().map(|(_, ops)| count(ops)).sum()
}

/// An editor empties a text before it types the next one: its words fade out over the
/// first half of the cue as it empties and in over the second as it fills, and empty it
/// draws nothing.
#[test]
fn a_text_emptied_fades_its_words_out_and_filled_fades_them_in() {
    for (a, b) in [("Revenue grew", ""), ("", "Revenue grew")] {
        let mut fx = two_states(json!({ "text": a }), json!({ "text": b }));
        let (early, late) = (words(&fx.dl("two", 60.0), "t"), words(&fx.dl("two", 330.0), "t"));
        let (fading, faded) = if b.is_empty() { (early, late) } else { (late, early) };
        assert_eq!(fading.len(), 2, "{a:?} to {b:?}: {fading:?}");
        assert!(fading.iter().all(|w| w.1 > 0.0 && w.1 < 1.0), "{a:?} to {b:?}: {fading:?}");
        assert!(faded.is_empty(), "{a:?} to {b:?}: {faded:?}");
        for (state, text) in [("one", a), ("two", b)] {
            let drawn = glyphs(&fx.dl(state, f64::INFINITY), "t");
            assert_eq!(drawn == 0, text.is_empty(), "{state}: {drawn} glyphs for {text:?}");
        }
    }
}

#[test]
fn a_word_that_changes_size_scales_between_its_boxes_as_its_drawings_cross_fade() {
    let mut fx =
        two_states(json!({ "text": "Growth", "role": "body" }), json!({ "text": "Growth", "role": "headline" }));
    let mid = words(&fx.dl("two", 210.0), "t");
    assert_eq!(mid.len(), 2, "the word at each size: {mid:?}");
    let (small, large) = (&mid[0], &mid[1]);
    assert!(small.1 + large.1 > 0.999 && small.1 < 1.0, "cross-fading: {} + {}", small.1, large.1);
    assert!(small.0[0] > 1.0 && large.0[0] < 1.0, "the small one grows, the large one shrinks to meet it");
}

#[test]
fn a_word_that_changes_color_keeps_its_glyphs_and_mixes_its_color() {
    let mut fx = two_states(
        json!({ "text": "Growth", "style": { "color": "ink" } }),
        json!({ "text": "Growth", "style": { "color": "accent" } }),
    );
    let mid = words(&fx.dl("two", 210.0), "t");
    assert_eq!(mid.len(), 1, "one drawing, moving and recoloring: {mid:?}");
    let (ink, accent) = (fx.theme.color("ink").unwrap(), fx.theme.color("accent").unwrap());
    let Paint::Solid(now) = mid[0].3[0] else { panic!("{:?}", mid[0].3) };
    assert!(now != ink && now != accent, "between the two: {now:?}");
}

/// Each layer `node` draws: its opacity and its ops.
fn layers(dl: &DisplayList, node: &str) -> Vec<(f32, Vec<Op>)> {
    (dl.ops.iter())
        .filter_map(|op| match op {
            Op::Layer { node: Some(n), opacity, ops, .. } if n == node => Some((*opacity, ops.clone())),
            _ => None,
        })
        .collect()
}

fn one_node(node: Value, a: Value, b: Value) -> Fx {
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "n": a } },
        { "id": "two", "transition": "standard", "props": { "n": b } }
    ]);
    Fx::new(json!({ "n": node }), states)
}

#[test]
fn a_shape_morphs_its_points_and_its_paint() {
    let tri = json!({ "type": "shape", "kind": "polygon", "points": [[0.5, 0], [1, 1], [0, 1]], "fill": "accent",
                      "at": { "col": [1, 4], "row": [2, 4] } });
    let mut fx = one_node(tri, json!({}), json!({ "points": [[0, 0], [1, 0], [0.5, 1]], "fill": "ink" }));
    let fill = |dl: &DisplayList| match &layers(dl, "n")[..] {
        [(1.0, ops)] => match &ops[..] {
            [Op::Fill { path, paint: Paint::Solid(c), .. }] => (path.0.clone(), *c),
            other => panic!("{other:?}"),
        },
        other => panic!("one layer, opaque: {other:?}"),
    };
    let (start, end, mid) =
        (fill(&fx.dl("one", f64::INFINITY)), fill(&fx.dl("two", f64::INFINITY)), fill(&fx.dl("two", 210.0)));
    assert_eq!(start.1, fx.theme.color("accent").unwrap());
    assert_eq!(end.1, fx.theme.color("ink").unwrap());
    assert!(mid.1 != start.1 && mid.1 != end.1, "the fill mixes: {:?}", mid.1);
    // Each point is on its way, none where it started or ends.
    let xy = |el: &scaena_core::displaylist::PathEl| match el {
        scaena_core::displaylist::PathEl::MoveTo(p) | scaena_core::displaylist::PathEl::LineTo(p) => *p,
        other => panic!("{other:?}"),
    };
    for k in 0..3 {
        let (a, b, m) = (xy(&start.0[k]), xy(&end.0[k]), xy(&mid.0[k]));
        for d in 0..2 {
            let (lo, hi) = (a[d].min(b[d]), a[d].max(b[d]));
            assert!(lo == hi || (m[d] > lo && m[d] < hi), "point {k}: {m:?} between {a:?} and {b:?}");
        }
    }
}

#[test]
fn a_shader_morphs_its_palette_and_params_and_cross_fades_a_rate() {
    let g = json!({ "type": "shader", "kind": "gradient", "seed": 1, "palette": "torture", "params": { "angle": 0 },
                    "at": { "col": [1, 4], "row": [2, 4] } });
    let mut fx = one_node(g.clone(), json!({}), json!({ "params": { "angle": 90, "grain": 0.1 } }));
    let shader = |dl: &DisplayList| match &layers(dl, "n")[..] {
        [(1.0, ops)] => match &ops[..] {
            [Op::Shader { params, .. }] => params.clone(),
            other => panic!("{other:?}"),
        },
        other => panic!("one layer, opaque: {other:?}"),
    };
    let mid = shader(&fx.dl("two", 210.0));
    // `grain`, left out in the first state, morphs from its default, 0.
    assert!(mid["angle"] > 0.0 && mid["angle"] < 90.0 && mid["grain"] > 0.0 && mid["grain"] < 0.1, "{mid:?}");
    // A rate cross-fades: its phase on the global clock would race between the two.
    let mut fx = one_node(g, json!({}), json!({ "params": { "angle": 0, "speed": 30 } }));
    let mid = layers(&fx.dl("two", 210.0), "n");
    assert_eq!(mid.len(), 2, "both shaders, cross-fading: {mid:?}");
    assert!((mid[0].0 + mid[1].0 - 1.0).abs() < 1e-6 && mid[0].0 < 1.0, "{} + {}", mid[0].0, mid[1].0);
}

#[test]
fn punctuation_is_a_word_of_its_own() {
    let mut fx = two_states(json!({ "text": "Revenue grew." }), json!({ "text": "Revenue grew fast." }));
    let mid = words(&fx.dl("two", 210.0), "t");
    // `Revenue`, `grew`, and the period move or stay at full opacity; `fast` fades in.
    assert_eq!(mid.len(), 4, "{mid:?}");
    assert_eq!(mid.iter().filter(|w| w.1 == 1.0).count(), 3, "{mid:?}");
    let period = mid.iter().filter(|w| w.1 == 1.0).map(|w| w.0[4]).fold(0.0_f32, |a, dx| a.min(dx));
    assert!(period < -1.0, "the period is still on its way right: {period}");
}

#[test]
fn code_moves_its_card_and_fades_its_numbers_in() {
    // PLAN 2.136: between two states, code's card moves and resizes as a shape does, and line
    // numbers that only one side sets fade where that side sets them.
    let code = |numbers: bool| json!({ "language": "rust", "numbers": numbers });
    let nodes = json!({ "c": {
        "type": "text", "role": "body", "code": code(false), "text": "let x = 1;", "at": { "rect": [100, 100, 800, 400] }
    } });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "c": {} } },
        { "id": "two", "transition": "standard", "props": { "c": { "at": { "rect": [500, 300, 800, 400] }, "code": code(true) } } }
    ]);
    let mut fx = Fx::new(nodes, states);
    // The card's left edge on the canvas, and the opacity of each layer of glyphs under it.
    let parts = |dl: &DisplayList| {
        let Some(Op::Layer { transform, ops, .. }) =
            dl.ops.iter().find(|op| matches!(op, Op::Layer { node: Some(n), .. } if n == "c"))
        else {
            panic!("no layer for the code")
        };
        let mut card = None;
        let mut numbers = Vec::new();
        for op in ops {
            let Op::Layer { transform: t, opacity, ops, .. } = op else { continue };
            if ops.iter().any(|o| matches!(o, Op::Fill { .. })) && card.is_none() {
                card = Some(transform[4] + t[4]);
            } else if ops.iter().any(|o| matches!(o, Op::Glyphs { .. })) {
                numbers.push(*opacity);
            }
        }
        (card.expect("a card"), numbers)
    };
    let (before, _) = parts(&fx.dl("one", f64::INFINITY));
    let (after, numbers) = parts(&fx.dl("two", f64::INFINITY));
    assert_eq!((before, after), (100.0, 500.0));
    assert_eq!(numbers, [1.0], "at rest, the numbers in full");
    let (mid, numbers) = parts(&fx.dl("two", 150.0));
    assert!(before < mid && mid < after, "the card moves: {mid}");
    assert!(numbers.len() == 1 && numbers[0] > 0.0 && numbers[0] < 1.0, "the numbers fade in: {numbers:?}");
}

#[test]
fn a_change_of_focus_alone_glides_every_word() {
    // PLAN 2.140: a walkthrough's step changes only color, so every word pairs with its twin and
    // mixes its color in place, its numbers likewise: nothing fades, nothing scales.
    let code = "fn main() {\n    let total = 0;\n    run(total);\n}";
    let nodes = json!({ "c": {
        "type": "text", "role": "body", "code": { "language": "rust", "numbers": true }, "text": code,
        "at": { "rect": [100, 100, 1600, 700] }
    } });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "c": {} } },
        { "id": "two", "transition": "standard", "props": { "c": { "code": { "focus": [2] } } } }
    ]);
    let mut fx = Fx::new(nodes, states);
    let mid = words(&fx.dl("two", 210.0), "c");
    assert!(mid.len() > 10, "{mid:?}");
    // The torture theme bands the lines in focus: of the layers with no glyphs, the card stays
    // and the band fades in behind line 2, and every word glides.
    let (frame, mid): (Vec<WordLayer>, Vec<WordLayer>) = mid.into_iter().partition(|w| w.2.is_empty());
    let fading: Vec<f32> = frame.iter().map(|w| w.1).filter(|o| *o < 1.0).collect();
    assert!(fading.len() == 1 && fading[0] > 0.0, "the band fades in, the card stays: {frame:?}");
    for (transform, opacity, glyphs, _) in &mid {
        assert_eq!(*opacity, 1.0, "nothing fades: {glyphs:?}");
        assert_eq!((transform[0], transform[3]), (1.0, 1.0), "nothing scales: {glyphs:?}");
    }
    // The lines out of focus are on their way to their set-back colors: some paint is neither
    // side's.
    let (one, two) = (words(&fx.dl("one", f64::INFINITY), "c"), words(&fx.dl("two", f64::INFINITY), "c"));
    let paints = |w: &[WordLayer]| -> Vec<Paint> { w.iter().flat_map(|l| l.3.clone()).collect() };
    let (a, m, b) = (paints(&one), paints(&mid), paints(&two));
    assert!(m.iter().any(|p| !a.contains(p) && !b.contains(p)), "colors mid-glide");
}

#[test]
fn a_word_whose_runs_part_in_one_state_still_moves() {
    // PLAN 2.140: "Revenue" one run in the first state and two in the second, its last letters
    // colored, is the same drawing: it glides, its colors mixing, glyph by glyph across the runs
    // either side cuts it into, rather than fading one drawing into the other.
    let mut fx = two_states(
        json!({ "runs": [{ "text": "Revenue grew" }] }),
        json!({ "runs": [{ "text": "Rev" }, { "text": "enue", "style": { "color": "accent" } }, { "text": " grew" }] }),
    );
    let mid = words(&fx.dl("two", 210.0), "t");
    assert_eq!(mid.len(), 2, "two words, each one drawing: {mid:?}");
    assert!(mid.iter().all(|w| w.1 == 1.0), "{mid:?}");
    // The colored letters are their own runs mid-glide, the rest as they were.
    assert!(mid[0].3.len() >= 2, "{:?}", mid[0].3);
}

/// Code's layers while it morphs: each word's opacity, its vertical offset from where it ends, and
/// how many glyphs it draws; the card and the bands, which draw no glyphs, left out.
fn code_words(dl: &DisplayList) -> Vec<(f32, f32, usize)> {
    words(dl, "c").into_iter().filter(|w| !w.2.is_empty()).map(|w| (w.1, w.0[5], w.2.len())).collect()
}

/// Two states of one code text: `a`, then `b`, each in Rust, `numbers` set or not.
fn code_states(a: &str, b: &str, numbers: bool) -> Fx {
    let nodes = json!({ "c": {
        "type": "text", "role": "body", "code": { "language": "rust", "numbers": numbers }, "text": a,
        "at": { "rect": [100, 100, 1600, 700] }
    } });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "c": {} } },
        { "id": "two", "transition": "standard", "props": { "c": { "text": b } } }
    ]);
    Fx::new(nodes, states)
}

#[test]
fn code_moves_token_by_token_and_an_edited_token_alone_fades() {
    // PLAN 2.141, ADR-0029 item 7: `cart.Items` is one word to UAX #14 and three tokens to code.
    // With `cart` edited to `order`, `.` and `Items` move, and `cart` alone fades out.
    let mut fx = code_states("for item in cart.Items {", "for item in order.Items {", false);
    let early = code_words(&fx.dl("two", 40.0));
    let fading: usize = early.iter().filter(|w| w.0 < 1.0).map(|w| w.2).sum();
    assert_eq!(fading, 4, "`cart` alone fades out: {early:?}");
    let late = code_words(&fx.dl("two", 360.0));
    let coming: usize = late.iter().filter(|w| w.0 < 1.0).map(|w| w.2).sum();
    assert_eq!(coming, 5, "`order` alone fades in: {late:?}");
    let staying: usize = late.iter().filter(|w| w.0 == 1.0).map(|w| w.2).sum();
    assert_eq!(staying, "for item in .Items {".chars().filter(|c| !c.is_whitespace()).count(), "{late:?}");
}

#[test]
fn a_line_added_above_moves_the_rest_down_and_numbers_pair_by_number() {
    // PLAN 2.141: a line written above the rest moves every line under it down a line, nothing
    // fading but what came, and the numbers pair by number: 1 and 2 stay, and 3 fades in.
    let mut fx = code_states("alpha\nbeta", "zero\nalpha\nbeta", true);
    let mid = code_words(&fx.dl("two", 300.0));
    let opaque: Vec<&(f32, f32, usize)> = mid.iter().filter(|w| w.0 == 1.0).collect();
    let fading: Vec<&(f32, f32, usize)> = mid.iter().filter(|w| w.0 > 0.0 && w.0 < 1.0).collect();
    assert_eq!(opaque.len(), 4, "alpha, beta, 1, and 2 in full: {mid:?}");
    assert_eq!(fading.len(), 2, "zero and 3 fading in: {mid:?}");
    let moving = opaque.iter().filter(|w| w.1 < -1.0).count();
    let still = opaque.iter().filter(|w| w.1.abs() < 1e-3).count();
    assert_eq!((moving, still), (2, 2), "alpha and beta move down, 1 and 2 stay: {mid:?}");
}

#[test]
fn code_that_shrinks_keeps_one_size_across_its_states() {
    // PLAN 2.141, ADR-0029 item 7: a step that adds lines does not rescale every token. Code set
    // by `shrink` takes, in each state, the size at which every text the deck gives it fits.
    let short = "let a = 1;\nlet b = 2;";
    let long = (0..18).map(|i| format!("let v{i} = {i};")).collect::<Vec<_>>().join("\n");
    let nodes = json!({ "c": {
        "type": "text", "role": "body", "code": { "language": "rust" }, "text": short,
        "at": { "rect": [100, 100, 1200, 500] }
    } });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "c": {} } },
        { "id": "two", "transition": "standard", "props": { "c": { "text": long } } }
    ]);
    let mut fx = Fx::new(nodes, states);
    let scale = |fx: &mut Fx, state: &str| {
        let req = FrameRequest { deck: &fx.deck, theme: &fx.theme, data: &fx.data, state, t_ms: 0.0, format: None };
        fx.engine.text_layouts(&req, &["c"]).unwrap()[0].scale
    };
    let (one, two) = (scale(&mut fx, "one"), scale(&mut fx, "two"));
    assert!(two < 1.0, "the long text shrinks: {two}");
    assert_eq!(one, two, "both states at one size");
}

#[test]
fn a_line_inserted_above_its_twin_never_takes_a_token_of_it() {
    // PLAN 2.141: code pairs its lines first, as a diff does, and tokens only inside the lines
    // edited, added, or taken away. `.filter` written above `.sort` never takes `.sort`'s dot,
    // though the two dots tie: every line both sides write moves whole, one offset for each of its
    // glyphs, and only `.filter`'s line fades in.
    let nodes = json!({ "c": {
        "type": "text", "role": "body", "code": { "language": "typescript" },
        "text": "return [...talks]\n\t.sort(byVotes)\n\t.map(title);",
        "at": { "rect": [100, 100, 1600, 700] }
    } });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "c": {} } },
        { "id": "two", "transition": "standard", "props": { "c": {
            "text": "return [...talks]\n\t.filter(seated)\n\t.sort(byVotes)\n\t.map(title);"
        } } }
    ]);
    let mut fx = Fx::new(nodes, states);
    let mid = code_words(&fx.dl("two", 300.0));
    let fading: Vec<&(f32, f32, usize)> = mid.iter().filter(|w| w.0 > 0.0 && w.0 < 1.0).collect();
    // `return [...talks]` stays; `.sort(byVotes)` and `.map(title);` move down a line, together.
    let opaque: Vec<&(f32, f32, usize)> = mid.iter().filter(|w| w.0 == 1.0).collect();
    let still: usize = opaque.iter().filter(|w| w.1.abs() < 1e-3).map(|w| w.2).sum();
    let moved: Vec<f32> = opaque.iter().map(|w| w.1).filter(|dy| dy.abs() >= 1e-3).collect();
    assert_eq!(still, "return[...talks]".len(), "only the first line stays: {mid:?}");
    assert!(moved.iter().all(|dy| (dy - moved[0]).abs() < 1e-3 && *dy < -1.0), "one offset: {moved:?}");
    // `.`, `filter`, `(`, `seated`, and `)` fade in where they stand.
    assert_eq!(fading.len(), 5, "`.filter`'s line alone fades in: {mid:?}");
    assert!(fading.iter().all(|w| w.1.abs() < 1e-3), "{fading:?}");
}
