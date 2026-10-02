//! The schema checker (PLAN 1.2): it understands every keyword the generated schemas use,
//! passes every document in the repository, and says what is wrong in an author's terms.

use scaena_core::model::check::{ANNOTATIONS, Checker, KEYWORDS, Kind};
use scaena_core::model::{deck_schema, theme_schema};
use serde_json::{Value, json};

const REVENUE: &str = include_str!("../../../docs/examples/revenue.deck.json");

/// Every keyword in `schema`'s schema positions, as (keyword, where).
fn keywords(schema: &Value, at: &str, out: &mut Vec<(String, String)>) {
    let Value::Object(map) = schema else { return };
    for (key, value) in map {
        out.push((key.clone(), at.into()));
        match key.as_str() {
            "properties" | "$defs" => {
                for (name, child) in value.as_object().into_iter().flatten() {
                    keywords(child, &format!("{at}/{key}/{name}"), out);
                }
            }
            "anyOf" | "oneOf" => {
                for (i, child) in value.as_array().into_iter().flatten().enumerate() {
                    keywords(child, &format!("{at}/{key}/{i}"), out);
                }
            }
            "items" | "additionalProperties" | "propertyNames" | "if" | "then" => {
                keywords(value, &format!("{at}/{key}"), out)
            }
            _ => {}
        }
    }
}

#[test]
fn the_checker_understands_every_keyword_the_schemas_use() {
    for schema in [deck_schema(), theme_schema()] {
        let mut found = Vec::new();
        keywords(&schema, "", &mut found);
        for (keyword, at) in found {
            assert!(
                KEYWORDS.contains(&keyword.as_str()) || ANNOTATIONS.contains(&keyword.as_str()),
                "`{keyword}` at {at} is a keyword the checker does not check"
            );
        }
    }
}

#[test]
fn every_document_in_the_repository_passes() {
    let decks = [
        REVENUE,
        include_str!("../../../docs/examples/authorability/deck.json"),
        include_str!("../../../tests/fixtures/torture.scaena/deck.json"),
        include_str!("../../../tests/bench/b1.scaena/deck.json"),
    ];
    let themes = [
        include_str!("../../../docs/examples/themes/dusk.theme.json"),
        include_str!("../../../docs/examples/authorability/themes/dusk.theme.json"),
        include_str!("../../../docs/examples/authorability/themes/daybreak.theme.json"),
        include_str!("../../../tests/fixtures/torture.scaena/theme.json"),
        include_str!("../../../tests/bench/b1.scaena/theme.json"),
    ];
    for deck in decks {
        assert_eq!(Checker::deck().check(&serde_json::from_str(deck).unwrap()), []);
    }
    for theme in themes {
        assert_eq!(Checker::theme().check(&serde_json::from_str(theme).unwrap()), []);
    }
}

/// The one violation `edit` makes in the example deck, as (path, kind, message).
fn violation(edit: impl FnOnce(&mut Value)) -> (String, Kind, String) {
    let mut deck: Value = serde_json::from_str(REVENUE).unwrap();
    edit(&mut deck);
    let found = Checker::deck().check(&deck);
    assert_eq!(found.len(), 1, "{found:#?}");
    let v = found.into_iter().next().unwrap();
    (v.path, v.kind, v.message)
}

fn says(edit: impl FnOnce(&mut Value), path: &str, kind: Kind, message: &str) {
    assert_eq!(violation(edit), (path.to_string(), kind, message.to_string()));
}

#[test]
fn a_node_is_checked_as_its_own_type() {
    says(
        |d| d["nodes"]["rev"]["role"] = json!("body"),
        "/nodes/rev/role",
        Kind::Unknown,
        "`role` is not a property of a chart node; text nodes have it",
    );
    says(
        |d| d["nodes"]["title"]["padding"] = json!(24),
        "/nodes/title/padding",
        Kind::Unknown,
        "`padding` is not a property of a text node; stack, grid, and frame nodes have it",
    );
    says(
        |d| d["nodes"]["x"] = json!({"type": "video"}),
        "/nodes/x/type",
        Kind::Value,
        "`\"video\"` is not a node type: one of text, shape, image, chart, table, shader, stack, grid, frame, group",
    );
    says(|d| d["nodes"]["img"] = json!({"type": "image"}), "/nodes/img/src", Kind::Missing, "missing `src`");
}

#[test]
fn an_unknown_name_gets_the_closest_known_one() {
    says(
        |d| d["nodes"]["title"]["rolee"] = json!("body"),
        "/nodes/title/rolee",
        Kind::Unknown,
        "unknown property `rolee`; did you mean `role`?",
    );
    says(
        |d| d["states"][1]["props"]["title"] = json!({"durration": 300}),
        "/states/1/props/title/durration",
        Kind::Unknown,
        "unknown property `durration`",
    );
}

#[test]
fn values_say_what_is_allowed() {
    says(
        |d| d["nodes"]["title"]["fit"] = json!("cover"),
        "/nodes/title/fit",
        Kind::Value,
        "`\"cover\"` is not one of: wrap, shrink, grow, clip, error",
    );
    says(
        |d| d["states"][1]["props"]["title"] = json!({"fit": "squash"}),
        "/states/1/props/title/fit",
        Kind::Form,
        "`\"squash\"` is not one of: wrap, shrink, grow, clip, error, cover, contain, fill, null",
    );
    says(
        |d| d["nodes"]["bg"] = json!({"type": "shader", "kind": "mesh", "params": {"points": 40}}),
        "/nodes/bg/params/points",
        Kind::Value,
        "40 is above the maximum, 16",
    );
    says(
        |d| d["states"][1]["id"] = json!("Bad Id"),
        "/states/1/id",
        Kind::Value,
        "`\"Bad Id\"` is not a valid `Id`: A slug: unique within its collection (SPEC §3.2).",
    );
    says(
        |d| d["nodes"]["Bad Id"] = json!({"type": "group"}),
        "/nodes/Bad Id",
        Kind::Name,
        "`\"Bad Id\"` is not a valid `Id`: A slug: unique within its collection (SPEC §3.2).",
    );
}

#[test]
fn a_delta_may_delete_with_null_but_not_name_a_type() {
    let mut deck: Value = serde_json::from_str(REVENUE).unwrap();
    deck["states"][1]["props"]["title"] = json!({"alt": null, "at": {"in": null, "col": [1, 6]}});
    assert_eq!(Checker::deck().check(&deck), []);
    says(
        |d| d["states"][1]["props"]["title"] = json!({"type": "chart"}),
        "/states/1/props/title/type",
        Kind::Unknown,
        "unknown property `type`",
    );
}
