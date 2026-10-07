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
    // A spring carries a bar past its value and back, but not its number (SPEC §3.9).
    assert_eq!((numerals.count(0.0, 12.0, 1.03), numerals.count(0.0, 12.0, -0.02)), ("12".into(), "0".into()));
}

#[test]
fn date_categories_print_through_the_x_format() {
    let rows = json!([{ "m": "Jan 2025", "v": 3 }, { "m": "Apr 2025", "v": 5 }]);
    let chart = json!({ "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "m", "format": "Q%q ’%y" },
                        "y": { "field": "v" } });
    let schema = json!({ "m": "date", "v": "number" });
    let layout = compile(&deck("en-US", rows.clone(), schema.clone(), json!({ "m": "%b %Y" }), chart));
    assert_eq!(texts(&layout.ticks), ["Q1 ’25", "Q2 ’25"]);
    // A date column with no format prints by its unit, the first naming its year.
    let plain = json!({ "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "m" }, "y": { "field": "v" } });
    let layout = compile(&deck("en-US", rows, schema, json!({ "m": "%b %Y" }), plain));
    assert_eq!(texts(&layout.ticks), ["Jan 2025", "Apr"]);
}

/// A bar chart of `v` by date `m`, one bar a date, in a cell `size`.
fn dated(dates: &[&str], size: [f32; 2]) -> ChartLayout {
    let rows: Vec<Value> = dates.iter().enumerate().map(|(i, m)| json!({ "m": m, "v": i + 1 })).collect();
    let chart = json!({ "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "m" }, "y": { "field": "v" } });
    let d = deck("en-US", json!(rows), json!({ "m": "date", "v": "number" }), Value::Null, chart);
    compile_sized(&themed(json!({})), &d, size).unwrap()
}

#[test]
fn dates_with_no_format_print_by_their_unit_and_name_the_year_where_it_changes() {
    let wide = [1600.0, 700.0];
    let months = dated(&["2025-11-01", "2025-12-01", "2026-01-01", "2026-02-01"], wide);
    assert_eq!(texts(&months.ticks), ["Nov 2025", "Dec", "Jan 2026", "Feb"]);
    let days = dated(&["2026-03-05", "2026-03-06", "2026-03-07"], wide);
    assert_eq!(texts(&days.ticks), ["Mar 5, 2026", "Mar 6", "Mar 7"]);
    let years = dated(&["2024", "2025", "2026"], wide);
    assert_eq!(texts(&years.ticks), ["2024", "2025", "2026"]);
    // Times of day name the day where it changes, and minutes where any has them.
    let hours = dated(&["2026-03-05T22:00", "2026-03-05T23:00", "2026-03-06T00:00"], wide);
    assert_eq!(texts(&hours.ticks), ["Mar 5, 10 PM", "11 PM", "Mar 6, 12 AM"]);
    let minutes = dated(&["2026-03-05T09:00", "2026-03-05T09:30"], wide);
    assert_eq!(texts(&minutes.ticks), ["Mar 5, 9:00 AM", "9:30 AM"]);
    // One month's first and another's fifth: days, not months.
    let mixed = dated(&["2026-01-01", "2026-02-05"], wide);
    assert_eq!(texts(&mixed.ticks), ["Jan 1, 2026", "Feb 5"]);
}

#[test]
fn a_crowded_axis_of_dates_keeps_every_kth_label_on_the_calendar() {
    let months: Vec<String> = (0..24).map(|i| format!("{}-{:02}-01", 2025 + i / 12, i % 12 + 1)).collect();
    let months: Vec<&str> = months.iter().map(String::as_str).collect();
    // A year's months fit 1600 cu; two years' keep every other month.
    let year = dated(&months[..12], [1600.0, 700.0]);
    assert_eq!(year.ticks.len(), 12, "{:?}", texts(&year.ticks));
    let wide = dated(&months, [1600.0, 700.0]);
    let every_other = ["Jan 2025", "Mar", "May", "Jul", "Sep", "Nov", "Jan 2026", "Mar", "May", "Jul", "Sep", "Nov"];
    assert_eq!(texts(&wide.ticks), every_other);
    let narrow = dated(&months, [600.0, 400.0]);
    let kept = texts(&narrow.ticks);
    let stride = 24 / kept.len();
    assert!([3, 4, 6, 12].contains(&stride) && kept.len() * stride == 24, "{kept:?}");
    // From the first; each a stride on, by month; the year where it changes.
    assert_eq!(kept[0], "Jan 2025");
    assert_eq!(kept[12 / stride], "Jan 2026", "{kept:?}");
    assert_eq!(kept[1], ["", "", "", "Apr", "May", "", "Jul", "", "", "", "", "", "Jan 2026"][stride], "{kept:?}");
    // A space or more between neighbors, which the next stride down does not leave.
    let space = 8.0; // the torture theme's space unit
    for w in narrow.ticks.windows(2) {
        assert!(w[0].origin[0] + w[0].text.width + space <= w[1].origin[0] + 1e-3, "{kept:?}");
    }
    assert!(narrow.crowded.is_empty());
}

/// The ticks of a continuous x keep apart as a crowded ordered axis's labels do: every k-th
/// from the first, at the smallest stride that clears them. The first-deck walk's chart: six
/// months on a time axis in half a slide's width, where the first and last ticks, held inside
/// the plot, ran into their neighbors (`Apr 2026May 2026`).
#[test]
fn a_crowded_time_axis_keeps_every_kth_tick_label() {
    let visits = [1200, 1850, 2900, 3400, 3100, 2200];
    let rows: Vec<Value> = (4..=9).zip(visits).map(|(m, v)| json!({ "m": format!("2026-{m:02}"), "v": v })).collect();
    let chart = json!({ "type": "chart", "kind": "line", "data": "@q",
                        "x": { "field": "m", "type": "temporal" }, "y": { "field": "v" } });
    let d = deck("en-US", json!(rows), json!({ "m": "date", "v": "number" }), json!({ "m": "%Y-%m" }), chart);
    let space = 8.0; // the torture theme's space unit
    let apart = |layout: &ChartLayout| {
        (layout.ticks.windows(2)).all(|w| w[0].origin[0] + w[0].text.width + space <= w[1].origin[0] + 1e-3)
    };
    // Wide enough, every month.
    let wide = compile_sized(&themed(json!({})), &d, [1600.0, 700.0]).unwrap();
    assert_eq!(texts(&wide.ticks).len(), 6, "{:?}", texts(&wide.ticks));
    assert!(apart(&wide), "{:?}", wide.ticks.iter().map(|t| (t.origin[0], t.text.width)).collect::<Vec<_>>());
    // Narrow, every other month from the first, a space or more apart; the gridlines stay at
    // every tick.
    let narrow = compile_sized(&themed(json!({})), &d, [640.0, 400.0]).unwrap();
    assert_eq!(texts(&narrow.ticks), ["Apr 2026", "Jun 2026", "Aug 2026"]);
    assert!(apart(&narrow), "{:?}", narrow.ticks.iter().map(|t| (t.origin[0], t.text.width)).collect::<Vec<_>>());
}

#[test]
fn an_axis_of_text_keeps_every_category_and_says_which_overlap() {
    let rows = json!([
        { "k": "Riverside and the old mill district", "v": 3 },
        { "k": "Old Town north of the river", "v": 5 },
        { "k": "Hilltop", "v": 4 }
    ]);
    let chart = json!({ "type": "chart", "kind": "bar", "data": "@q", "x": { "field": "k" }, "y": { "field": "v" } });
    let d = deck("en-US", rows, json!({ "v": "number" }), Value::Null, chart);
    let narrow = compile_sized(&themed(json!({})), &d, [600.0, 400.0]).unwrap();
    assert_eq!(narrow.ticks.len(), 3);
    let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(
        narrow.crowded,
        [
            pair("Riverside and the old mill district", "Old Town north of the river"),
            pair("Old Town north of the river", "Hilltop")
        ],
        "{:?}",
        narrow.ticks.iter().map(|t| (t.origin[0], t.text.width)).collect::<Vec<_>>()
    );
    let wide = compile_sized(&themed(json!({})), &d, [2400.0, 700.0]).unwrap();
    assert!(wide.crowded.is_empty());
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
    // About five ticks would step by $5 to $35, eight of them; at most five step by $10.
    assert_eq!(layout.y_scale.domain, [0.0, 40.0], "31 widens to the next tick");
    let keys: Vec<&str> = layout.y_axis.iter().map(|t| t.key.as_str()).collect();
    assert_eq!(keys, ["$0", "$10", "$20", "$30", "$40"]);
    let [left, top, width, height] = layout.plot;
    // Labels right-aligned in the gutter, one space unit (8) from the plot.
    for tick in &layout.y_axis {
        let label = tick.label.as_ref().unwrap();
        assert!((label.origin[0] + label.text.width - (left - 8.0)).abs() < 1e-3, "{}", tick.key);
    }
    // A gridline at every tick but the baseline's, across the plot.
    let rules: Vec<f32> = layout.y_axis.iter().filter_map(|t| t.rule.as_ref()).map(|r| r.from[1]).collect();
    assert_eq!(rules.len(), 4);
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
        // The entry keeps its series' color; the name is set in the legend's own, so a
        // grey a swatch would show never has to carry text.
        let path = layout.paths.iter().find(|p| p.key == e.key).unwrap();
        assert_eq!(e.color, path.color);
        let text = layout.legend[0].label.text.runs[0].color;
        assert!(e.label.text.runs.iter().all(|r| r.color == text), "{}", e.key);
    }
    // The plot gives up only what the names need past the last point: here they fit in
    // its half band, so it gives up nothing, and nothing beside it needs it to clip.
    assert_eq!((layout.plot[2], layout.clip), (1600.0, None));
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
    assert!(wide.clip.is_some(), "marks leaving pass under the plot's side, not over the names");
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
fn grouped_bars_keep_a_key_over_the_plot_and_the_rest_name_their_series() {
    // Bars side by side have no end to stand a name by: their key stands over the plot,
    // unless the chart asks for names.
    let grouped = by_series("bar", json!({}));
    assert!(grouped.legend.iter().all(|e| e.swatch.w > 0.0 && e.swatch.y + e.swatch.h < grouped.plot[1]));
    let named = by_series("bar", json!({ "legend": "direct" }));
    assert!(named.legend.iter().all(|e| e.swatch.w == 0.0), "names, no swatches");
    // A dot plot names each series beside its last dot, past the dot's edge.
    let dots = by_series("dot", json!({}));
    assert_eq!(dots.legend.len(), 3);
    for e in &dots.legend {
        let Shape::Dot { x, r, .. } = dots.marks.iter().find(|m| m.key == format!("Q4\u{1f}{}", e.key)).unwrap().shape
        else {
            panic!("a dot plot's marks are dots")
        };
        assert!(e.swatch.w == 0.0 && e.label.origin[0] > x + r, "{}", e.key);
        assert!(e.label.origin[0] + e.label.text.width <= 1600.0 + 1e-3, "{}: inside the chart", e.key);
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
    assert!(w < top.plot[2] && right.clip.is_some() && top.clip.is_none());
    assert!(right.ticks.iter().all(|t| t.origin[0] + t.text.width <= x + w + 1e-3), "labels stay in the plot");
    let none = by_series("bar", json!({ "legend": "none" }));
    assert!(none.legend.is_empty() && none.plot[1] < top.plot[1]);
}

#[test]
fn stacked_bars_pile_up_by_series_and_label_their_totals() {
    let layout = by_series("stackedBar", json!({ "labels": { "show": "all" }, "axes": { "y": { "show": true } } }));
    assert_eq!(layout.y_scale.domain, [0.0, 60.0], "the tallest stack, 48, widens to 60: at most five ticks");
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
    // By area, the largest two and a half of the theme's dots across (6 by default), and
    // none smaller than a dot.
    assert_eq!(radius("b"), 15.0);
    assert!((radius("a") - 15.0 * (30.0_f32 / 120.0).sqrt()).abs() < 1e-4);
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
fn a_donut_names_each_slice_beside_its_value() {
    let rows = json!([{ "c": "Direct", "v": 42 }, { "c": "Partners", "v": 27 }, { "c": "Online", "v": 19 }, { "c": "Retail", "v": 12 }]);
    let chart = json!({ "type": "chart", "kind": "donut", "data": "@q", "x": { "field": "c" }, "y": { "field": "v" } });
    let layout = compile(&deck("en-US", rows, json!({ "v": "number" }), Value::Null, chart));
    // No key: each name stands with its slice's value, on the value's far side from the
    // ring: over it on the ring's upper half (Direct, whose middle is right of the top),
    // under it on the lower (Partners).
    let names: Vec<&str> = layout.legend.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(names, ["Direct", "Partners", "Online", "Retail"]);
    assert!(layout.legend.iter().all(|e| e.swatch.w == 0.0));
    let value = |k: &str| layout.labels.iter().find(|l| l.key == k).unwrap();
    let name = |k: &str| &layout.legend.iter().find(|e| e.key == k).unwrap().label;
    let (direct, partners) = ((name("Direct"), value("Direct")), (name("Partners"), value("Partners")));
    assert!((direct.0.origin[1] + direct.0.text.height - direct.1.origin[1]).abs() < 1e-3, "over its value");
    assert!((partners.0.origin[1] - (partners.1.origin[1] + partners.1.text.height)).abs() < 1e-3, "under its value");
    // Aligned as the value is: Direct's both start at its point.
    assert_eq!(direct.0.origin[0], direct.1.origin[0]);
    // The ring leaves the names room inside the chart.
    for e in &layout.legend {
        let l = &e.label;
        assert!(l.origin[0] >= 0.0 && l.origin[0] + l.text.width <= 1600.0 && l.origin[1] >= 0.0, "{}", e.key);
    }
}

#[test]
fn a_value_label_never_covers_another_mark() {
    // Close values at Q1: A's label over its dot would sit on B's dot just above it, so it
    // goes under its own dot instead.
    let rows = json!([
        { "q": "Q1", "s": "A", "v": 10.0 }, { "q": "Q2", "s": "A", "v": 30 },
        { "q": "Q1", "s": "B", "v": 11.0 }, { "q": "Q2", "s": "B", "v": 10 }
    ]);
    let chart = json!({ "type": "chart", "kind": "dot", "data": "@q", "x": { "field": "q" }, "y": { "field": "v" },
                        "series": { "field": "s" } });
    let layout = compile(&deck("en-US", rows.clone(), json!({ "v": "number" }), Value::Null, chart.clone()));
    let label = layout.labels.iter().find(|l| l.key == "Q1\u{1f}A").expect("A's Q1 value shows");
    let dot = |k: &str| match layout.marks.iter().find(|m| m.key == k).unwrap().shape {
        Shape::Dot { x, y, r } => (x, y, r),
        other => panic!("{other:?}"),
    };
    let (_, ay, ar) = dot("Q1\u{1f}A");
    let (top, _) = cap_box(label);
    assert!(top > ay + ar, "under its dot: {top} vs {}", ay + ar);
    assert!(label.value.unwrap().below);
    // Where neither side is clear, a value nobody asked for hides; one the chart asked for
    // shows, and lint hears of it (W310).
    let crowded = json!([
        { "q": "Q1", "s": "A", "v": 10.0 }, { "q": "Q1", "s": "B", "v": 10.6 }, { "q": "Q1", "s": "C", "v": 9.4 },
        { "q": "Q2", "s": "A", "v": 30 }, { "q": "Q2", "s": "B", "v": 20 }, { "q": "Q2", "s": "C", "v": 10 }
    ]);
    let quiet = compile(&deck("en-US", crowded.clone(), json!({ "v": "number" }), Value::Null, chart.clone()));
    assert!(quiet.labels.iter().all(|l| l.key != "Q1\u{1f}A") && quiet.covers.is_empty());
    let mut asked = chart;
    asked["labels"] = json!({ "show": "all" });
    let loud = compile(&deck("en-US", crowded, json!({ "v": "number" }), Value::Null, asked));
    assert!(loud.labels.iter().any(|l| l.key == "Q1\u{1f}A"));
    assert!(loud.covers.iter().any(|(a, _)| a == "Q1\u{1f}A"), "{:?}", loud.covers);
    // A bar's label wider than its bar leans off the taller bar beside it: it starts at
    // its own bar's left edge, away from Core's, and covers nothing.
    let asked = series_deck("bar", json!({ "y": { "field": "rev", "format": "$,.1f" }, "labels": { "show": "all" } }));
    let narrow = compile_sized(&themed(json!({})), &asked, [600.0, 500.0]).unwrap();
    let cloud = bar(&narrow.marks.iter().find(|m| m.key == "Q4\u{1f}Cloud").unwrap().shape);
    let label = narrow.labels.iter().find(|l| l.key == "Q4\u{1f}Cloud").unwrap();
    assert!(label.text.width > cloud.w, "{} over a bar {} wide", label.text.width, cloud.w);
    assert!((label.origin[0] - cloud.x).abs() < 1e-3, "{:?} from {}", label.origin, cloud.x);
    assert!(label.value.unwrap().align < 0.5);
    assert!(narrow.covers.is_empty(), "{:?}", narrow.covers);
}

#[test]
fn a_scatter_names_each_series_beside_its_own_last_point() {
    let rows = json!([
        { "n": "a", "x": 1, "y": 2, "s": "Left" }, { "n": "b", "x": 2, "y": 3, "s": "Left" },
        { "n": "c", "x": 8, "y": 5, "s": "Right" }, { "n": "d", "x": 9, "y": 6, "s": "Right" }
    ]);
    let chart = json!({ "type": "chart", "kind": "scatter", "data": "@q", "key": "n",
                        "x": { "field": "x", "type": "quantitative" }, "y": { "field": "y" }, "series": { "field": "s" } });
    let layout = compile(&deck("en-US", rows, json!({ "x": "number", "y": "number" }), Value::Null, chart));
    for (key, last) in [("Left", "b"), ("Right", "d")] {
        let e = layout.legend.iter().find(|e| e.key == key).unwrap();
        let point = layout.marks.iter().find(|m| m.key == format!("{last}\u{1f}{key}")).map(|m| m.shape);
        let Some(Shape::Dot { x, r, .. }) = point else { panic!("{key}'s last point") };
        // Past its own last point's edge, by a space: not in a column past the farthest.
        assert!(
            e.label.origin[0] > x + r && e.label.origin[0] < x + r + 20.0,
            "{key}: {} vs {}",
            e.label.origin[0],
            x + r
        );
    }
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
fn a_lines_end_values_take_room_beside_a_continuous_axis() {
    // On a time axis the first and last points stand on the plot's sides. The plot moves
    // in from each, so the first value ends at its point and the last begins there:
    // clear of the line, two spaces from the value axis's labels, and inside the chart.
    let rows: Vec<Value> =
        (1..=12).map(|m| json!({ "m": format!("2024-{m:02}"), "v": 1000.0 + 10.0 * f64::from(m) })).collect();
    let chart = json!({ "type": "chart", "kind": "line", "data": "@q", "x": { "field": "m", "type": "temporal" },
                        "y": { "field": "v", "format": "$,.0f" }, "axes": { "y": { "show": true } } });
    let layout = compile(&deck("en-US", json!(rows), json!({ "m": "date", "v": "number" }), Value::Null, chart));
    let [left, _, width, _] = layout.plot;
    let (first, last) = (&layout.labels[0], &layout.labels[1]);
    assert_eq!((layout.labels.len(), first.text.text.as_str(), last.text.text.as_str()), (2, "$1,010", "$1,120"));
    assert!((first.origin[0] + first.text.width - left).abs() < 0.01, "the first value ends at its point");
    assert!((last.origin[0] - (left + width)).abs() < 0.01, "the last begins at its point");
    assert!((last.origin[0] + last.text.width - 1600.0).abs() < 0.01, "and ends at the chart's side");
    let ticks = layout.y_axis.iter().filter_map(|t| t.label.as_ref());
    let gutter = ticks.map(|l| l.origin[0] + l.text.width).fold(0.0_f32, f32::max);
    assert!((first.origin[0] - gutter - 16.0).abs() < 0.01, "two spaces from the axis labels");
    // The plot clips across the values' room as well as its own width.
    let clip = layout.clip.unwrap();
    assert!(clip[0] <= first.origin[0] && (clip[1] - (last.origin[0] + last.text.width)).abs() < 0.01, "{clip:?}");

    // Named where they end, a line's names stand past its last value, which begins at
    // its point; its first value ends at its own.
    let rows = json!([
        { "x": 2, "s": "A", "v": 3 }, { "x": 10, "s": "A", "v": 8 },
        { "x": 2, "s": "Bravo", "v": 5 }, { "x": 10, "s": "Bravo", "v": 2 }
    ]);
    let chart = json!({ "type": "chart", "kind": "line", "data": "@q", "x": { "field": "x", "type": "quantitative" },
                        "y": { "field": "v" }, "series": { "field": "s" } });
    let named = compile(&deck("en-US", rows, json!({ "x": "number", "v": "number" }), Value::Null, chart));
    let x = |key: &str| match named.marks.iter().find(|m| m.key == key).unwrap().shape {
        Shape::Dot { x, .. } => x,
        other => panic!("{other:?}"),
    };
    for path in &named.paths {
        let label = |key: &str| named.labels.iter().find(|l| l.key == key).unwrap();
        let (a, b) = (&path.marks[0], path.marks.last().unwrap());
        assert!((label(a).origin[0] + label(a).text.width - x(a)).abs() < 0.01 && label(a).origin[0] >= 0.0);
        assert!((label(b).origin[0] - x(b)).abs() < 0.01, "{b} begins at its point");
        let name = named.legend.iter().find(|e| e.key == path.key).unwrap();
        assert!(name.label.origin[0] >= label(b).origin[0] + label(b).text.width, "{}", path.key);
    }
}

#[test]
fn a_lines_first_values_keep_their_room_where_the_names_narrow_the_plot() {
    // In a narrow chart, as a column of a portrait slide leaves one, the first values
    // are wider than half a band, and the names past the last points take a share of
    // the width. The room beside the plot is what the plot that leaves needs, not what
    // the whole width would: each first value still ends at its point, its line leaving
    // it clear (PLAN 1.32), and none is pushed in over the line.
    let money = json!({ "y": { "field": "rev", "format": "$,.0f" } });
    let lines = compile_sized(&themed(json!({})), &series_deck("line", money), [320.0, 700.0]).unwrap();
    let [left, ..] = lines.plot;
    let x = |key: &str| match lines.marks.iter().find(|m| m.key == key).unwrap().shape {
        Shape::Dot { x, .. } => x,
        other => panic!("{other:?}"),
    };
    let names = lines.legend.iter().map(|e| e.label.origin[0]).fold(f32::INFINITY, f32::min);
    assert!(names < 320.0 && lines.paths.len() == 3, "names stand at {names}");
    for path in &lines.paths {
        let first = &path.marks[0];
        let label = lines.labels.iter().find(|l| l.key == *first).unwrap();
        assert!(label.text.width > x(first) - left, "{first} is wider than its room inside the plot");
        let end = label.origin[0] + label.text.width;
        assert!((end - x(first)).abs() < 0.01, "{first} ends at {end}, its point at {}", x(first));
        assert!(label.origin[0] >= -0.01, "{first} stays inside the chart");
    }
}

#[test]
fn the_baseline_rules_zero_only_where_the_value_axis_reaches_it() {
    // Values far from zero: a line's or a dot plot's axis starts near them, and no rule
    // at the plot's foot claims to be zero. The lowest gridline rules there instead.
    let rows = json!([{ "k": "a", "v": 120 }, { "k": "b", "v": 131 }, { "k": "c", "v": 127 }]);
    let chart = |kind: &str| {
        json!({ "type": "chart", "kind": kind, "data": "@q", "x": { "field": "k" }, "y": { "field": "v" },
                "axes": { "y": { "show": true, "gridlines": true } } })
    };
    for kind in ["line", "dot"] {
        let layout = compile(&deck("en-US", rows.clone(), json!({ "v": "number" }), Value::Null, chart(kind)));
        assert!(layout.y_scale.domain[0] > 0.0 && layout.baseline.is_none(), "{kind}: {:?}", layout.y_scale);
        let foot = &layout.y_axis[0];
        assert!(foot.value == layout.y_scale.domain[0] && foot.rule.is_some(), "{kind}");
    }
    // Bars stand on zero: the baseline rules it, in place of a gridline.
    let bars = compile(&deck("en-US", rows, json!({ "v": "number" }), Value::Null, chart("bar")));
    assert_eq!(bars.y_scale.domain[0], 0.0);
    assert!(bars.baseline.is_some() && bars.y_axis[0].rule.is_none());
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

/// The theme's color `token`, at `alpha` of its own.
fn token(token: &str, alpha: f32) -> scaena_core::displaylist::Color {
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let scaena_core::displaylist::Color([r, g, b, a]) = theme.color(token).unwrap();
    scaena_core::displaylist::Color([r, g, b, (f32::from(a) * alpha).round() as u8])
}

/// The annotation color the torture theme sets, its signal, at `alpha` of its own.
fn note_color(alpha: f32) -> scaena_core::displaylist::Color {
    token("signal", alpha)
}

#[test]
fn a_rule_marks_a_value_and_the_axis_widens_to_reach_it() {
    let notes = json!({ "annotations": [{ "kind": "rule", "at": { "y": 30 }, "text": "Target" }] });
    let layout = by_series("bar", notes.clone());
    // The data reaches 22; the target, 30, is the top of the plot.
    assert_eq!(layout.y_scale.domain, [0.0, 30.0]);
    let [left, top, width, _] = layout.plot;
    let note = &layout.notes[0];
    assert_eq!(note.key, "rule\u{1f}y\u{1f}0");
    let rule = note.rule.as_ref().unwrap();
    assert_eq!((rule.from[0], rule.to[0]), (left, left + width));
    assert!((rule.from[1] - top).abs() < 1e-3 && rule.from[1] == rule.to[1], "{rule:?}");
    assert_eq!(rule.color, note_color(1.0), "in the theme's annotation color");
    let unset = try_compile_in(&themed(json!({ "annotation": null })), &series_deck("bar", notes)).unwrap();
    assert_eq!(unset.notes[0].rule.as_ref().unwrap().color, token("accent", 1.0), "the accent where it sets none");
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
    assert_eq!(color, note_color(0.12), "a band fills at 0.12 of the annotation color");
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

/// The value at `y` on the chart's value scale.
fn value_at(layout: &ChartLayout, y: f32) -> f64 {
    let (zero, one) = (layout.y_scale.map(0.0), layout.y_scale.map(1.0));
    f64::from((y - zero) / (one - zero))
}

#[test]
fn a_rule_breaks_where_it_crosses_text_and_annotation_text_steps_off_rules() {
    // A rule through the middle of the value label over Q4 Cloud's bar. The axis's
    // bounds are the author's, so the rule moves nothing.
    let fixed = json!({ "field": "rev", "domain": [0, 40] });
    let plain = by_series("bar", json!({ "y": fixed }));
    let (top, baseline) = cap_box(plain.labels.iter().find(|l| l.key == "Q4\u{1f}Cloud").unwrap());
    let through = value_at(&plain, 0.5 * (top + baseline));
    let ruled = by_series("bar", json!({ "y": fixed, "annotations": [{ "kind": "rule", "at": { "y": through } }] }));
    let note = &ruled.notes[0];
    let (rule, value) = (note.rule.as_ref().unwrap(), ruled.labels.iter().find(|l| l.key == "Q4\u{1f}Cloud").unwrap());
    // It leaves out the label's width and half a space unit either side, and only there.
    let keys: Vec<&str> = note.gaps.iter().map(|g| g.key.as_str()).collect();
    assert_eq!(keys, ["Q4\u{1f}Cloud"], "{:?}", note.gaps);
    let gap = &note.gaps[0];
    assert!((gap.along[0] - (value.origin[0] - 4.0)).abs() < 1e-3, "{gap:?}");
    assert!((gap.along[1] - (value.origin[0] + value.text.width + 4.0)).abs() < 1e-3, "{gap:?}");
    assert!(gap.across[0] < rule.from[1] && rule.from[1] < gap.across[1]);
    // A rule clear of every label breaks for none.
    let clear = by_series("bar", json!({ "y": fixed, "annotations": [{ "kind": "rule", "at": { "y": 35 } }] }));
    assert!(clear.notes[0].gaps.is_empty(), "{:?}", clear.notes[0].gaps);

    // A callout whose text would sit on a rule rises past it, its leader crossing the rule.
    let callout = json!({ "kind": "callout", "at": { "x": "Q4", "series": "Core" }, "text": "Best" });
    let alone = by_series("bar", json!({ "y": fixed, "annotations": [callout] }));
    let (top, baseline) = cap_box(alone.notes[0].label.as_ref().unwrap());
    let on = value_at(&alone, 0.5 * (top + baseline));
    let both = by_series("bar", json!({ "y": fixed, "annotations": [callout, { "kind": "rule", "at": { "y": on } }] }));
    let level = both.notes[1].rule.as_ref().unwrap().from[1];
    let text = both.notes[0].label.as_ref().unwrap();
    let below = text.origin[1] + text.text.lines[0].baseline + text.text.lines[0].descent;
    assert!(below <= level - 8.0, "a space unit clear of the rule: {below} over {level}");
    let leader = both.notes[0].rule.as_ref().unwrap();
    assert!(leader.to[1] < level && level < leader.from[1], "{leader:?} crosses {level}");

    // A band's text, inside its top corner, steps down past a rule along the band's top.
    let band = by_series(
        "bar",
        json!({ "annotations": [{ "kind": "band", "at": { "x": ["Q3", "Q4"] }, "text": "Launch" },
                                 { "kind": "rule", "at": { "y": 30 }, "text": "Target" }] }),
    );
    let level = band.notes[1].rule.as_ref().unwrap().from[1];
    assert!((level - band.plot[1]).abs() < 1e-3, "the rule is the plot's top, and the band's");
    let (top, _) = cap_box(band.notes[0].label.as_ref().unwrap());
    assert!(top >= level + 8.0 - 1e-3, "a space unit under the rule: {top} under {level}");
}

#[test]
fn a_moving_rule_breaks_only_while_it_crosses_the_text() {
    // The rule leaves the label over Q4 Cloud's bar for a value clear of every label.
    let fixed = json!({ "field": "rev", "domain": [0, 40] });
    let plain = by_series("bar", json!({ "y": fixed }));
    let (top, baseline) = cap_box(plain.labels.iter().find(|l| l.key == "Q4\u{1f}Cloud").unwrap());
    let through = value_at(&plain, 0.5 * (top + baseline));
    let mut d = series_deck("bar", json!({ "y": fixed, "annotations": [{ "kind": "rule", "at": { "y": through } }] }));
    let mut next: Value = serde_json::to_value(&d.states[0]).unwrap();
    next["id"] = json!("t");
    next["transition"] = json!({ "duration": 400, "ease": "linear" });
    next["props"]["c"] = json!({ "annotations": [{ "kind": "rule", "at": { "y": 35 } }] });
    d.states.push(serde_json::from_value(next).unwrap());
    // Strokes across the plot in the annotation color: their pieces.
    let pieces = |t_ms: f64| -> Vec<usize> {
        let mut out = Vec::new();
        walk(&frame(&d, "t", t_ms).ops, &mut |op| {
            if let scaena_core::displaylist::Op::Stroke {
                path, paint: scaena_core::displaylist::Paint::Solid(c), ..
            } = op
                && c.0[..3] == note_color(1.0).0[..3]
            {
                let moves = path.0.iter().filter(|e| matches!(e, scaena_core::displaylist::PathEl::MoveTo(_))).count();
                out.push(moves);
            }
        });
        out
    };
    // Just under way the rule still crosses the label and breaks there; half way it has
    // left it, and is whole.
    assert_eq!(pieces(4.0), [2], "two pieces either side of the label");
    assert_eq!(pieces(200.0), [1], "whole once past the label");
}

#[test]
fn a_key_that_turns_into_names_fades_where_it_stands() {
    // Grouped bars keep a key over the plot; stacked, each series is named beside the
    // last stack.
    let mut d = series_deck("bar", json!({}));
    let mut next: Value = serde_json::to_value(&d.states[0]).unwrap();
    next["id"] = json!("t");
    next["transition"] = json!({ "duration": 400, "ease": "linear" });
    next["props"]["c"] = json!({ "kind": "stackedBar" });
    d.states.push(serde_json::from_value(next).unwrap());
    let [a, b] = <[ChartLayout; 2]>::try_from(scenes(&d)).unwrap();
    assert!(a.legend.iter().all(|e| e.swatch.w > 0.0) && b.legend.iter().all(|e| e.swatch.w == 0.0));
    // Half way, "Core" is drawn twice, each at half strength: the key's entry where it
    // stood, and the name where it will stand. Nothing crosses the plot.
    let mut cores = Vec::new();
    walk(&frame(&d, "t", 200.0).ops, &mut |op| {
        if let scaena_core::displaylist::Op::Layer { opacity, ops, .. } = op
            && ops.iter().any(|o| matches!(o, scaena_core::displaylist::Op::Glyphs { text, .. } if text == "Core"))
        {
            cores.push(*opacity);
        }
    });
    assert_eq!(cores.len(), 2, "{cores:?}");
    assert!(cores.iter().all(|o| (o - 0.5).abs() < 1e-3), "{cores:?}");
}

/// The display list of `state` in `d`, `t_ms` into its cue.
fn frame(d: &Deck, state: &str, t_ms: f64) -> scaena_core::displaylist::DisplayList {
    let mut fonts = BundleFonts::new();
    for font in &d.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let mut engine = scaena_engine::render::Engine::new(fonts);
    let data = DataFiles::new();
    let req = scaena_engine::render::FrameRequest { deck: d, theme: &theme, data: &data, state, t_ms, format: None };
    engine.frame(&req).unwrap().display_list
}

#[test]
fn a_highlight_dims_everything_it_does_not_pick_out() {
    let plain = by_series("line", json!({ "labels": { "show": "ends" } }));
    let lit = by_series(
        "line",
        json!({ "labels": { "show": "ends" }, "annotations": [{ "kind": "highlight", "at": { "series": "Cloud" } }] }),
    );
    // What it picks takes the signal color (the theme's `signal`); the rest keeps its
    // color, at half its opacity, and its words dim half as far.
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let signal = theme.color("signal").unwrap();
    let alpha = |c: scaena_core::displaylist::Color| c.0[3];
    for (was, now) in plain.paths.iter().zip(&lit.paths) {
        match now.key.as_str() {
            "Cloud" => assert_eq!(now.color, signal),
            key => assert_eq!(alpha(now.color), (f32::from(alpha(was.color)) * 0.5).round() as u8, "{key}"),
        }
    }
    for label in &lit.labels {
        let expected = if label.key.ends_with("Cloud") { 1.0 } else { 0.75 };
        assert_eq!(label.opacity, expected, "{}", label.key);
    }
    let legend: Vec<(&str, f32)> = lit.legend.iter().map(|e| (e.key.as_str(), e.label.opacity)).collect();
    assert_eq!(legend, [("Core", 0.75), ("Cloud", 1.0), ("Edge", 0.75)]);
    // Cloud's name is set in the signal color with its line.
    let cloud = lit.legend.iter().find(|e| e.key == "Cloud").unwrap();
    assert!(cloud.color == signal && cloud.label.text.runs.iter().all(|r| r.color == signal));
    // A category: its marks take the signal, the rest dim; every series has a mark in
    // it and others out of it, so no legend entry changes.
    let q3 = by_series("bar", json!({ "annotations": [{ "kind": "highlight", "at": { "x": ["Q3"] } }] }));
    for m in &q3.marks {
        assert_eq!(m.color == signal, m.key.starts_with("Q3"), "{}", m.key);
    }
    let bars = by_series("bar", json!({}));
    let entries =
        |c: &ChartLayout| c.legend.iter().map(|e| (e.key.clone(), e.color, e.label.opacity)).collect::<Vec<_>>();
    assert_eq!(entries(&q3), entries(&bars));
    let err = by_series_err("bar", json!({ "annotations": [{ "kind": "highlight", "at": { "series": "Mobile" } }] }));
    assert!(err.contains("Mobile"), "{err}");
}

#[test]
fn a_highlighted_series_keeps_its_name_in_a_legends_text_color() {
    // A theme whose first series is the legend's text color (ink), and whose signal is
    // its accent: the picked series' swatch takes the signal, and its name, which is not
    // a direct name, stays in the legend's color.
    let mut t: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    t["tokens"]["data"]["categorical"][0] = t["tokens"]["color"]["ink"].clone();
    t["charts"]["signal"] = json!("accent");
    let theme = Theme::from_json(&t.to_string()).unwrap();
    let top = |extra: Value| {
        let mut props = json!({ "legend": "top" });
        props.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        try_compile_in(&theme, &series_deck("line", props)).unwrap()
    };
    let plain = top(json!({}));
    let lit = top(json!({ "annotations": [{ "kind": "highlight", "at": { "series": "Core" } }] }));
    let core = |c: &ChartLayout| c.legend.iter().find(|e| e.key == "Core").unwrap().clone();
    let (was, now) = (core(&plain), core(&lit));
    assert_eq!(was.color, was.label.text.runs[0].color, "the swatch and the name share a color");
    assert_eq!(now.color, theme.color("accent").unwrap());
    let colors = |e: &charts::LegendEntry| e.label.text.runs.iter().map(|r| r.color).collect::<Vec<_>>();
    assert_eq!(colors(&now), colors(&was));
}

#[test]
fn a_theme_sets_how_many_reference_lines_the_value_axis_draws() {
    let deck = bars(json!({ "y": { "show": true, "gridlines": true } }), json!([0, null]));
    let keys = |charts: Value| -> Vec<String> {
        try_compile_in(&themed(charts), &deck).unwrap().y_axis.iter().map(|t| t.key.clone()).collect()
    };
    // At most five by default, so $10 steps; a theme that allows eight gets d3's $5.
    assert_eq!(keys(json!({})), ["$0", "$10", "$20", "$30", "$40"]);
    assert_eq!(keys(json!({ "maxTicks": 8 })), ["$0", "$5", "$10", "$15", "$20", "$25", "$30", "$35"]);
    // Below what d3's steps can give, the axis asks for two ticks and takes what comes.
    assert_eq!(keys(json!({ "maxTicks": 2 })), ["$0", "$20", "$40"]);
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
    let accent = note_color(1.0);
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

// --- forecasts and estimates (PLAN 1.28) ------------------------------------------------

/// Five years of revenue, the last two estimated: `estimate` true, `kind` `forecast`.
fn forecast() -> Value {
    let years = [("2021", 12, false), ("2022", 15, false), ("2023", 18, false), ("2024", 22, true), ("2025", 25, true)];
    let rows: Vec<Value> = (years.iter())
        .map(|&(year, rev, est)| {
            json!({ "year": year, "rev": rev, "estimate": est, "kind": if est { "forecast" } else { "actual" } })
        })
        .collect();
    json!(rows)
}

/// A chart of `kind` over `forecast()`, with `extra` props.
fn forecast_deck(kind: &str, extra: Value) -> Deck {
    let mut chart = json!({ "type": "chart", "kind": kind, "data": "@q", "x": { "field": "year" },
                            "y": { "field": "rev", "format": "$,.0f" } });
    chart.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    deck("en-US", forecast(), json!({ "rev": "number", "estimate": "boolean" }), Value::Null, chart)
}

/// The paths drawn in `color`'s hue in the display list of `state` at `t_ms`: where each
/// starts and ends across, its dash, and its alpha.
fn drawn(d: &Deck, state: &str, t_ms: f64, color: scaena_core::displaylist::Color) -> Vec<([f32; 2], Vec<f32>, u8)> {
    use scaena_core::displaylist::{Op, Paint, PathEl};
    let across = |path: &scaena_core::displaylist::Path| {
        let xs: Vec<f32> = (path.0.iter())
            .filter_map(|e| match e {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(p[0]),
                _ => None,
            })
            .collect();
        [xs.iter().copied().fold(f32::INFINITY, f32::min), xs.iter().copied().fold(f32::NEG_INFINITY, f32::max)]
    };
    let mut out = Vec::new();
    walk(&frame(d, state, t_ms).ops, &mut |op| match op {
        Op::Stroke { path, paint: Paint::Solid(c), dash, .. } if c.0[..3] == color.0[..3] => {
            out.push((across(path), dash.clone(), c.0[3]))
        }
        Op::Fill { path, paint: Paint::Solid(c), .. } if c.0[..3] == color.0[..3] => {
            out.push((across(path), Vec::new(), c.0[3]))
        }
        _ => {}
    });
    out
}

#[test]
fn a_forecast_runs_dashed_from_the_last_actual_point() {
    let d = forecast_deck("line", json!({ "projected": { "field": "estimate" } }));
    // The chart as the frame lays it out, in its cell.
    let layout = scenes(&d).remove(0);
    let line = &layout.paths[0];
    assert_eq!(line.projected, ["2024", "2025"]);
    let x = |key: &str| layout.marks.iter().find(|m| m.key == key).unwrap().shape.point()[0];
    let width = line.stroke.unwrap();
    // Solid through 2023, the last actual year; dashed from it, three widths on, two off.
    let strokes = drawn(&d, "s", 0.0, line.color);
    assert_eq!(strokes.len(), 2, "{strokes:?}");
    assert_eq!((strokes[0].0, strokes[0].1.is_empty()), ([x("2021"), x("2023")], true));
    assert_eq!((strokes[1].0, strokes[1].1.clone()), ([x("2023"), x("2025")], vec![3.0 * width, 2.0 * width]));
    // Nothing projected: one solid stroke, as before.
    let plain = forecast_deck("line", json!({}));
    let strokes = drawn(&plain, "s", 0.0, scenes(&plain)[0].paths[0].color);
    assert_eq!(strokes.len(), 1);
    assert!(strokes[0].1.is_empty());
}

#[test]
fn an_area_is_lighter_under_what_is_projected() {
    let d = forecast_deck("area", json!({ "projected": { "field": "kind", "value": "forecast" } }));
    let layout = scenes(&d).remove(0);
    let area = &layout.paths[0];
    let x = |key: &str| layout.marks.iter().find(|m| m.key == key).unwrap().shape.point()[0];
    let fills = drawn(&d, "s", 0.0, area.color);
    assert_eq!(fills.len(), 2, "{fills:?}");
    assert_eq!((fills[0].0, fills[0].2), ([x("2021"), x("2023")], area.color.0[3]));
    assert_eq!(fills[1].0, [x("2023"), x("2025")]);
    let half = f32::from(area.color.0[3]) * 0.5;
    assert!((f32::from(fills[1].2) - half).abs() <= 1.0, "{} is half of {}", fills[1].2, area.color.0[3]);
}

#[test]
fn a_projected_value_says_it_is_an_estimate() {
    let projected = json!({ "projected": { "field": "estimate" } });
    let layout = compile(&forecast_deck("line", projected.clone()));
    // A line prints its first value and its last, an estimate.
    assert_eq!(texts(&layout.labels), ["$12", "$25\u{a0}est."]);
    assert_eq!(layout.labels.iter().map(|l| l.noted).collect::<Vec<_>>(), [false, true]);
    // The note is the chart's, else the theme's, which also sets the dash and the fill.
    let theme = themed(json!({ "projected": { "note": "forecast", "dash": [4, 1], "opacity": 0.25 } }));
    let theirs = try_compile_in(&theme, &forecast_deck("line", projected)).unwrap();
    assert_eq!(texts(&theirs.labels)[1], "$25\u{a0}forecast");
    let width = theirs.paths[0].stroke.unwrap();
    assert_eq!((theirs.paths[0].dash, theirs.paths[0].fade), ([4.0 * width, width], 0.25));
    let mine = forecast_deck("line", json!({ "projected": { "field": "estimate", "note": "proj." } }));
    assert_eq!(texts(&try_compile_in(&theme, &mine).unwrap().labels)[1], "$25\u{a0}proj.");
}

#[test]
fn a_forecast_that_comes_true_turns_solid_halfway() {
    // A year on, 2024 is actual and 2025 is estimated higher.
    let d = forecast_deck("line", json!({ "projected": { "field": "estimate" } }));
    let mut v = serde_json::to_value(&d).unwrap();
    let mut rows = forecast();
    rows[3]["estimate"] = json!(false);
    rows[3]["kind"] = json!("actual");
    rows[4]["rev"] = json!(26);
    v["data"]["q2"] = json!({ "source": { "inline": rows }, "schema": { "rev": "number", "estimate": "boolean" } });
    let mut next = v["states"][0].clone();
    next["id"] = json!("t");
    next["transition"] = json!({ "duration": 400, "ease": "linear" });
    next["props"]["c"] = json!({ "data": "@q2" });
    v["states"].as_array_mut().unwrap().push(next);
    let d: Deck = serde_json::from_value(v).unwrap();
    let layout = scenes(&d).remove(0);
    let x = |key: &str| layout.marks.iter().find(|m| m.key == key).unwrap().shape.point()[0];
    let dashed_from = |t_ms: f64| {
        let strokes = drawn(&d, "t", t_ms, layout.paths[0].color);
        strokes.iter().find(|s| !s.1.is_empty()).map(|s| s.0[0])
    };
    assert_eq!(dashed_from(100.0), Some(x("2023")), "before halfway, 2024 is still an estimate");
    assert_eq!(dashed_from(300.0), Some(x("2024")), "from halfway, it is actual");
    // The estimate's value cross-fades, its note with it, rather than count: halfway, the
    // old and the new each at half strength.
    let mut estimates = Vec::new();
    walk(&frame(&d, "t", 200.0).ops, &mut |op| {
        if let scaena_core::displaylist::Op::Layer { opacity, ops, .. } = op
            && ops
                .iter()
                .any(|o| matches!(o, scaena_core::displaylist::Op::Glyphs { text, .. } if text.contains("est.")))
        {
            estimates.push(*opacity);
        }
    });
    assert_eq!(estimates.len(), 2, "{estimates:?}");
    assert!(estimates.iter().all(|o| (o - 0.5).abs() < 1e-3), "{estimates:?}");
}

/// A rule's text over it stands where no mark is behind it (PLAN 2.84): at the plot's start
/// where it can, else along the rule past the bars that rise through it.
#[test]
fn a_rules_text_moves_along_it_clear_of_the_bars() {
    let mut deck = bars(Value::Null, Value::Null);
    let chart = deck.nodes.get_mut("c").unwrap();
    chart.props.insert("annotations".into(), json!([{ "kind": "rule", "at": { "y": 10 }, "text": "Target" }]));
    let layout = compile_sized(&themed(json!({})), &deck, [900.0, 500.0]).unwrap();
    let [left, ..] = layout.plot;
    let label = layout.notes[0].label.as_ref().unwrap();
    let first = &label.text.lines[0];
    let (x0, x1) = (label.origin[0], label.origin[0] + label.text.width);
    let (y0, y1) = (label.origin[1] + first.baseline - first.ascent, label.origin[1] + first.baseline);
    let bars: Vec<[f32; 4]> = (layout.marks.iter())
        .filter_map(|m| match m.shape {
            charts::Shape::Bar(r) => Some([r.x, r.top(), r.x + r.w, r.bottom()]),
            _ => None,
        })
        .collect();
    // The first bar, 12, rises past the rule at 10 where the text would start.
    assert!(bars[0][0] < left + label.text.width && bars[0][1] < y1, "{bars:?}");
    assert!(x0 > left, "the text moves on from the plot's start: {x0} {left}");
    for b in &bars {
        assert!(
            x1 <= b[0] || b[2] <= x0 || b[1] >= y1 || b[3] <= y0,
            "the text {:?} is over a bar {b:?}",
            [x0, y0, x1, y1]
        );
    }
}
