//! The credit at the foot of a chart or a table whose data was fetched from a service (ADR-0030,
//! PLAN O.2a), on `docs/examples/weather.deck.json`: the place and the forecast's day there, then
//! the service's credit, each run linked where it links, in a layer of its own below the rows,
//! never under 12 pt at presentation size, and "edited" once its rows were edited.

use scaena_core::Deck;
use scaena_core::displaylist::{LinkTarget, Op};
use scaena_core::fetched::FOOT_CELL;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::sample::Content;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};

const EXAMPLES: &str = "../../docs/examples";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{EXAMPLES}/{path}")).unwrap()
}

fn setup() -> (Deck, Theme, Engine) {
    let deck: Deck = serde_json::from_slice(&read("weather.deck.json")).unwrap();
    let theme = Theme::from_json(&String::from_utf8(read("themes/dusk.theme.json")).unwrap()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    (deck, theme, Engine::new(fonts))
}

/// The words a layer's glyphs draw, run by run, as the reader reads them.
fn words(ops: &[Op]) -> String {
    scaena_core::reading::drawn(ops)
}

#[test]
fn a_tables_fetched_data_is_credited_at_its_foot() {
    let (deck, theme, mut engine) = setup();
    let data = DataFiles::new();
    let req =
        FrameRequest { deck: &deck, theme: &theme, data: &data, state: "week", t_ms: f64::INFINITY, format: None };
    let scene = engine.at_rest(&req).unwrap();
    let (cell, table) = scene
        .nodes
        .iter()
        .find_map(|n| match &n.content {
            Content::Table { cell, table } if n.id == "week" => Some((*cell, table)),
            _ => None,
        })
        .expect("the table");
    let foot = table.credit.as_ref().expect("a credit at its foot");
    // At the foot of its box, under every row, at 24 cu or more on a 1080 canvas.
    assert!((foot.origin[1] + foot.text.height - cell[3]).abs() < 0.5, "{:?} in {cell:?}", foot.origin);
    let lowest = table.cells.iter().map(|c| c.origin[1] + c.text.height).fold(0.0f32, f32::max);
    assert!(lowest <= foot.origin[1], "the rows stand above the credit: {lowest} > {}", foot.origin[1]);
    assert!(foot.text.runs.iter().all(|r| r.size >= 24.0 - 1e-3));

    let dl = engine.frame(&req).unwrap().display_list;
    let layer = dl
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Layer { node: Some(n), ops, .. } if n == "week" => Some(ops),
            _ => None,
        })
        .unwrap()
        .iter()
        .find_map(|op| match op {
            Op::Layer { cell: Some(c), ops, .. } if *c == FOOT_CELL => Some(ops),
            _ => None,
        })
        .expect("the credit's own layer");
    assert_eq!(
        words(layer),
        "Boston, Massachusetts · Oct 15, 2026 · Weather data by Open-Meteo.com · CC BY 4.0 · location data by GeoNames"
    );
    let hrefs: Vec<LinkTarget> = dl.links().into_iter().map(|l| l.target).collect();
    for href in ["https://open-meteo.com/", "https://creativecommons.org/licenses/by/4.0/"] {
        assert!(hrefs.contains(&LinkTarget::Href(href.into())), "{href}: {hrefs:?}");
    }

    // Its rows edited since they came: the credit says so, as CC BY asks.
    let mut edited = deck.clone();
    edited.data.get_mut("boston").unwrap().fetched.as_mut().unwrap().edited = true;
    let req = FrameRequest { deck: &edited, ..req };
    let scene = engine.at_rest(&req).unwrap();
    let said: String = scene
        .nodes
        .iter()
        .find_map(|n| match &n.content {
            Content::Table { table, .. } => table.credit.clone(),
            _ => None,
        })
        .map(|f| f.text.text.clone())
        .unwrap();
    assert!(said.ends_with(" · edited"), "{said}");
}

#[test]
fn data_fetched_from_no_service_draws_no_credit() {
    let (mut deck, theme, mut engine) = setup();
    deck.data.get_mut("boston").unwrap().fetched = None;
    let data = DataFiles::new();
    let req =
        FrameRequest { deck: &deck, theme: &theme, data: &data, state: "week", t_ms: f64::INFINITY, format: None };
    let scene = engine.at_rest(&req).unwrap();
    assert!(scene.nodes.iter().all(|n| !matches!(&n.content, Content::Table { table, .. } if table.credit.is_some())));
}
