//! Charts through the engine (SPEC §3.7, PLAN 1.9), with the torture deck's fonts and
//! theme: labels print through the encodings' formats in the deck's language
//! (`docs/spec/format.md`), and count through them.

use scaena_core::Deck;
use scaena_engine::charts::{self, ChartLayout, Ctx};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::text::TextEngine;
use scaena_engine::theme::Theme;
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

/// The torture deck's fonts and theme, `lang`, one inline source `rows` with `schema`
/// (and `parse`), and one chart `chart` over it.
fn deck(lang: &str, rows: Value, schema: Value, parse: Value, chart: Value) -> Deck {
    let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    d["meta"] = json!({ "lang": lang });
    d["data"] = json!({ "q": { "source": { "inline": rows }, "schema": schema } });
    if !parse.is_null() {
        d["data"]["q"]["parse"] = parse;
    }
    d["nodes"] = json!({ "c": chart });
    d["states"] = json!([{ "id": "s", "layout": "specimen", "props": { "c": {} } }]);
    serde_json::from_value(d).unwrap()
}

fn compile(deck: &Deck) -> ChartLayout {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let (mut text, data) = (TextEngine::new(), DataFiles::new());
    let mut cx = Ctx { text: &mut text, fonts: &mut fonts, theme: &theme, deck, data: &data };
    charts::compile(&mut cx, &deck.nodes["c"].props, [1600.0, 700.0]).unwrap()
}

fn texts(labels: &[charts::Label]) -> Vec<String> {
    labels.iter().map(|l| l.text.text.clone()).collect()
}

#[test]
fn value_labels_print_in_the_encodings_format_and_the_decks_language() {
    let rows = json!([{ "k": "a", "v": 1234.5 }, { "k": "b", "v": -20 }, { "k": "c", "v": 0.004 }]);
    let chart = |format: &str| {
        json!({ "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "k" },
                "y": { "field": "v", "format": format }, "labels": { "show": "all" } })
    };
    let schema = json!({ "v": "number" });
    let en = compile(&deck("en-US", rows.clone(), schema.clone(), Value::Null, chart("$,.0f")));
    // These fonts have no U+2212, so the minus is a hyphen-minus.
    assert_eq!(texts(&en.labels), ["$1,235", "-$20", "$0"]);
    let de = compile(&deck("de-DE", rows, schema, Value::Null, chart(",.1f")));
    assert_eq!(texts(&de.labels), ["1.234,5", "-20,0", "0,0"]);
    // Counting labels count in the format.
    let numerals = en.numerals.as_ref().unwrap();
    assert_eq!(numerals.count(0.0, 1234.5, 0.5), "$617");
    assert_eq!(numerals.count(-20.0, 20.0, 0.25), "-$10");
    assert!(numerals.compose(&numerals.count(0.0, 1234.5, 0.5)).is_some(), "$ and , have figures");
}

#[test]
fn without_a_format_labels_print_as_d3_does_and_count_at_the_places_either_end_shows() {
    let rows = json!([{ "k": "a", "v": 0.30000000000000004 }, { "k": "b", "v": 12 }]);
    let chart = json!({ "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "k" }, "y": { "field": "v" },
                        "labels": { "show": "all" } });
    let layout = compile(&deck("en-US", rows, json!({ "v": "number" }), Value::Null, chart));
    assert_eq!(texts(&layout.labels), ["0.3", "12"]);
    let numerals = layout.numerals.as_ref().unwrap();
    assert_eq!(numerals.count(0.25, 12.0, 0.5), "6.13", "two places, as 0.25 shows; 6.125 rounds up");
    assert_eq!(numerals.count(7.0, 12.0, 0.5), "10", "9.5 rounds away from zero");
}

#[test]
fn date_categories_print_through_the_x_format() {
    let rows = json!([{ "m": "Jan 2025", "v": 3 }, { "m": "Apr 2025", "v": 5 }]);
    let chart = json!({ "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "m", "format": "Q%q ’%y" },
                        "y": { "field": "v" } });
    let schema = json!({ "m": "date", "v": "number" });
    let layout = compile(&deck("en-US", rows.clone(), schema.clone(), json!({ "m": "%b %Y" }), chart));
    assert_eq!(texts(&layout.ticks), ["Q1 ’25", "Q2 ’25"]);
    // A date column with no format prints ISO 8601.
    let plain = json!({ "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "m" }, "y": { "field": "v" } });
    let layout = compile(&deck("en-US", rows, schema, json!({ "m": "%b %Y" }), plain));
    assert_eq!(texts(&layout.ticks), ["2025-01-01", "2025-04-01"]);
}
