//! What a data source's rows draw, and the rows behind what is drawn (PLAN 2.64), with the
//! torture deck's fonts and theme: a bar chart of an aggregate beside a table of a filter, both
//! reading one source. A point on a bar names the rows its group was made from, and a point on
//! a table's row the row it shows; the rows of the source a row is made from draw in every
//! chart and table that reads it.

use scaena_core::Deck;
use scaena_core::displaylist::Rect;
use scaena_engine::charts::ChartLayout;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::marks::DataMark;
use scaena_engine::sample::{Content, Scene};
use scaena_engine::tables::TableLayout;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

/// The torture deck's fonts, a source of five sales, and in state `s` each of `nodes`.
fn deck(nodes: Value) -> Deck {
    let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    d["meta"] = json!({ "lang": "en-US" });
    d["data"] = json!({ "q": {
        "source": { "inline": [
            { "region": "NA", "year": 2024, "revenue": 10 },
            { "region": "EU", "year": 2024, "revenue": 7 },
            { "region": "NA", "year": 2025, "revenue": 14 },
            { "region": "EU", "year": 2025, "revenue": 5 },
            { "region": "APAC", "year": 2025, "revenue": 9 }
        ] },
        "schema": { "region": "string", "year": "number", "revenue": "number" }
    } });
    let props: serde_json::Map<String, Value> =
        nodes.as_object().unwrap().keys().map(|k| (k.clone(), json!({}))).collect();
    d["nodes"] = nodes;
    d["states"] = json!([{ "id": "s", "layout": "specimen", "props": props }]);
    serde_json::from_value(d).unwrap()
}

/// Revenue by region, summed, on the left; the 2025 sales, on the right.
fn sales() -> Deck {
    deck(json!({
        "c": { "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "region" }, "y": { "field": "total" },
               "dataTransform": [{ "aggregate": { "total": "sum(revenue)" }, "groupby": ["region"] }],
               "at": { "col": [1, 6], "row": [2, 8] } },
        "t": { "type": "table", "data": "@q", "key": "region",
               "columns": [{ "field": "region" }, { "field": "revenue" }],
               "dataTransform": [{ "filter": "year == 2025" }],
               "at": { "col": [7, 12], "row": [2, 8] } }
    }))
}

fn at_rest(deck: &Deck) -> Scene {
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let data = DataFiles::new();
    let req = FrameRequest { deck, theme: &theme, data: &data, state: "s", t_ms: f64::INFINITY, format: None };
    Engine::new(fonts).at_rest(&req).unwrap()
}

fn chart<'a>(scene: &'a Scene, id: &str) -> (Rect, &'a ChartLayout) {
    scene
        .nodes
        .iter()
        .find_map(|n| match &n.content {
            Content::Chart { cell, chart } if n.id == id => Some((*cell, chart.as_ref())),
            _ => None,
        })
        .unwrap()
}

fn table<'a>(scene: &'a Scene, id: &str) -> (Rect, &'a TableLayout) {
    scene
        .nodes
        .iter()
        .find_map(|n| match &n.content {
            Content::Table { cell, table } if n.id == id => Some((*cell, table.as_ref())),
            _ => None,
        })
        .unwrap()
}

fn center(r: Rect) -> [f32; 2] {
    [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0]
}

fn named(m: &DataMark) -> (&str, &str, &str, &[usize]) {
    (m.node.as_str(), m.source.as_str(), m.key.as_str(), m.rows.as_slice())
}

#[test]
fn a_point_on_a_mark_names_the_rows_it_was_made_from() {
    let scene = at_rest(&sales());
    // Each bar is a region's sales, summed: North America's from rows 0 and 2, Europe's from 1
    // and 3, Asia Pacific's from 4.
    let (cell, c) = chart(&scene, "c");
    for (key, rows) in [("NA", [0, 2].as_slice()), ("EU", &[1, 3]), ("APAC", &[4])] {
        let m = c.marks.iter().find(|m| m.key == key).unwrap();
        let [x, y] = m.shape.point();
        // Inside the bar, below its top.
        let at = [cell[0] + x, cell[1] + y + 4.0];
        let found = scene.mark_at(at).unwrap_or_else(|| panic!("nothing at {at:?} for {key}"));
        assert_eq!(named(&found), ("c", "q", key, rows));
        assert!(found.outline.starts_with('M'), "{}", found.outline);
        let [x0, y0, w, h] = found.rect;
        assert!(at[0] >= x0 && at[0] <= x0 + w && at[1] >= y0 && at[1] <= y0 + h, "{at:?} in {:?}", found.rect);
    }
    // Between two bars: the chart, and no mark.
    let (a, b) = (c.marks[0].shape.point()[0], c.marks[1].shape.point()[0]);
    let between = [cell[0] + (a + b) / 2.0, cell[1] + c.plot[1] + c.plot[3] / 2.0];
    assert!(scene.hit(between).first().is_some_and(|h| h.node == "c") && scene.mark_at(between).is_none());
    // Each of the table's rows is a 2025 sale, the row it shows.
    let (cell, t) = table(&scene, "t");
    assert_eq!(t.bands.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["NA", "EU", "APAC"]);
    for ((key, band), row) in t.bands.iter().zip([2, 3, 4]) {
        let at = center([cell[0] + band[0], cell[1] + band[1], band[2], band[3]]);
        let found = scene.mark_at(at).unwrap();
        assert_eq!(named(&found), ("t", "q", key.as_str(), [row].as_slice()));
    }
    // Off any chart or table: nothing.
    assert!(scene.mark_at([1.0, 1.0]).is_none());
}

