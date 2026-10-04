//! What stands where (ADR-0013), on the torture deck: the `containers` case's nested stacks,
//! grid, frame, and group, and the `formats` case laid out again in 9:16. A point hits what
//! draws there, topmost first, with the containers it sits in; each box is where the frame at
//! rest draws its node.

use scaena_core::Deck;
use scaena_core::displaylist::{Op, Rect};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::geometry::NodeBox;
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
