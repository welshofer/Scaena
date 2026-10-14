//! Motion on the timeline (SPEC §2.4, §3.9; PLAN 1.11): when cues run (`with`, `after`,
//! delays, staggers, sequences), what they do to whole nodes and to the units a cue
//! splits them into, spring transitions, and the deck's states end to end.

use scaena_core::Deck;
use scaena_core::displaylist::{DisplayList, Op};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, Frame, FrameRequest, Timed};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The torture theme, edited.
fn theme(edit: impl FnOnce(&mut Value)) -> Theme {
    let mut t: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    edit(&mut t);
    Theme::from_json(&t.to_string()).unwrap()
}

/// The torture deck's fonts and canvas, with these nodes and states.
fn deck(nodes: Value, states: Value) -> Deck {
    let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    d["nodes"] = nodes;
    d["states"] = states;
    serde_json::from_value(d).unwrap()
}

struct Fx {
    deck: Deck,
    theme: Theme,
    data: DataFiles,
    engine: Engine,
}

impl Fx {
    fn new(deck: Deck, theme: Theme) -> Fx {
        let mut fonts = BundleFonts::new();
        for font in &deck.fonts {
            fonts.register(&font.file, read(&font.file)).unwrap();
        }
        Fx { deck, theme, data: DataFiles::new(), engine: Engine::new(fonts) }
    }

    fn frame(&mut self, state: &str, t_ms: f64) -> Frame {
        let req = FrameRequest { deck: &self.deck, theme: &self.theme, data: &self.data, state, t_ms, format: None };
        self.engine.frame(&req).unwrap_or_else(|e| panic!("{state} at {t_ms}: {e}"))
    }

    fn dl(&mut self, state: &str, t_ms: f64) -> DisplayList {
        self.frame(state, t_ms).display_list
    }
}

/// The layer `node` draws, if it draws one.
fn layer<'a>(dl: &'a DisplayList, node: &str) -> Option<&'a Op> {
    dl.ops.iter().find(|op| matches!(op, Op::Layer { node: Some(n), .. } if n == node))
}

/// `node`'s layer's transform and opacity.
fn seen(dl: &DisplayList, node: &str) -> Option<([f32; 6], f32)> {
    match layer(dl, node)? {
        Op::Layer { transform, opacity, .. } => Some((*transform, *opacity)),
        _ => None,
    }
}

/// How many of `node`'s units it draws: the layers inside its layer.
fn units(dl: &DisplayList, node: &str) -> usize {
    match layer(dl, node) {
        Some(Op::Layer { ops, .. }) => ops.iter().filter(|op| matches!(op, Op::Layer { .. })).count(),
        _ => 0,
    }
}

fn text(s: &str, slot: &str) -> Value {
    json!({ "type": "text", "role": "body", "text": s, "at": { "in": slot } })
}

#[test]
fn a_split_cue_brings_words_in_one_by_one() {
    let nodes = json!({ "t": text("One two three", "main") });
    let states = json!([{
        "id": "s", "layout": "specimen", "props": { "t": {} },
        "choreography": [{ "target": "t", "split": "words", "enter": "rise", "stagger": 100, "timing": "with" }]
    }]);
    let mut fx = Fx::new(deck(nodes, states), theme(|_| {}));
    // `rise` takes `standard` (420 ms); three words 100 ms apart: the last rests at 620.
    assert_eq!(fx.frame("s", f64::INFINITY).duration_ms, 620.0);
    let counts: Vec<usize> = [50.0, 150.0, 250.0].map(|t| units(&fx.dl("s", t), "t")).to_vec();
    assert_eq!(counts, [1, 2, 3], "each word starts 100 ms after the one before");
    // A word rises: 24 cu below where it rests, as it starts, and on its way up.
    let Some(Op::Layer { ops, .. }) = layer(&fx.dl("s", 150.0), "t").cloned() else { panic!("no text layer") };
    let lifts: Vec<f32> = ops
        .iter()
        .filter_map(|op| match op {
            Op::Layer { transform, opacity, .. } => Some((transform[5], *opacity)),
            _ => None,
        })
        .map(|(dy, opacity)| {
            assert!((0.0..1.0).contains(&opacity), "fading in: {opacity}");
            dy
        })
        .collect();
    assert!(lifts[0] > 0.0 && lifts[0] < lifts[1] && lifts[1] <= 24.0, "the later word is lower: {lifts:?}");
    // At its span the state is at rest, and nothing is split any more.
    let rest = fx.dl("s", f64::INFINITY);
    assert_eq!(fx.dl("s", 620.0), rest);
    assert_eq!(units(&rest, "t"), 0);
}

#[test]
fn after_waits_for_the_transition_and_with_runs_beside_it() {
    let nodes = json!({ "a": text("Alpha", "case"), "b": text("Beta", "main"), "c": text("Gamma", "main") });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "a": {} } },
        { "id": "two", "transition": "standard", "props": { "b": {} },
          "choreography": [{ "target": "b", "enter": "fade" }] },
        { "id": "three", "transition": "standard", "remove": ["b"], "props": { "c": {} },
          "choreography": [{ "target": "c", "enter": "fade", "timing": "with", "delay": 100 }] }
    ]);
    let mut fx = Fx::new(deck(nodes, states), theme(|_| {}));
    // `after`, the default: once the 420 ms transition ends, for `fade`'s 420.
    assert_eq!(fx.frame("two", f64::INFINITY).duration_ms, 840.0);
    assert!(layer(&fx.dl("two", 300.0), "b").is_none(), "not drawn before its entrance");
    let (_, opacity) = seen(&fx.dl("two", 630.0), "b").unwrap();
    assert!(opacity > 0.5 && opacity < 1.0, "halfway, eased out: {opacity}");
    // `with`, after a delay: inside the transition.
    assert_eq!(fx.frame("three", f64::INFINITY).duration_ms, 520.0);
    assert!(layer(&fx.dl("three", 50.0), "c").is_none());
    assert!(seen(&fx.dl("three", 300.0), "c").is_some_and(|(_, o)| o > 0.0 && o < 1.0));
}

