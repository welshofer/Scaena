//! What an inspector offers for a node in a state (ADR-0013, PLAN 2.33), on the example deck:
//! each property it edits, what it takes from the theme or the schema, the value the state
//! shows, and where that value lives.

use scaena_core::Deck;
use scaena_core::choices::{Choices, Field, StateChoices, Takes, Where, characters, choices, state_choices};
use scaena_core::data::Texts;
use scaena_core::model::Theme;
use scaena_core::model::theme::Vocabulary;
use scaena_core::patch::compile;
use scaena_core::validate::BundleFiles;
use serde_json::{Value, json};
use std::path::Path;

const EXAMPLE: &str = include_str!("../../../docs/examples/revenue.deck.json");
const DUSK: &str = include_str!("../../../docs/examples/themes/dusk.theme.json");

struct Examples;

impl BundleFiles for Examples {
    fn exists(&self, path: &str) -> bool {
        Path::new("../../docs/examples").join(path).is_file()
    }

    fn read_text(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(Path::new("../../docs/examples").join(path)).ok()
    }
}

fn offered(doc: &Value, state: &str, node: &str) -> Choices {
    let deck: Deck = serde_json::from_value(doc.clone()).unwrap();
    choices(&deck, &Theme::from_json(DUSK).unwrap(), state, node, &Texts(&Examples)).unwrap()
}

fn field<'a>(c: &'a Choices, prop: &str) -> &'a Field {
    c.fields.iter().find(|f| f.prop == prop).unwrap_or_else(|| panic!("no {prop} in {:?}", c.fields))
}

fn example() -> Value {
    serde_json::from_str(EXAMPLE).unwrap()
}

#[test]
fn a_text_offers_its_role_style_and_props_from_the_theme_and_the_schema() {
    let c = offered(&example(), "revenue", "title");
    let props: Vec<&str> = c.fields.iter().map(|f| f.prop.as_str()).collect();
    assert_eq!(
        props,
        [
            "role",
            "style/family",
            "style/weight",
            "style/italic",
            "style/size",
            "style/color",
            "style/case",
            "align/x",
            "fit",
            "wrap",
            "maxLines",
            "opacity",
            "transform/rotate",
            "enter",
            "exit",
            "alt",
            "semantic"
        ]
    );
    let role = field(&c, "role");
    let Takes::Name { of: Vocabulary::TextRole, names, overrides: false } = &role.takes else { panic!("{role:?}") };
    assert_eq!(names[..3], ["display", "headline", "title"], "the theme's roles, in its order");
    assert_eq!(
        (role.value.clone(), role.lives.clone()),
        (Some(json!("headline")), Some(Where::State("revenue".into())))
    );

    // A color is a role or a token, each once; one written out is an override.
    let color = field(&c, "style/color");
    let Takes::Name { of: Vocabulary::Color, names, overrides: true } = &color.takes else { panic!("{color:?}") };
    assert_eq!(names.iter().filter(|n| *n == "accent").count(), 1, "{names:?}");
    assert_eq!((color.value.as_ref(), color.lives.as_ref()), (None, None), "the role's color shows");

    // What the schema allows: words, numbers in range, and a text size, always an override.
    assert_eq!(
        field(&c, "fit").takes,
        Takes::Word { words: ["wrap", "shrink", "grow", "clip", "error"].map(String::from).into() }
    );
    assert_eq!(
        field(&c, "style/weight").takes,
        Takes::Number { min: Some(1.0), above: None, max: Some(1000.0), whole: true, overrides: false }
    );
    assert_eq!(
        field(&c, "style/size").takes,
        Takes::Number { min: None, above: Some(0.0), max: None, whole: false, overrides: true }
    );
    assert_eq!(
        field(&c, "opacity").takes,
        Takes::Number { min: Some(0.0), above: None, max: Some(1.0), whole: false, overrides: false }
    );
    let Takes::Name { of: Vocabulary::MotionPreset, names, .. } = &field(&c, "enter").takes else { panic!() };
    assert!(names.contains(&"rise".to_string()), "{names:?}");

    // Where its lines stand across its box (PLAN 3.25): none stretches, and the slot's shows.
    let align = field(&c, "align/x");
    assert_eq!(align.takes, Takes::Word { words: ["start", "center", "end"].map(String::from).into() });
    assert_eq!((align.value.as_ref(), align.lives.as_ref()), (None, None), "the slot's alignment shows");

    // `mix` tracks the role from `revenue`, where it lives; `intro` shows the node's own.
    assert_eq!(field(&offered(&example(), "mix", "title"), "role").lives, Some(Where::State("revenue".into())));
    let intro = offered(&example(), "intro", "title");
    assert_eq!(
        (field(&intro, "role").value.clone(), field(&intro, "role").lives.clone()),
        (Some(json!("display")), Some(Where::Node))
    );
}

