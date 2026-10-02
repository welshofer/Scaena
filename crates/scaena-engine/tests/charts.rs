//! Charts through the engine (SPEC §3.7, PLAN 1.9), with the torture deck's fonts and
//! theme: labels print through the encodings' formats in the deck's language
//! (`docs/spec/format.md`), and count through them.

use scaena_core::Deck;
use scaena_core::validate::{BundleFiles, validate_bundle};
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

/// The torture theme, with `charts` merged into its chart styles.
fn themed(charts: Value) -> Theme {
    let mut t: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    t["charts"].as_object_mut().unwrap().extend(charts.as_object().unwrap().clone());
    Theme::from_json(&t.to_string()).unwrap()
}

fn try_compile(deck: &Deck) -> Result<ChartLayout, String> {
    try_compile_in(&themed(json!({})), deck)
}

fn try_compile_in(theme: &Theme, deck: &Deck) -> Result<ChartLayout, String> {
    compile_sized(theme, deck, [1600.0, 700.0])
}

fn compile_sized(theme: &Theme, deck: &Deck, size: [f32; 2]) -> Result<ChartLayout, String> {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let (mut text, data) = (TextEngine::new(), DataFiles::new());
    let mut cx = Ctx { text: &mut text, fonts: &mut fonts, theme, deck, data: &data, colors: &[], lenient: false };
    charts::compile(&mut cx, &deck.nodes["c"].props, size).map_err(|e| e.to_string())
}