#[test]
fn an_exit_cue_keeps_its_node_on_screen_until_it_leaves() {
    let nodes = json!({ "a": text("Alpha", "case"), "b": text("Beta", "main") });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "a": {}, "b": {} } },
        { "id": "two", "transition": "standard", "remove": ["a"],
          "choreography": [{ "target": "a", "exit": "fade", "delay": 100 }] }
    ]);
    let mut fx = Fx::new(deck(nodes, states), theme(|_| {}));
    // It leaves 100 ms after the transition, over 420 ms.
    assert_eq!(fx.frame("two", f64::INFINITY).duration_ms, 940.0);
    assert_eq!(seen(&fx.dl("two", 300.0), "a").map(|(_, o)| o), Some(1.0), "at rest until its exit");
    let (_, opacity) = seen(&fx.dl("two", 730.0), "a").unwrap();
    assert!(opacity > 0.0 && opacity < 0.5, "fading out, eased: {opacity}");
    assert!(layer(&fx.dl("two", 940.0), "a").is_none(), "gone once it has left");
    // A node that leaves with no cue fades out with the transition, as before.
    let nodes = json!({ "a": text("Alpha", "case") });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "a": {} } },
        { "id": "two", "transition": "standard", "remove": ["a"] }
    ]);
    let mut fx = Fx::new(deck(nodes, states), theme(|_| {}));
    assert!(seen(&fx.dl("two", 210.0), "a").is_some_and(|(_, o)| o > 0.0 && o < 1.0));
    assert!(layer(&fx.dl("two", 420.0), "a").is_none());
}

#[test]
fn a_spring_transition_lasts_its_settle_time_and_overshoots() {
    let nodes = json!({ "a": text("Alpha", "case") });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "a": {} } },
        { "id": "two", "transition": { "spring": "snappy" }, "props": { "a": { "at": { "in": "main" } } } }
    ]);
    let mut fx = Fx::new(deck(nodes, states), theme(|_| {}));
    let span = fx.frame("two", f64::INFINITY).duration_ms;
    assert!(span > 200.0 && span < 800.0, "snappy settles in {span} ms");
    let y = |fx: &mut Fx, t: f64| seen(&fx.dl("two", t), "a").unwrap().0[5];
    let (from, to) = (y(&mut fx, 0.0), y(&mut fx, f64::INFINITY));
    let path: Vec<f32> = (1..40).map(|k| y(&mut fx, span * k as f64 / 40.0)).collect();
    let beyond = if to > from {
        path.iter().copied().fold(f32::MIN, f32::max) - to
    } else {
        to - path.iter().copied().fold(f32::MAX, f32::min)
    };
    assert!(beyond > 0.0, "it passes its place and comes back: {beyond} cu, from {from} to {to}");
}

#[test]
fn an_emphasis_goes_out_and_back_once_and_does_not_track() {
    let nodes = json!({ "a": text("Alpha", "main") });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "a": {} } },
        { "id": "two", "transition": "fast", "props": { "a": { "emphasis": "pulse" } } },
        { "id": "three", "transition": "fast" }
    ]);
    let pulse = |t: &mut Value| {
        t["motion"]["presets"]["pulse"] =
            json!({ "to": { "transform": { "scale": 1.2 } }, "duration": 400, "ease": "linear" })
    };
    let mut fx = Fx::new(deck(nodes, states), theme(pulse));
    // `after` the 180 ms transition, out for 200 ms and back for 200.
    assert_eq!(fx.frame("two", f64::INFINITY).duration_ms, 580.0);
    let scale = |fx: &mut Fx, t: f64| seen(&fx.dl("two", t), "a").unwrap().0[0];
    assert!((scale(&mut fx, 380.0) - 1.2).abs() < 1e-4, "at its peak halfway");
    assert!(scale(&mut fx, 280.0) > 1.0 && scale(&mut fx, 280.0) < 1.2);
    assert_eq!(scale(&mut fx, 580.0), 1.0);
    // The next state does not pulse again.
    assert_eq!(fx.frame("three", f64::INFINITY).duration_ms, 180.0);
}

#[test]
fn children_enter_in_flow_order() {
    let child = |s: &str, index: u64| json!({ "type": "text", "role": "body", "text": s, "at": { "parent": "row", "index": index } });
    let nodes = json!({
        "row": { "type": "stack", "axis": "x", "gap": "space.5", "at": { "in": "main" } },
        "x": child("X", 2), "y": child("Y", 0), "z": child("Z", 1)
    });
    let states = json!([{
        "id": "s", "layout": "specimen", "props": { "row": {}, "x": {}, "y": {}, "z": {} },
        "choreography": [{ "target": "row", "split": "children", "enter": "fade", "stagger": 100, "timing": "with" }]
    }]);
    let mut fx = Fx::new(deck(nodes, states), theme(|_| {}));
    let drawn = |fx: &mut Fx, t: f64| {
        let dl = fx.dl("s", t);
        ["x", "y", "z"].into_iter().filter(|n| layer(&dl, n).is_some()).collect::<Vec<_>>()
    };
    assert_eq!(drawn(&mut fx, 50.0), ["y"], "index 0 first");
    assert_eq!(drawn(&mut fx, 150.0), ["y", "z"]);
    assert_eq!(drawn(&mut fx, 250.0), ["x", "y", "z"]);
    // Children are counted from the document: the timeline lays nothing out for them,
    // and agrees with the cue laid out.
    let timeline = fx.engine.timeline(&fx.deck, &fx.theme, &fx.data).unwrap();
    let cue = fx.engine.transition(&fx.deck, &fx.theme, &fx.data, "s").unwrap();
    assert_eq!((timeline.slots[0].span, cue.span_ms()), (620.0, 620.0));
}