#[test]
fn each_node_type_offers_what_its_theme_names() {
    // A shader takes the presets of its kind: Dusk's `backdrop` is a mesh, `texture` noise.
    let bg = offered(&example(), "intro", "bg");
    let Takes::Name { names, .. } = &field(&bg, "preset").takes else { panic!() };
    assert_eq!(names, &["backdrop"]);
    assert_eq!(
        (field(&bg, "palette").value.clone(), field(&bg, "palette").lives.clone()),
        (Some(json!("ambient")), Some(Where::Node))
    );

    // A chart's kind is a word, and `mix` sets its own.
    let rev = offered(&example(), "mix", "rev");
    let kind = field(&rev, "kind");
    let Takes::Word { words } = &kind.takes else { panic!("{kind:?}") };
    assert!(words.contains(&"stackedBar".to_string()) && words.contains(&"donut".to_string()), "{words:?}");
    assert_eq!((kind.value.clone(), kind.lives.clone()), (Some(json!("stackedBar")), Some(Where::State("mix".into()))));
    assert!(rev.fields.iter().any(|f| f.prop == "labels/role"));

    // A node not on screen offers nothing: an inspector edits what the state shows.
    let deck: Deck = serde_json::from_value(example()).unwrap();
    let e = choices(&deck, &Theme::from_json(DUSK).unwrap(), "intro", "rev", &Texts(&Examples)).unwrap_err();
    assert!(e.contains("not on screen in `intro`"), "{e}");
}

/// What a chart reads, and how, from its data (PLAN 2.41): its source, from the deck's; each
/// channel's field, from the columns of the table it reads that the channel can read; the
/// type it reads it as; and its key.
#[test]
fn a_chart_offers_what_it_reads_from_the_columns_it_has() {
    let rev = offered(&example(), "revenue", "rev");
    let props: Vec<&str> = rev.fields.iter().map(|f| f.prop.as_str()).collect();
    assert_eq!(
        props,
        [
            "data",
            "kind",
            "orient",
            "x/field",
            "x/type",
            "y/field",
            "y/type",
            "series/field",
            "color/field",
            "sizeEncoding/field",
            "facet/field",
            "key",
            "labels/show",
            "labels/role",
            "opacity",
            "transform/rotate",
            "enter",
            "exit",
            "alt",
            "semantic"
        ]
    );
    let words = |prop: &str| match &field(&rev, prop).takes {
        Takes::Word { words } => words.clone(),
        takes => panic!("{prop}: {takes:?}"),
    };
    assert_eq!(words("data"), ["@q3"]);
    // A bar chart's bars turn (PLAN 1.29); a chart of another kind has none to turn.
    assert_eq!(words("orient"), ["vertical", "horizontal"]);
    let mut lines = example();
    lines["nodes"]["rev"]["kind"] = json!("line");
    assert!(offered(&lines, "revenue", "rev").fields.iter().all(|f| f.prop != "orient"));
    // A range's values have intervals, each end a column of numbers (PLAN 1.30); no other
    // kind's do.
    assert!(rev.fields.iter().all(|f| !f.prop.starts_with("interval/")));
    let mut ranges = example();
    ranges["nodes"]["rev"]["kind"] = json!("range");
    let ranges = offered(&ranges, "revenue", "rev");
    for end in ["interval/low", "interval/high"] {
        let Takes::Word { words } = &field(&ranges, end).takes else { panic!("{end} offers columns") };
        assert_eq!(words, &["revenue", "customers"], "{end}");
    }
    assert_eq!(
        (field(&rev, "data").value.clone(), field(&rev, "data").lives.clone()),
        (Some(json!("@q3")), Some(Where::Node))
    );
    // An ordinal x reads any column; a quantitative y, and a size, the numbers.
    assert_eq!(words("x/field"), ["quarter", "product", "revenue", "customers"]);
    assert_eq!(words("y/field"), ["revenue", "customers"]);
    assert_eq!(words("sizeEncoding/field"), ["revenue", "customers"]);
    assert_eq!(words("x/type"), ["quantitative", "ordinal", "nominal", "temporal"]);
    assert_eq!(words("key"), ["quarter", "product", "revenue", "customers"]);
    assert_eq!(field(&rev, "y/field").value, Some(json!("revenue")));
    assert_eq!(field(&rev, "sizeEncoding/field").value, None);

    // A source not handed over offers no columns, and its source all the same.
    let deck: Deck = serde_json::from_value(example()).unwrap();
    let none = std::collections::BTreeMap::<String, Vec<u8>>::new();
    let blind = choices(&deck, &Theme::from_json(DUSK).unwrap(), "revenue", "rev", &none).unwrap();
    assert!(blind.fields.iter().any(|f| f.prop == "data"));
    assert!(!blind.fields.iter().any(|f| f.prop.ends_with("/field") || f.prop == "key"));
}

