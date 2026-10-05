//! What stands where, and where a node may go (ADR-0013), on the torture deck: the
//! `containers` case's nested stacks, grid, frame, and group, and the `formats` case laid out
//! again in 9:16. A point hits what draws there, topmost first, with the containers it sits
//! in; each box is where the frame at rest draws its node; and a box dropped on a node's
//! targets snaps to a place its patch puts it.

use scaena_core::Deck;
use scaena_core::displaylist::{Op, Rect};
use scaena_core::patch::Op as PatchOp;
use scaena_core::validate::BundleFiles;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::geometry::{By, NodeBox, Snap, Targets};
use scaena_engine::images::BundleImages;
use scaena_engine::sample::Scene;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

fn torture() -> (Deck, Theme, Engine) {
    let deck: Deck = serde_json::from_slice(&read("deck.json")).unwrap();
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let mut images = BundleImages::new();
    for path in deck.image_files() {
        images.register(&path, &read(&path)).unwrap();
    }
    (deck, theme, Engine::new(fonts).with_images(images))
}

fn at_rest(state: &str, format: Option<&str>) -> Scene {
    let (deck, theme, mut engine) = torture();
    let data = DataFiles::new();
    let req = FrameRequest { deck: &deck, theme: &theme, data: &data, state, t_ms: f64::INFINITY, format };
    engine.at_rest(&req).unwrap()
}

fn find<'a>(boxes: &'a [NodeBox], node: &str) -> &'a NodeBox {
    boxes.iter().find(|b| b.node == node).unwrap_or_else(|| panic!("no box for `{node}`"))
}

fn center(r: Rect) -> [f32; 2] {
    [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0]
}

fn inside(outer: Rect, inner: Rect) -> bool {
    let e = 0.01;
    inner[0] >= outer[0] - e
        && inner[1] >= outer[1] - e
        && inner[0] + inner[2] <= outer[0] + outer[2] + e
        && inner[1] + inner[3] <= outer[1] + outer[3] + e
}

#[test]
fn a_point_hits_what_draws_there_topmost_first_with_its_containers() {
    let scene = at_rest("containers", None);
    let boxes = scene.boxes();
    let hit = |node: &str| scene.hit(center(find(&boxes, node).rect));
    let names = |hits: &[scaena_engine::geometry::Hit]| hits.iter().map(|h| h.node.clone()).collect::<Vec<_>>();

    // A figure in a card in a row of cards: the figure, inside its card, inside the row.
    let figure = hit("stat-a-figure");
    assert_eq!(figure[0].node, "stat-a-figure");
    assert_eq!(figure[0].containers, ["stat-a", "stats"]);
    // A label on a pill on a photo, all in a frame: each, topmost first.
    let label = names(&hit("card-tag-label"));
    assert_eq!(label[..3], ["card-tag-label", "card-tag", "card-photo"], "{label:?}");
    // A grid's areas: the dot in its named area.
    let dot = hit("board-dot");
    assert_eq!((dot[0].node.as_str(), dot[0].containers.as_slice()), ("board-dot", &["board".to_string()][..]));
    // A group holds its marks: the dot over the ring, each in the group.
    let marks = hit("marks-dot");
    assert_eq!(names(&marks)[..2], ["marks-dot", "marks-ring"]);
    assert!(marks.iter().all(|h| h.containers == ["marks"]));
    // Off the canvas, nothing.
    assert!(scene.hit([-50.0, -50.0]).is_empty());
    assert!(scene.hit([1e6, 540.0]).is_empty());
}

#[test]
fn every_visible_node_has_a_box_inside_its_container() {
    let scene = at_rest("containers", None);
    let boxes = scene.boxes();
    for node in ["stats", "stat-a", "stat-a-figure", "tally", "board", "board-photo", "card", "card-tag", "marks"] {
        find(&boxes, node);
    }
    // A group draws nothing of its own; a node is listed once.
    assert!(!find(&boxes, "marks").draws);
    assert!(find(&boxes, "stat-a-figure").draws);
    let mut ids: Vec<&str> = boxes.iter().map(|b| b.node.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), boxes.len());
    for b in &boxes {
        if let Some(parent) = &b.parent {
            assert!(inside(find(&boxes, parent).rect, b.rect), "`{}` {:?} outside `{parent}`", b.node, b.rect);
        }
    }
}

