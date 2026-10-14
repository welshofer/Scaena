//! Connectors (PLAN 2.109) run between what they join as it is drawn: from the edge of an end
//! turned by its transform, and through a cue, between their ends as each frame draws them.

use scaena_core::Deck;
use scaena_core::displaylist::{DisplayList, Op, PathEl};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use serde_json::{Value, json};

/// How far each end stands off its box in the torture theme: its `space.2`.
const GAP: f32 = 8.0;

fn theme() -> Theme {
    Theme::from_json(include_str!("../../../tests/fixtures/torture.scaena/theme.json")).unwrap()
}

/// A deck of `nodes`, each placed by its own `at.rect`, and `states`, each a delta over `nodes`.
fn deck(nodes: Value, states: Value) -> Deck {
    let deck = json!({
        "scaena": scaena_core::FORMAT_VERSION,
        "canvas": { "width": 1920, "height": 1080 },
        "nodes": nodes,
        "states": states,
    });
    Deck::from_json(&deck.to_string()).unwrap()
}

/// How far `p` lies from the box `r`: none inside it.
fn off(p: [f32; 2], r: [f32; 4]) -> f32 {
    let dx = (r[0] - p[0]).max(p[0] - (r[0] + r[2])).max(0.0);
    let dy = (r[1] - p[1]).max(p[1] - (r[1] + r[3])).max(0.0);
    dx.hypot(dy)
}

/// Where the line `node` draws in `dl` starts, and where it ends, in canvas units: its stroke's
/// first point, and of all it draws (an arrowhead too) the point farthest from there.
fn ends(dl: &DisplayList, node: &str) -> Option<([f32; 2], [f32; 2])> {
    fn walk(ops: &[Op], m: [f32; 6], node: &str, inside: bool, out: &mut Vec<Vec<[f32; 2]>>) {
        let at = |[x, y]: [f32; 2]| [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
        for op in ops {
            match op {
                Op::Layer { node: n, transform: t, ops, .. } => {
                    let m = [
                        m[0] * t[0] + m[2] * t[1],
                        m[1] * t[0] + m[3] * t[1],
                        m[0] * t[2] + m[2] * t[3],
                        m[1] * t[2] + m[3] * t[3],
                        m[0] * t[4] + m[2] * t[5] + m[4],
                        m[1] * t[4] + m[3] * t[5] + m[5],
                    ];
                    walk(ops, m, node, inside || n.as_deref() == Some(node), out);
                }
                Op::Stroke { path, .. } | Op::Fill { path, .. } if inside => out.push(
                    (path.0.iter())
                        .filter_map(|el| match el {
                            PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(at(*p)),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => {}
            }
        }
    }
    let mut paths = Vec::new();
    walk(&dl.ops, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], node, false, &mut paths);
    let start = *paths.first()?.first()?;
    let far = |p: &&[f32; 2]| (p[0] - start[0]).hypot(p[1] - start[1]);
    let end = *paths.iter().flatten().max_by(|a, b| far(a).total_cmp(&far(b)))?;
    Some((start, end))
}

/// An end turned a quarter by its transform (SPEC §3.3): the connector leaves the edge it is
/// drawn with, not the edge of its box as laid out.
#[test]
fn a_connector_leaves_its_end_where_the_end_is_drawn() {
    let nodes = json!({
        "a": { "type": "shape", "kind": "rect", "fill": "accent", "at": { "rect": [100, 100, 400, 100] },
               "transform": { "rotate": 90 } },
        "b": { "type": "shape", "kind": "rect", "fill": "accent", "at": { "rect": [800, 100, 200, 100] } },
        "ab": { "type": "shape", "kind": "arrow", "connect": { "from": "a", "to": "b" } }
    });
    let deck = deck(nodes, json!([{ "id": "one", "props": { "a": {}, "b": {}, "ab": {} } }]));
    let theme = theme();
    let data = DataFiles::new();
    let mut engine = Engine::new(BundleFonts::new());
    let req = FrameRequest { deck: &deck, theme: &theme, data: &data, state: "one", t_ms: f64::INFINITY, format: None };
    let scene = engine.at_rest(&req).unwrap();
    // `a` is drawn turned about its middle, (300, 150): from x 250 to 350.
    let route = scene.route("ab").unwrap();
    assert!((route[0][0] - (350.0 + GAP)).abs() < 1.0, "it leaves a's drawn edge: {route:?}");
    assert!((route[1][0] - (800.0 - GAP)).abs() < 1.0, "it meets b's edge: {route:?}");
    let (start, _) = ends(&scene.draw_at(0.0), "ab").unwrap();
    assert!((start[0] - (350.0 + GAP)).abs() < 1.0, "drawn from there: {start:?}");
    // A drag of `b` runs it from where `a` is drawn, too.
    let (start, end) = ends(&scene.moved(0.0, &["b"], [0.0, 300.0]), "ab").unwrap();
    assert!(off(start, [250.0, -50.0, 100.0, 400.0]) <= GAP + 1.0, "{start:?}");
    assert!(off(end, [800.0, 400.0, 200.0, 100.0]) <= GAP + 1.0, "{end:?}");
}

/// Through a cue (PLAN 2.109), a connector runs between its ends as each frame draws them: one
/// end still, the other moving from beside it to below it, the line never leaves either.
#[test]
fn a_connector_stays_on_its_ends_through_a_cue() {
    let rect = |r: [f32; 4]| json!({ "type": "shape", "kind": "rect", "fill": "accent", "at": { "rect": r } });
    let nodes = json!({
        "a": rect([100.0, 100.0, 400.0, 200.0]),
        "b": rect([1200.0, 100.0, 400.0, 200.0]),
        "ab": { "type": "shape", "kind": "arrow", "connect": { "from": "a", "to": "b" } }
    });
    let states = json!([
        { "id": "one", "props": { "a": {}, "b": {}, "ab": {} } },
        { "id": "two", "transition": { "duration": "slow", "ease": "standard" },
          "props": { "b": { "at": { "rect": [100, 900, 400, 200] } } } }
    ]);
    let deck = deck(nodes, states);
    let theme = theme();
    let data = DataFiles::new();
    let mut engine = Engine::new(BundleFonts::new());
    let cue = engine.transition(&deck, &theme, &data, "two").unwrap();
    for k in 1..10 {
        let t = 80.0 * f64::from(k);
        let (start, end) = ends(&cue.frame(t), "ab").unwrap_or_else(|| panic!("no arrow at {t} ms"));
        let a = off(start, [100.0, 100.0, 400.0, 200.0]);
        assert!(a > 0.0 && a <= GAP + 2.0, "{t} ms: it leaves a, {start:?}");
        // `b` stands where the frame draws it, somewhere between where it was and where it goes.
        let between = [100.0, 100.0, 1500.0, 1000.0];
        assert!(off(end, between) <= GAP + 2.0, "{t} ms: {end:?}");
    }
}