/// The example, with a second source of other columns, inline: segments, not quarters.
fn with_segments() -> Value {
    let mut doc = example();
    doc["data"]["segments"] = json!({
        "source": { "inline": [
            { "segment": "Core", "region": "West", "sales": 30.1, "accounts": 1600 },
            { "segment": "Pro", "region": "West", "sales": 21.4, "accounts": 990 },
        ] },
        "schema": { "segment": "string", "region": "string", "sales": "number", "accounts": "number" },
    });
    doc
}

/// A source chosen (PLAN 2.41) is written where the chart's `data` lives, and what it cannot
/// serve is pointed again there: an axis whose field it lacks reads a column it can that no
/// other channel reads, of the type its field had where there is one, and a series or a color
/// with none is taken away; with `fork`, all of it is the state's own, a data update.
#[test]
fn a_source_chosen_points_again_what_it_cannot_serve() {
    let choose = |doc: &Value, state: &str, fork: bool| {
        let ops = json!([{ "op": "choose", "node": "rev", "prop": "data", "value": "@segments", "state": state, "fork": fork }]);
        compile(doc, ops.as_array().unwrap(), &Examples).map(|c| c.doc)
    };
    let doc = choose(&with_segments(), "revenue", false).unwrap();
    let rev = &doc["nodes"]["rev"];
    assert_eq!(rev["data"], "@segments");
    assert_eq!(rev["x"], json!({ "field": "segment", "type": "ordinal" }), "the first column x can read");
    assert_eq!(rev["y"]["field"], "sales", "the first number");
    assert!(rev.get("series").is_none() && rev.get("color").is_none(), "taken away: {rev}");
    assert_eq!(rev["y"]["format"], "$,.1f", "what it reads by is kept");
    let c = offered(&doc, "revenue", "rev");
    assert_eq!(field(&c, "x/field").value, Some(json!("segment")));

    // Kept to `mix`: a data update there, the node as it was.
    let doc = choose(&with_segments(), "mix", true).unwrap();
    assert_eq!(doc["nodes"]["rev"]["data"], "@q3");
    let mix = &doc["states"][2]["props"]["rev"];
    assert_eq!(mix["data"], "@segments", "{mix}");
    assert_eq!((mix["x"]["field"].clone(), mix["y"]["field"].clone()), (json!("segment"), json!("sales")));
    assert_eq!((mix["series"].clone(), mix["color"].clone()), (Value::Null, Value::Null), "taken away there");

    // A source of the same columns changes the source alone.
    let mut same = with_segments();
    same["data"]["q4"] = same["data"]["q3"].clone();
    let ops = json!([{ "op": "choose", "node": "rev", "prop": "data", "value": "@q4", "state": "revenue" }]);
    let ours = compile(&same, ops.as_array().unwrap(), &Examples).unwrap().doc;
    let mut expected = same.clone();
    expected["nodes"]["rev"]["data"] = json!("@q4");
    assert_eq!(ours["nodes"], expected["nodes"]);

    // An axis takes a column of the type its field had: a name for the quarters, a number for
    // the revenue, wherever the source has them.
    let mut first = with_segments();
    first["data"]["numbers"] = json!({
        "source": { "inline": [{ "sales": 30.1, "accounts": 1600, "region": "West" }] },
        "schema": { "sales": "number", "accounts": "number", "region": "string" },
    });
    let ops = json!([{ "op": "choose", "node": "rev", "prop": "data", "value": "@numbers", "state": "revenue" }]);
    let doc = compile(&first, ops.as_array().unwrap(), &Examples).unwrap().doc;
    let rev = &doc["nodes"]["rev"];
    assert_eq!((rev["x"]["field"].clone(), rev["y"]["field"].clone()), (json!("region"), json!("sales")), "{rev}");

    // A source with no column a `y` reads refuses, and says what it has.
    let mut names = with_segments();
    names["data"]["names"] = json!({ "source": { "inline": [{ "name": "Ada" }] } });
    let ops = json!([{ "op": "choose", "node": "rev", "prop": "data", "value": "@names", "state": "revenue" }]);
    let e = compile(&names, ops.as_array().unwrap(), &Examples).unwrap_err().to_string();
    assert!(
        e.contains("`@names` has no column the chart's `y` can read as quantitative") && e.contains("`name`"),
        "{e}"
    );
    // One the deck does not declare says which it does.
    let e = choose(&example(), "revenue", false).unwrap_err().to_string();
    assert!(e.contains("no data source `@segments`; it has `@q3`"), "{e}");

    // A table keeps the columns it lists that the source has, and shows every column where it
    // has none of them; a key the source lacks goes.
    let mut doc = with_segments();
    doc["nodes"]["tbl"] = json!({
        "type": "table", "data": "@q3", "key": "revenue",
        "columns": [{ "field": "quarter" }, { "field": "revenue" }],
    });
    let ops = json!([{ "op": "choose", "node": "tbl", "prop": "data", "value": "@segments" }]);
    let doc = compile(&doc, ops.as_array().unwrap(), &Examples).unwrap().doc;
    assert_eq!(doc["nodes"]["tbl"], json!({ "type": "table", "data": "@segments" }));
}

