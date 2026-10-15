//! What the engine draws over a fetched picture (ADR-0030, PLAN O.1a), on the torture deck's
//! `maps` case: a pin whose tip stands on the point of the picture it marks, through the image's
//! crop, fit, focal, and transform; nothing where that point does not show; its label inside what
//! shows and clear of the credit; and the credit, linked, never under 12 pt at presentation size,
//! inside the picture or below it.

use scaena_core::Deck;
use scaena_core::displaylist::{LinkTarget, Op};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::ImageNode;
use scaena_engine::sample::{Content, Scene};
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

/// The torture deck, its theme, and an engine with its fonts and the map card.
fn setup() -> (Deck, Theme, Engine) {
    let deck: Deck = serde_json::from_slice(&read("deck.json")).unwrap();
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let mut engine = Engine::new(fonts);
    engine.add_image("assets/map-card.png", &read("assets/map-card.png")).unwrap();
    (deck, theme, engine)
}

/// The `maps` state at rest, with `node`'s props merged with `props` first.
fn scene(node: &str, props: Value) -> Scene {
    let (mut deck, theme, mut engine) = setup();
    let Value::Object(props) = props else { panic!("an object") };
    for (k, v) in props {
        deck.nodes.get_mut(node).unwrap().props.insert(k, v);
    }
    let data = DataFiles::new();
    let req =
        FrameRequest { deck: &deck, theme: &theme, data: &data, state: "maps", t_ms: f64::INFINITY, format: None };
    engine.at_rest(&req).unwrap()
}

fn image<'a>(scene: &'a Scene, id: &str) -> &'a ImageNode {
    scene
        .nodes
        .iter()
        .find_map(|n| match &n.content {
            Content::Image(i) if n.id == id => Some(i),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no image `{id}`"))
}

/// A box-local point of `id`'s, on the canvas, through its transform.
fn on_canvas(scene: &Scene, id: &str, p: [f32; 2]) -> [f32; 2] {
    let rect = image(scene, id).rect;
    let m = scene.posed(id, true);
    let [x, y] = [f64::from(rect[0] + p[0]), f64::from(rect[1] + p[1])];
    [(m[0] * x + m[2] * y + m[4]) as f32, (m[1] * x + m[3] * y + m[5]) as f32]
}

fn near(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() < 0.5 && (a[1] - b[1]).abs() < 0.5
}

#[test]
fn a_pin_stands_where_its_picture_puts_the_place() {
    // As the deck has it: the map covers its box, centered, the pin on the cross at its middle.
    let s = scene("map-inside", json!({}));
    let map = image(&s, "map-inside");
    let pin = map.marks.pin.expect("a pin");
    let [_, _, w, h] = map.rect;
    let (src, dst) = map.placement();
    // The picture's middle pixel, (400, 225), where it is drawn in the box.
    let middle = [dst[0] + (400.0 - src[0]) / src[2] * dst[2], dst[1] + (225.0 - src[1]) / src[3] * dst[3]];
    assert!(near(pin.tip, middle), "{:?} {middle:?} in {w} × {h}", pin.tip);
    assert!((pin.height - 48.0).abs() < 1e-3, "48 cu tall at 1080p: {}", pin.height);

    // Cropped to the picture's right half, covering its box: the middle is the crop's left edge's
    // middle, and a point three quarters across stands where the crop puts it.
    let s = scene("map-inside", json!({ "crop": [0.5, 0, 0.5, 1], "fit": "fill", "pin": { "at": [0.75, 0.5] } }));
    let map = image(&s, "map-inside");
    let [_, _, w, h] = map.rect;
    assert!(near(map.marks.pin.unwrap().tip, [w / 2.0, h / 2.0]), "{:?}", map.marks.pin);

    // Contained, focal at the top: the picture stands at the top of its box, its middle there.
    let s = scene("map-inside", json!({ "fit": "contain", "focal": [0.5, 0] }));
    let map = image(&s, "map-inside");
    let shown = map.shown();
    assert!(near(map.marks.pin.unwrap().tip, [shown[0] + shown[2] / 2.0, shown[1] + shown[3] / 2.0]));

    // Turned a quarter about its middle: the tip goes where the turn takes the picture's middle.
    let s = scene("map-inside", json!({ "transform": { "rotate": 90 } }));
    let map = image(&s, "map-inside");
    let tip = on_canvas(&s, "map-inside", map.marks.pin.unwrap().tip);
    let [x, y, w, h] = map.rect;
    assert!(near(tip, [x + w / 2.0, y + h / 2.0]), "the middle stays where it is under a turn about it: {tip:?}");
    let off = on_canvas(&s, "map-inside", [w / 2.0 + 100.0, h / 2.0]);
    assert!(near(off, [x + w / 2.0, y + h / 2.0 + 100.0]), "a point to its right turns below it: {off:?}");
    // The display list draws it in the image's layer, through the layer's transform.
    let (deck, theme, mut engine) = setup();
    let data = DataFiles::new();
    let req =
        FrameRequest { deck: &deck, theme: &theme, data: &data, state: "maps", t_ms: f64::INFINITY, format: None };
    let dl = engine.frame(&req).unwrap().display_list;
    let layer = dl
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Layer { node: Some(n), ops, .. } if n == "map-inside" => Some(ops),
            _ => None,
        })
        .unwrap();
    assert!(matches!(layer[0], Op::Image { .. }), "the picture first");
    assert!(
        matches!(layer[1], Op::Stroke { .. }) && matches!(layer[2], Op::Fill { .. }),
        "then the pin's ring and the pin"
    );
}