/// A node's box is where the frame at rest draws it: the middle of each text node's glyphs
/// stands in its box. (A text can set ink a little past its box, a hung quote or a cap set on
/// its top edge, never its middle.)
#[test]
fn boxes_are_where_the_frame_draws() {
    let (deck, theme, mut engine) = torture();
    let data = DataFiles::new();
    for state in ["containers", "anchors", "formats", "hanging"] {
        let req = FrameRequest { deck: &deck, theme: &theme, data: &data, state, t_ms: f64::INFINITY, format: None };
        let boxes = engine.at_rest(&req).unwrap().boxes();
        let dl = engine.frame(&req).unwrap().display_list;
        let mut texts = 0;
        for op in &dl.ops {
            let Op::Layer { node: Some(node), transform, ops, .. } = op else { continue };
            let Some(b) = boxes.iter().find(|b| &b.node == node) else { continue };
            let points: Vec<[f32; 2]> = ops
                .iter()
                .filter_map(|op| match op {
                    Op::Glyphs { glyphs, .. } => Some(glyphs.iter().map(|g| [g.x + transform[4], g.y + transform[5]])),
                    _ => None,
                })
                .flatten()
                .collect();
            if points.is_empty() {
                continue;
            }
            let span =
                |i: usize| points.iter().map(|p| p[i]).fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
            let ((x0, x1), (y0, y1)) = (span(0), span(1));
            let middle = [(x0 + x1) / 2.0, (y0 + y1) / 2.0];
            let [x, y, w, h] = b.rect;
            assert!(
                middle[0] >= x && middle[0] <= x + w && middle[1] >= y && middle[1] <= y + h,
                "{state}: `{node}`'s glyphs centre on {middle:?}, outside its box {:?}",
                b.rect
            );
            texts += 1;
        }
        assert!(texts > 0, "{state}: no text drawn in a box");
    }
}

/// In another format the same nodes stand where that format lays them out.
#[test]
fn boxes_follow_the_format() {
    let wide = at_rest("formats", None).boxes();
    let tall = at_rest("formats", Some("9:16")).boxes();
    let (mut moved, mut shared) = (0, 0);
    for b in &wide {
        if let Some(t) = tall.iter().find(|t| t.node == b.node) {
            shared += 1;
            moved += usize::from(t.rect != b.rect);
        }
    }
    assert!(shared > 0 && moved > 0, "{shared} shared, {moved} moved");
    // Every tall box lies on the tall canvas.
    for b in &tall {
        assert!(b.rect[0] + b.rect[2] <= 1080.0 + 0.01 && b.rect[1] + b.rect[3] <= 1920.0 + 0.01, "{:?}", b);
    }
}

fn targets(deck: &Deck, state: &str, node: &str, format: Option<&str>) -> Targets {
    let (_, theme, mut engine) = torture();
    let data = DataFiles::new();
    let req = FrameRequest { deck, theme: &theme, data: &data, state, t_ms: f64::INFINITY, format };
    engine.targets(&req, node).unwrap()
}

/// The torture bundle, as the patch compiler reads it.
struct Bundle;

impl BundleFiles for Bundle {
    fn exists(&self, path: &str) -> bool {
        std::path::Path::new(BUNDLE).join(path).is_file()
    }

    fn read_text(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(format!("{BUNDLE}/{path}")).ok()
    }
}

/// `deck` with `target`'s patch, made in `state`, applied: every op where it lives.
fn placed(deck: &Deck, state: &str, target: &scaena_engine::geometry::Target) -> Deck {
    let ops: Vec<serde_json::Value> = target
        .ops(Some(state), false)
        .into_iter()
        .map(|op| serde_json::to_value(PatchOp::Semantic(Box::new(op))).unwrap())
        .collect();
    let doc = serde_json::to_value(deck).unwrap();
    let compiled = scaena_core::patch::compile(&doc, &ops, &Bundle).unwrap();
    serde_json::from_value(compiled.doc).unwrap()
}

