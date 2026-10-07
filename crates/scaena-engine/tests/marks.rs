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

/// Sales by year, a bar for each region, annotated: a rule at 12, a band over 2024, a callout
/// on North America's 2025, and a highlight of Asia Pacific's.
fn annotated() -> Deck {
    deck(json!({ "c": { "type": "chart", "kind": "bar", "data": "@q",
        "x": { "field": "year", "type": "ordinal" }, "y": { "field": "revenue" }, "series": { "field": "region" },
        "annotations": [
            { "kind": "rule", "at": { "y": 12 }, "text": "Target" },
            { "kind": "band", "at": { "x": [2024, 2024] }, "text": "Last year" },
            { "kind": "callout", "at": { "x": 2025, "series": "NA" }, "text": "Best" },
            { "kind": "highlight", "at": { "x": 2025, "series": "APAC" } }
        ],
        "at": { "col": [1, 12], "row": [2, 8] } } }))
}

#[test]
fn a_mark_says_where_its_annotations_stand() {
    use scaena_core::model::values::{AnnotationAt, Place, Scalar};
    let scene = at_rest(&annotated());
    let (cell, c) = chart(&scene, "c");
    let found = |key: &str| {
        let m = c.marks.iter().find(|m| m.key == key).unwrap_or_else(|| panic!("no mark {key}"));
        let [x, y] = m.shape.point();
        scene.mark_at([cell[0] + x, cell[1] + y + 4.0]).unwrap().notes.unwrap()
    };
    let one = |v: Scalar| Some(Place::One(v));
    let at = |x: f64, series: &str| AnnotationAt {
        x: one(Scalar::Number(x)),
        y: None,
        series: one(Scalar::Text(series.into())),
    };
    // North America's 2025 sales: 14, the one mark of its year and series.
    let na = found("2025\u{1f}NA");
    assert_eq!((&na.x, na.value, na.series.as_deref(), na.axes), (&Scalar::Number(2025.0), 14.0, Some("NA"), true));
    assert_eq!((&na.callout, &na.highlight), (&at(2025.0, "NA"), &at(2025.0, "NA")));
    assert!(na.highlighted.is_empty());
    // Asia Pacific's 2025 is the highlight's, the fourth annotation.
    assert_eq!(found("2025\u{1f}APAC").highlighted, [3]);

    // Without series, a callout stands at a mark's x; where several marks share it, at its
    // value too.
    let scatter = deck(
        json!({ "c": { "type": "chart", "kind": "scatter", "data": "@q", "x": { "field": "year", "type": "quantitative" },
                                      "y": { "field": "revenue" }, "key": "id",
                                      "dataTransform": [{ "derive": { "id": "revenue * 1000 + year" } }],
                                      "at": { "col": [1, 12], "row": [2, 8] } } }),
    );
    let scene = at_rest(&scatter);
    let (cell, c) = chart(&scene, "c");
    let m = c
        .marks
        .iter()
        .find(|m| m.shape.point()[1] == c.marks.iter().map(|m| m.shape.point()[1]).fold(f32::INFINITY, f32::min))
        .unwrap();
    let [x, y] = m.shape.point();
    let top = scene.mark_at([cell[0] + x, cell[1] + y]).unwrap().notes.unwrap();
    assert_eq!(
        top.callout,
        AnnotationAt { x: one(Scalar::Number(2025.0)), y: one(Scalar::Number(14.0)), series: None }
    );

    // A donut's slice takes a highlight of its own, and nothing that needs axes.
    let donut = deck(json!({ "c": { "type": "chart", "kind": "donut", "data": "@q", "x": { "field": "region" },
                                    "y": { "field": "total" },
                                    "dataTransform": [{ "aggregate": { "total": "sum(revenue)" }, "groupby": ["region"] }],
                                    "at": { "col": [1, 12], "row": [2, 8] } } }));
    let scene = at_rest(&donut);
    let mark = scene.marks_of("q", &[4]).into_iter().next().unwrap();
    let notes = mark.notes.unwrap();
    assert!(!notes.axes);
    assert_eq!(notes.highlight, AnnotationAt { x: one(Scalar::Text("APAC".into())), y: None, series: None });
    assert!(scene.callout_at("c", center(mark.rect)).is_none(), "a donut takes no callouts");
}