#[test]
fn a_row_marks_what_it_draws_in_every_chart_and_table() {
    let scene = at_rest(&sales());
    // Row 2 (North America, 2025) is summed into its region's bar and shown in the table.
    let marks = scene.marks_of("q", &[2]);
    assert_eq!(marks.iter().map(|m| (m.node.as_str(), m.key.as_str())).collect::<Vec<_>>(), [("c", "NA"), ("t", "NA")]);
    // Row 0 (North America, 2024) is summed too, and filtered out of the table.
    let marks = scene.marks_of("q", &[0]);
    assert_eq!(marks.iter().map(|m| (m.node.as_str(), m.key.as_str())).collect::<Vec<_>>(), [("c", "NA")]);
    // Several rows: what any of them draws, in paint order.
    let keys = |rows: &[usize]| {
        scene.marks_of("q", rows).into_iter().map(|m| format!("{}:{}", m.node, m.key)).collect::<Vec<_>>()
    };
    assert_eq!(keys(&[1, 4]), ["c:EU", "c:APAC", "t:APAC"]);
    // A row nothing draws, or another source: nothing.
    assert!(keys(&[9]).is_empty() && scene.marks_of("other", &[0]).is_empty());
    // A mark found at a point is the mark its rows draw.
    let at = scene.marks_of("q", &[4]).into_iter().find(|m| m.node == "c").unwrap();
    let found = scene.mark_at(center(at.rect)).unwrap();
    assert_eq!(found, at);
}

#[test]
fn a_point_finds_a_slice_a_line_and_an_area() {
    let kinds = |kind: &str| {
        deck(json!({ "c": { "type": "chart", "kind": kind, "data": "@q", "x": { "field": "year", "type": "ordinal" },
                            "y": { "field": "revenue" }, "series": { "field": "region" },
                            "at": { "col": [1, 12], "row": [2, 8] } } }))
    };
    // A donut by region: each slice is its region's rows.
    let donut = deck(json!({ "c": { "type": "chart", "kind": "donut", "data": "@q", "x": { "field": "region" },
                                    "y": { "field": "total" },
                                    "dataTransform": [{ "aggregate": { "total": "sum(revenue)" }, "groupby": ["region"] }],
                                    "at": { "col": [1, 12], "row": [2, 8] } } }));
    let scene = at_rest(&donut);
    let (cell, c) = chart(&scene, "c");
    for m in &c.marks {
        let scaena_engine::charts::Shape::Arc { cx, cy, inner, outer, start, end } = m.shape else { panic!() };
        // Its middle: halfway round and halfway out.
        let (turn, r) = ((start + end) / 2.0 * std::f32::consts::TAU, (inner + outer) / 2.0);
        let at = [cell[0] + cx + r * turn.sin(), cell[1] + cy - r * turn.cos()];
        assert_eq!(scene.mark_at(at).map(|f| f.key), Some(m.key.clone()), "slice {} at {at:?}", m.key);
    }
    // A line: a point it runs through, within reach, names its row; between points, nothing.
    let scene = at_rest(&kinds("line"));
    let (cell, c) = chart(&scene, "c");
    let m = c.marks.iter().find(|m| m.key.starts_with("2025")).unwrap();
    let [x, y] = m.shape.point();
    let found = scene.mark_at([cell[0] + x + 5.0, cell[1] + y - 5.0]).unwrap();
    assert_eq!(found.key, m.key);
    assert_eq!(found.rows.len(), 1);
    // An area: anywhere in a column's span, the nearest column across.
    let scene = at_rest(&kinds("area"));
    let (cell, c) = chart(&scene, "c");
    for m in &c.marks {
        let scaena_engine::charts::Shape::Span { x, top, base } = m.shape else { panic!() };
        if (top - base).abs() < 1.0 {
            continue;
        }
        let at = [cell[0] + x + 3.0, cell[1] + (top + base) / 2.0];
        assert_eq!(scene.mark_at(at).map(|f| f.key), Some(m.key.clone()), "span {} at {at:?}", m.key);
    }
}