#[test]
fn a_value_written_out_lives_in_the_overrides_and_says_so() {
    let ops =
        json!([{ "op": "choose", "node": "title", "prop": "style/color", "value": "#ff3366", "state": "revenue" }]);
    let doc = compile(&example(), ops.as_array().unwrap(), &Examples).unwrap().doc;
    for state in ["intro", "revenue", "close"] {
        let c = offered(&doc, state, "title");
        let color = field(&c, "style/color");
        assert_eq!(color.value, Some(json!("#ff3366")), "{state}");
        assert_eq!(color.lives, Some(Where::Overrides), "{state}");
        assert!(color.literal, "{state}");
    }
    assert!(!field(&offered(&doc, "revenue", "title"), "role").literal);
}

/// How far a node is turned (PLAN 2.51) is its `transform`'s `rotate`: a number of degrees,
/// never an override, written where the transform lives, its other keys kept. `null` takes
/// the turn away there, and the transform with it when nothing else is left.
#[test]
fn a_turn_is_written_where_the_transform_lives() {
    let turn = |doc: &Value, state: &str, value: Value, fork: bool| {
        let ops = json!([{ "op": "choose", "node": "title", "prop": "transform/rotate", "value": value, "state": state, "fork": fork }]);
        compile(doc, ops.as_array().unwrap(), &Examples).unwrap().doc
    };
    let angle = field(&offered(&example(), "revenue", "title"), "transform/rotate").clone();
    assert_eq!(angle.takes, Takes::Number { min: None, above: None, max: None, whole: false, overrides: false });
    assert_eq!(shown(&angle), (None, None), "upright: nothing sets it");

    // Nothing sets it: the node's own, in every state.
    let doc = turn(&example(), "revenue", json!(90), false);
    assert_eq!(doc["nodes"]["title"]["transform"], json!({ "rotate": 90 }));
    for state in ["intro", "revenue", "mix", "close"] {
        let angle = field(&offered(&doc, state, "title"), "transform/rotate").clone();
        assert_eq!(shown(&angle), (Some(json!(90)), Some(Where::Node)), "{state}");
        assert!(!angle.literal, "{state}");
    }

    // Kept to `revenue`: its own, and `mix`, which tracks from it, shows it too.
    let doc = turn(&example(), "revenue", json!(-6), true);
    assert!(doc["nodes"]["title"].get("transform").is_none());
    assert_eq!(doc["states"][1]["props"]["title"]["transform"], json!({ "rotate": -6 }));
    let mix = field(&offered(&doc, "mix", "title"), "transform/rotate").clone();
    assert_eq!(shown(&mix), (Some(json!(-6)), Some(Where::State("revenue".into()))));
    assert_eq!(shown(field(&offered(&doc, "intro", "title"), "transform/rotate")), (None, None));

    // Turned again from `mix`: where it lives, `revenue`, its other keys kept.
    let mut scaled = doc.clone();
    scaled["states"][1]["props"]["title"]["transform"]["scale"] = json!(2);
    let doc = turn(&scaled, "mix", json!(30), false);
    assert_eq!(doc["states"][1]["props"]["title"]["transform"], json!({ "rotate": 30, "scale": 2 }));
    assert!(doc["states"][2]["props"]["title"].get("transform").is_none());

    // Taken away where it lives: the scale stays; with nothing else, the transform goes.
    let doc = turn(&doc, "mix", Value::Null, false);
    assert_eq!(doc["states"][1]["props"]["title"]["transform"], json!({ "scale": 2 }));
    let upright = turn(&turn(&example(), "revenue", json!(90), false), "revenue", Value::Null, false);
    assert_eq!(upright["nodes"]["title"], example()["nodes"]["title"]);
}

