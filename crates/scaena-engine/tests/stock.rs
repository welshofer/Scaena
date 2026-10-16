//! A price series kept in the deck (ADR-0030, PLAN O.3a), on `docs/examples/stock.deck.json`: a lit
//! line whose credit at its foot says how far its prices run, in the exchange's own time; and a
//! highlight that names nothing, which lights every mark as one listing every x does.

use scaena_core::Deck;
use scaena_core::displaylist::{DisplayList, Op};
use scaena_core::fetched::FOOT_CELL;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};

const EXAMPLES: &str = "../../docs/examples";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{EXAMPLES}/{path}")).unwrap()
}

fn setup() -> (Deck, Theme, DataFiles, Engine) {
    let deck: Deck = serde_json::from_slice(&read("stock.deck.json")).unwrap();
    let theme = Theme::from_json(&String::from_utf8(read("themes/dusk.theme.json")).unwrap()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let mut data = DataFiles::new();
    data.insert("data/acme.csv", read("data/acme.csv"));
    (deck, theme, data, Engine::new(fonts))
}

fn frame(engine: &mut Engine, deck: &Deck, theme: &Theme, data: &DataFiles) -> DisplayList {
    let req = FrameRequest { deck, theme, data, state: "three-months", t_ms: f64::INFINITY, format: None };
    engine.frame(&req).unwrap().display_list
}

#[test]
fn a_price_charts_credit_says_how_far_its_prices_run() {
    let (deck, theme, data, mut engine) = setup();
    let dl = frame(&mut engine, &deck, &theme, &data);
    let foot = dl
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Layer { node: Some(n), ops, .. } if n == "prices" => Some(ops),
            _ => None,
        })
        .unwrap()
        .iter()
        .find_map(|op| match op {
            Op::Layer { cell: Some(c), ops, .. } if *c == FOOT_CELL => Some(ops),
            _ => None,
        })
        .expect("the credit's own layer");
    assert_eq!(scaena_core::reading::drawn(foot), "Data provided by Twelve Data · prices to Oct 15, 2026");
}

#[test]
fn a_highlight_that_names_nothing_lights_every_mark_as_listing_every_x_does() {
    let (deck, theme, data, mut engine) = setup();
    let none = frame(&mut engine, &deck, &theme, &data);
    let csv = String::from_utf8(read("data/acme.csv")).unwrap();
    let every: Vec<&str> = csv.lines().skip(1).map(|l| l.split(',').next().unwrap()).collect();
    let mut listed = deck.clone();
    let mut chart = serde_json::to_value(&listed.nodes["prices"]).unwrap();
    chart["annotations"] = serde_json::json!([{ "kind": "highlight", "at": { "x": every } }]);
    listed.nodes.insert("prices".into(), serde_json::from_value(chart).unwrap());
    let all = frame(&mut engine, &listed, &theme, &data);
    assert_eq!(none.digest().unwrap(), all.digest().unwrap());
    let signal = theme.color("accent").unwrap();
    let lit = none.ops.iter().any(|op| match op {
        Op::Layer { node: Some(n), ops, .. } if n == "prices" => format!("{ops:?}").contains(&format!("{signal:?}")),
        _ => false,
    });
    assert!(lit, "the line in the signal color");
}