#[test]
fn an_annotation_is_found_where_it_is_drawn() {
    use scaena_core::model::values::AnnotationKind;
    let scene = at_rest(&annotated());
    let (cell, c) = chart(&scene, "c");
    let note = |index: usize| c.notes.iter().find(|n| n.index == index).unwrap_or_else(|| panic!("no note {index}"));
    let on = |p: [f32; 2]| [cell[0] + p[0], cell[1] + p[1]];
    let text = |index: usize| {
        let l = note(index).label.as_ref().unwrap();
        on([l.origin[0] + l.text.width / 2.0, l.origin[1] + l.text.height / 2.0])
    };
    let named = |p: [f32; 2]| scene.note_at(p).map(|n| (n.node, n.index, n.kind));
    // Each one's text names it.
    assert_eq!(named(text(0)), Some(("c".into(), 0, AnnotationKind::Rule)));
    assert_eq!(named(text(1)), Some(("c".into(), 1, AnnotationKind::Band)));
    assert_eq!(named(text(2)), Some(("c".into(), 2, AnnotationKind::Callout)));
    // The rule, near its end, where no text is.
    let r = note(0).rule.as_ref().unwrap();
    let near_end = on([r.from[0] + 0.9 * (r.to[0] - r.from[0]), r.from[1] + 1.0]);
    assert_eq!(named(near_end).map(|n| n.1), Some(0));
    // The callout's leader.
    let leader = note(2).rule.as_ref().unwrap();
    assert_eq!(named(on([leader.from[0], (leader.from[1] + leader.to[1]) / 2.0])).map(|n| n.1), Some(2));
    // Inside the band where no mark is; on a mark in it, the mark.
    let ([bx, by, _, bh], _) = note(1).band.unwrap();
    let inside = on([bx + 1.0, by + bh - 1.0]);
    assert!(scene.mark_at(inside).is_none());
    assert_eq!(named(inside).map(|n| n.1), Some(1));
    let bar = c.marks.iter().find(|m| m.key == "2024\u{1f}EU").unwrap();
    let [x, y] = bar.shape.point();
    assert!(named(on([x, y + 10.0])).is_none(), "a mark in a band is the mark's");
    // Its outline holds the point that found it.
    let found = scene.note_at(text(2)).unwrap();
    let [x0, y0, w, h] = found.rect;
    assert!(found.outline.starts_with('M') && w > 0.0 && h > 0.0);
    assert!(text(2)[0] >= x0 && text(2)[0] <= x0 + w && text(2)[1] >= y0 && text(2)[1] <= y0 + h);
    // Off any annotation, and off the chart: nothing.
    assert!(scene.note_at([1.0, 1.0]).is_none());
}

#[test]
fn a_callout_dropped_stands_on_the_mark_there_or_at_the_value_there() {
    use scaena_core::model::values::{AnnotationAt, Place, Scalar};
    let scene = at_rest(&annotated());
    let (cell, c) = chart(&scene, "c");
    let one = |v: Scalar| Some(Place::One(v));
    // On Europe's 2024 bar: on it.
    let bar = c.marks.iter().find(|m| m.key == "2024\u{1f}EU").unwrap();
    let [x, y] = bar.shape.point();
    assert_eq!(
        scene.callout_at("c", [cell[0] + x, cell[1] + y + 4.0]),
        Some(AnnotationAt { x: one(Scalar::Number(2024.0)), y: None, series: one(Scalar::Text("EU".into())) })
    );
    // Over 2024's bars, near the plot's top: at 2024, and the value there, to a tenth of the
    // value axis's step.
    let [left, top, _, _] = c.plot;
    let p = [left + 2.0, top + 3.0];
    let at = scene.callout_at("c", [cell[0] + p[0], cell[1] + p[1]]).unwrap();
    assert_eq!((&at.x, &at.series), (&one(Scalar::Number(2024.0)), &None));
    let Some(Place::One(Scalar::Number(v))) = at.y else { panic!("{at:?}") };
    let [d0, d1] = c.y_scale.domain;
    let unit = scaena_engine::scale::tick_step(d0, d1, 5) / 10.0;
    assert!((v - c.y_scale.invert(p[1])).abs() <= unit / 2.0 + 1e-9, "{v} for {}", c.y_scale.invert(p[1]));
    assert!(((v / unit).round() * unit - v).abs() < 1e-9 && v <= d1.max(d0), "{v} on a step of {unit}");
    // Not a chart: nowhere.
    assert!(scene.callout_at("nothing", p).is_none());
}