fn close(a: Rect, b: Rect) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.01)
}

#[test]
fn what_holds_a_node_says_where_it_may_go() {
    let (deck, ..) = torture();
    let boxes = at_rest("containers", None).boxes();
    // A root in a slot of the state's template: the theme's grid, its tracks, the slots.
    let case = targets(&deck, "containers", "case", None);
    assert_eq!(case.by, By::Grid);
    assert_eq!((case.columns.len(), case.rows.len()), (12, 8));
    let names: Vec<&str> = case.slots.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["case", "main", "left", "right", "canvas", "grid"]);
    assert_eq!(case.cell, case.slots[0].1);
    assert_eq!(case.cell, [96.0, 96.0, 1728.0, case.rows[0][1] - 96.0]);
    // By cells: the cells it names.
    let tally = targets(&deck, "containers", "tally", None);
    assert_eq!(
        tally.cell,
        [tally.columns[0][0], tally.rows[4][0], tally.columns[10][1] - 96.0, tally.rows[4][1] - tally.rows[4][0]]
    );
    // In a stack: its order, along x.
    let stat = targets(&deck, "containers", "stat-b", None);
    assert_eq!(stat.by, By::Stack { parent: "stats".into(), across: true });
    let flow: Vec<&str> = stat.flow.iter().map(|(n, ..)| n.as_str()).collect();
    assert_eq!(flow, ["stat-a", "stat-b", "stat-c"]);
    assert_eq!(stat.within, find(&boxes, "stats").rect);
    // In a grid container: its own tracks, and its areas as slots.
    let dot = targets(&deck, "containers", "board-dot", None);
    assert_eq!(dot.by, By::Cells { parent: "board".into() });
    assert_eq!((dot.columns.len(), dot.rows.len()), (2, 2));
    let areas: Vec<&str> = dot.slots.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(areas, ["mark", "note", "photo"]);
    // Its area is its cell, which its own size centers it in.
    assert_eq!(dot.cell, dot.slots[0].1);
    assert!(inside(dot.cell, find(&boxes, "board-dot").rect) && dot.cell != find(&boxes, "board-dot").rect);
    assert!(inside(find(&boxes, "board").rect, dot.slots[2].1));
    // In a frame: a rect from its padding edge.
    let tag = targets(&deck, "containers", "card-tag", None);
    assert_eq!(tag.by, By::Frame { parent: "card".into() });
    assert!(close(tag.cell, find(&boxes, "card-tag").rect));
    assert!(close([tag.within[0] + 24.0, tag.within[1] + 24.0, 132.0, 44.0], tag.cell));
    // A group's member stands on the theme's grid.
    assert_eq!(targets(&deck, "containers", "marks-dot", None).by, By::Grid);
}

