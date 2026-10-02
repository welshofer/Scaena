//! Tables through the engine (SPEC §3.3, PLAN 1.9), with the torture deck's fonts and
//! theme: columns from the data, numbers at the column's end in tabular figures, the
//! first column taking the room to spare, and rows that move by key between states.

use scaena_core::Deck;
use scaena_core::displaylist::{DisplayList, Op};
use scaena_core::validate::{BundleFiles, validate_bundle};
use scaena_engine::charts::Ctx;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::render::{Engine, FrameRequest};
use scaena_engine::tables::{self, TableLayout};
use scaena_engine::text::TextEngine;
use scaena_engine::theme::Theme;
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

fn theme() -> Theme {
    Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap()
}

fn fonts(deck: &Deck) -> BundleFonts {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    fonts
}

/// The torture deck's fonts, one inline source `q` of `rows`, and table `t` in each of
/// `states` (props by state).
fn deck(rows: Value, states: Value) -> Deck {
    let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    d["meta"] = json!({ "lang": "en-US" });
    d["data"] = json!({ "q": { "source": { "inline": rows }, "schema": { "rev": "number", "growth": "number" } } });
    d["nodes"] = json!({ "t": { "type": "table", "data": "@q", "key": "region" } });
    d["states"] = states;
    serde_json::from_value(d).unwrap()
}

fn regions() -> Value {
    json!([
        { "region": "North America", "rev": 1240.5, "growth": 0.12 },
        { "region": "Europe", "rev": 860, "growth": -0.031 },
        { "region": "Asia Pacific", "rev": 455.25, "growth": 0.27 }
    ])
}

fn layout(props: Value, size: [f32; 2]) -> Result<TableLayout, String> {
    let d = deck(regions(), json!([{ "id": "s", "layout": "specimen", "props": { "t": props } }]));
    let snap = &scaena_core::resolve_states(&d).unwrap()[0];
    let (theme, mut fonts, mut text, data) = (theme(), fonts(&d), TextEngine::new(), DataFiles::new());
    let mut cx =
        Ctx { text: &mut text, fonts: &mut fonts, theme: &theme, deck: &d, data: &data, colors: &[], lenient: false };
    tables::compile(&mut cx, &snap.nodes["t"], size).map_err(|e| e.to_string())
}

#[test]
fn columns_come_from_the_data_with_numbers_at_their_end() {
    let columns = json!([
        { "field": "region", "title": "Region" },
        { "field": "rev", "title": "Revenue", "format": "$,.0f" },
        { "field": "growth", "title": "Growth", "format": "+.1%" }
    ]);
    let t = layout(json!({ "columns": columns }), [1600.0, 600.0]).unwrap();
    let heads: Vec<&str> = t.header.iter().map(|c| c.text.text.as_str()).collect();
    assert_eq!(heads, ["Region", "Revenue", "Growth"]);
    let text = |row: &str, column: &str| {
        let c = t.cells.iter().find(|c| c.row == row && c.column == column).unwrap();
        (c.text.text.clone(), c.origin, c.anchor, c.text.width)
    };
    // Formats, with the font's minus or a hyphen-minus.
    assert_eq!(text("North America", "rev").0, "$1,241");
    assert_eq!(text("Europe", "growth").0, "-3.1%");
    // Numbers end at the column's end; text starts at its start.
    let (_, origin, anchor, width) = text("Europe", "rev");
    assert!((origin[0] + width - anchor[0]).abs() < 1e-3);
    let (_, origin, anchor, _) = text("Europe", "region");
    assert_eq!((origin[0], anchor[0]), (0.0, 0.0));
    // The last column ends at the cell's far side: the first took the spare room.
    let (_, origin, _, width) = text("Asia Pacific", "growth");
    assert!((origin[0] + width - 1600.0).abs() < 1e-3);
    // The header's rule sits under the header, and each row under the one before.
    let rule = t.rule.as_ref().unwrap();
    assert!(t.header.iter().all(|c| c.anchor[1] < rule.from[1]));
    let rows: Vec<f32> = ["North America", "Europe", "Asia Pacific"].iter().map(|r| text(r, "region").2[1]).collect();
    assert!(rule.from[1] < rows[0] && rows[0] < rows[1] && rows[1] < rows[2], "{rows:?}");
    // Every column of the data, without `columns`.
    let all = layout(json!({ "header": false }), [1600.0, 600.0]).unwrap();
    assert!(all.header.is_empty() && all.rule.is_none());
    assert_eq!(all.cells.len(), 9);
}

#[test]
fn a_table_that_does_not_fit_says_how_to_make_it() {
    let err = layout(json!({}), [200.0, 600.0]).unwrap_err();
    assert!(err.contains("show fewer columns, or give it more room"), "{err}");
    let err = layout(json!({}), [1600.0, 90.0]).unwrap_err();
    assert!(err.contains("\"limit\""), "{err}");
    let twice = json!([{ "region": "A", "rev": 1, "growth": 0 }, { "region": "A", "rev": 2, "growth": 0 }]);
    let d = deck(twice, json!([{ "id": "s", "layout": "specimen", "props": { "t": {} } }]));
    let snap = &scaena_core::resolve_states(&d).unwrap()[0];
    let (theme, mut fonts, mut text, data) = (theme(), fonts(&d), TextEngine::new(), DataFiles::new());
    let mut cx =
        Ctx { text: &mut text, fonts: &mut fonts, theme: &theme, deck: &d, data: &data, colors: &[], lenient: false };
    let err = tables::compile(&mut cx, &snap.nodes["t"], [1600.0, 600.0]).unwrap_err().to_string();
    assert!(err.contains("row key `A` repeats"), "{err}");
}

