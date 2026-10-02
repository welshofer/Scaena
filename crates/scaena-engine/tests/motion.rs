//! Motion on the timeline (SPEC §2.4, §3.9; PLAN 1.11): when cues run (`with`, `after`,
//! delays, staggers, sequences), what they do to whole nodes and to the units a cue
//! splits them into, spring transitions, and the deck's states end to end.

use scaena_core::Deck;
use scaena_core::displaylist::{DisplayList, Op};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, Frame, FrameRequest};
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
        let req = FrameRequest { deck: &self.deck, theme: &self.theme, data: &self.data, state, t_ms };
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