#[test]
fn a_dropped_box_snaps_and_its_patch_puts_the_node_there() {
    let (deck, ..) = torture();
    let state = "containers";
    let pitch = |t: &Targets| t.columns[1][0] - t.columns[0][0];
    let check = |node: &str, how: Snap, drop: &dyn Fn(&Targets) -> Rect| {
        let before = targets(&deck, state, node, None);
        let target = before.snap(how, drop(&before)).unwrap_or_else(|| panic!("{node} {how:?}: no target"));
        let after = targets(&placed(&deck, state, &target), state, node, None);
        (target, after)
    };
    // Moved a column right and a little down: the same span, one track on.
    let (target, after) =
        check("tally", Snap::Move, &|t| [t.cell[0] + pitch(t) * 1.2, t.cell[1] + 20.0, t.cell[2], t.cell[3]]);
    assert_eq!(serde_json::to_value(&target.spots[0].1).unwrap(), serde_json::json!({ "col": [2, 12], "row": 5 }));
    assert!(close(after.cell, target.cell), "{:?} {:?}", after.cell, target.cell);
    // Pushed past the grid's edge, it stops at the edge.
    let (target, _) = check("stats", Snap::Move, &|t| [t.cell[0] + 900.0, t.cell[1], t.cell[2], t.cell[3]]);
    assert_eq!(serde_json::to_value(&target.spots[0].1).unwrap(), serde_json::json!({ "col": [1, 12], "row": [2, 4] }));
    // Its right edge dragged in: each edge to the nearest track's.
    let (target, after) =
        check("board", Snap::Resize, &|t| [t.cell[0], t.cell[1], t.cell[2] - pitch(t) * 2.4, t.cell[3]]);
    assert_eq!(serde_json::to_value(&target.spots[0].1).unwrap(), serde_json::json!({ "col": [1, 6], "row": [6, 8] }));
    assert!(close(after.cell, target.cell));
    // Into the slot it covers most.
    let (target, after) = check("case", Snap::Slot, &|t| {
        let right = t.slots.iter().find(|(n, _)| n == "right").unwrap().1;
        [right[0] + 40.0, right[1] + 30.0, right[2] * 0.8, right[3] * 0.7]
    });
    assert_eq!(serde_json::to_value(&target.spots[0].1).unwrap(), serde_json::json!({ "in": "right" }));
    assert_eq!(after.cell, target.cell);
    // A grid container's child into another of its cells, and into an area.
    let (target, after) =
        check("board-dot", Snap::Move, &|t| [t.columns[0][0] + 10.0, t.rows[0][0] + 5.0, t.cell[2], t.cell[3]]);
    assert_eq!(serde_json::to_value(&target.spots[0].1).unwrap(), serde_json::json!({ "col": 1, "row": 1 }));
    assert!(close(after.cell, target.cell));
    let (target, after) = check("board-dot", Snap::Slot, &|t| t.slots.iter().find(|(n, _)| n == "note").unwrap().1);
    assert_eq!(serde_json::to_value(&target.spots[0].1).unwrap(), serde_json::json!({ "area": "note" }));
    assert!(close(after.cell, target.cell));
    // In a frame, where it was dropped, in whole canvas units from its padding edge.
    let (target, after) = check("card-tag", Snap::Free, &|t| [t.cell[0] + 10.4, t.cell[1] + 7.6, t.cell[2], t.cell[3]]);
    assert_eq!(
        serde_json::to_value(&target.spots[0].1).unwrap(),
        serde_json::json!({ "rect": [34.0, 32.0, 132.0, 44.0] })
    );
    assert!(close(after.cell, target.cell));
    // Off the theme's grid: a rect on the canvas.
    let (target, after) = check("marks-dot", Snap::Free, &|t| [t.cell[0] - 300.0, t.cell[1], t.cell[2], t.cell[3]]);
    assert!(target.spots[0].1.rect.is_some());
    assert!(close(after.cell, target.cell));
    // In a stack, past its last child: the others keep their order, and only the indexes
    // that change are written.
    let (target, after) = check("stat-a", Snap::Order, &|t| {
        let last = t.flow[2].1;
        [last[0] + last[2] * 0.75, last[1], t.cell[2], t.cell[3]]
    });
    let flow: Vec<&str> = after.flow.iter().map(|(n, ..)| n.as_str()).collect();
    assert_eq!(flow, ["stat-b", "stat-c", "stat-a"]);
    assert_eq!(target.spots.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["stat-c", "stat-a"]);
    assert_eq!(target.cell[2], 0.0, "a guide across the row: {:?}", target.cell);
    // Dropped where it is, nothing changes.
    let (target, _) = check("stat-b", Snap::Order, &|t| t.cell);
    assert!(target.spots.is_empty());
    // A way that does not place this node is no target.
    let stat = targets(&deck, state, "stat-a", None);
    assert!(stat.snap(Snap::Move, stat.cell).is_none() && stat.snap(Snap::Free, stat.cell).is_none());
}

/// In another format, a node's targets are that format's grid and slots.
#[test]
fn targets_follow_the_format() {
    let (deck, ..) = torture();
    let wide = targets(&deck, "formats", "case", None);
    let tall = targets(&deck, "formats", "case", Some("9:16"));
    assert!(tall.within[2] == 1080.0 && tall.within[3] == 1920.0, "{:?}", tall.within);
    assert_ne!(wide.columns, tall.columns);
    assert!(tall.slots.iter().all(|(_, r)| r[0] + r[2] <= 1080.0 + 0.01));
    assert!(tall.slots.iter().zip(&wide.slots).any(|(t, w)| t.0 == w.0 && t.1 != w.1));
}

