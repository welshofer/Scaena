//! What an inspector offers for a node in a state (ADR-0013, PLAN 2.33), on the example deck:
//! each property it edits, what it takes from the theme or the schema, the value the state
//! shows, and where that value lives.

use scaena_core::Deck;
use scaena_core::choices::{Choices, Field, Takes, Where, choices};
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