#[test]
fn a_callout_dropped_on_bars_across_reads_its_category_down_and_its_value_across() {
    use scaena_core::model::values::{AnnotationAt, Place, Scalar};
    // The annotated chart with its bars across (PLAN 1.29).
    let mut d = serde_json::to_value(annotated()).unwrap();
    d["nodes"]["c"]["orient"] = json!("horizontal");
    let scene = at_rest(&serde_json::from_value(d).unwrap());
    let (cell, c) = chart(&scene, "c");
    assert!(c.horizontal);
    let one = |v: Scalar| Some(Place::One(v));
    // On Europe's 2024 bar, at its middle: on it.
    let scaena_engine::charts::Shape::Bar(bar) = c.marks.iter().find(|m| m.key == "2024\u{1f}EU").unwrap().shape else {
        panic!("bars")
    };
    let middle = [cell[0] + bar.x + 0.5 * bar.w, cell[1] + bar.y + 0.5 * bar.h];
    assert_eq!(
        scene.callout_at("c", middle),
        Some(AnnotationAt { x: one(Scalar::Number(2024.0)), y: None, series: one(Scalar::Text("EU".into())) })
    );
    // Off the bars, beside the plot's end in 2024's band: at 2024, and the value across.
    let [left, _, width, _] = c.plot;
    let band = c.categories.iter().find(|(x, _)| *x == Scalar::Number(2024.0)).unwrap().1;
    let p = [left + width - 2.0, band[0] + 1.0];
    let at = scene.callout_at("c", [cell[0] + p[0], cell[1] + p[1]]).unwrap();
    assert_eq!((&at.x, &at.series), (&one(Scalar::Number(2024.0)), &None));
    let Some(Place::One(Scalar::Number(v))) = at.y else { panic!("{at:?}") };
    let unit = scaena_engine::scale::tick_step(c.y_scale.domain[0], c.y_scale.domain[1], 5) / 10.0;
    assert!((v - c.y_scale.invert(p[0])).abs() <= unit / 2.0 + 1e-9, "{v} for {}", c.y_scale.invert(p[0]));
}

#[test]
fn a_ranges_interval_and_its_value_are_its_points_to_click_and_no_marks_of_their_own() {
    // A poll's estimates with their intervals (PLAN 1.30).
    let mut d = serde_json::to_value(deck(json!({
        "c": { "type": "chart", "kind": "range", "data": "@p", "x": { "field": "option" }, "y": { "field": "est" },
               "interval": { "low": "lo", "high": "hi" }, "at": { "col": [1, 12], "row": [2, 8] } }
    })))
    .unwrap();
    d["data"]["p"] = json!({
        "source": { "inline": [
            { "option": "A", "est": 42, "lo": 38, "hi": 46 },
            { "option": "B", "est": 35, "lo": 31, "hi": 39 }
        ] },
        "schema": { "est": "number", "lo": "number", "hi": "number" }
    });
    let scene = at_rest(&serde_json::from_value(d).unwrap());
    let (cell, c) = chart(&scene, "c");
    let point = |key: &str| {
        let [x, y] = c.marks.iter().find(|m| m.key == key).unwrap().shape.point();
        [cell[0] + x, cell[1] + y]
    };
    // The interval's top, and the value over it: each A's, the row it was made from.
    let value = c.labels.iter().find(|l| l.key == "A\u{1f}high").unwrap();
    let baseline = value.origin[1] + value.text.lines[0].baseline;
    let on_value = [cell[0] + value.origin[0] + 0.5 * value.text.width, cell[1] + baseline - 4.0];
    for at in [point("A\u{1f}high"), on_value] {
        let m = scene.mark_at(at).unwrap_or_else(|| panic!("a mark at {at:?}"));
        assert_eq!(named(&m), ("c", "p", "A", [0].as_slice()), "at {at:?}");
    }
    // The keys reach the values alone.
    let (marks, _) = scene.marks_in("c").unwrap();
    assert_eq!(marks.iter().map(|m| m.key.as_str()).collect::<Vec<_>>(), ["A", "B"]);
    assert_eq!(scene.marks_of("p", &[1]).iter().map(|m| m.key.as_str()).collect::<Vec<_>>(), ["B"]);
}

#[test]
fn a_point_in_a_panel_of_small_multiples_names_that_panels_mark_and_rows() {
    // Revenue by year, a panel for each region (PLAN 1.31): the bar under the point is the
    // panel's, made from that region's row.
    let scene = at_rest(&deck(json!({
        "c": { "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "year", "type": "ordinal" },
               "y": { "field": "revenue" }, "facet": { "field": "region" }, "at": { "col": [1, 12], "row": [2, 8] } }
    })));
    let (cell, c) = chart(&scene, "c");
    assert_eq!(c.panels.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["NA", "EU", "APAC"]);
    // EU's 2025 bar: the source's row 3.
    let eu = &c.panels[1];
    let bar = eu.chart.marks.iter().find(|m| m.key == "2025").unwrap();
    let [x, y] = bar.shape.point();
    let m = scene.mark_at([cell[0] + eu.at[0] + x, cell[1] + eu.at[1] + y + 4.0]).unwrap();
    assert_eq!(named(&m), ("c", "q", "2025", [3].as_slice()));
    // The keys reach every panel's marks, panel by panel; a row is outlined in its panel.
    let (marks, _) = scene.marks_in("c").unwrap();
    assert_eq!(marks.iter().map(|m| m.rows.clone()).collect::<Vec<_>>(), [vec![0], vec![2], vec![1], vec![3], vec![4]]);
    assert_eq!(scene.marks_of("q", &[4]).len(), 1);
}