#[test]
fn an_entrance_moves_only_what_enters() {
    let child = |s: &str, index: u64| json!({ "type": "text", "role": "body", "text": s, "at": { "parent": "list", "index": index } });
    let nodes = json!({
        "list": { "type": "stack", "axis": "y", "gap": "space.3", "at": { "in": "main" } },
        "one": child("One", 0), "two": child("Two", 1), "title": text("Title", "case")
    });
    let states = json!([
        { "id": "a", "layout": "specimen", "props": { "list": {}, "one": {}, "title": {} } },
        { "id": "b", "transition": "standard", "props": { "two": {} },
          "choreography": [
            { "target": "list", "split": "children", "enter": "fade", "stagger": 100, "timing": "with" },
            { "target": "title", "enter": "fade", "timing": "with" }
          ] }
    ]);
    let mut fx = Fx::new(deck(nodes, states), theme(|_| {}));
    // The list gains its second child, which enters as the second unit; the first, and
    // the title, stay where they were and as they were.
    let dl = fx.dl("b", 50.0);
    assert_eq!(seen(&dl, "one").map(|(_, o)| o), Some(1.0));
    assert_eq!(seen(&dl, "title").map(|(_, o)| o), Some(1.0), "an entrance on a node that stays does nothing");
    assert!(layer(&dl, "two").is_none(), "the new child starts 100 ms in");
    assert!(seen(&fx.dl("b", 300.0), "two").is_some_and(|(_, o)| o > 0.0 && o < 1.0));
}

#[test]
fn a_sequence_runs_its_items_one_after_another() {
    let nodes = json!({ "a": text("Alpha", "case"), "b": text("Beta", "main") });
    let states = json!([{
        "id": "s", "layout": "specimen", "props": { "a": {}, "b": {} },
        "choreography": [{ "sequence": [
            { "target": "a", "enter": "fade", "duration": 200 },
            { "target": "b", "enter": "fade", "duration": 200, "delay": 50 }
        ], "timing": "with" }]
    }]);
    let mut fx = Fx::new(deck(nodes, states), theme(|_| {}));
    assert_eq!(fx.frame("s", f64::INFINITY).duration_ms, 450.0);
    let dl = fx.dl("s", 220.0);
    assert!(layer(&dl, "a").is_some() && layer(&dl, "b").is_none(), "b waits for a, then 50 ms more");
    assert!(layer(&fx.dl("s", 300.0), "b").is_some());
}

/// A node's `transform` (PLAN 2.51) is drawn about its box, and a cue's look moves the node
/// inside it, in its own frame: a node turned a quarter clockwise rises in from its left, and
/// rests turned.
#[test]
fn a_cue_moves_a_turned_node_in_its_own_frame() {
    let mut a = text("Alpha", "main");
    a["transform"] = json!({ "rotate": 90 });
    let states = json!([{
        "id": "s", "layout": "specimen", "props": { "a": {} },
        "choreography": [{ "target": "a", "enter": "rise", "timing": "with" }]
    }]);
    let mut fx = Fx::new(deck(json!({ "a": a }), states), theme(|_| {}));
    let (rest, _) = seen(&fx.dl("s", f64::INFINITY), "a").unwrap();
    assert!(rest[0].abs() < 1e-6 && (rest[1] - 1.0).abs() < 1e-6, "its x runs down the canvas: {rest:?}");
    // From 24 cu below where it rests, in its own frame: 24 cu to the left, on the canvas.
    let (start, _) = seen(&fx.dl("s", 1.0), "a").unwrap();
    let (dx, dy) = (start[4] - rest[4], start[5] - rest[5]);
    assert!((-24.0..-20.0).contains(&dx) && dy.abs() < 1e-3, "rises from ({dx}, {dy})");
    assert_eq!(start[..4], rest[..4], "it rises turned");
}

#[test]
fn anim_tracks_move_a_node_by_keyframes() {
    let nodes = json!({ "a": text("Alpha", "main") });
    let states = json!([{
        "id": "s", "layout": "specimen",
        "props": { "a": { "anim": { "translate": [
            { "t": 0, "v": [0, 40] }, { "t": 200, "v": [0, 0], "ease": "linear" }
        ] } } }
    }]);
    let mut fx = Fx::new(deck(nodes, states), theme(|_| {}));
    assert_eq!(fx.frame("s", f64::INFINITY).duration_ms, 200.0);
    let rest = seen(&fx.dl("s", f64::INFINITY), "a").unwrap().0[5];
    let half = seen(&fx.dl("s", 100.0), "a").unwrap().0[5];
    assert!((half - rest - 20.0).abs() < 1e-3, "halfway down from 40 cu: {}", half - rest);
}

#[test]
fn states_lie_end_to_end_with_their_holds_and_shaders_keep_that_time() {
    let nodes = json!({
        "bg": { "type": "shader", "kind": "mesh", "seed": 3, "palette": "ambient", "at": { "in": "main" } },
        "a": text("Alpha", "case")
    });
    let states = json!([
        { "id": "one", "layout": "specimen", "hold": 1000, "props": { "bg": {} } },
        { "id": "two", "transition": "standard", "hold": 500, "props": { "a": {} },
          "choreography": [{ "target": "a", "enter": "fade" }] },
        { "id": "three", "transition": "slow" }
    ]);
    let ambient = |t: &mut Value| {
        t["shaders"]["palettes"]["ambient"] = json!(["#1B1430", "#3A1F4F", "#B0452C", "#FF6A3D"]);
    };
    let mut fx = Fx::new(deck(nodes, states), theme(ambient));
    let timeline = fx.engine.timeline(&fx.deck, &fx.theme, &fx.data).unwrap();
    let starts: Vec<(String, f64, f64)> = timeline.slots.iter().map(|s| (s.state.clone(), s.start, s.span)).collect();
    assert_eq!(starts, [("one".into(), 0.0, 0.0), ("two".into(), 1000.0, 840.0), ("three".into(), 2340.0, 800.0)]);
    assert_eq!(timeline.duration(), 3140.0);
    let (slot, t) = timeline.locate(2000.0).unwrap();
    assert_eq!((slot.state.as_str(), t), ("two", 1000.0), "two holds at rest");
    // The shader's clock is the global timeline's: on through the cue, at rest the
    // moment the state comes to rest, and on through its hold.
    let clock = |dl: &DisplayList| {
        let Some(Op::Layer { ops, .. }) = layer(dl, "bg") else { panic!("no bg") };
        ops.iter().find_map(|op| if let Op::Shader { t, .. } = op { Some(*t) } else { None }).unwrap()
    };
    assert_eq!(clock(&fx.dl("two", 100.0)), 1.1);
    assert_eq!(clock(&fx.dl("two", f64::INFINITY)), 1.84);
    assert_eq!(clock(&fx.dl("two", 1200.0)), 2.2);
    assert_eq!(clock(&fx.dl("one", f64::INFINITY)), 0.0);
}