/// What a reader hears of a node (PLAN 2.56, SPEC §3.12): its description (`alt`), words for
/// people, and its part in the story (`semantic`), one of the schema's words, `decoration`
/// among them. Each is written where it lives, as any choice is, and taken away there.
#[test]
fn what_a_reader_hears_is_chosen_where_it_lives() {
    let choose = |doc: &Value, prop: &str, state: &str, value: Value| {
        let ops = json!([{ "op": "choose", "node": "title", "prop": prop, "value": value, "state": state }]);
        compile(doc, ops.as_array().unwrap(), &Examples).unwrap().doc
    };
    let title = offered(&example(), "mix", "title");
    let alt = field(&title, "alt");
    assert_eq!((&alt.takes, shown(alt)), (&Takes::Text, (None, None)), "the title says its words");
    assert!(!alt.literal);
    let part = field(&title, "semantic");
    let Takes::Word { words } = &part.takes else { panic!("{part:?}") };
    assert_eq!(
        words,
        &["claim", "evidence", "annotation", "context", "comparison", "takeaway", "source", "navigation", "decoration"]
    );
    // `revenue` makes the title the claim, and `mix` tracks it from there; `intro` shows its own.
    assert_eq!(shown(part), (Some(json!("claim")), Some(Where::State("revenue".into()))));
    assert_eq!(
        shown(field(&offered(&example(), "intro", "title"), "semantic")),
        (Some(json!("navigation")), Some(Where::Node))
    );
    let rev = field(&offered(&example(), "mix", "rev"), "alt").clone();
    assert_eq!(shown(&rev), (Some(json!("Quarterly revenue by product, Q4 2025 through Q3 2026.")), Some(Where::Node)));

    // A description nothing sets goes on the node, in every state; a part, where it lives.
    let doc = choose(&example(), "alt", "mix", json!("Revenue, quarter by quarter"));
    assert_eq!(doc["nodes"]["title"]["alt"], json!("Revenue, quarter by quarter"));
    let doc = choose(&doc, "semantic", "mix", json!("takeaway"));
    assert_eq!(doc["states"][1]["props"]["title"]["semantic"], json!("takeaway"));
    assert_eq!(doc["nodes"]["title"]["semantic"], json!("navigation"), "the node's own is kept");
    for (state, part) in [("intro", "navigation"), ("revenue", "takeaway"), ("mix", "takeaway")] {
        assert_eq!(field(&offered(&doc, state, "title"), "semantic").value, Some(json!(part)), "{state}");
    }

    // Taken away where it lives: the node's own description goes, and `intro`'s part with it.
    let doc = choose(&choose(&doc, "alt", "close", Value::Null), "semantic", "intro", Value::Null);
    assert!(doc["nodes"]["title"].get("alt").is_none() && doc["nodes"]["title"].get("semantic").is_none());
}