#[test]
fn a_pin_outside_what_shows_draws_nothing() {
    // Covering a square box, the picture shows its middle 450 px across: a point at its right
    // edge does not show, and neither does one cropped away.
    let s = scene(
        "map-inside",
        json!({ "pin": { "at": [0.97, 0.5], "label": "Far" }, "at": { "rect": [100, 200, 300, 300] } }),
    );
    let map = image(&s, "map-inside");
    assert!(map.marks.pin.is_none() && map.marks.label.is_none(), "{:?}", map.marks);
    let s = scene("map-inside", json!({ "crop": [0, 0, 0.5, 1], "pin": { "at": [0.75, 0.5] } }));
    assert!(image(&s, "map-inside").marks.pin.is_none());
    // Nothing in the display list but the picture and its credit.
    let s = scene("map-inside", json!({ "pin": { "at": [1.5, 0.5] } }));
    assert!(image(&s, "map-inside").marks.pin.is_none(), "a point off the picture");
}

#[test]
fn a_label_stays_inside_and_clear_of_the_credit() {
    let clear = |a: [f32; 4], b: [f32; 4]| {
        a[0] >= b[0] + b[2] || b[0] >= a[0] + a[2] || a[1] >= b[1] + b[3] || b[1] >= a[1] + a[3]
    };
    let inside = |r: [f32; 4], s: [f32; 4]| {
        r[0] >= s[0] - 0.5 && r[1] >= s[1] - 0.5 && r[0] + r[2] <= s[0] + s[2] + 0.5 && r[1] + r[3] <= s[1] + s[3] + 0.5
    };
    // Room to the right of the pin's head: the label stands there.
    let s = scene("map-inside", json!({}));
    let map = image(&s, "map-inside");
    let (pin, label) = (map.marks.pin.unwrap(), map.marks.label.as_ref().expect("a label"));
    assert!(label.rect[0] > pin.tip[0], "to the right: {label:?}");
    assert!(inside(label.rect, map.shown()) && clear(label.rect, map.marks.credit.as_ref().unwrap().rect));
    assert!((label.rect[1] + label.rect[3] / 2.0 - pin.head()[1]).abs() < 1.0, "level with the pin's head");
    // Near the right edge: to its left.
    let s = scene("map-below", json!({}));
    let map = image(&s, "map-below");
    let (pin, label) = (map.marks.pin.unwrap(), map.marks.label.as_ref().expect("a label"));
    assert!(label.rect[0] + label.rect[2] < pin.tip[0], "to the left: {label:?} {pin:?}");
    assert!(inside(label.rect, map.shown()));
    // Low on the picture, where the credit's card stands: beside the pin, clear of it, or none.
    let s = scene("map-inside", json!({ "pin": { "at": [0.3, 0.97], "label": "Down by the credit" } }));
    let map = image(&s, "map-inside");
    if let Some(label) = &map.marks.label {
        assert!(clear(label.rect, map.marks.credit.as_ref().unwrap().rect), "{label:?}");
        assert!(inside(label.rect, map.shown()));
    }
    // An empty label draws none.
    let s = scene("map-inside", json!({ "pin": { "at": [0.5, 0.5], "label": "" } }));
    assert!(image(&s, "map-inside").marks.label.is_none());
}

