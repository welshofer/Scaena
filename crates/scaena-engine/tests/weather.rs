//! The weather chart (ADR-0030, PLAN O.2b), on `docs/examples/weather.deck.json`: a forecast's
//! periods keyed by time, in columns across a wide box and rows down a tall one, one period alone
//! drawn large; hours stepping to fit; units converted at layout; the moon for an hour of night; a
//! highlight's band on its day; and the credit at its foot, linked.

use scaena_core::Deck;
use scaena_core::displaylist::{LinkTarget, Op};
use scaena_engine::charts::ChartLayout;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::sample::{Content, Scene};
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use serde_json::{Value, json};

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

/// `state` at rest, with `node`'s props merged with `props` first.
fn scene(state: &str, node: &str, props: Value) -> (Scene, Engine, Deck, Theme) {
    let (mut deck, theme, mut engine) = setup();
    let Value::Object(props) = props else { panic!("an object") };
    for (k, v) in props {
        deck.nodes.get_mut(node).unwrap().props.insert(k, v);
    }
    let data = DataFiles::new();
    let req = FrameRequest { deck: &deck, theme: &theme, data: &data, state, t_ms: f64::INFINITY, format: None };
    let scene = engine.at_rest(&req).unwrap();
    (scene, engine, deck, theme)
}

fn chart<'a>(scene: &'a Scene, id: &str) -> &'a ChartLayout {
    scene
        .nodes
        .iter()
        .find_map(|n| match &n.content {
            Content::Chart { chart, .. } if n.id == id => Some(&**chart),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no chart `{id}`"))
}

/// Each period's part `part`, by its period's key, with where it stands.
fn parts<'a>(chart: &'a ChartLayout, part: &str) -> Vec<(&'a str, [f32; 2], &'a str)> {
    let end = format!("\u{1f}{part}");
    chart
        .ticks
        .iter()
        .filter(|l| l.key.ends_with(&end))
        .map(|l| (&l.key[..l.key.len() - end.len()], l.origin, l.text.text.as_str()))
        .collect()
}