fn state(doc: &Value, state: &str) -> StateChoices {
    let deck: Deck = serde_json::from_value(doc.clone()).unwrap();
    state_choices(&deck, &Theme::from_json(DUSK).unwrap(), state).unwrap()
}

fn own<'a>(c: &'a StateChoices, prop: &str) -> &'a Field {
    c.fields.iter().find(|f| f.prop == prop).unwrap_or_else(|| panic!("no {prop} in {:?}", c.fields))
}

/// The value a field shows, and where it lives.
fn shown(f: &Field) -> (Option<Value>, Option<Where>) {
    (f.value.clone(), f.lives.clone())
}

#[test]
fn a_state_offers_its_layout_transition_hold_and_notes() {
    let mix = state(&example(), "mix");
    let props: Vec<&str> = mix.fields.iter().map(|f| f.prop.as_str()).collect();
    assert_eq!(
        props,
        ["layout", "transition/duration", "transition/ease", "transition/spring", "transition/match", "hold", "notes"]
    );
    // The layout `mix` takes from `revenue`, which it builds on, and where it lives.
    let layout = own(&mix, "layout");
    let Takes::Name { of: Vocabulary::Layout, names, overrides: false } = &layout.takes else { panic!("{layout:?}") };
    assert_eq!(
        names,
        &["full", "figure", "narrow-figure"],
        "the theme's layouts with a slot for each node `revenue` and `mix` place in one, in its order"
    );
    assert_eq!(shown(layout), (Some(json!("figure")), Some(Where::State("revenue".into()))));
    // A bare duration is the transition's duration; its other keys are not set.
    assert_eq!(shown(own(&mix, "transition/duration")), (Some(json!("slow")), Some(Where::State("mix".into()))));
    let Takes::Name { of: Vocabulary::Easing, names, .. } = &own(&mix, "transition/ease").takes else { panic!() };
    assert_eq!(names, &["standard", "in", "out", "linear"]);
    assert_eq!(shown(own(&mix, "transition/ease")), (None, None));
    let Takes::Name { of: Vocabulary::Spring, .. } = &own(&mix, "transition/spring").takes else { panic!() };
    assert_eq!(own(&mix, "transition/match").takes, Takes::Word { words: vec!["id".into(), "none".into()] });
    // Its hold, a number of milliseconds, and its notes, words for people.
    assert_eq!(
        own(&mix, "hold").takes,
        Takes::Number { min: Some(0.0), above: None, max: None, whole: false, overrides: false }
    );
    assert_eq!(shown(own(&mix, "hold")), (Some(json!(6000.0)), Some(Where::State("mix".into()))));
    assert_eq!(own(&mix, "notes").takes, Takes::Text);
    assert!(own(&mix, "notes").value.as_ref().is_some_and(|n| n.as_str().unwrap().starts_with("Same bars")));

    // `revenue` sets its own layout and a transition object; `intro` sets none, so it cuts.
    let revenue = state(&example(), "revenue");
    assert_eq!(shown(own(&revenue, "layout")), (Some(json!("figure")), Some(Where::State("revenue".into()))));
    assert_eq!(
        shown(own(&revenue, "transition/ease")),
        (Some(json!("standard")), Some(Where::State("revenue".into())))
    );
    assert!(
        state(&example(), "intro")
            .fields
            .iter()
            .filter(|f| f.prop.starts_with("transition/"))
            .all(|f| f.value.is_none())
    );
    assert_eq!(shown(own(&revenue, "notes")), (None, None));
    let intro = state(&example(), "intro");
    let Takes::Name { names, .. } = &own(&intro, "layout").takes else { panic!() };
    assert_eq!(names, &["title"], "the title and the subtitle have slots in `title` alone");

    // An empty slide (`absolute`) with no layout takes none from anywhere.
    let ops = json!([{ "op": "add_state", "state": { "id": "blank", "mode": "absolute" }, "after": "close" }]);
    let doc = compile(&example(), ops.as_array().unwrap(), &Examples).unwrap().doc;
    let blank = state(&doc, "blank");
    assert_eq!(shown(own(&blank, "layout")), (None, None));
    let Takes::Name { names, .. } = &own(&blank, "layout").takes else { panic!() };
    assert_eq!(names.len(), 11, "with nothing placed in a slot, every layout fits: {names:?}");
    let deck: Deck = serde_json::from_value(example()).unwrap();
    assert!(state_choices(&deck, &Theme::from_json(DUSK).unwrap(), "nowhere").is_err());
}