#[test]
fn a_credit_is_drawn_linked_and_never_under_12_pt() {
    let (deck, theme, mut engine) = setup();
    let data = DataFiles::new();
    let req =
        FrameRequest { deck: &deck, theme: &theme, data: &data, state: "maps", t_ms: f64::INFINITY, format: None };
    let s = engine.at_rest(&req).unwrap();
    for id in ["map-inside", "map-below"] {
        let map = image(&s, id);
        let credit = map.marks.credit.as_ref().expect("a credit");
        assert!(credit.text.runs.iter().all(|r| r.size >= 24.0), "never under 24 cu at 1080p: {id}");
        assert_eq!(credit.text.written, scaena_core::fetched::said(&deck_credit(&deck, id)));
        let hrefs: Vec<&LinkTarget> = credit.text.underlines.iter().filter_map(|u| u.target.as_ref()).collect();
        for href in
            ["https://www.openstreetmap.org/copyright", "https://openmaptiles.org/", "https://www.geoapify.com/"]
        {
            assert!(hrefs.contains(&&LinkTarget::Href(href.into())), "{id} links {href}: {hrefs:?}");
        }
    }
    // Inside: on a card in the bottom-left corner of what shows.
    let inside = image(&s, "map-inside");
    let (card, shown) = (inside.marks.credit.as_ref().unwrap(), inside.shown());
    assert!(card.fill.is_some());
    assert!((card.rect[0] - shown[0]).abs() < 0.5 && (card.rect[1] + card.rect[3] - shown[1] - shown[3]).abs() < 0.5);
    assert!(card.rect[2] <= shown[2] * 0.7 + 0.5, "clear of the service's own credit in the bottom right");
    // Below: no card, under the picture, inside the box.
    let below = image(&s, "map-below");
    let (card, shown) = (below.marks.credit.as_ref().unwrap(), below.shown());
    assert!(card.fill.is_none());
    assert!(card.rect[1] >= shown[1] + shown[3] - 0.5, "under the picture: {card:?} {shown:?}");
    assert!(card.rect[1] + card.rect[3] <= below.rect[3] + 0.5, "inside the box");
    // The frame: each linked run a link op, in the credit's own layer.
    let dl = engine.frame(&req).unwrap().display_list;
    let hrefs: Vec<LinkTarget> = dl.links().into_iter().map(|l| l.target).collect();
    assert!(hrefs.contains(&LinkTarget::Href("https://www.geoapify.com/".into())), "{hrefs:?}");
    let credit_layers = |id: &str| {
        dl.ops
            .iter()
            .find_map(|op| match op {
                Op::Layer { node: Some(n), ops, .. } if n == id => Some(ops),
                _ => None,
            })
            .unwrap()
            .iter()
            .filter(|op| matches!(op, Op::Layer { cell: Some(c), .. } if *c == scaena_core::fetched::CREDIT_CELL))
            .count()
    };
    assert_eq!((credit_layers("map-inside"), credit_layers("map-below")), (1, 1));
    // On a smaller canvas, the caption's 24 cu still holds in proportion.
    let mut small = deck.clone();
    small.canvas.width = 960.0;
    small.canvas.height = 540.0;
    let req = FrameRequest { deck: &small, ..req };
    let s = engine.at_rest(&req).unwrap();
    let credit = image(&s, "map-inside").marks.credit.clone().unwrap();
    assert!(credit.text.runs.iter().all(|r| r.size >= 12.0 - 1e-3), "12 cu on a 540 cu canvas");
}

fn deck_credit(deck: &Deck, id: &str) -> Vec<scaena_core::fetched::CreditRun> {
    serde_json::from_value(deck.nodes[id].props["fetched"]["credit"].clone()).unwrap()
}

/// W314 (ADR-0030): a credit that covers more than a quarter of a small map is found, and its fix
/// sets it below, which lint keeps once the state laid out with it shows the credit reads there;
/// the torture deck's maps are found clean.
#[test]
fn a_credit_that_cannot_be_read_is_w314_with_its_fix() {
    let (mut deck, theme, mut engine) = setup();
    let data = DataFiles::new();
    let w314 = |engine: &mut Engine, deck: &Deck| -> Vec<scaena_core::lint::Finding> {
        let found = scaena_engine::lint::lint_state(engine, deck, &theme, &data, None, "maps").unwrap();
        found.into_iter().filter(|f| f.code == "W314" && f.format.is_none()).collect()
    };
    assert!(w314(&mut engine, &deck).is_empty(), "the torture deck's maps read");
    let small = deck.nodes.get_mut("map-inside").unwrap();
    small.props.insert("at".into(), json!({ "rect": [96, 220, 400, 225] }));
    let found = w314(&mut engine, &deck);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].message.contains("covers more than a quarter"), "{}", found[0].message);
    assert_eq!(
        found[0].fix.as_deref(),
        Some(&[json!({ "op": "add", "path": "/nodes/map-inside/credit", "value": "below" })][..])
    );
}
