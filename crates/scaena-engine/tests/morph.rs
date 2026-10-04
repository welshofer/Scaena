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