/// Every paint inside `op`, in order.
fn paints(op: &Op) -> Vec<scaena_core::displaylist::Paint> {
    match op {
        Op::Layer { ops, .. } => ops.iter().flat_map(paints).collect(),
        Op::Fill { paint, .. } | Op::Stroke { paint, .. } | Op::Glyphs { paint, .. } => vec![paint.clone()],
        _ => vec![],
    }
}

#[test]
fn a_color_look_mixes_every_paint_toward_it_and_back() {
    let nodes = json!({ "a": text("Alpha", "main") });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "a": {} } },
        { "id": "two", "transition": "fast", "props": { "a": { "emphasis": "flash" } } }
    ]);
    let flash = |t: &mut Value| {
        t["motion"]["presets"]["flash"] = json!({ "to": { "color": "accent" }, "duration": 400, "ease": "linear" })
    };
    let mut fx = Fx::new(deck(nodes, states), theme(flash));
    let solid = |fx: &mut Fx, t: f64| match paints(layer(&fx.dl("two", t), "a").unwrap())[..] {
        [scaena_core::displaylist::Paint::Solid(c)] => c,
        ref other => panic!("{other:?}"),
    };
    let (ink, accent) = (solid(&mut fx, 0.0), fx.theme.color("accent").unwrap());
    // `after` the 180 ms transition: out to the accent for 200 ms, and back.
    assert_eq!(solid(&mut fx, 380.0), accent, "at its peak, the accent");
    let between = solid(&mut fx, 280.0);
    assert!(between != ink && between != accent, "{between:?}");
    assert_eq!(solid(&mut fx, f64::INFINITY), ink);
}

