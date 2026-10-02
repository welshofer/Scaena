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
    let mut cx = Ctx { text: &mut text, fonts: &mut fonts, theme: &theme, deck, data: &data, colors: &[] };
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
    let mut chart = json!({ "type": "chart", "kind": kind, "data": "@q", "x": { "field": "q" }, "y": { "field": "rev" },
                            "series": { "field": "product" } });
    chart.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    compile(&deck("en-US", revenue(), json!({ "rev": "number" }), Value::Null, chart))
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
    // The largest at the theme's dot radius (8 by default), the rest by area.
    assert_eq!(radius("b"), 8.0);
    assert!((radius("a") - 8.0 * (30.0_f32 / 120.0).sqrt()).abs() < 1e-4);
    let a = layout.marks[0].shape.center_x();
    assert!(a > layout.plot[0] + 8.0, "x widens to round values: no dot on the plot's edge");
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
    assert!((arcs[0].2 - 0.6 * arcs[0].3).abs() < 1e-3, "the theme's hole");
    assert!(layout.baseline.is_none() && layout.y_axis.is_empty() && layout.ticks.is_empty());
    assert_eq!(layout.legend.len(), 4);
    // Labels sit outside, on their slice's side: Direct (the right half) starts at its
    // point, Online (the left) ends at its.
    let align = |k: &str| layout.labels.iter().find(|l| l.key == k).unwrap().value.unwrap().align;
    assert_eq!((align("Direct"), align("Online")), (0.0, 1.0));
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
    let rows = json!([{ "k": "a", "v": 1 }, { "k": "b", "v": 2 }]);
    let chart = |enter: Value| {
        json!({ "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "k" }, "y": { "field": "v" },
                "enter": enter })
    };
    let layout = |enter: Value| {
        let d = deck("en-US", rows.clone(), json!({ "v": "number" }), Value::Null, chart(enter));
        compile(&d)
    };
    // `rise`: from transparent, 24 cu down, eased `out` over `standard`.
    let rise = layout(json!("rise")).enter.unwrap();
    assert_eq!((rise.opacity, rise.translate, rise.grow), (0.0, [0.0, 24.0], false));
    assert_eq!((rise.duration, rise.stagger, rise.spring), (Some(420.0), 0.0, None));
    assert_eq!(rise.ease, Some(scaena_core::timeline::CubicBezier(0.0, 0.0, 0.2, 1.0)));
    // `grow` scales its marks, so they grow from zero; the call's stagger wins.
    let grow = layout(json!({ "preset": "grow", "stagger": 60 })).enter.unwrap();
    assert!(grow.grow && grow.opacity == 1.0);
    assert_eq!(grow.stagger, 60.0);
    let (spring, settle) = grow.spring.unwrap();
    assert_eq!((spring.stiffness, spring.damping), (420.0, 34.0));
    assert!(settle > 0.2 && settle < 0.8, "snappy settles in {settle} s");
    // Splitting a chart into anything but its marks waits for choreography.
    let d = deck(
        "en-US",
        rows.clone(),
        json!({ "v": "number" }),
        Value::Null,
        chart(json!({ "preset": "fade", "split": "words" })),
    );
    let mut fonts = BundleFonts::new();
    for font in &d.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let (mut text, data) = (TextEngine::new(), DataFiles::new());
    let mut cx = Ctx { text: &mut text, fonts: &mut fonts, theme: &theme, deck: &d, data: &data, colors: &[] };
    let err = charts::compile(&mut cx, &d.nodes["c"].props, [1600.0, 700.0]).unwrap_err();
    assert!(err.to_string().contains("PLAN 1.11"), "{err}");
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