fn compile(deck: &Deck) -> ChartLayout {
    try_compile(deck).unwrap()
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

fn bars(axes: Value, domain: Value) -> Deck {
    let rows = json!([{ "k": "a", "v": 12 }, { "k": "b", "v": 31 }, { "k": "c", "v": 7 }]);
    let chart = json!({ "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "k", "title": "Quarter" },
                        "y": { "field": "v", "format": "$,.0f", "domain": domain }, "axes": axes });
    deck("en-US", rows, json!({ "v": "number" }), Value::Null, chart)
}

#[test]
fn the_value_axis_widens_to_round_ticks_and_rules_them() {
    let layout =
        compile(&bars(json!({ "y": { "show": true, "gridlines": true, "title": "Revenue" } }), json!([0, null])));
    assert_eq!(layout.y_scale.domain, [0.0, 35.0], "31 widens to the next tick");
    let keys: Vec<&str> = layout.y_axis.iter().map(|t| t.key.as_str()).collect();
    assert_eq!(keys, ["$0", "$5", "$10", "$15", "$20", "$25", "$30", "$35"]);
    let [left, top, width, height] = layout.plot;
    // Labels right-aligned in the gutter, one space unit (8) from the plot.
    for tick in &layout.y_axis {
        let label = tick.label.as_ref().unwrap();
        assert!((label.origin[0] + label.text.width - (left - 8.0)).abs() < 1e-3, "{}", tick.key);
    }
    // A gridline at every tick but the baseline's, across the plot.
    let rules: Vec<f32> = layout.y_axis.iter().filter_map(|t| t.rule.as_ref()).map(|r| r.from[1]).collect();
    assert_eq!(rules.len(), 7);
    assert!((rules.last().unwrap() - top).abs() < 1e-3, "the top tick is the plot's top");
    assert!(layout.y_axis.iter().all(|t| t.rule.as_ref().is_none_or(|r| r.from[0] == left && r.to[0] == left + width)));
    assert_eq!(layout.base, top + height);
    // Titles: the value axis's above the plot, the category axis's under the labels.
    let titles: Vec<(&str, &str)> = layout.titles.iter().map(|t| (t.key.as_str(), t.text.text.as_str())).collect();
    assert_eq!(titles, [("y", "Revenue"), ("x", "Quarter")]);
    assert!(layout.titles[0].origin[1] + layout.titles[0].text.height < top);
    // Bands share the plot, not the gutter.
    let scaena_engine::charts::Shape::Bar(first) = layout.marks[0].shape else { panic!("bars") };
    assert!(first.x > left && (first.center_x() - (left + width / 6.0)).abs() < 1e-3);
}

#[test]
fn a_bound_the_author_sets_stays_and_gridlines_can_show_alone() {
    let layout = compile(&bars(json!({ "y": { "gridlines": true } }), json!([0, 37])));
    assert_eq!(layout.y_scale.domain, [0.0, 37.0]);
    assert!(layout.y_axis.iter().all(|t| t.label.is_none()), "no labels without show");
    assert_eq!(layout.y_axis.iter().map(|t| t.value).collect::<Vec<_>>(), [0.0, 10.0, 20.0, 30.0]);
    assert_eq!(layout.plot[0], 0.0, "no gutter");
    // Without the axis, the domain is the data's: bars reach the top of the plot.
    let bare = compile(&bars(json!({}), json!([0, null])));
    assert_eq!(bare.y_scale.domain, [0.0, 31.0]);
    assert!(bare.y_axis.is_empty() && bare.titles.len() == 1);
    let hidden = compile(&bars(json!({ "x": { "show": false } }), json!([0, null])));
    assert!(hidden.ticks.is_empty());
}

use scaena_engine::charts::Shape;

/// Three products over four quarters.
fn revenue() -> Value {
    let quarters = ["Q1", "Q2", "Q3", "Q4"];
    let products = [("Core", [12, 15, 18, 22]), ("Cloud", [6, 9, 14, 19]), ("Edge", [3, 4, 4, 7])];
    let rows: Vec<Value> = products
        .iter()
        .flat_map(|(p, v)| quarters.iter().zip(v).map(move |(q, v)| json!({ "q": q, "product": p, "rev": v })))
        .collect();
    json!(rows)
}

fn by_series(kind: &str, extra: Value) -> ChartLayout {
    compile(&series_deck(kind, extra))
}

/// A chart of `kind` over `revenue()`, by product, with `extra` props.
fn series_deck(kind: &str, extra: Value) -> Deck {
    let mut chart = json!({ "type": "chart", "kind": kind, "data": "@q", "x": { "field": "q" }, "y": { "field": "rev" },
                            "series": { "field": "product" } });
    chart.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    deck("en-US", revenue(), json!({ "rev": "number" }), Value::Null, chart)
}

/// A label's cap height: where it stands is the middle of it.
fn cap_box(l: &charts::Label) -> (f32, f32) {
    let first = &l.text.lines[0];
    let baseline = l.origin[1] + first.baseline;
    (baseline - first.cap_height.unwrap_or(first.ascent), baseline)
}

#[test]
fn lines_name_their_series_where_they_end() {
    let layout = by_series("line", json!({}));
    let names: Vec<&str> = layout.legend.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(names, ["Core", "Cloud", "Edge"]);
    // No swatches, and nothing above the plot: a column of names past the lines' ends,
    // and past the value labels over their last points, inside the chart.
    let column = layout.legend[0].label.origin[0];
    let end = |p: &str| layout.marks.iter().find(|m| m.key == format!("Q4\u{1f}{p}")).unwrap().shape;
    let last_values = layout.labels.iter().filter(|l| l.key.starts_with("Q4"));
    assert!(last_values.clone().count() == 3 && last_values.clone().all(|l| l.origin[0] + l.text.width < column));
    for e in &layout.legend {
        assert_eq!((e.label.origin[0], e.swatch.w, e.swatch.h), (column, 0.0, 0.0), "{}", e.key);
        assert!(column > end(&e.key).center_x() && column + e.label.text.width <= 1600.0 + 1e-3);
        // Level with its series' last point, the middle of its cap height there.
        let Shape::Dot { y, .. } = end(&e.key) else { panic!("a line's points are dots") };
        let (top, bottom) = cap_box(&e.label);
        assert!((0.5 * (top + bottom) - y).abs() < 0.5, "{}: {top}..{bottom} vs {y}", e.key);
        // In its series' color, the text as well as the entry.
        let path = layout.paths.iter().find(|p| p.key == e.key).unwrap();
        assert_eq!(e.color, path.color);
        assert!(e.label.text.runs.iter().all(|r| r.color == path.color), "{}", e.key);
    }
    // The plot gives up only what the names need past the last point: here they fit in
    // its half band, so it gives up nothing, and nothing beside it needs it to clip.
    assert_eq!((layout.plot[2], layout.clipped), (1600.0, false));
    // Along a continuous x whose last point is a round value, it stands on the plot's
    // side, which the names move in by their width and a space.
    let rows = json!([
        { "x": 2, "s": "A", "v": 3 }, { "x": 10, "s": "A", "v": 8 },
        { "x": 2, "s": "Bravo", "v": 5 }, { "x": 10, "s": "Bravo", "v": 2 }
    ]);
    let chart = json!({ "type": "chart", "kind": "line", "data": "@q", "x": { "field": "x", "type": "quantitative" },
                        "y": { "field": "v" }, "series": { "field": "s" } });
    let wide = compile(&deck("en-US", rows, json!({ "x": "number", "v": "number" }), Value::Null, chart));
    let [x, _, w, _] = wide.plot;
    let bravo = &wide.legend[1].label;
    assert!(w < 1600.0 && bravo.origin[0] > x + w && bravo.origin[0] + bravo.text.width <= 1600.0 + 1e-3);
    assert!(wide.clipped, "marks leaving pass under the plot's side, not over the names");
}

#[test]
fn names_that_would_touch_move_apart_in_order() {
    // A and B end a hair apart: their names stack, A's over B's, clear of each other.
    let rows = json!([
        { "q": "Q1", "s": "A", "v": 10 }, { "q": "Q2", "s": "A", "v": 21.2 },
        { "q": "Q1", "s": "B", "v": 4 }, { "q": "Q2", "s": "B", "v": 21 }
    ]);
    let chart = json!({ "type": "chart", "kind": "line", "data": "@q", "x": { "field": "q" }, "y": { "field": "v" },
                        "series": { "field": "s" } });
    let layout = compile(&deck("en-US", rows, json!({ "v": "number" }), Value::Null, chart));
    let (a, b) = (cap_box(&layout.legend[0].label), cap_box(&layout.legend[1].label));
    assert!(a.1 < b.0, "A's name ends above B's starts: {a:?} {b:?}");
}

#[test]
fn stacked_bars_name_their_series_beside_the_last_stack() {
    let layout = by_series("stackedBar", json!({}));
    for e in &layout.legend {
        let segment = bar(&layout.marks.iter().find(|m| m.key == format!("Q4\u{1f}{}", e.key)).unwrap().shape);
        assert!(e.label.origin[0] > segment.x + segment.w, "{}", e.key);
        let (top, bottom) = cap_box(&e.label);
        assert!((0.5 * (top + bottom) - (segment.y + 0.5 * segment.h)).abs() < 0.5, "{}: the segment's middle", e.key);
    }
    // Its stacks still print their totals.
    assert_eq!(texts(&layout.labels), ["21", "28", "36", "48"]);
}

#[test]
fn charts_without_ends_keep_their_legend_on_top() {
    for kind in ["bar", "dot"] {
        let layout = by_series(kind, json!({ "legend": "direct" }));
        assert!(layout.legend.iter().all(|e| e.swatch.w > 0.0 && e.swatch.y + e.swatch.h < layout.plot[1]), "{kind}");
    }
    let err = by_series_err("line", json!({ "legend": { "place": "direct", "title": "Product" } }));
    assert!(err.contains("takes no title"), "{err}");
}

#[test]
fn the_theme_places_the_legends_charts_leave_to_it() {
    let theme = themed(json!({ "legend": { "place": "top" } }));
    let top = try_compile_in(&theme, &series_deck("line", json!({}))).unwrap();
    assert!(top.legend.iter().all(|e| e.swatch.w > 0.0 && e.swatch.y + e.swatch.h < top.plot[1]));
    // `auto` is the theme's place too; a chart that names one keeps it.
    let auto = try_compile_in(&theme, &series_deck("line", json!({ "legend": "auto" }))).unwrap();
    assert_eq!(
        auto.legend.iter().map(|e| e.swatch).collect::<Vec<_>>(),
        top.legend.iter().map(|e| e.swatch).collect::<Vec<_>>()
    );
    let direct = try_compile_in(&theme, &series_deck("line", json!({ "legend": "direct" }))).unwrap();
    assert!(direct.legend.iter().all(|e| e.swatch.w == 0.0));
    // So is an object's with no place, but a titled legend `direct` would take goes on top.
    let right = themed(json!({ "legend": { "place": "right" } }));
    let titled = try_compile_in(&right, &series_deck("line", json!({ "legend": { "title": "Region" } }))).unwrap();
    assert!(titled.legend.iter().all(|e| e.swatch.w > 0.0 && e.swatch.x > titled.plot[0] + titled.plot[2]));
    let titled = by_series("line", json!({ "legend": { "title": "Region" } }));
    assert!(titled.legend.iter().all(|e| e.swatch.w > 0.0 && e.swatch.y + e.swatch.h < titled.plot[1]));
    assert!(by_series("line", json!({ "legend": {} })).legend.iter().all(|e| e.swatch.w == 0.0));
}

#[test]
fn values_print_where_the_kind_reads_them_unless_asked() {
    // Unasked: every bar's value; a line's first and last; an area's and a scatter's none,
    // read off the value axis they show instead.
    assert_eq!(by_series("bar", json!({})).labels.len(), 12);
    let lines = by_series("line", json!({}));
    assert_eq!(lines.labels.len(), 6);
    assert!(lines.y_axis.is_empty());
    let areas = by_series("area", json!({}));
    assert!(areas.labels.is_empty() && !areas.y_axis.is_empty());
    let rows = json!([{ "x": 1, "y": 4 }, { "x": 3, "y": 9 }]);
    let scatter = json!({ "type": "chart", "kind": "scatter", "data": "@q", "x": { "field": "x", "type": "quantitative" },
                          "y": { "field": "y" } });
    let scatter = compile(&deck("en-US", rows, json!({ "x": "number", "y": "number" }), Value::Null, scatter));
    assert!(scatter.labels.is_empty() && !scatter.y_axis.is_empty());
    // A chart that says, or a theme that does, decides.
    assert!(by_series("bar", json!({ "labels": { "show": "none" } })).labels.is_empty());
    let quiet = themed(json!({ "label": { "role": "label", "show": "none" } }));
    assert!(try_compile_in(&quiet, &series_deck("bar", json!({}))).unwrap().labels.is_empty());
    let ends = themed(json!({ "label": { "role": "label", "show": "ends" } }));
    assert_eq!(try_compile_in(&ends, &series_deck("bar", json!({}))).unwrap().labels.len(), 6);
}

#[test]
fn values_nobody_asked_for_hide_where_they_collide() {
    // The ends of three lines that finish close together: unasked, the labels that would
    // collide hide; asked for, they collide and lint says so (W310).
    let rows = json!([
        { "q": "Q1", "s": "A", "v": 10 }, { "q": "Q2", "s": "A", "v": 22 },
        { "q": "Q1", "s": "B", "v": 4 }, { "q": "Q2", "s": "B", "v": 21.5 },
        { "q": "Q1", "s": "C", "v": 2 }, { "q": "Q2", "s": "C", "v": 21 }
    ]);
    let chart = |labels: Value| {
        let mut c = json!({ "type": "chart", "kind": "line", "data": "@q", "x": { "field": "q" }, "y": { "field": "v" },
                            "series": { "field": "s" } });
        if !labels.is_null() {
            c["labels"] = labels;
        }
        compile(&deck("en-US", rows.clone(), json!({ "v": "number" }), Value::Null, c))
    };
    let unasked = chart(Value::Null);
    let ends: Vec<&str> = unasked.labels.iter().filter(|l| l.key.starts_with("Q2")).map(|l| l.key.as_str()).collect();
    assert_eq!((ends, unasked.collisions.len()), (vec!["Q2\u{1f}A", "Q2\u{1f}C"], 0));
    assert_eq!(chart(json!({ "show": "ends" })).collisions.len(), 2);
}

fn bar(s: &Shape) -> scaena_engine::charts::RoundRect {
    match s {
        Shape::Bar(r) => *r,
        other => panic!("not a bar: {other:?}"),
    }
}

#[test]
fn grouped_bars_share_their_category_and_the_legend_names_each_series() {
    let layout = by_series("bar", json!({}));
    assert_eq!(layout.marks.len(), 12);
    assert_eq!(layout.marks[0].key, "Q1\u{1f}Core");
    // Within Q1: Core, Cloud, Edge side by side, left to right, same width.
    let q1: Vec<_> = ["Core", "Cloud", "Edge"]
        .iter()
        .map(|p| bar(&layout.marks.iter().find(|m| m.key == format!("Q1\u{1f}{p}")).unwrap().shape))
        .collect();
    assert!(q1[0].x + q1[0].w < q1[1].x && q1[1].x + q1[1].w < q1[2].x, "{q1:?}");
    assert!((q1[0].w - q1[2].w).abs() < 1e-4);
    let legend: Vec<&str> = layout.legend.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(legend, ["Core", "Cloud", "Edge"]);
    // Each series in its palette color, and its legend swatch in the same.
    let core = layout.marks.iter().find(|m| m.key == "Q4\u{1f}Core").unwrap();
    assert_eq!(core.color, layout.legend[0].color);
    assert_ne!(layout.legend[0].color, layout.legend[1].color);
    // A single series needs no legend.
    let one = compile(&bars(json!({}), json!([0, null])));
    assert!(one.legend.is_empty());
}

#[test]
fn a_legend_stands_above_the_plot_at_its_foot_or_beside_it() {
    let top = by_series("bar", json!({}));
    let [_, plot_top, ..] = top.plot;
    assert!(top.legend.iter().all(|e| e.swatch.y + e.swatch.h < plot_top), "above the plot by default");
    let foot = by_series("bar", json!({ "legend": "bottom" }));
    let [_, y, _, h] = foot.plot;
    assert!(foot.legend.iter().all(|e| e.swatch.y > y + h), "under the plot");
    assert!(foot.plot[1] < top.plot[1], "the plot takes the room the legend left");
    // At the right: a column beside the plot, which narrows and clips at its side.
    let right = by_series("bar", json!({ "legend": "right" }));
    let [x, y, w, _] = right.plot;
    let column: Vec<f32> = right.legend.iter().map(|e| e.swatch.x).collect();
    assert!(column.iter().all(|&c| c == column[0] && c > x + w), "{column:?}");
    let rows: Vec<f32> = right.legend.iter().map(|e| e.swatch.y).collect();
    assert!(rows.windows(2).all(|r| r[0] < r[1]) && rows[0] >= y, "one entry per line from the plot's top");
    assert!(w < top.plot[2] && right.clipped && !top.clipped);
    assert!(right.ticks.iter().all(|t| t.origin[0] + t.text.width <= x + w + 1e-3), "labels stay in the plot");
    let none = by_series("bar", json!({ "legend": "none" }));
    assert!(none.legend.is_empty() && none.plot[1] < top.plot[1]);
}

#[test]
fn stacked_bars_pile_up_by_series_and_label_their_totals() {
    let layout = by_series("stackedBar", json!({ "labels": { "show": "all" }, "axes": { "y": { "show": true } } }));
    assert_eq!(layout.y_scale.domain, [0.0, 50.0], "the tallest stack, 48, widens to 50");
    for q in ["Q1", "Q2", "Q3", "Q4"] {
        let seg = |p: &str| bar(&layout.marks.iter().find(|m| m.key == format!("{q}\u{1f}{p}")).unwrap().shape);
        let (core, cloud, edge) = (seg("Core"), seg("Cloud"), seg("Edge"));
        assert!((core.top() - cloud.bottom()).abs() < 1e-3 && (cloud.top() - edge.bottom()).abs() < 1e-3, "{q}");
        assert!((core.bottom() - layout.base).abs() < 1e-3);
        // Each segment knows its stack and its place in it, from the foot up.
        let mark = layout.marks.iter().find(|m| m.key == format!("{q}\u{1f}Cloud")).unwrap();
        let place = mark.stack.as_ref().unwrap();
        assert_eq!(
            (place.key.as_str(), place.from, place.to),
            (format!("{q}\u{1f}+").as_str(), cloud.bottom(), cloud.top())
        );
    }
    let totals: Vec<&str> = layout.labels.iter().map(|l| l.text.text.as_str()).collect();
    assert_eq!(totals, ["21", "28", "36", "48"]);
}

#[test]
fn lines_and_areas_run_through_their_series_points() {
    let lines = by_series("line", json!({}));
    assert_eq!(lines.paths.len(), 3);
    let core = &lines.paths[0];
    assert_eq!((core.key.as_str(), core.marks.len()), ("Core", 4));
    assert!(core.stroke.is_some(), "a line strokes");
    assert!(lines.marks.iter().all(|m| matches!(m.shape, Shape::Dot { r, .. } if r == 0.0)), "no dots by default");
    // A line's points sit at its categories' centers, like bars.
    let xs: Vec<f32> =
        core.marks.iter().map(|k| lines.marks.iter().find(|m| &m.key == k).unwrap().shape.center_x()).collect();
    assert!(xs.windows(2).all(|w| w[0] < w[1]));
    // Areas stack: each series' base is the top of the one under it.
    let areas = by_series("area", json!({}));
    let span = |key: &str| match areas.marks.iter().find(|m| m.key == key).unwrap().shape {
        Shape::Span { top, base, .. } => (top, base),
        other => panic!("{other:?}"),
    };
    let (core_top, core_base) = span("Q2\u{1f}Core");
    let (_, cloud_base) = span("Q2\u{1f}Cloud");
    assert!((core_base - areas.base).abs() < 1e-3 && (cloud_base - core_top).abs() < 1e-3);
    assert!(areas.paths.iter().all(|p| p.stroke.is_none()), "an area fills");
}

#[test]
fn a_scatter_sizes_dots_by_area_on_round_axes() {
    let rows = json!([{ "n": "a", "x": 12, "y": 4, "s": 30 }, { "n": "b", "x": 85, "y": 18.5, "s": 120 }]);
    let chart = json!({ "type": "chart", "kind": "scatter", "data": "@q", "key": "n", "x": { "field": "x", "type": "quantitative" },
                        "y": { "field": "y" }, "sizeEncoding": { "field": "s" } });
    let layout =
        compile(&deck("en-US", rows, json!({ "x": "number", "y": "number", "s": "number" }), Value::Null, chart));
    let radius = |k: &str| match layout.marks.iter().find(|m| m.key == k).unwrap().shape {
        Shape::Dot { r, .. } => r,
        other => panic!("{other:?}"),
    };
    // The largest at the theme's dot radius (6 by default: small dots, little ink), the
    // rest by area.
    assert_eq!(radius("b"), 6.0);
    assert!((radius("a") - 6.0 * (30.0_f32 / 120.0).sqrt()).abs() < 1e-4);
    let a = layout.marks[0].shape.center_x();
    assert!(a > layout.plot[0] + 6.0, "x widens to round values: no dot on the plot's edge");
}

#[test]
fn a_donut_turns_each_value_into_its_share_of_a_ring() {
    let rows = json!([{ "c": "Direct", "v": 42 }, { "c": "Partners", "v": 27 }, { "c": "Online", "v": 19 }, { "c": "Retail", "v": 12 }]);
    let chart = json!({ "type": "chart", "kind": "donut", "data": "@q", "x": { "field": "c" }, "y": { "field": "v" },
                        "labels": { "show": "all" } });
    let layout = compile(&deck("en-US", rows, json!({ "v": "number" }), Value::Null, chart));
    let arcs: Vec<(f32, f32, f32, f32)> = layout
        .marks
        .iter()
        .map(|m| match m.shape {
            Shape::Arc { start, end, inner, outer, .. } => (start, end, inner, outer),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(arcs[0].0, 0.0);
    assert!((arcs[0].1 - 0.42).abs() < 1e-6 && (arcs[3].1 - 1.0).abs() < 1e-6, "{arcs:?}");
    assert!(arcs.windows(2).all(|w| w[0].1 == w[1].0), "slices meet");
    assert!((arcs[0].2 - 0.72 * arcs[0].3).abs() < 1e-3, "the default hole: a thin ring");
    assert!(layout.baseline.is_none() && layout.y_axis.is_empty() && layout.ticks.is_empty());
    assert_eq!(layout.legend.len(), 4);
    // Labels sit outside, on their slice's side: Direct (the right half) starts at its
    // point, Online (the left) ends at its.
    let align = |k: &str| layout.labels.iter().find(|l| l.key == k).unwrap().value.unwrap().align;
    assert_eq!((align("Direct"), align("Online")), (0.0, 1.0));
}

#[test]
fn a_donut_leaves_its_values_room_on_every_side() {
    // Wide values on a square cell: the ring shrinks until each stays inside the chart,
    // the ones at its sides too, not only those over and under it.
    let rows = json!([{ "c": "Grants", "v": 184000 }, { "c": "Members", "v": 124000 },
                      { "c": "Partners", "v": 60000 }, { "c": "Events", "v": 32000 }]);
    let chart = json!({ "type": "chart", "kind": "donut", "data": "@q", "x": { "field": "c" },
                        "y": { "field": "v", "format": "$,.0f" }, "legend": "none" });
    let d = deck("en-US", rows, json!({ "v": "number" }), Value::Null, chart);
    let layout = compile_sized(&themed(json!({})), &d, [600.0, 600.0]).unwrap();
    assert_eq!(layout.labels.len(), 4);
    for l in &layout.labels {
        let (x, y) = (l.origin[0], l.origin[1]);
        assert!(
            x >= 0.0 && x + l.text.width <= 600.0 && y >= 0.0 && y + l.text.height <= 600.0,
            "{}: {:?}",
            l.key,
            l.origin
        );
    }
    // And no smaller than that: one value touches its side.
    let Shape::Arc { outer, .. } = layout.marks[0].shape else { panic!("arcs") };
    let snug =
        layout.labels.iter().any(|l| (l.origin[0] + l.text.width - 600.0).abs() < 0.5 || l.origin[0].abs() < 0.5);
    assert!(snug && outer < 300.0, "outer {outer}");
}

#[test]
fn a_lines_end_values_stand_away_from_it() {
    // A line leaves its first point and comes to its last, so the first value ends at
    // its point and the last begins there: neither crosses the line.
    let lines = by_series("line", json!({}));
    let x = |key: &str| match lines.marks.iter().find(|m| m.key == key).unwrap().shape {
        Shape::Dot { x, .. } => x,
        other => panic!("{other:?}"),
    };
    let label = |key: &str| lines.labels.iter().find(|l| l.key == key).unwrap();
    assert_eq!(lines.paths.len(), 3);
    for path in &lines.paths {
        let (first, last) = (&path.marks[0], path.marks.last().unwrap());
        let (a, b) = (label(first), label(last));
        assert!((a.origin[0] + a.text.width - x(first)).abs() < 0.01, "{first} ends at its point");
        assert!((b.origin[0] - x(last)).abs() < 0.01, "{last} begins at its point");
    }
    // The names stand a space past the widest of them.
    let column = lines.legend.iter().map(|e| e.label.origin[0]).fold(f32::INFINITY, f32::min);
    let reach = lines.labels.iter().map(|l| l.origin[0] + l.text.width).fold(0.0_f32, f32::max);
    assert!(column >= reach, "names at {column}, values reach {reach}");
}

#[test]
fn a_time_axis_ticks_on_calendar_boundaries() {
    let rows: Vec<Value> =
        (1..=12).map(|m| json!({ "m": format!("2024-{m:02}"), "v": f64::from(m) / 2.0 + 4.0 })).collect();
    let chart = json!({ "type": "chart", "kind": "line", "data": "@q", "x": { "field": "m", "type": "temporal" },
                        "y": { "field": "v" } });
    let layout = compile(&deck("en-US", json!(rows), json!({ "m": "date", "v": "number" }), Value::Null, chart));
    let ticks: Vec<&str> = layout.ticks.iter().map(|t| t.text.text.as_str()).collect();
    assert_eq!(ticks, ["Jan 2024", "Apr 2024", "Jul 2024", "Oct 2024"]);
    // The first label would hang past the plot's left edge: it starts there instead.
    assert_eq!(layout.ticks[0].origin[0], layout.plot[0]);
}

/// The chart `c` of each of `deck`'s states, laid out by the engine.
fn scenes(deck: &Deck) -> Vec<ChartLayout> {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let mut engine = scaena_engine::render::Engine::new(fonts);
    let snapshots = scaena_core::resolve_states(deck).unwrap();
    snapshots
        .iter()
        .map(|snap| {
            let scene = engine.scene(deck, &theme, &DataFiles::new(), snap).unwrap();
            scene
                .nodes
                .into_iter()
                .find_map(|n| match n.content {
                    scaena_engine::sample::Content::Chart { chart, .. } => Some(*chart),
                    _ => None,
                })
                .unwrap()
        })
        .collect()
}

#[test]
fn a_series_keeps_its_color_from_state_to_state() {
    // The first state shows Core, Cloud, and Edge; the second drops Cloud.
    let mut d = deck(
        "en-US",
        revenue(),
        json!({ "rev": "number" }),
        Value::Null,
        json!({
        "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "q" }, "y": { "field": "rev" },
        "series": { "field": "product" } }),
    );
    let without_cloud: Vec<Value> =
        revenue().as_array().unwrap().iter().filter(|r| r["product"] != "Cloud").cloned().collect();
    let mut raw = serde_json::to_value(&d).unwrap();
    raw["data"]["q2"] = json!({ "source": { "inline": without_cloud }, "schema": { "rev": "number" } });
    raw["states"] = json!([
        { "id": "all", "layout": "specimen", "props": { "c": {} } },
        { "id": "two", "layout": "specimen", "props": { "c": { "data": "@q2" } } }
    ]);
    d = serde_json::from_value(raw).unwrap();
    let [all, two] = <[ChartLayout; 2]>::try_from(scenes(&d)).unwrap();
    let color = |c: &ChartLayout, key: &str| c.legend.iter().find(|e| e.key == key).unwrap().color;
    assert_eq!(color(&two, "Edge"), color(&all, "Edge"), "Edge stays the third color");
    assert_ne!(color(&two, "Edge"), color(&all, "Cloud"));
    assert_eq!(color(&two, "Core"), color(&all, "Core"));
}

#[test]
fn chart_presets_read_the_themes_motion_and_calls_override_it() {
    use scaena_core::model::values::SplitUnit;
    use scaena_core::timeline::{CubicBezier, Curve, Item, Look, Motion};
    let rows = json!([{ "k": "a", "v": 1 }, { "k": "b", "v": 2 }]);
    let chart = |enter: Value| {
        json!({ "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "k" }, "y": { "field": "v" },
                "enter": enter })
    };
    // The chart enters with the deck's first state: its own preset is one of its cues.
    let cue = |enter: Value| {
        let d = deck("en-US", rows.clone(), json!({ "v": "number" }), Value::Null, chart(enter));
        let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
        let snapshots = scaena_core::resolve_states(&d).unwrap();
        let items = scaena_engine::motion::items(&d, &theme, &d.states[0], None, &snapshots[0], true).unwrap();
        match <[Item; 1]>::try_from(items) {
            Ok([Item::Cue(cue)]) => cue,
            other => panic!("one cue, not {other:?}"),
        }
    };
    // `rise`: from transparent, 24 cu down, eased `out` over `standard`, a mark at a time.
    let rise = cue(json!("rise"));
    assert_eq!(rise.motion, Motion::Enter(Look { opacity: 0.0, translate: [0.0, 24.0], ..Look::REST }));
    assert_eq!((rise.duration, rise.stagger, rise.split), (420.0, 0.0, Some(SplitUnit::Marks)));
    assert_eq!(rise.curve, Curve::Ease(CubicBezier(0.0, 0.0, 0.2, 1.0)));
    // `grow` scales its marks from their foot; the call's stagger wins; a spring lasts
    // its settle time.
    let grow = cue(json!({ "preset": "grow", "stagger": 60 }));
    let Motion::Enter(look) = grow.motion else { panic!("{:?}", grow.motion) };
    assert_eq!((look.scale, look.anchor), ([1.0, 0.0], [0.5, 1.0]));
    assert_eq!(grow.stagger, 60.0);
    let Curve::Spring(spring, settle) = grow.curve else { panic!("{:?}", grow.curve) };
    assert_eq!((spring.stiffness, spring.damping), (420.0, 34.0));
    assert!(settle > 0.2 && settle < 0.8, "snappy settles in {settle} s");
    assert_eq!(grow.duration, 1000.0 * settle);
    // A chart's own presets move its marks, whatever the preset would split.
    assert_eq!(cue(json!({ "preset": "fade", "split": "words" })).split, Some(SplitUnit::Marks));
}

/// Two lines that end a hair apart, their end labels on top of each other.
fn close_ends(collide: Value) -> ChartLayout {
    let rows = json!([
        { "q": "Q1", "s": "A", "v": 10 }, { "q": "Q2", "s": "A", "v": 22 },
        { "q": "Q1", "s": "B", "v": 4 }, { "q": "Q2", "s": "B", "v": 21.5 },
        { "q": "Q1", "s": "C", "v": 2 }, { "q": "Q2", "s": "C", "v": 21 }
    ]);
    let mut labels = json!({ "show": "ends" });
    if !collide.is_null() {
        labels["collide"] = collide;
    }
    let chart = json!({ "type": "chart", "kind": "line", "data": "@q", "x": { "field": "q" }, "y": { "field": "v" },
                        "series": { "field": "s" }, "labels": labels });
    compile(&deck("en-US", rows, json!({ "v": "number" }), Value::Null, chart))
}

#[test]
fn value_labels_that_overlap_are_reported_hidden_or_nudged_apart() {
    // As laid out: B's end label overlaps A's above it and C's below (W310 reads this).
    let raw = close_ends(Value::Null);
    let ends = |c: &ChartLayout| -> Vec<String> {
        c.labels.iter().filter(|l| l.key.starts_with("Q2")).map(|l| l.key.clone()).collect()
    };
    assert_eq!(ends(&raw).len(), 3);
    let pair = |x: &str, y: &str| (format!("Q2\u{1f}{x}"), format!("Q2\u{1f}{y}"));
    assert_eq!(raw.collisions, [pair("A", "B"), pair("B", "C")]);
    // `hide` keeps the largest values' labels that touch none kept before them.
    let hidden = close_ends(json!("hide"));
    assert_eq!(ends(&hidden), ["Q2\u{1f}A", "Q2\u{1f}C"]);
    assert!(hidden.collisions.is_empty());
    // `nudge` moves them apart, in their order, each by as much as its label rides.
    let nudged = close_ends(json!("nudge"));
    assert!(nudged.collisions.is_empty());
    let label = |c: &ChartLayout, k: &str| c.labels.iter().find(|l| l.key == format!("Q2\u{1f}{k}")).unwrap().clone();
    let (a, b, c) = (label(&nudged, "A"), label(&nudged, "B"), label(&nudged, "C"));
    assert!(a.origin[1] < b.origin[1] && b.origin[1] < c.origin[1], "A above B above C");
    for k in ["A", "B", "C"] {
        let (was, now) = (label(&raw, k), label(&nudged, k));
        let dy = now.origin[1] - was.origin[1];
        assert!((now.value.unwrap().drop - was.value.unwrap().drop - dy).abs() < 1e-4, "{k} rides at its new height");
    }
    // Least movement: the middle label stays about where it was.
    assert!((b.origin[1] - label(&raw, "B").origin[1]).abs() < 0.5 * b.text.height);
}

#[test]
fn a_chart_reads_its_data_through_its_transform() {
    // Wide rows, a column per product, folded long and trimmed to the top three.
    let rows = json!([
        { "q": "Q1", "Core": 12, "Cloud": 6, "Edge": 3 },
        { "q": "Q2", "Core": 15, "Cloud": 9, "Edge": 4 }
    ]);
    let chart = json!({
        "type": "chart", "kind": "bar", "data": "@q",
        "dataTransform": [
            { "fold": ["Core", "Cloud", "Edge"], "as": ["product", "rev"] },
            { "filter": "product != 'Edge'" },
            { "sort": "-rev" },
            { "limit": 3 }
        ],
        "x": { "field": "q" }, "y": { "field": "rev" }, "series": { "field": "product" }
    });
    let schema = json!({ "Core": "number", "Cloud": "number", "Edge": "number" });
    let layout = compile(&deck("en-US", rows, schema, Value::Null, chart));
    let keys: Vec<&str> = layout.marks.iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys, ["Q2\u{1f}Core", "Q1\u{1f}Core", "Q2\u{1f}Cloud"]);
    let legend: Vec<&str> = layout.legend.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(legend, ["Core", "Cloud"]);
}

/// `by_series`, but what went wrong.
fn by_series_err(kind: &str, extra: Value) -> String {
    let mut chart = json!({ "type": "chart", "kind": kind, "data": "@q", "x": { "field": "q" }, "y": { "field": "rev" },
                            "series": { "field": "product" } });
    chart.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    try_compile(&deck("en-US", revenue(), json!({ "rev": "number" }), Value::Null, chart)).unwrap_err()
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
fn validate_finds_the_key_that_compiling_refuses() {
    // Q2 of Cloud is a gap, which has no key: its `id` is Q3's too.
    let rows = json!([
        { "q": "Q1", "product": "Core", "rev": 12, "id": "a" },
        { "q": "Q1", "product": "Cloud", "rev": 6, "id": "b" },
        { "q": "Q2", "product": "Core", "rev": 15, "id": "d" },
        { "q": "Q2", "product": "Cloud", "rev": null, "id": "c" },
        { "q": "Q3", "product": "Cloud", "rev": 9, "id": "c" }
    ]);
    let (q, product) = (json!({ "field": "q" }), json!({ "field": "product" }));
    for (props, repeats) in [
        // By x, joined with the series; a series that is x joins, and one that is the key does not.
        (json!({}), Some("Q1")),
        (json!({ "series": product }), None),
        (json!({ "series": q }), Some("Q1 · Q1")),
        (json!({ "series": q, "key": "q" }), Some("Q1")),
        (json!({ "series": product, "key": "product" }), Some("Core")),
        // By `key`, which a gap does not take.
        (json!({ "key": "id" }), None),
        (json!({ "series": product, "key": "id" }), None),
        // A color field of text groups as a series does; one of numbers shades.
        (json!({ "color": product }), None),
        (json!({ "color": { "field": "rev" } }), Some("Q1")),
        // A donut's keys are its categories, whatever its series.
        (json!({ "kind": "donut", "x": product, "series": q }), Some("Core")),
        (json!({ "kind": "donut", "x": { "field": "id" } }), None),
        // Through the chart's transform.
        (json!({ "dataTransform": [{ "filter": "q != 'Q1'" }] }), None),
    ] {
        let mut chart = json!({ "type": "chart", "kind": "bar", "data": "@q", "x": q, "y": { "field": "rev" } });
        chart.as_object_mut().unwrap().extend(props.as_object().unwrap().clone());
        let deck = deck("en-US", rows.clone(), json!({ "rev": "number" }), Value::Null, chart);
        let refused = try_compile(&deck).err().map(|e| {
            assert!(e.contains("repeats; chart keys must be unique"), "{props}: {e}");
            first_key(&e).unwrap()
        });
        assert_eq!(refused.as_deref(), repeats, "compiling {props}");
        let found = validate_bundle(&deck.to_json().unwrap(), &Bundle).unwrap();
        let repeat = found.iter().find(|f| f.code == "E103" && f.message.contains(" repeat"));
        assert_eq!(repeat.and_then(|f| first_key(&f.message)).as_deref(), repeats, "validating {props}");
    }
}

/// The theme's accent, at `alpha` of its own.
fn accent(alpha: f32) -> scaena_core::displaylist::Color {
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let scaena_core::displaylist::Color([r, g, b, a]) = theme.color("accent").unwrap();
    scaena_core::displaylist::Color([r, g, b, (f32::from(a) * alpha).round() as u8])
}

#[test]
fn a_rule_marks_a_value_and_the_axis_widens_to_reach_it() {
    let notes = json!({ "annotations": [{ "kind": "rule", "at": { "y": 30 }, "text": "Target" }] });
    let layout = by_series("bar", notes);
    // The data reaches 22; the target, 30, is the top of the plot.
    assert_eq!(layout.y_scale.domain, [0.0, 30.0]);
    let [left, top, width, _] = layout.plot;
    let note = &layout.notes[0];
    assert_eq!(note.key, "rule\u{1f}y\u{1f}0");
    let rule = note.rule.as_ref().unwrap();
    assert_eq!((rule.from[0], rule.to[0]), (left, left + width));
    assert!((rule.from[1] - top).abs() < 1e-3 && rule.from[1] == rule.to[1], "{rule:?}");
    assert_eq!(rule.color, accent(1.0), "in the annotation color, accent by default");
    // Its text over it, at the plot's start.
    let label = note.label.as_ref().unwrap();
    assert_eq!((label.text.text.as_str(), label.origin[0]), ("Target", left));
    assert!(label.origin[1] + label.text.lines[0].baseline < rule.from[1]);
    // Up the plot through a category's middle, its text beside its top.
    let up = by_series("bar", json!({ "annotations": [{ "kind": "rule", "at": { "x": "Q3" }, "text": "Launch" }] }));
    let [left, top, width, height] = up.plot;
    let rule = up.notes[0].rule.as_ref().unwrap();
    assert_eq!(up.notes[0].key, "rule\u{1f}x\u{1f}0");
    assert!((rule.from[0] - (left + 2.5 * width / 4.0)).abs() < 1e-3 && rule.from[0] == rule.to[0]);
    assert_eq!((rule.from[1], rule.to[1]), (top, top + height));
    assert!(up.notes[0].label.as_ref().unwrap().origin[0] > rule.from[0]);
    // A bound the author set does not move: a rule past it is an error.
    let past = json!({ "y": { "field": "rev", "domain": [0, 25] },
                       "annotations": [{ "kind": "rule", "at": { "y": 30 } }] });
    assert!(by_series_err("bar", past).contains("outside the value axis"));
}

#[test]
fn a_band_spans_categories_or_values_under_the_marks() {
    let notes = json!({ "annotations": [
        { "kind": "band", "at": { "x": ["Q2", "Q3"] }, "text": "Launch" },
        { "kind": "band", "at": { "y": [20, 10] } }
    ] });
    let layout = by_series("bar", notes);
    let [left, top, width, height] = layout.plot;
    let band = width / 4.0;
    let (rect, color) = layout.notes[0].band.unwrap();
    assert!((rect[0] - (left + band)).abs() < 1e-3 && (rect[2] - 2.0 * band).abs() < 1e-3, "{rect:?}");
    assert_eq!((rect[1], rect[3]), (top, height));
    assert_eq!(color, accent(0.12), "a band fills at 0.12 of the annotation color");
    // Its text inside its top-left corner.
    let label = layout.notes[0].label.as_ref().unwrap();
    assert!(label.origin[0] > rect[0] && label.origin[1] > rect[1] - label.text.height);
    let (rect, _) = layout.notes[1].band.unwrap();
    let y = |v: f64| layout.y_scale.map(v);
    assert_eq!((rect[0], rect[2]), (left, width));
    assert!((rect[1] - y(20.0)).abs() < 1e-3 && (rect[1] + rect[3] - y(10.0)).abs() < 1e-3, "either order");
    assert_eq!(layout.notes[1].key, "band\u{1f}y\u{1f}0");
}

#[test]
fn a_callout_points_at_its_mark_clear_of_its_value_label() {
    let notes = json!({ "labels": { "show": "all" },
                        "annotations": [{ "kind": "callout", "at": { "x": "Q4", "series": "Cloud" }, "text": "Record" }] });
    let layout = by_series("bar", notes);
    let mark = layout.marks.iter().find(|m| m.key == "Q4\u{1f}Cloud").unwrap();
    let value = layout.labels.iter().find(|l| l.key == mark.key).unwrap();
    let leader = layout.notes[0].rule.as_ref().unwrap();
    let cap_top = value.origin[1] + value.text.lines[0].baseline - value.text.lines[0].cap_height.unwrap();
    // The leader rises from over the value label, through the mark's middle.
    assert_eq!(leader.from[0], bar(&mark.shape).center_x());
    assert!(leader.from[1] < cap_top && leader.to[1] < leader.from[1], "{leader:?}");
    // Its text over the leader, centered on it.
    let text = layout.notes[0].label.as_ref().unwrap();
    assert!(text.origin[1] + text.text.lines[0].baseline < leader.to[1]);
    assert!((text.origin[0] + 0.5 * text.text.width - leader.from[0]).abs() < 1e-3);
    // The plot made room above for it: the callout over the tallest mark stays in the chart.
    assert!(text.origin[1] >= 0.0, "{:?}", text.origin);
    // On a value instead of a mark.
    let at = by_series(
        "bar",
        json!({ "annotations": [{ "kind": "callout", "at": { "x": "Q1", "y": 10 }, "text": "Ten" }] }),
    );
    assert!((at.notes[0].rule.as_ref().unwrap().from[1] - (at.y_scale.map(10.0) - 8.0 * 0.5)).abs() < 1e-3);
    // Three products stand at Q4: which one is the author's to say.
    let err = by_series_err("bar", json!({ "annotations": [{ "kind": "callout", "at": { "x": "Q4" }, "text": "?" }] }));
    assert!(err.contains("stands on 3 marks") && err.contains("at.series"), "{err}");
    let err = by_series_err("bar", json!({ "annotations": [{ "kind": "callout", "at": { "x": "Q9" }, "text": "?" }] }));
    assert!(err.contains("Q9"), "{err}");
}

#[test]
fn a_highlight_dims_everything_it_does_not_pick_out() {
    let plain = by_series("line", json!({ "labels": { "show": "ends" } }));
    let lit = by_series(
        "line",
        json!({ "labels": { "show": "ends" }, "annotations": [{ "kind": "highlight", "at": { "series": "Cloud" } }] }),
    );
    let alpha = |c: scaena_core::displaylist::Color| c.0[3];
    for (was, now) in plain.paths.iter().zip(&lit.paths) {
        let expected =
            if now.key == "Cloud" { alpha(was.color) } else { (f32::from(alpha(was.color)) * 0.3).round() as u8 };
        assert_eq!(alpha(now.color), expected, "{}", now.key);
    }
    for label in &lit.labels {
        let expected = if label.key.ends_with("Cloud") { 1.0 } else { 0.3 };
        assert_eq!(label.opacity, expected, "{}", label.key);
    }
    let legend: Vec<(&str, f32)> = lit.legend.iter().map(|e| (e.key.as_str(), e.label.opacity)).collect();
    assert_eq!(legend, [("Core", 0.3), ("Cloud", 1.0), ("Edge", 0.3)]);
    // A category: its marks stay, the rest dim; every series has a mark in it, so no
    // legend entry dims.
    let q3 = by_series("bar", json!({ "annotations": [{ "kind": "highlight", "at": { "x": ["Q3"] } }] }));
    for m in &q3.marks {
        let bright = plain.legend.iter().any(|e| e.color == m.color);
        assert_eq!(bright, m.key.starts_with("Q3"), "{}", m.key);
    }
    assert!(q3.legend.iter().all(|e| e.label.opacity == 1.0));
    let err = by_series_err("bar", json!({ "annotations": [{ "kind": "highlight", "at": { "series": "Mobile" } }] }));
    assert!(err.contains("Mobile"), "{err}");
}

#[test]
fn gridlines_cross_a_category_axis_between_bars_and_through_points() {
    let bars = by_series("bar", json!({ "axes": { "x": { "gridlines": true } } }));
    let [left, top, width, height] = bars.plot;
    let at: Vec<(&str, f32)> = bars.x_grid.iter().map(|t| (t.key.as_str(), t.rule.as_ref().unwrap().from[0])).collect();
    let band = width / 4.0;
    assert_eq!(at.len(), 3, "between the four bands: {at:?}");
    for (k, (key, x)) in at.iter().enumerate() {
        assert_eq!(*key, ["Q2", "Q3", "Q4"][k]);
        assert!((x - (left + (k + 1) as f32 * band)).abs() < 1e-3);
    }
    let rule = bars.x_grid[0].rule.as_ref().unwrap();
    assert_eq!((rule.from[1], rule.to[1]), (top, top + height));
    let lines = by_series("line", json!({ "axes": { "x": { "gridlines": true } } }));
    let xs: Vec<f32> = lines.x_grid.iter().map(|t| t.rule.as_ref().unwrap().from[0]).collect();
    let centers: Vec<f32> = lines.ticks.iter().map(|t| t.origin[0] + 0.5 * t.text.width).collect();
    assert_eq!(xs.len(), 4);
    for (x, c) in xs.iter().zip(&centers) {
        assert!((x - c).abs() < 1.0, "through each category: {xs:?} {centers:?}");
    }
}

#[test]
fn a_legend_can_carry_a_title() {
    let row = by_series("bar", json!({ "legend": { "title": "Product" } }));
    let title = row.titles.iter().find(|t| t.key == "legend").unwrap();
    assert_eq!(title.text.text, "Product");
    // The entries follow it on its baseline.
    let baseline = title.origin[1] + title.text.lines[0].baseline;
    let first = &row.legend[0];
    assert!(first.swatch.x > title.origin[0] + title.text.width);
    assert!((first.label.origin[1] + first.label.text.lines[0].baseline - baseline).abs() < 1e-3);
    // In a column, it is the column's first line.
    let column = by_series("bar", json!({ "legend": { "place": "right", "title": "Product" } }));
    let title = column.titles.iter().find(|t| t.key == "legend").unwrap();
    assert_eq!(title.origin[0], column.legend[0].swatch.x);
    assert!(title.origin[1] + title.text.height <= column.legend[0].swatch.y + 1e-3);
    // No entries, no title.
    let one = compile(&bars(json!({}), json!([0, null])));
    assert!(one.titles.iter().all(|t| t.key != "legend"));
}

#[test]
fn annotations_move_to_where_the_next_state_puts_them() {
    // The target rises from 20 to 30; a callout appears.
    let rows = revenue();
    let chart = json!({ "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "q" }, "y": { "field": "rev" },
                        "series": { "field": "product" },
                        "annotations": [{ "kind": "rule", "at": { "y": 20 }, "text": "Target" }] });
    let mut d = deck("en-US", rows, json!({ "rev": "number" }), Value::Null, chart);
    let mut next: Value = serde_json::to_value(&d.states[0]).unwrap();
    next["id"] = json!("t");
    next["transition"] = json!({ "duration": 400, "ease": "linear" });
    next["props"]["c"] = json!({ "annotations": [
        { "kind": "rule", "at": { "y": 30 }, "text": "Target" },
        { "kind": "callout", "at": { "x": "Q4", "series": "Core" }, "text": "Best" }
    ] });
    d.states.push(serde_json::from_value(next).unwrap());
    let [a, b] = <[ChartLayout; 2]>::try_from(scenes(&d)).unwrap();
    let (ya, yb) = (a.notes[0].rule.as_ref().unwrap().from[1], b.notes[0].rule.as_ref().unwrap().from[1]);
    assert_eq!(a.notes[0].key, b.notes[0].key, "one rule, matched");
    // Mid-way the rule is between, solid; the callout fades in.
    let mut fonts = BundleFonts::new();
    for font in &d.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let mut engine = scaena_engine::render::Engine::new(fonts);
    let data = DataFiles::new();
    let req = scaena_engine::render::FrameRequest {
        deck: &d,
        theme: &theme,
        data: &data,
        state: "t",
        t_ms: 200.0,
        format: None,
    };
    let dl = engine.frame(&req).unwrap().display_list;
    let accent = accent(1.0);
    let mut strokes = Vec::new();
    walk(&dl.ops, &mut |op| {
        if let scaena_core::displaylist::Op::Stroke { path, paint: scaena_core::displaylist::Paint::Solid(c), .. } = op
            && c.0[..3] == accent.0[..3]
            && let [scaena_core::displaylist::PathEl::MoveTo(f), scaena_core::displaylist::PathEl::LineTo(t)] =
                &path.0[..]
        {
            strokes.push((*f, *t, c.0[3]));
        }
    });
    let across: Vec<_> = strokes.iter().filter(|(f, t, _)| f[1] == t[1]).collect();
    assert_eq!(across.len(), 1, "{strokes:?}");
    let (f, _, alpha) = across[0];
    assert!(f[1] < ya.max(yb) && f[1] > ya.min(yb) && *alpha == 255, "between {ya} and {yb}: {f:?}");
    let leader: Vec<_> = strokes.iter().filter(|(f, t, _)| f[0] == t[0]).collect();
    assert!(leader.len() == 1 && leader[0].2 > 0 && leader[0].2 < 255, "fading in: {leader:?}");
}

fn walk(ops: &[scaena_core::displaylist::Op], f: &mut impl FnMut(&scaena_core::displaylist::Op)) {
    for op in ops {
        f(op);
        if let scaena_core::displaylist::Op::Layer { ops, .. } = op {
            walk(ops, f);
        }
    }
}
