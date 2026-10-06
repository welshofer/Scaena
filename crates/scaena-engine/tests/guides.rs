//! Guides (PLAN 2.57, ADR-0013), on the torture deck: the theme's grid as each format lays it
//! out, and what a box a drag moves may meet in a state at rest. The torture theme's grid is
//! 12 × 8 with 24 cu gutters inside a 96 cu margin (in 9:16, 96 above and below and 64 beside),
//! with a baseline every 8 cu from the top margin.

use scaena_core::Deck;
use scaena_core::displaylist::Rect;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::guides;
use scaena_engine::images::BundleImages;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

fn deck() -> Deck {
    serde_json::from_slice(&read("deck.json")).unwrap()
}

/// The torture theme, as `edit` leaves it.
fn theme(edit: impl FnOnce(&mut Value)) -> Theme {
    let mut t: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    edit(&mut t);
    Theme::from_json(&t.to_string()).unwrap()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn guides_draw_the_grid_each_format_lays_out() {
    let (deck, theme) = (deck(), theme(|_| {}));
    let wide = guides::grid(&deck, &theme, None).unwrap();
    assert_eq!(wide.canvas, [1920.0, 1080.0]);
    // Twelve columns 122 wide, 24 apart, inside the margin; eight rows 90 tall.
    assert_eq!((wide.columns.len(), wide.columns[0], wide.columns[11]), (12, [96.0, 218.0], [1702.0, 1824.0]));
    assert_eq!((wide.rows.len(), wide.rows[0], wide.rows[7]), (8, [96.0, 186.0], [894.0, 984.0]));
    // A baseline every 8 cu from the top margin to the bottom one.
    assert_eq!((wide.baselines.len(), wide.baselines[0], wide.baselines[111]), (112, 96.0, 984.0));
    assert!(wide.baselines.windows(2).all(|w| near(w[1] - w[0], 8.0)));

    // In 9:16, the format's own grid on its own canvas.
    let tall = guides::grid(&deck, &theme, Some("9:16")).unwrap();
    assert_eq!(tall.canvas, [1080.0, 1920.0]);
    assert!(near(tall.columns[0][0], 64.0) && near(tall.columns[11][1], 1016.0), "{:?}", tall.columns);
    assert!(near(tall.rows[0][0], 96.0) && near(tall.rows[7][1], 1824.0), "{:?}", tall.rows);
    assert_eq!(tall.baselines.len(), 217);
    assert!(near(*tall.baselines.last().unwrap(), 1824.0));

    // A format the deck does not list is an error that says so.
    let err = guides::grid(&deck, &theme, Some("1:1")).unwrap_err().to_string();
    assert!(err.contains("not one of the deck's formats"), "{err}");
}

#[test]
fn a_grid_with_no_baseline_or_one_too_fine_to_tell_apart_draws_none() {
    let deck = deck();
    let none = theme(|t| {
        t["grid"].as_object_mut().unwrap().remove("baseline");
    });
    assert!(guides::grid(&deck, &none, None).unwrap().baselines.is_empty());
    let fine = theme(|t| t["grid"]["baseline"] = json!(0.25));
    let lines = guides::grid(&deck, &fine, None).unwrap();
    assert!(lines.baselines.is_empty(), "{} lines", lines.baselines.len());
    assert_eq!(lines.columns.len(), 12, "the rest of the grid is drawn still");
}

#[test]
fn a_box_moved_meets_what_else_draws_but_not_what_moves_with_it() {
    let (deck, theme) = (deck(), theme(|_| {}));
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let mut images = BundleImages::new();
    for path in deck.image_files() {
        images.register(&path, &read(&path)).unwrap();
    }
    let mut engine = Engine::new(fonts).with_images(images);
    let data = DataFiles::new();
    let req = FrameRequest {
        deck: &deck,
        theme: &theme,
        data: &data,
        state: "containers",
        t_ms: f64::INFINITY,
        format: None,
    };
    let scene = engine.at_rest(&req).unwrap();
    let boxes = scene.boxes();
    let rect = |node: &str| boxes.iter().find(|b| b.node == node).unwrap().rect;
    let around = guides::around(&boxes, scene.canvas, &["card"]);
    // The frame and all it holds move together: none of them is met.
    for held in ["card", "card-photo", "card-tag", "card-tag-label"] {
        assert!(!around.contains(&rect(held)), "`{held}` moves with the card");
    }
    // What else draws is, and the canvas, last.
    assert!(around.contains(&rect("board-photo")) && around.contains(&rect("stat-a-figure")));
    assert_eq!(around.last(), Some(&[0.0, 0.0, 1920.0, 1080.0]));
    // A stack with no panel draws nothing, and is not met.
    assert!(!around.contains(&rect("stats")));

    // The card dragged so that its left edge is 3 units off the photo's in the grid beside it
    // goes onto it, and a guide runs down both.
    let (card, photo): (Rect, Rect) = (rect("card"), rect("board-photo"));
    let dropped = [photo[0] + 3.0, card[1] - 40.0, card[2], card[3]];
    let aligned = guides::align(dropped, card, &around, 6.0);
    assert!(near(aligned[0], photo[0]), "{aligned:?} against {photo:?}");
    let lines = guides::meets(aligned, &around);
    let down = lines.iter().find(|l| l[0] == l[2] && near(l[0], photo[0])).expect("a guide down the photo's left edge");
    assert!(
        down[1] <= photo[1].min(aligned[1]) + 1e-3
            && down[3] >= (photo[1] + photo[3]).max(aligned[1] + aligned[3]) - 1e-3
    );
}

/// A layout's slots as the canvas shows them (PLAN 2.71), on the torture deck's `formats`
/// case: its four slots on the grid of the format shown, each as the theme writes it; in 9:16
/// the halves stack, written by the format itself, and the rest are the layout's own.
#[test]
fn a_layout_is_its_slots_in_the_format_shown() {
    let (deck, theme) = (deck(), theme(|_| {}));
    let (name, wide) = guides::layout(&deck, &theme, "formats", None).unwrap().unwrap();
    assert_eq!(name, "specimen");
    let names: Vec<&str> = wide.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["case", "main", "left", "right"]);
    let grid = guides::grid(&deck, &theme, None).unwrap();
    let slot = |slots: &[guides::SlotBox], n: &str| slots.iter().find(|s| s.name == n).unwrap().clone();
    // `left` is columns 1–6 and rows 2–8 of the wide grid.
    let left = slot(&wide, "left");
    assert_eq!(left.col, Some(json!([1, 6])));
    assert!(
        near(left.rect[0], grid.columns[0][0]) && near(left.rect[0] + left.rect[2], grid.columns[5][1]),
        "{left:?}"
    );
    assert!(near(left.rect[1], grid.rows[1][0]) && near(left.rect[1] + left.rect[3], grid.rows[7][1]), "{left:?}");
    assert!(wide.iter().all(|s| !s.own), "on the deck's own canvas, nothing is a format's own");
    // In 9:16: `left` spans the grid's width on rows 2–4, written by the format.
    let (_, tall) = guides::layout(&deck, &theme, "formats", Some("9:16")).unwrap().unwrap();
    let tall_grid = guides::grid(&deck, &theme, Some("9:16")).unwrap();
    let left = slot(&tall, "left");
    assert!(left.own && !slot(&tall, "main").own);
    assert_eq!((left.col, left.row), (Some(json!([1, 12])), Some(json!([2, 4]))));
    assert!(near(left.rect[2], tall_grid.columns[11][1] - tall_grid.columns[0][0]));
    // A state with no layout has none; one that is not there is an error.
    assert!(guides::layout(&deck, &theme, "nowhere", None).is_err());
}