/// The torture bundle, as `scaena validate` reads it.
struct Bundle;

impl BundleFiles for Bundle {
    fn exists(&self, path: &str) -> bool {
        std::path::Path::new(BUNDLE).join(path).is_file()
    }

    fn read_text(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(format!("{BUNDLE}/{path}")).ok()
    }
}

/// The key a message names first, in its first pair of backticks.
fn first_key(message: &str) -> Option<String> {
    message.split('`').nth(1).map(String::from)
}

#[test]
fn validate_finds_the_row_key_that_compiling_refuses() {
    // Region `A` twice, with one `rev` and two `growth`s.
    let rows = json!([{ "region": "A", "rev": 1, "growth": 0 }, { "region": "A", "rev": 1, "growth": 0.5 }]);
    for (props, repeats) in [
        (json!({}), Some("A")),
        (json!({ "key": "growth" }), None),
        // Without `key`, the first column listed, else the data's.
        (json!({ "key": null, "columns": [{ "field": "rev" }, { "field": "region" }] }), Some("1")),
        (json!({ "key": null, "columns": [{ "field": "growth" }, { "field": "region" }] }), None),
        (json!({ "key": null }), Some("A")),
    ] {
        let d = deck(rows.clone(), json!([{ "id": "s", "layout": "specimen", "props": { "t": props } }]));
        let snap = &scaena_core::resolve_states(&d).unwrap()[0];
        let (theme, mut fonts, mut text, data) = (theme(), fonts(&d), TextEngine::new(), DataFiles::new());
        let mut cx = Ctx {
            text: &mut text,
            fonts: &mut fonts,
            theme: &theme,
            deck: &d,
            data: &data,
            colors: &[],
            lenient: false,
        };
        let refused = tables::compile(&mut cx, &snap.nodes["t"], [1600.0, 600.0]).err().map(|e| {
            let e = e.to_string();
            assert!(e.contains("repeats; table keys must be unique"), "{props}: {e}");
            first_key(&e).unwrap()
        });
        assert_eq!(refused.as_deref(), repeats, "compiling {props}");
        let found = validate_bundle(&d.to_json().unwrap(), &Bundle).unwrap();
        let repeat = found.iter().find(|f| f.code == "E103" && f.message.contains(" repeat"));
        assert_eq!(repeat.and_then(|f| first_key(&f.message)).as_deref(), repeats, "validating {props}");
    }
}

/// Each cell of table `t` in `dl`: the glyphs it draws and where its text box stands.
fn cells(dl: &DisplayList) -> Vec<(Vec<u32>, [f32; 2], f32)> {
    let ops = dl
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Layer { node: Some(n), ops, .. } if n == "t" => Some(ops),
            _ => None,
        })
        .expect("a `t` layer");
    let mut out = Vec::new();
    for op in ops {
        if let Op::Layer { transform, opacity, ops, .. } = op {
            let glyphs: Vec<u32> = ops
                .iter()
                .flat_map(|g| match g {
                    Op::Glyphs { glyphs, .. } => glyphs.iter().map(|g| g.id).collect(),
                    _ => Vec::new(),
                })
                .collect();
            out.push((glyphs, [transform[4], transform[5]], *opacity));
        }
    }
    out
}

#[test]
fn rows_move_by_key_when_the_order_changes() {
    // By revenue, then by growth: Asia Pacific rises from last to first.
    let states = json!([
        { "id": "by-rev", "layout": "specimen",
          "props": { "t": { "dataTransform": [{ "sort": "-rev" }], "columns": [{ "field": "region" }, { "field": "rev" }] } } },
        { "id": "by-growth", "layout": "specimen", "transition": "standard",
          "props": { "t": { "dataTransform": [{ "sort": "-growth" }] } } }
    ]);
    let d = deck(regions(), states);
    let theme = theme();
    let mut engine = Engine::new(fonts(&d));
    let data = DataFiles::new();
    let mut frame = |state: &str, t_ms: f64| {
        let req = FrameRequest { deck: &d, theme: &theme, data: &data, state, t_ms, format: None };
        engine.frame(&req).unwrap().display_list
    };
    let (before, after) = (cells(&frame("by-rev", f64::INFINITY)), cells(&frame("by-growth", f64::INFINITY)));
    let mid = cells(&frame("by-growth", 210.0));
    // Asia Pacific's name: last row before, first after, between them mid-way, solid.
    let asia = |cells: &[(Vec<u32>, [f32; 2], f32)]| {
        let name = &before.iter().max_by(|a, b| a.1[1].total_cmp(&b.1[1])).unwrap().0;
        cells.iter().filter(|c| c.0 == *name).map(|c| (c.1[1], c.2)).collect::<Vec<_>>()
    };
    let (from, to, now) = (asia(&before)[0].0, asia(&after)[0].0, asia(&mid));
    assert!(to < from, "Asia Pacific moves up: {from} → {to}");
    assert_eq!(now.len(), 1, "one Asia Pacific, moving, not two fading: {now:?}");
    assert!(to < now[0].0 && now[0].0 < from && now[0].1 == 1.0, "{now:?}");
}