/// An arrow's shaft end and its head's tip, along x.
fn arrow(dl: &DisplayList, node: &str) -> (f32, f32) {
    use scaena_core::displaylist::PathEl;
    let Some(Op::Layer { ops, .. }) = layer(dl, node) else { panic!("no layer for `{node}`") };
    match &ops[..] {
        [Op::Stroke { path, .. }, Op::Fill { path: head, .. }] => match (path.0.last(), head.0.first()) {
            (Some(PathEl::LineTo(end)), Some(PathEl::MoveTo(tip))) => (end[0], tip[0]),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

fn drawn_arrow(node: Value) -> Fx {
    let mut nodes = json!({ "x": { "type": "shape", "kind": "arrow", "points": [[0, 0.5], [1, 0.5]],
                                   "stroke": { "paint": "accent", "width": 4 }, "at": { "col": [1, 4], "row": 2 } } });
    nodes["x"].as_object_mut().unwrap().extend(node.as_object().unwrap().clone());
    let states = json!([
        { "id": "one", "layout": "specimen" },
        { "id": "two", "transition": "fast", "props": { "x": {} } }
    ]);
    let draw = |t: &mut Value| {
        t["motion"]["presets"]["draw"] =
            json!({ "from": { "params": { "progress": 0 } }, "duration": 400, "ease": "linear" })
    };
    Fx::new(deck(nodes, states), theme(draw))
}

#[test]
fn a_draw_look_strokes_a_shape_on_with_its_arrow_head_riding_the_tip() {
    let mut fx = drawn_arrow(json!({ "enter": "draw" }));
    let (_, full) = arrow(&fx.dl("two", f64::INFINITY), "x");
    let (end, tip) = arrow(&fx.dl("two", 200.0), "x");
    assert!((tip - full / 2.0).abs() < 1e-2, "halfway: the tip at {tip} of {full}");
    assert!((tip - end - 16.0).abs() < 1e-3, "the shaft stops at the head, four stroke widths back: {end}");
    // Early on, the head grows in over its own length.
    let (end, tip) = arrow(&fx.dl("two", 4.0), "x");
    assert!(tip - end > 0.0 && tip - end < 16.0, "{end} {tip}");
    // A call's params set the look's: from halfway.
    let mut fx = drawn_arrow(json!({ "enter": { "preset": "draw", "params": { "progress": 0.5 } } }));
    let (_, tip) = arrow(&fx.dl("two", 0.001), "x");
    assert!((tip - full / 2.0).abs() < 0.1, "{tip}");
    // So does an `anim` track.
    let anim = json!({ "anim": { "progress": [{ "t": 0, "v": 0 }, { "t": 200, "v": 1, "ease": "linear" }] } });
    let mut fx = drawn_arrow(anim);
    let (_, tip) = arrow(&fx.dl("two", 100.0), "x");
    assert!((tip - full / 2.0).abs() < 1e-2, "{tip}");
}

#[test]
fn a_look_names_what_it_takes() {
    let mut t: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    t["motion"]["presets"]["blurry"] = json!({ "from": { "blur": 8 } });
    let err = Theme::from_json(&t.to_string()).unwrap_err().to_string();
    assert!(err.contains("`blur`") && err.contains("`opacity`, `transform`, `color`"), "{err}");
}

/// A group at half opacity holding two overlapping squares, the first outlined.
fn squares(group: Value, states: Value) -> Fx {
    let mut g = json!({ "type": "group", "opacity": 0.5 });
    g.as_object_mut().unwrap().extend(group.as_object().unwrap().clone());
    let nodes = json!({
        "g": g,
        "p": { "type": "shape", "kind": "rect", "fill": "accent", "stroke": { "paint": "ink", "width": 4 },
               "at": { "parent": "g", "rect": [100, 100, 200, 200] } },
        "q": { "type": "shape", "kind": "rect", "fill": "ink", "at": { "parent": "g", "rect": [200, 200, 200, 200] } }
    });
    let draw = |t: &mut Value| {
        t["motion"]["presets"]["draw"] =
            json!({ "from": { "params": { "progress": 0 } }, "duration": 400, "ease": "linear" })
    };
    Fx::new(deck(nodes, states), theme(draw))
}

/// `group`'s layer: its transform, its opacity, and its members' layers' nodes and opacities.
fn group_layer(dl: &DisplayList, group: &str) -> ([f32; 6], f32, Vec<(String, f32)>) {
    let Some(Op::Layer { transform, opacity, ops, .. }) = layer(dl, group) else { panic!("no layer for `{group}`") };
    let members = (ops.iter())
        .filter_map(|op| match op {
            Op::Layer { node: Some(n), opacity, .. } => Some((n.clone(), *opacity)),
            _ => None,
        })
        .collect();
    (*transform, *opacity, members)
}

const ALL: fn() -> Value = || json!({ "g": {}, "p": {}, "q": {} });

#[test]
fn a_group_composites_its_members_as_one_layer() {
    let mut fx = squares(json!({}), json!([{ "id": "s", "layout": "specimen", "props": ALL() }]));
    let (_, opacity, members) = group_layer(&fx.dl("s", f64::INFINITY), "g");
    assert_eq!(opacity, 0.5);
    assert_eq!(members, [("p".to_string(), 1.0), ("q".to_string(), 1.0)], "each member at its own opacity");
}

#[test]
fn a_group_comes_and_goes_as_one_layer() {
    let states = json!([
        { "id": "one", "layout": "specimen" },
        { "id": "two", "transition": "fast", "props": ALL() },
        { "id": "three", "transition": "fast", "remove": ["g", "p", "q"] }
    ]);
    let mut fx = squares(json!({}), states.clone());
    // With the transition: the group's layer fades, its members whole inside it.
    let (_, opacity, members) = group_layer(&fx.dl("two", 90.0), "g");
    assert!(opacity > 0.0 && opacity < 0.5, "{opacity}");
    assert_eq!(members.iter().map(|m| m.1).collect::<Vec<_>>(), [1.0, 1.0]);
    let (_, opacity, _) = group_layer(&fx.dl("three", 90.0), "g");
    assert!(opacity > 0.0 && opacity < 0.5, "and out: {opacity}");
    assert!(layer(&fx.dl("three", 180.0), "g").is_none(), "gone");
    // With its own entrance: the layer moves and fades by the preset, not the transition.
    let mut fx = squares(json!({ "enter": "rise" }), states);
    let (transform, opacity, members) = group_layer(&fx.dl("two", 90.0), "g");
    assert!(transform[5] > 0.0 && opacity > 0.0 && opacity < 0.5, "{transform:?} {opacity}");
    let still = |dl: &DisplayList| match layer(dl, "g") {
        Some(Op::Layer { ops, .. }) => {
            ops.iter().all(|op| matches!(op, Op::Layer { transform, .. } if transform[0] == 1.0))
        }
        _ => false,
    };
    assert!(still(&fx.dl("two", 90.0)) && members.len() == 2, "the members stay put inside it");
}

#[test]
fn a_cue_on_a_groups_children_moves_each_inside_the_groups_layer() {
    let states = json!([
        { "id": "one", "layout": "specimen" },
        { "id": "two", "transition": "fast", "props": ALL(), "choreography": [
            { "target": "g", "split": "children", "enter": "fade", "stagger": 300, "timing": "with" }
        ] }
    ]);
    let mut fx = squares(json!({}), states);
    let (_, opacity, members) = group_layer(&fx.dl("two", 200.0), "g");
    assert_eq!(opacity, 0.5, "the group comes in with its children's cue, not the transition");
    assert_eq!(members.len(), 1, "the second square has not started: {members:?}");
    assert!(members[0].1 > 0.0 && members[0].1 < 1.0, "{members:?}");
}

#[test]
fn a_draw_look_on_a_group_draws_its_members_outlines_on() {
    let states = json!([
        { "id": "one", "layout": "specimen" },
        { "id": "two", "transition": "fast", "props": ALL() }
    ]);
    let mut fx = squares(json!({ "enter": "draw" }), states);
    let outline = |dl: &DisplayList| {
        let Some(Op::Layer { ops, .. }) = layer(dl, "g") else { panic!() };
        let Some(Op::Layer { ops, .. }) = ops.first() else { panic!() };
        match &ops[..] {
            [Op::Fill { .. }, Op::Stroke { path, .. }] => path.0.len(),
            other => panic!("{other:?}"),
        }
    };
    // A quarter of the way, the outline has run along its first side only.
    assert_eq!(outline(&fx.dl("two", 100.0)), 2, "a move and one line");
    assert_eq!(outline(&fx.dl("two", f64::INFINITY)), 5, "the whole closed rect at rest");
}

#[test]
fn a_color_look_reaches_split_units() {
    let nodes = json!({ "a": text("Alpha beta", "main") });
    let states = json!([
        { "id": "one", "layout": "specimen" },
        { "id": "two", "transition": "fast", "props": { "a": {} }, "choreography": [
            { "target": "a", "split": "words", "enter": "warm", "stagger": 100, "timing": "with" }
        ] }
    ]);
    let warm = |t: &mut Value| {
        t["motion"]["presets"]["warm"] = json!({ "from": { "color": "accent" }, "duration": 400, "ease": "linear" })
    };
    let mut fx = Fx::new(deck(nodes, states), theme(warm));
    let accent = fx.theme.color("accent").unwrap();
    let first = |dl: &DisplayList| match layer(dl, "a") {
        Some(Op::Layer { ops, .. }) => paints(&ops[0]),
        _ => panic!(),
    };
    assert_eq!(
        first(&fx.dl("two", 0.001))[0],
        scaena_core::displaylist::Paint::Solid(accent),
        "it starts in the accent"
    );
    assert_ne!(first(&fx.dl("two", 200.0))[0], scaena_core::displaylist::Paint::Solid(accent));
}

#[test]
fn a_slide_transition_moves_each_slide_whole() {
    // Slide transitions (PLAN 2.111): the slide left, at rest, and the slide entered, each a layer
    // with its own ground, nothing matched across them; the style moves, uncovers, or fades them.
    let nodes = json!({ "a": text("Alpha", "case"), "b": text("Beta", "main") });
    let deck_with = |transition: Value| {
        let states = json!([
            { "id": "one", "layout": "specimen", "props": { "a": {}, "b": {} } },
            { "id": "two", "transition": transition, "remove": ["a"] }
        ]);
        Fx::new(deck(nodes.clone(), states), theme(|_| {}))
    };
    let linear = |style: &str, direction: Option<&str>| {
        let mut t = json!({ "duration": 400, "ease": [0, 0, 1, 1], "style": style });
        if let Some(d) = direction {
            t["direction"] = json!(d);
        }
        t
    };
    // The two layers a quarter of the way through, each its transform, opacity, clip, and ops.
    type Slid = ([f32; 6], f32, Option<scaena_core::displaylist::Path>, Vec<Op>);
    let quarter = |fx: &mut Fx| -> [Slid; 2] {
        let dl = fx.dl("two", 100.0);
        let layers: Vec<Slid> = dl
            .ops
            .iter()
            .map(|op| match op {
                Op::Layer { node: None, transform, opacity, clip, ops, .. } => {
                    (*transform, *opacity, clip.clone(), ops.clone())
                }
                other => panic!("a slide's layer, not {other:?}"),
            })
            .collect();
        layers.try_into().unwrap_or_else(|l: Vec<Slid>| panic!("two layers, not {}", l.len()))
    };
    let holds =
        |ops: &[Op], node: &str| ops.iter().any(|op| matches!(op, Op::Layer { node: Some(n), .. } if n == node));
    let near = |a: f32, b: f32| (a - b).abs() < 0.01;

    // Push, right to left: the slide left goes a quarter of the canvas left, the slide entered
    // comes in from the right; each holds its own nodes, `b` in both, nothing morphing.
    let mut fx = deck_with(linear("push", None));
    let w = fx.dl("two", f64::INFINITY).viewport[0];
    assert_eq!(fx.frame("two", f64::INFINITY).duration_ms, 400.0);
    let [left, entered] = quarter(&mut fx);
    assert!(near(left.0[4], -0.25 * w) && near(entered.0[4], 0.75 * w), "{:?} {:?}", left.0, entered.0);
    assert!(holds(&left.3, "a") && holds(&left.3, "b") && !holds(&entered.3, "a") && holds(&entered.3, "b"));
    assert_eq!((left.1, entered.1), (1.0, 1.0));
    // Before it and once it ends, each slide as at rest.
    assert_eq!(fx.dl("two", 0.0), fx.dl("one", f64::INFINITY));
    assert_eq!(fx.dl("two", 400.0), fx.dl("two", f64::INFINITY));

    // Move in from the top: the slide left stays; the slide entered comes down over it.
    let [left, entered] = quarter(&mut deck_with(linear("cover", Some("down"))));
    let h = fx.dl("two", f64::INFINITY).viewport[1];
    assert!(near(left.0[5], 0.0) && near(entered.0[5], -0.75 * h), "{:?} {:?}", left.0, entered.0);
    // Reveal, left to right: the slide entered lies still under the slide left, which goes right.
    let [under, over] = quarter(&mut deck_with(linear("reveal", Some("right"))));
    assert!(near(under.0[4], 0.0) && near(over.0[4], 0.25 * w) && holds(&over.3, "a"), "{:?}", over.0);
    // Dissolve: the slide entered, a quarter opaque, over the slide left.
    let [left, entered] = quarter(&mut deck_with(linear("dissolve", None)));
    assert!(near(left.1, 1.0) && near(entered.1, 0.25) && left.0 == entered.0, "{} {}", left.1, entered.1);
    // Wipe, bottom to top: the slide entered shows below an edge a quarter of the way up.
    let [_, entered] = quarter(&mut deck_with(linear("wipe", Some("up"))));
    let clip = entered.2.expect("a wipe clips the slide entered");
    assert_eq!(clip, scaena_core::displaylist::Path::rect([0.0, 0.75 * h, w, 0.25 * h]));
}

#[test]
fn the_shipped_builds_move_as_a_presentation_apps_do() {
    // The builds the shipped themes offer (PLAN 2.112), on the torture theme's grid: Dusk's motion
    // presets and the springs they spring on. `b` enters the second slide with the build named,
    // after a cut; on the third, it is emphasized.
    let dusk: Value = serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
    let builds = |t: &mut Value| {
        t["motion"]["presets"] = dusk["motion"]["presets"].clone();
        t["motion"]["springs"] = dusk["motion"]["springs"].clone();
    };
    let deck_with = |enter: &str, emphasis: &str| {
        let nodes = json!({ "a": text("Alpha", "case"), "b": text("Beta", "main") });
        let states = json!([
            { "id": "one", "layout": "specimen", "props": { "a": {} } },
            { "id": "two", "props": { "b": {} }, "choreography": [{ "target": "b", "enter": enter }] },
            { "id": "three", "props": { "a": { "text": "Again" } }, "choreography": [{ "target": "b", "emphasis": emphasis }] }
        ]);
        Fx::new(deck(nodes, states), theme(builds))
    };
    // Each frame of a state's cue, every 10 ms, and its span.
    let frames = |fx: &mut Fx, state: &str| {
        let span = fx.frame(state, f64::INFINITY).duration_ms;
        let at: Vec<f64> = (1..).map(|k| f64::from(k) * 10.0).take_while(|t| *t < span).collect();
        (at.iter().map(|t| fx.dl(state, *t)).collect::<Vec<_>>(), span)
    };
    let look = |dl: &DisplayList| seen(dl, "b").expect("b is drawn");

    // Appear: no time at all; Typewriter: a letter every 40 ms, each at once.
    let mut fx = deck_with("appear", "blink");
    assert_eq!(fx.frame("two", f64::INFINITY).duration_ms, 0.0, "it appears as the slide does");
    let mut fx2 = deck_with("typewriter", "blink");
    assert_eq!(fx2.frame("two", f64::INFINITY).duration_ms, 120.0, "Beta: four letters 40 ms apart");
    assert_eq!([50.0, 100.0].map(|t| units(&fx2.dl("two", t), "b")), [2, 3]);
    // Blink, as emphasis: dims to 15% at its middle, and comes back.
    let (dims, _) = frames(&mut fx, "three");
    let least = dims.iter().map(|dl| look(dl).1).fold(1.0, f32::min);
    assert!((least - 0.15).abs() < 0.05, "it dims to {least}");

    // Pop: up from half its size, past it on the spring, and back to rest.
    let (popping, span) = frames(&mut deck_with("pop", "jiggle"), "two");
    let scales: Vec<f32> = popping.iter().map(|dl| look(dl).0[0]).collect();
    assert!(scales[0] < 0.75 && scales.iter().copied().fold(0.0, f32::max) > 1.05, "{scales:?}");
    assert!(span > 300.0 && span < 1500.0, "the bouncy spring settles in {span} ms");
    // Jiggle, as emphasis: turns one way, then the other, and settles.
    let (jiggling, _) = frames(&mut deck_with("pop", "jiggle"), "three");
    let turns: Vec<f32> = jiggling.iter().map(|dl| look(dl).0[1]).collect();
    assert!(turns.iter().any(|b| *b > 0.05) && turns.iter().any(|b| *b < -0.01), "{turns:?}");

    // Drop: from 240 above, past its place on the spring, and back.
    let mut fx = deck_with("drop", "pulse");
    let rest = look(&fx.dl("two", f64::INFINITY)).0[5];
    let (dropping, _) = frames(&mut fx, "two");
    let falls: Vec<f32> = dropping.iter().map(|dl| look(dl).0[5] - rest).collect();
    assert!(falls[0] < -150.0 && falls.iter().any(|dy| *dy > 10.0), "{falls:?}");

    // Flip: turned in about its middle, as wide as nothing at first; Spin: turned and growing.
    let mut fx = deck_with("flip", "pulse");
    let [a, _, _, d, _, _] = look(&fx.dl("two", 100.0)).0;
    assert!(a > 0.0 && a < 1.0 && d == 1.0, "flipping: {a} {d}");
    let mut fx = deck_with("spin", "pulse");
    let [a, b, ..] = look(&fx.dl("two", 200.0)).0;
    assert!(b.abs() > 0.01 && a.hypot(b) < 1.0, "spinning in: {a} {b}");
    // Each at rest when its cue ends, as the state draws at rest.
    for build in [
        "appear",
        "typewriter",
        "pop",
        "scale",
        "fadeScale",
        "scaleBig",
        "drop",
        "skid",
        "spin",
        "pivot",
        "swing",
        "flip",
    ] {
        let mut fx = deck_with(build, "pulse");
        let span = fx.frame("two", f64::INFINITY).duration_ms;
        assert_eq!(fx.dl("two", span), fx.dl("two", f64::INFINITY), "{build} rests");
    }
}

/// An engine keeps each state's span by what it is worked out from: a deck an edit changed in one
/// state is timed as a fresh engine times it, that state worked out again and the rest kept.
#[test]
fn an_edited_deck_is_timed_as_a_fresh_engine_times_it() {
    let split = |s: &str| {
        json!({
            "id": s, "layout": "specimen", "mode": "absolute", "props": { "t": {} },
            "choreography": [{ "target": "t", "split": "words", "enter": "rise", "stagger": 100, "timing": "with" }]
        })
    };
    let nodes = json!({ "t": text("One two three", "main") });
    let mut fx = Fx::new(deck(nodes.clone(), json!([split("a"), split("b"), split("c")])), theme(|_| {}));
    // An editor's engine keeps each state's span; one made for a render keeps none.
    fx.engine = std::mem::replace(&mut fx.engine, Engine::new(BundleFonts::new())).with_timed(Timed::default());
    let spans = |timeline: &scaena_core::timeline::Timeline| -> Vec<(f64, f64)> {
        timeline.slots.iter().map(|s| (s.start, s.span)).collect()
    };
    assert_eq!(
        spans(&fx.engine.timeline(&fx.deck, &fx.theme, &fx.data).unwrap()),
        [(0.0, 620.0), (620.0, 620.0), (1240.0, 620.0)]
    );
    // `b` says two words more: it runs 200 ms longer, and `c` starts 200 ms later.
    let mut states = json!([split("a"), split("b"), split("c")]);
    states[1]["props"]["t"] = json!({ "text": "One two three four five" });
    let edited = deck(nodes, states);
    let kept = fx.engine.timeline(&edited, &fx.theme, &fx.data).unwrap();
    let mut fresh = Fx::new(edited.clone(), theme(|_| {}));
    let anew = fresh.engine.timeline(&edited, &fresh.theme, &fresh.data).unwrap();
    assert_eq!(spans(&kept), spans(&anew));
    assert_eq!(spans(&kept), [(0.0, 620.0), (620.0, 820.0), (1440.0, 620.0)]);
    // And back: the deck as it was, timed as it was.
    assert_eq!(
        spans(&fx.engine.timeline(&fx.deck, &fx.theme, &fx.data).unwrap()),
        [(0.0, 620.0), (620.0, 620.0), (1240.0, 620.0)]
    );
}

/// A state's motions are read as the states write them: a node's own `enter` timed anew under
/// overrides that set one over it, which an engine keeping each state's span times as a fresh
/// engine does.
#[test]
fn a_motion_written_under_the_overrides_is_timed_as_a_fresh_engine_times_it() {
    let states = json!([{ "id": "a", "layout": "specimen", "mode": "absolute", "props": { "t": {} } }]);
    let entering = |ms: f64| -> Deck {
        let mut t = text("One", "main");
        t["enter"] = json!({ "preset": "fade", "duration": ms });
        let mut d = serde_json::to_value(deck(json!({ "t": t }), states.clone())).unwrap();
        d["overrides"] = json!({ "t": { "enter": "fade" } });
        serde_json::from_value(d).unwrap()
    };
    let mut fx = Fx::new(entering(300.0), theme(|_| {}));
    fx.engine = std::mem::replace(&mut fx.engine, Engine::new(BundleFonts::new())).with_timed(Timed::default());
    let span = |timeline: &scaena_core::timeline::Timeline| timeline.slots[0].span;
    assert_eq!(span(&fx.engine.timeline(&fx.deck, &fx.theme, &fx.data).unwrap()), 300.0);
    let edited = entering(600.0);
    let kept = fx.engine.timeline(&edited, &fx.theme, &fx.data).unwrap();
    let mut fresh = Fx::new(edited.clone(), theme(|_| {}));
    assert_eq!(span(&kept), span(&fresh.engine.timeline(&edited, &fresh.theme, &fresh.data).unwrap()));
    assert_eq!(span(&kept), 600.0);
}

/// The torture theme with Dusk's builds and the springs they spring on (PLAN 2.112).
fn dusk_builds(t: &mut Value) {
    let dusk: Value = serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
    t["motion"]["presets"] = dusk["motion"]["presets"].clone();
    t["motion"]["springs"] = dusk["motion"]["springs"].clone();
}

#[test]
fn a_slide_transition_plays_no_exit_of_the_slide_it_leaves() {
    // The slide left is drawn whole, at rest (SPEC §3.9): a node's own exit there would not draw,
    // so it adds no time to the cue and no motion to it.
    let nodes = json!({ "a": text("Alpha", "case"), "b": text("Beta", "main") });
    for exit in ["spin", "words"] {
        let states = json!([
            { "id": "one", "layout": "specimen", "props": { "a": { "exit": exit }, "b": {} } },
            { "id": "two", "transition": { "style": "push", "duration": 400, "ease": [0, 0, 1, 1] }, "remove": ["a"] }
        ]);
        let mut fx = Fx::new(deck(nodes.clone(), states), theme(dusk_builds));
        assert_eq!(fx.frame("two", f64::INFINITY).duration_ms, 400.0, "{exit}: the push alone");
        let cue = fx.engine.transition(&fx.deck, &fx.theme, &fx.data, "two").unwrap();
        assert!(cue.schedule().cues.is_empty(), "{exit}: {:?}", cue.schedule().cues);
    }
}

#[test]
fn a_slide_transition_into_the_first_slide_moves_it_over_the_ground() {
    // Nothing comes before the first state: the slide left is the canvas's ground, never nothing.
    let nodes = json!({ "a": text("Alpha", "case") });
    for style in ["push", "dissolve", "wipe"] {
        let states = json!([
            { "id": "one", "layout": "specimen", "props": { "a": {} },
              "transition": { "style": style, "duration": 400, "ease": [0, 0, 1, 1] } }
        ]);
        let mut fx = Fx::new(deck(nodes.clone(), states), theme(|_| {}));
        let ground = fx.dl("one", 0.0).ops;
        assert!(matches!(ground[..], [Op::Fill { .. }]), "{style}: before it starts, the ground: {ground:?}");
        let dl = fx.dl("one", 100.0);
        match &dl.ops[0] {
            Op::Layer { node: None, ops, .. } => assert_eq!(ops, &ground, "{style}: the slide left is the ground"),
            other => panic!("{style}: the slide left's layer, not {other:?}"),
        }
    }
}

#[test]
fn a_slide_transition_on_a_spring_stops_where_it_arrives() {
    // Bouncy swings past its end and back as it settles; a slide it moves stops where it first
    // arrives (SPEC §3.9), never sliding back to show a strip of the slide it left.
    let nodes = json!({ "a": text("Alpha", "case"), "b": text("Beta", "main") });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "a": {} } },
        { "id": "two", "transition": { "style": "push", "spring": "bouncy" }, "props": { "b": {} }, "remove": ["a"] }
    ]);
    let mut fx = Fx::new(deck(nodes, states), theme(dusk_builds));
    let span = fx.frame("two", f64::INFINITY).duration_ms;
    assert!(span > 100.0 && span < 130.0, "it arrives in {span} ms");
    // Where the slide entered stands across the canvas, each ms, until well past the spring's settle.
    let rest = fx.dl("two", f64::INFINITY);
    let xs: Vec<f32> = (1..900)
        .map(|t| {
            let dl = fx.dl("two", f64::from(t));
            match &dl.ops[..] {
                [Op::Layer { node: None, .. }, Op::Layer { node: None, transform, .. }] => transform[4],
                _ => {
                    assert_eq!(dl, rest, "at {t} ms, at rest");
                    0.0
                }
            }
        })
        .collect();
    assert!(xs.windows(2).all(|w| w[1] <= w[0]), "it only comes in: {xs:?}");
    assert_eq!(xs[150..], vec![0.0; xs.len() - 150][..], "and stays");
}