#[test]
fn a_week_is_a_row_of_days() {
    let (s, ..) = scene("week", "week", json!({}));
    let week = chart(&s, "week");
    let days = parts(week, "period");
    let names: Vec<&str> = days.iter().map(|d| d.2).collect();
    assert_eq!(names, ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"]);
    assert!(days.windows(2).all(|w| w[0].1[0] < w[1].1[0] && (w[0].1[1] - w[1].1[1]).abs() < 0.5), "{days:?}");
    // A sky's icon for each day, and a drop beside each chance of 10% or more.
    let skies: Vec<&str> = week.icons.iter().filter(|i| i.cell.is_some()).map(|i| i.name).collect();
    assert_eq!(skies, ["cloud-sun", "cloud-rain", "cloud-rain", "cloud", "sun", "sun", "cloud-rain"]);
    assert_eq!(week.icons.iter().filter(|i| i.name == "drop").count(), 5);
    let highs: Vec<&str> = parts(week, "high").iter().map(|p| p.2).collect();
    assert_eq!(highs, ["17°", "14°", "13°", "15°", "19°", "20°", "17°"]);
    // Each part stands in its cell: its period's row, its place among the parts.
    let sky = week.icons.iter().find(|i| i.key.starts_with("2026-10-17")).unwrap();
    assert_eq!((sky.cell, sky.words.as_str()), (Some([3, 1]), "Rain"));
}

#[test]
fn a_tall_box_sets_the_days_down() {
    let (s, ..) = scene("week", "week", json!({ "at": { "rect": [700, 160, 520, 860] } }));
    let days = parts(chart(&s, "week"), "period");
    assert_eq!(days.len(), 7);
    assert!(days.windows(2).all(|w| w[0].1[1] < w[1].1[1] && (w[0].1[0] - w[1].1[0]).abs() < 0.5), "{days:?}");
}

#[test]
fn one_day_is_drawn_large() {
    let one = json!({ "dataTransform": [{ "limit": 1 }] });
    let (s, ..) = scene("week", "week", one);
    let alone = chart(&s, "week");
    let (w, ..) = scene("week", "week", json!({}));
    let week = chart(&w, "week");
    let size = |c: &ChartLayout| c.icons.iter().find(|i| i.cell.is_some()).unwrap().size;
    assert!(size(alone) > 2.0 * size(week), "{} against {}", size(alone), size(week));
    let high = |c: &ChartLayout| parts(c, "high")[0].2.to_string();
    assert_eq!(high(alone), "17°");
    let tall = |c: &ChartLayout| c.ticks.iter().find(|l| l.key.ends_with("high")).unwrap().text.height;
    assert!(tall(alone) > 1.5 * tall(week));
}

#[test]
fn hours_step_to_fit() {
    let (s, ..) = scene("today", "day", json!({}));
    let hours = parts(chart(&s, "day"), "period");
    assert_eq!(hours.len(), 12, "two hours a column across the slide: {hours:?}");
    assert_eq!((hours[0].2, hours[1].2), ("12 AM", "2 AM"));
    let (s, ..) = scene("today", "day", json!({ "weather": { "every": 6 } }));
    let names: Vec<&str> = parts(chart(&s, "day"), "period").iter().map(|p| p.2).collect();
    assert_eq!(names, ["12 AM", "6 AM", "12 PM", "6 PM"]);
    let (s, ..) = scene("today", "day", json!({ "at": { "rect": [100, 200, 600, 300] } }));
    assert_eq!(parts(chart(&s, "day"), "period").len(), 4, "six hours a row down a narrow box");
}

#[test]
fn units_convert_at_layout() {
    let (s, ..) = scene("week", "week", json!({ "weather": { "units": "us", "show": ["period", "high", "wind"] } }));
    let week = chart(&s, "week");
    // 17.4 °C and 13.1 km/h, the first day's.
    assert_eq!(parts(week, "high")[0].2, "63°");
    assert_eq!(parts(week, "wind")[0].2, "8 mph");
    assert!(week.icons.is_empty(), "no sky shown");
    let (s, ..) = scene("today", "day", json!({ "weather": { "units": "metric" } }));
    assert_eq!(parts(chart(&s, "day"), "high")[0].2, "11°");
}

#[test]
fn night_hours_take_the_moon() {
    let (s, ..) = scene("today", "day", json!({}));
    let day = chart(&s, "day");
    let sky = |key: &str| day.icons.iter().find(|i| i.key.starts_with(key) && i.cell.is_some()).unwrap().name;
    assert_eq!(sky("2026-10-15T02:00"), "moon");
    assert_eq!(sky("2026-10-15T06:00"), "cloud-moon");
    assert_eq!(sky("2026-10-15T08:00"), "cloud-sun");
    assert_eq!(sky("2026-10-15T18:00"), "cloud-rain");
}

#[test]
fn a_highlight_marks_its_day() {
    let (s, ..) = scene("week", "week", json!({}));
    let week = chart(&s, "week");
    let bands: Vec<[f32; 4]> = week.notes.iter().filter_map(|n| n.band.map(|b| b.0)).collect();
    assert_eq!(bands.len(), 1);
    let saturday = parts(week, "period").into_iter().find(|p| p.2 == "Sat").unwrap();
    let band = bands[0];
    assert!(band[0] <= saturday.1[0] && saturday.1[0] < band[0] + band[2], "{band:?} {saturday:?}");
    let (s, ..) = scene("week", "week", json!({ "annotations": null }));
    assert!(chart(&s, "week").notes.is_empty());
}

#[test]
fn the_credit_is_drawn_and_links() {
    let (deck, theme, mut engine) = setup();
    let data = DataFiles::new();
    let req =
        FrameRequest { deck: &deck, theme: &theme, data: &data, state: "today", t_ms: f64::INFINITY, format: None };
    let dl = engine.frame(&req).unwrap().display_list;
    let layer = dl
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Layer { node: Some(n), ops, .. } if n == "day" => Some(ops),
            _ => None,
        })
        .unwrap();
    let foot = layer
        .iter()
        .find_map(|op| match op {
            Op::Layer { cell: Some(c), ops, .. } if *c == scaena_core::fetched::FOOT_CELL => Some(ops),
            _ => None,
        })
        .expect("the credit's layer");
    assert!(
        scaena_core::reading::drawn(foot).starts_with("Boston, Massachusetts\u{a0}· Oct 15, 2026\u{a0}· Weather data")
    );
    let hrefs: Vec<LinkTarget> = dl.links().into_iter().map(|l| l.target).collect();
    assert!(hrefs.contains(&LinkTarget::Href("https://open-meteo.com/".into())), "{hrefs:?}");
    // The sky's cells say the sky in words.
    let said: Vec<&str> = layer
        .iter()
        .filter_map(|op| match op {
            Op::Layer { cell: Some(_), label: Some(l), .. } => Some(l.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(said.first(), Some(&"Mostly clear"));
}

/// A highlight that moves from one day to another between two states moves its band by key, as a
/// chart's parts morph: halfway through the cue, it stands between them.
#[test]
fn a_highlight_moves_between_states_by_key() {
    let (mut deck, theme, mut engine) = setup();
    let mut next = deck.states[0].clone();
    next.id = "sunday".into();
    next.props = serde_json::from_value(
        json!({ "week": { "annotations": [{ "kind": "highlight", "at": { "x": ["2026-10-18"] } }] } }),
    )
    .unwrap();
    next.transition = Some(json!({ "duration": 1000 }));
    deck.states.insert(1, next);
    let data = DataFiles::new();
    let band = |engine: &mut Engine, deck: &Deck, state: &str, t_ms: f64| -> f32 {
        let req = FrameRequest { deck, theme: &theme, data: &data, state, t_ms, format: None };
        let dl = engine.frame(&req).unwrap().display_list;
        let layer = dl
            .ops
            .iter()
            .find_map(|op| match op {
                Op::Layer { node: Some(n), ops, .. } if n == "week" => Some(ops.clone()),
                _ => None,
            })
            .unwrap();
        layer
            .iter()
            .find_map(|op| match op {
                Op::Fill { path, .. } => match path.0.first() {
                    Some(scaena_core::displaylist::PathEl::MoveTo([x, _])) => Some(*x),
                    _ => None,
                },
                _ => None,
            })
            .expect("the band, first")
    };
    let saturday = band(&mut engine, &deck, "week", f64::INFINITY);
    let sunday = band(&mut engine, &deck, "sunday", f64::INFINITY);
    let between = band(&mut engine, &deck, "sunday", 500.0);
    assert!(saturday < between && between < sunday, "{saturday} {between} {sunday}");
}

/// A forecast squashed into a small box sets its parts smaller than 12 pt at presentation size
/// (W312), and leaves too little room to read it (W313).
#[test]
fn a_squashed_forecast_is_found() {
    let (mut deck, theme, mut engine) = setup();
    deck.nodes.get_mut("week").unwrap().props.insert("at".into(), json!({ "rect": [200, 200, 110, 600] }));
    let data = DataFiles::new();
    let found = scaena_engine::lint::lint(&mut engine, &deck, &theme, &data, None).unwrap();
    let codes: Vec<(&str, Option<&str>)> =
        found.iter().map(|f| (f.code.as_str(), f.node.as_deref())).filter(|(_, n)| *n == Some("week")).collect();
    assert!(codes.contains(&("W312", Some("week"))), "{found:#?}");
    assert!(codes.contains(&("W313", Some("week"))), "{found:#?}");
    let (deck, theme, mut engine) = setup();
    let found = scaena_engine::lint::lint(&mut engine, &deck, &theme, &data, None).unwrap();
    assert!(found.iter().all(|f| !matches!(f.code.as_str(), "W312" | "W313")), "{found:#?}");
}

/// A credit too long for one line breaks after a separator, never before it: no line of it
/// starts with ` · `, whose space before the dot is laid out as one that does not break.
#[test]
fn no_line_of_a_credit_starts_with_its_separator() {
    let (scene, ..) = scene("week", "week", json!({}));
    let foot = chart(&scene, "week").credit.as_ref().expect("the credit");
    let text = &foot.text;
    assert!(text.lines.len() > 1, "the week's credit runs to two lines: {:?}", text.text);
    for line in &text.lines {
        let words = text.text[line.text.clone()].trim_start();
        assert!(!words.starts_with('\u{b7}'), "a line starts with the separator: {words:?}");
    }
}
