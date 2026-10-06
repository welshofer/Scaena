//! A node's look, picked up and put down (PLAN 2.58), on the example deck: the properties of
//! its type's look with the values a state shows, and a `choose` of each that another node shows
//! otherwise, written where that node's own value lives.

use scaena_core::Deck;
use scaena_core::data::Texts;
use scaena_core::looks::{Look, Part, look, putting};
use scaena_core::model::Theme;
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

fn deck(doc: &Value) -> Deck {
    serde_json::from_value(doc.clone()).unwrap()
}

fn picked(doc: &Value, state: &str, node: &str) -> Look {
    look(&deck(doc), &Theme::from_json(DUSK).unwrap(), state, node, &Texts(&Examples)).unwrap()
}

/// `look` put on `nodes` in `state`, and the deck its patch makes.
fn put_on(doc: &Value, state: &str, look: &Look, nodes: &[&str]) -> (scaena_core::looks::Put, Value) {
    let nodes: Vec<String> = nodes.iter().map(|n| n.to_string()).collect();
    let put = putting(&deck(doc), &Theme::from_json(DUSK).unwrap(), state, look, &nodes, &Texts(&Examples)).unwrap();
    let made = compile(doc, &put.patch, &Examples).unwrap().doc;
    (put, made)
}

#[test]
fn a_look_is_its_types_props_as_the_state_shows_them() {
    let doc: Value = serde_json::from_str(EXAMPLE).unwrap();
    // The title in `revenue` is a headline there; nothing else of its look is set.
    let title = picked(&doc, "revenue", "title");
    let props: Vec<&str> = title.props.iter().map(|p| p.prop.as_str()).collect();
    assert_eq!(
        props,
        ["role", "style/family", "style/weight", "style/italic", "style/size", "style/color", "style/case"]
    );
    assert_eq!(title.props[0], Part { prop: "role".into(), value: Some(json!("headline")) });
    assert!(title.props[1..].iter().all(|p| p.value.is_none()), "{:?}", title.props);
    // A chart's look is its labels.
    let rev = picked(&doc, "revenue", "rev");
    assert_eq!(
        rev.props,
        [
            Part { prop: "labels/show".into(), value: Some(json!("ends")) },
            Part { prop: "labels/role".into(), value: None }
        ]
    );
}

#[test]
fn a_look_put_down_is_chosen_where_each_nodes_own_lives() {
    let doc: Value = serde_json::from_str(EXAMPLE).unwrap();
    let title = picked(&doc, "revenue", "title");
    let (put, made) = put_on(&doc, "revenue", &title, &["note", "rev", "title"]);
    // The note's role lives on the node: the headline goes there, in every state it shows in.
    assert_eq!(put.took, ["note"]);
    assert_eq!(made["nodes"]["note"]["role"], "headline");
    // A chart takes none of a text's look, and the title is where it came from.
    let refused: Vec<(&str, &str)> = put.refused.iter().map(|r| (r.node.as_str(), r.why.as_str())).collect();
    assert_eq!(refused, [("rev", "a chart takes none of a text's look"), ("title", "is where the look was picked up")]);
    // Put down again, it changes nothing: the note looks so already.
    let (again, _) = put_on(&made, "revenue", &title, &["note"]);
    assert!(again.patch.is_empty() && again.same == ["note"], "{again:?}");
}

#[test]
fn an_override_goes_down_as_one_and_the_themes_value_takes_one_away() {
    let mut doc: Value = serde_json::from_str(EXAMPLE).unwrap();
    // The note in an accent written out; the subtitle in a size of its own.
    doc["overrides"] = json!({ "note": { "style": { "color": "#E4572E" } }, "subtitle": { "style": { "size": 40 } } });
    let note = picked(&doc, "revenue", "note");
    let color = note.props.iter().find(|p| p.prop == "style/color").unwrap();
    assert_eq!(color.value, Some(json!("#E4572E")));
    // Put on the title in `intro`: its color goes in the deck's overrides, as the note's is.
    let (_, made) = put_on(&doc, "intro", &note, &["title"]);
    assert_eq!(made["overrides"]["title"]["style"]["color"], "#E4572E", "{}", made["overrides"]);
    assert_eq!(made["nodes"]["title"]["role"], "caption");
    // The title's look in `intro` sets no size: put on the subtitle, its own is taken away, so
    // the theme's shows, as on the title.
    let title = picked(&doc, "intro", "title");
    let (put, made) = put_on(&doc, "intro", &title, &["subtitle"]);
    assert!(put.took == ["subtitle"], "{put:?}");
    assert!(
        made["overrides"].get("subtitle").is_none_or(|s| s["style"].get("size").is_none()),
        "{}",
        made["overrides"]
    );
    assert_eq!(made["nodes"]["subtitle"]["role"], "display");
}