#[test]
fn characters_offer_a_runs_look_and_show_the_first_ones() {
    let deck = |doc: &Value| -> Deck { serde_json::from_value(doc.clone()).unwrap() };
    let theme = Theme::from_json(DUSK).unwrap();
    // "doubled" made bold in `revenue`, where the title's text lives.
    let ops = json!([{ "op": "style_text", "node": "title", "state": "revenue", "from": 8, "to": 15,
                       "look": { "style/weight": 700, "style/color": "accent" } }]);
    let doc = compile(&example(), ops.as_array().unwrap(), &Examples).unwrap().doc;
    let c = characters(&deck(&doc), &theme, "revenue", "title", (9, 12)).unwrap();
    let props: Vec<&str> = c.fields.iter().map(|f| f.prop.as_str()).collect();
    assert_eq!(
        props,
        ["role", "emphasis", "style/family", "style/weight", "style/italic", "style/tracking", "style/color"]
    );
    assert!(matches!(field(&c, "style/italic").takes, Takes::Flag), "italic, or not");
    assert!(
        matches!(field(&c, "style/tracking").takes, Takes::Number { overrides: false, whole: false, .. }),
        "tracking in em, a pair's on its first letter (PLAN 1.38)"
    );
    let at = |prop: &str| (field(&c, prop).value.clone(), field(&c, prop).lives.clone());
    assert_eq!(at("style/weight"), (Some(json!(700)), Some(Where::State("revenue".into()))));
    assert_eq!(at("style/color"), (Some(json!("accent")), Some(Where::State("revenue".into()))));
    assert_eq!(at("role"), (None, None), "the run takes the node's role");
    assert!(
        matches!(&field(&c, "role").takes, Takes::Name { of: Vocabulary::TextRole, names, overrides: false } if names.contains(&"caption".to_string()))
    );
    assert!(
        matches!(&field(&c, "style/color").takes, Takes::Name { overrides: false, .. }),
        "a run takes no color written out"
    );
    assert!(matches!(&field(&c, "emphasis").takes, Takes::Word { words } if words == &["high", "low"]));
    // Its first characters read as the node does: nothing of their own.
    let c = characters(&deck(&doc), &theme, "revenue", "title", (0, 12)).unwrap();
    assert!(c.fields.iter().all(|f| f.value.is_none()), "{:?}", c.fields);
    // No characters, past the end, or no text: refused.
    let e = characters(&deck(&doc), &theme, "revenue", "title", (3, 3)).unwrap_err();
    assert!(e.contains("15 characters"), "{e}");
    assert!(characters(&deck(&doc), &theme, "revenue", "title", (3, 16)).is_err());
    assert!(characters(&deck(&doc), &theme, "revenue", "rev", (0, 1)).unwrap_err().contains("no text"));
}

/// One keyword aligns both axes: an inspector shows it as the axis's own (PLAN 3.25).
#[test]
fn one_keyword_shows_as_each_axis() {
    let mut doc = example();
    doc["nodes"]["note"]["align"] = json!("center");
    let note = offered(&doc, "revenue", "note");
    let align = field(&note, "align/x");
    assert_eq!((align.value.clone(), align.lives.clone()), (Some(json!("center")), Some(Where::Node)));
    doc["nodes"]["note"]["align"] = json!({ "y": "end" });
    assert_eq!(field(&offered(&doc, "revenue", "note"), "align/x").value, None, "it sets no `x`");
}
