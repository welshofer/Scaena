//! What an inspector offers for a node in a state (ADR-0013, PLAN 2.33), on the example deck:
//! each property it edits, what it takes from the theme or the schema, the value the state
//! shows, and where that value lives.

use scaena_core::Deck;
use scaena_core::choices::{Choices, Field, StateChoices, Takes, Where, characters, choices, state_choices};
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
    choices(&deck, &Theme::from_json(DUSK).unwrap(), state, node).unwrap()
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
            "fit",
            "wrap",
            "maxLines",
            "opacity",
            "enter",
            "exit"
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
    let e = choices(&deck, &Theme::from_json(DUSK).unwrap(), "intro", "rev").unwrap_err();
    assert!(e.contains("not on screen in `intro`"), "{e}");
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
    assert_eq!(props, ["role", "emphasis", "style/family", "style/weight", "style/italic", "style/color"]);
    assert!(matches!(field(&c, "style/italic").takes, Takes::Flag), "italic, or not");
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