/// `still` as a drag of `held` shows it: their layers on top, in their order, each that
/// stands outside them `by` away.
fn carried(still: &[Op], held: &[&str]) -> Vec<Op> {
    let mine = |op: &Op| matches!(op, Op::Layer { node: Some(id), .. } if held.contains(&id.as_str()));
    let (over, mut rest): (Vec<Op>, Vec<Op>) = still.iter().cloned().partition(mine);
    rest.extend(over);
    rest
}

/// `moved` is `still` with the outermost layer of each node in `held` `by` away, and every
/// other op as it was. Layers that hold others here stand at the origin, unturned.
fn moved_by(still: &[Op], moved: &[Op], held: &[&str], by: [f32; 2]) {
    assert_eq!(still.len(), moved.len());
    for (was, is) in still.iter().zip(moved) {
        match (was, is) {
            (Op::Layer { node: Some(id), .. }, _) if held.contains(&id.as_str()) => {
                let mut expected = was.clone();
                if let Op::Layer { transform, .. } = &mut expected {
                    transform[4] += by[0];
                    transform[5] += by[1];
                }
                assert_eq!(&expected, is, "`{id}` moves by {by:?}");
            }
            (Op::Layer { ops: inner, .. }, Op::Layer { ops: inner_now, .. }) => {
                let (mut outer, mut outer_now) = (was.clone(), is.clone());
                for op in [&mut outer, &mut outer_now] {
                    if let Op::Layer { ops, .. } = op {
                        ops.clear();
                    }
                }
                assert_eq!(outer, outer_now);
                moved_by(inner, inner_now, held, by);
            }
            _ => assert_eq!(was, is),
        }
    }
}

/// A drag moves its nodes' layers and those of everything they hold, and nothing else: a
/// frame and what is in it, a group composited as one layer, a member inside that layer, a
/// stack of stacks, several nodes at once. Nothing is laid out again (ADR-0013).
#[test]
fn a_drag_moves_a_node_and_what_it_holds_and_nothing_else() {
    let scene = at_rest("containers", None);
    let time = 3.25;
    let still = scene.draw_at(time);
    let by = [40.0, -20.0];
    for (node, held) in [
        ("card", &["card", "card-photo", "card-tag", "card-tag-label"][..]),
        ("marks", &["marks", "marks-ring", "marks-dot"][..]),
        ("marks-dot", &["marks-dot"][..]),
        ("tally-label", &["tally-label"][..]),
        (
            "stats",
            &[
                "stats",
                "stat-a",
                "stat-a-figure",
                "stat-a-label",
                "stat-b",
                "stat-b-figure",
                "stat-b-label",
                "stat-c",
                "stat-c-figure",
                "stat-c-label",
            ][..],
        ),
    ] {
        let moved = scene.moved(time, &[node], by);
        assert_ne!(moved, still, "`{node}` moves");
        // Held above the rest, and where it is dragged.
        moved_by(&carried(&still.ops, held), &moved.ops, held, by);
        let top = moved
            .ops
            .iter()
            .rev()
            .take_while(|op| matches!(op, Op::Layer { node: Some(id), .. } if held.contains(&id.as_str())));
        assert!(top.count() > 0 || node == "marks-dot", "`{node}` is drawn over the rest");
    }
    // Composited, the group is one layer: its members move inside it, with it.
    let group = still.ops.iter().find(|op| matches!(op, Op::Layer { node: Some(id), .. } if id == "marks"));
    assert!(matches!(group, Some(Op::Layer { ops, .. }) if ops.len() == 2), "{group:?}");
    // Several at once (PLAN 2.42): each with what it holds, over the rest, by the same distance.
    let held = ["card", "card-photo", "card-tag", "card-tag-label", "tally-label"];
    let moved = scene.moved(time, &["card", "tally-label"], by);
    moved_by(&carried(&still.ops, &held), &moved.ops, &held, by);
    // Not there: the frame as it was.
    assert_eq!(scene.moved(time, &["nowhere"], by), still);
}