#[test]
fn an_exit_on_a_spring_does_not_come_back_once_gone() {
    // Pop as a node leaves springs on bouncy: once the node has gone it stays gone, no faint ghost
    // of it swinging back, and the cue ends there.
    let nodes = json!({ "a": text("Alpha", "case"), "b": text("Beta", "main") });
    let states = json!([
        { "id": "one", "layout": "specimen", "props": { "a": {}, "b": {} } },
        { "id": "two", "remove": ["b"], "choreography": [{ "target": "b", "exit": "pop" }] }
    ]);
    let mut fx = Fx::new(deck(nodes, states), theme(dusk_builds));
    let span = fx.frame("two", f64::INFINITY).duration_ms;
    assert!(span > 100.0 && span < 130.0, "it is gone in {span} ms");
    let opacity: Vec<f32> = (0..900).map(|t| seen(&fx.dl("two", f64::from(t)), "b").map_or(0.0, |(_, o)| o)).collect();
    let gone = opacity.iter().position(|o| *o == 0.0).expect("it goes");
    assert!(opacity[gone..].iter().all(|o| *o == 0.0), "gone at {gone} ms, and after: {opacity:?}");
    assert!(opacity[..gone].windows(2).all(|w| w[1] <= w[0]), "it only fades: {opacity:?}");
}
