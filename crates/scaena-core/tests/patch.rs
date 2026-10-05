//! Patches (SPEC §7.3): semantic ops compile to RFC 6902 against the deck as the ops before
//! them leave it, check what they name, and apply all or none. Against the example deck,
//! whose bundle is `docs/examples/`.

use scaena_core::Deck;
use scaena_core::patch::{Compiled, JsonOp, PatchError, Renamed, compile};
use scaena_core::validate::{BundleFiles, validate_bundle};
use serde_json::{Value, json};
use std::path::Path;

const EXAMPLE: &str = include_str!("../../../docs/examples/revenue.deck.json");

/// `docs/examples/`, the example deck's bundle.
struct Examples;

impl BundleFiles for Examples {
    fn exists(&self, path: &str) -> bool {
        Path::new("../../docs/examples").join(path).is_file()
    }

    fn read_text(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(Path::new("../../docs/examples").join(path)).ok()
    }
}

fn example() -> Value {
    serde_json::from_str(EXAMPLE).unwrap()
}

fn patch(doc: &Value, ops: Value) -> Result<Compiled, PatchError> {
    compile(doc, ops.as_array().unwrap(), &Examples)
}

/// The compiled patch as JSON, to compare with what it should be.
fn rfc(c: &Compiled) -> Value {
    serde_json::to_value(&c.patch).unwrap()
}

fn deck(doc: &Value) -> Deck {
    serde_json::from_value(doc.clone()).unwrap()
}

/// What validation finds in a patched deck, as codes.
fn errors(doc: &Value) -> Vec<String> {
    let found = validate_bundle(&doc.to_string(), &Examples).unwrap();
    found.iter().map(|f| format!("{} {}", f.code, f.message)).collect()
}

fn keys(v: &Value) -> Vec<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

#[test]
fn each_op_compiles_against_the_deck_the_ops_before_it_leave() {
    let c = patch(
        &example(),
        json!([
            { "op": "add_node", "id": "kicker", "node": { "type": "text", "role": "caption", "text": "Q3 FY26" },
              "state": "revenue", "props": { "at": { "in": "note" } } },
            { "op": "set_prop", "node": "kicker", "state": "mix", "prop": "text", "value": "Q3 FY26, by product" },
            { "op": "set_prop", "node": "rev", "state": "mix", "prop": "legend", "value": "bottom" },
            { "op": "set_prop", "node": "title", "state": "revenue", "prop": "at/in", "value": "main" },
            { "op": "set_prop", "node": "rev", "prop": "y/title", "value": "Revenue (USD M)" },
            { "op": "set_text", "node": "subtitle", "text": "A quarter that changed the business." },
            { "op": "test", "path": "/nodes/subtitle/text", "value": "A quarter that changed the business." },
            // `close` lays out a title slide, which has no note.
            { "op": "hide_node", "node": "kicker", "state": "close" },
        ]),
    )
    .unwrap();
    assert_eq!(
        rfc(&c),
        json!([
            { "op": "add", "path": "/nodes/kicker", "value": { "type": "text", "role": "caption", "text": "Q3 FY26" } },
            { "op": "add", "path": "/states/1/props/kicker", "value": { "at": { "in": "note" } } },
            { "op": "add", "path": "/states/2/props/kicker", "value": { "text": "Q3 FY26, by product" } },
            { "op": "add", "path": "/states/2/props/rev/legend", "value": "bottom" },
            { "op": "add", "path": "/states/1/props/title/at/in", "value": "main" },
            { "op": "add", "path": "/nodes/rev/y/title", "value": "Revenue (USD M)" },
            { "op": "add", "path": "/nodes/subtitle/text", "value": "A quarter that changed the business." },
            { "op": "test", "path": "/nodes/subtitle/text", "value": "A quarter that changed the business." },
            { "op": "add", "path": "/states/3/remove/-", "value": "kicker" },
        ])
    );
    let snaps = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    assert!(!snaps[0].nodes.contains_key("kicker"));
    assert_eq!(snaps[1].nodes["kicker"]["text"], "Q3 FY26");
    assert_eq!(snaps[2].nodes["kicker"]["text"], "Q3 FY26, by product");
    assert_eq!(snaps[1].nodes["title"]["at"], json!({ "in": "main" }));
    assert_eq!(errors(&c.doc), Vec::<String>::new());
}

#[test]
fn rename_node_keeps_the_node_its_place_and_every_reference() {
    let before = example();
    let c = patch(&before, json!([{ "op": "rename_node", "id": "rev", "to": "revenue-chart" }])).unwrap();
    assert_eq!(c.renamed, [Renamed::Node { from: "rev".into(), to: "revenue-chart".into() }]);
    // Paint order at equal `z` is scene-graph order: the node keeps its place.
    assert_eq!(keys(&c.doc["nodes"]), ["bg", "title", "subtitle", "revenue-chart", "note"]);
    assert_eq!(keys(&c.doc["states"][1]["props"]), ["title", "revenue-chart", "note"]);
    assert_eq!(c.doc["states"][1]["choreography"][0]["target"], "revenue-chart");
    assert_eq!(c.doc["states"][3]["remove"], json!(["revenue-chart", "note"]));
    // Every state resolves to what it did, under the new name.
    let (was, now) =
        (scaena_core::resolve_states(&deck(&before)).unwrap(), scaena_core::resolve_states(&deck(&c.doc)).unwrap());
    for (a, b) in was.iter().zip(&now) {
        let renamed: Vec<String> =
            a.nodes.keys().map(|k| if k == "rev" { "revenue-chart".into() } else { k.clone() }).collect();
        assert_eq!(b.nodes.keys().cloned().collect::<Vec<_>>(), renamed);
        for (id, props) in &a.nodes {
            let id = if id == "rev" { "revenue-chart" } else { id };
            assert_eq!(&b.nodes[id], props, "{id} in {}", a.state_id);
        }
    }
    assert_eq!(errors(&c.doc), Vec::<String>::new());
    // And a state's id: the states that build on it, and the beats.
    let c = patch(&before, json!([{ "op": "rename_state", "id": "revenue", "to": "growth" }])).unwrap();
    assert_eq!(c.doc["states"][1]["id"], "growth");
    assert_eq!(c.doc["states"][2]["slide"], "growth");
    assert_eq!(c.doc["spine"]["sections"][1]["beats"][0]["states"], json!(["growth", "mix"]));
    assert_eq!(errors(&c.doc), Vec::<String>::new());
}

#[test]
fn remove_node_takes_every_reference_and_a_container_must_be_empty() {
    let c = patch(&example(), json!([{ "op": "remove_node", "id": "note" }])).unwrap();
    assert_eq!(
        rfc(&c),
        json!([
            { "op": "remove", "path": "/states/1/props/note" },
            { "op": "remove", "path": "/states/1/choreography/1" },
            { "op": "replace", "path": "/states/3/remove", "value": ["rev"] },
            { "op": "remove", "path": "/nodes/note" },
        ])
    );
    assert_eq!(errors(&c.doc), Vec::<String>::new());
    // A container holding nodes: they would fall out of it.
    let err = patch(
        &example(),
        json!([
            { "op": "add_node", "id": "row", "node": { "type": "stack", "at": { "in": "main" } } },
            { "op": "set_prop", "node": "note", "prop": "at", "value": { "parent": "row" } },
            { "op": "remove_node", "id": "row" },
        ]),
    )
    .unwrap_err();
    assert_eq!(err.index, 2);
    assert!(err.message.contains("`row` holds `note`"), "{err}");
    // Renamed, the container keeps what it holds.
    let c = patch(
        &example(),
        json!([
            { "op": "add_node", "id": "row", "node": { "type": "stack", "at": { "in": "main" } } },
            { "op": "set_prop", "node": "note", "prop": "at", "value": { "parent": "row" } },
            { "op": "rename_node", "id": "row", "to": "strip" },
        ]),
    )
    .unwrap();
    assert_eq!(c.doc["nodes"]["note"]["at"], json!({ "parent": "strip" }));
}

#[test]
fn states_are_added_moved_and_removed_with_their_references() {
    let c = patch(
        &example(),
        json!([
            { "op": "add_state", "after": "mix", "beat": "doubled",
              "state": { "id": "pro", "slide": "revenue", "props": { "title": { "text": "Pro led it" } } } },
            { "op": "move_state", "id": "pro", "before": "mix" },
        ]),
    )
    .unwrap();
    let ids: Vec<&str> = c.doc["states"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["intro", "revenue", "pro", "mix", "close"]);
    assert_eq!(c.doc["spine"]["sections"][1]["beats"][0]["states"], json!(["revenue", "mix", "pro"]));
    assert_eq!(
        rfc(&c)[2],
        json!({ "op": "move", "from": "/states/3", "path": "/states/2" }),
        "the index it moves to is counted without it"
    );
    assert_eq!(errors(&c.doc), Vec::<String>::new());
    // A state another builds on stays; the last state of a beat leaves the beat's list.
    let err = patch(&example(), json!([{ "op": "remove_state", "id": "revenue" }])).unwrap_err();
    assert!(err.message.contains("`mix` build on it"), "{err}");
    let c = patch(&example(), json!([{ "op": "remove_state", "id": "close" }])).unwrap();
    assert_eq!(c.doc["states"].as_array().unwrap().len(), 3);
    assert_eq!(c.doc["spine"]["sections"][2]["beats"][0]["states"], json!([]));
    // Moved before the state it builds on, a build tracks from what came before: the
    // validation that follows a patch reports it, not the op.
    let c = patch(&example(), json!([{ "op": "move_state", "id": "intro", "after": "close" }])).unwrap();
    assert_eq!(c.doc["states"][3]["id"], "intro");
}

#[test]
fn show_and_hide_make_a_node_enter_and_leave() {
    let c = patch(
        &example(),
        json!([
            // `note` entered in `revenue`; in `mix` it leaves.
            { "op": "hide_node", "node": "note", "state": "mix" },
            // `subtitle` enters in `intro` by its delta: hidden there, it never enters.
            { "op": "hide_node", "node": "subtitle", "state": "intro" },
            // `subtitle` comes back in `mix`, set as a caption.
            { "op": "show_node", "node": "subtitle", "state": "mix", "props": { "role": "caption" } },
            // `rev` leaves in `close`: shown there, it stays instead.
            { "op": "show_node", "node": "rev", "state": "close" },
        ]),
    )
    .unwrap();
    assert_eq!(
        rfc(&c),
        json!([
            { "op": "add", "path": "/states/2/remove", "value": ["note"] },
            { "op": "remove", "path": "/states/0/props/subtitle" },
            { "op": "add", "path": "/states/2/props/subtitle", "value": { "role": "caption" } },
            { "op": "replace", "path": "/states/3/remove", "value": ["note"] },
        ])
    );
    let snaps = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    let shown = |i: usize, id: &str| snaps[i].nodes.contains_key(id);
    assert!(!shown(0, "subtitle") && !shown(2, "note") && shown(2, "subtitle") && shown(3, "rev"));
    // A state that removes a node and gives it a delta has it enter again, from its
    // defaults: hidden there, it leaves, listed once.
    let c = patch(
        &example(),
        json!([
            { "op": "add", "path": "/states/3/props/note", "value": {} },
            { "op": "hide_node", "node": "note", "state": "close" },
        ]),
    )
    .unwrap();
    assert_eq!(c.doc["states"][3]["remove"], json!(["rev", "note"]));
    assert_eq!(c.doc["states"][3]["props"].get("note"), None);
    // Hidden where it was not on screen, shown where it was: nothing to do, said so.
    let err = patch(&example(), json!([{ "op": "hide_node", "node": "rev", "state": "intro" }])).unwrap_err();
    assert!(err.message.contains("not on screen in `intro`"), "{err}");
    let err = patch(&example(), json!([{ "op": "show_node", "node": "rev", "state": "mix" }])).unwrap_err();
    assert!(err.message.contains("on screen in `mix` already"), "{err}");
}

#[test]
fn presets_data_and_themes() {
    let c = patch(
        &example(),
        json!([
            { "op": "apply_preset", "node": "note", "preset": "rise", "motion": "enter", "state": "revenue" },
            { "op": "apply_preset", "node": "bg", "preset": "texture" },
            { "op": "bind_data", "node": "rev", "data": "@q4", "state": "mix",
              "source": { "source": "data/q3-revenue.csv", "schema": { "quarter": "string", "product": "string",
                                                                      "revenue": "number", "customers": "number" } } },
            { "op": "retheme", "theme": "themes/dusk.theme.json" },
        ]),
    )
    .unwrap();
    assert_eq!(
        rfc(&c),
        json!([
            { "op": "add", "path": "/states/1/props/note/enter", "value": "rise" },
            // The shader takes the preset whole: its kind, and none of its own palette or params.
            { "op": "add", "path": "/nodes/bg/kind", "value": "noise" },
            { "op": "add", "path": "/nodes/bg/preset", "value": "texture" },
            { "op": "remove", "path": "/nodes/bg/palette" },
            { "op": "remove", "path": "/nodes/bg/params" },
            { "op": "add", "path": "/data/q4", "value": { "source": "data/q3-revenue.csv", "schema": {
                "quarter": "string", "product": "string", "revenue": "number", "customers": "number" } } },
            { "op": "add", "path": "/states/2/props/rev/data", "value": "@q4" },
            { "op": "add", "path": "/theme", "value": "themes/dusk.theme.json" },
        ])
    );
    assert_eq!(errors(&c.doc), Vec::<String>::new());
    for (ops, says) in [
        (
            json!([{ "op": "apply_preset", "node": "note", "preset": "bounce", "motion": "enter" }]),
            "no motion preset `bounce`",
        ),
        (json!([{ "op": "apply_preset", "node": "note", "preset": "rise" }]), "name the `motion`"),
        (json!([{ "op": "apply_preset", "node": "bg", "preset": "mesh-loud" }]), "`backdrop`, `texture`"),
        (json!([{ "op": "bind_data", "node": "rev", "data": "q4" }]), "no data source `q4`"),
        (json!([{ "op": "bind_data", "node": "note", "data": "q3" }]), "charts and tables read data"),
        (json!([{ "op": "retheme", "theme": "themes/daybreak.theme.json" }]), "`scaena theme --apply` copies"),
    ] {
        let err = patch(&example(), ops.clone()).unwrap_err();
        assert!(err.message.contains(says), "{ops}: {err}");
    }
}

#[test]
fn an_op_that_would_not_do_what_it_says_is_refused() {
    for (ops, says) in [
        // A delta in a state where the node is not on screen would make it enter.
        (json!({ "op": "set_prop", "node": "rev", "state": "intro", "prop": "kind", "value": "line" }), "`show_node`"),
        (json!({ "op": "set_prop", "node": "rev", "prop": "type", "value": "table" }), "E104"),
        (json!({ "op": "set_prop", "node": "rev", "prop": "y/domain/1", "value": 10 }), "one level deep"),
        (json!({ "op": "set_text", "node": "rev", "text": "Revenue" }), "`set_text` sets a text node's"),
        (json!({ "op": "set_prop", "node": "nobody", "prop": "z", "value": 1 }), "no node `nobody`"),
        (json!({ "op": "set_prop", "node": "rev", "state": "later", "prop": "z", "value": 1 }), "no state `later`"),
        // A misspelled member is an error, not a change somewhere else.
        (json!({ "op": "set_prop", "node": "rev", "stat": "mix", "prop": "z", "value": 1 }), "unknown field `stat`"),
        (json!({ "op": "add_node", "id": "rev", "node": { "type": "text", "text": "x" } }), "a node `rev` already"),
        (json!({ "op": "add_node", "id": "Rev", "node": { "type": "text", "text": "x" } }), "not an id"),
        (json!({ "op": "add_node", "id": "x", "node": { "type": "video" } }), "unknown variant `video`"),
        (json!({ "op": "add_node", "id": "x", "node": { "type": "text" }, "props": {} }), "name the `state`"),
        (json!({ "op": "move_state", "id": "mix", "after": "intro", "before": "close" }), "not both"),
        (json!({ "op": "move_state", "id": "mix" }), "say where"),
        (json!({ "op": "add_state", "state": { "id": "mix" } }), "a state `mix` already"),
        (json!({ "op": "frobnicate" }), "unknown variant `frobnicate`"),
        (json!({ "path": "/nodes" }), "needs a string `op`"),
        (json!({ "op": "remove", "path": "/nodes/nobody" }), "is not there"),
    ] {
        // After a first op that applies: a refusal takes the whole patch with it.
        let ops = json!([{ "op": "set_text", "node": "title", "text": "Q3" }, ops]);
        let err = patch(&example(), ops.clone()).unwrap_err();
        assert_eq!(err.index, 1, "{ops}");
        assert!(err.message.contains(says), "{ops}: {err}");
    }
}

#[test]
fn set_text_takes_runs_away_and_null_takes_a_property_away() {
    let doc = json!({
        "scaena": "0.10", "canvas": { "width": 1920, "height": 1080 },
        "nodes": { "t": { "type": "text", "runs": [{ "text": "Hello" }], "fit": "shrink", "at": { "in": "canvas" } } },
        "states": [{ "id": "a", "props": { "t": {} } }, { "id": "b" }],
    });
    let c = patch(
        &doc,
        json!([
            { "op": "set_text", "node": "t", "state": "b", "text": "Goodbye" },
            { "op": "set_prop", "node": "t", "prop": "fit", "value": null },
            { "op": "set_prop", "node": "t", "state": "b", "prop": "at/in", "value": null },
        ]),
    )
    .unwrap();
    assert_eq!(
        rfc(&c),
        json!([
            { "op": "add", "path": "/states/1/props", "value": { "t": { "text": "Goodbye", "runs": null } } },
            { "op": "remove", "path": "/nodes/t/fit" },
            { "op": "add", "path": "/states/1/props/t/at", "value": { "in": null } },
        ])
    );
    let snaps = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    assert_eq!(snaps[1].nodes["t"].get("runs"), None);
    assert_eq!(snaps[1].nodes["t"]["text"], "Goodbye");
    assert_eq!(snaps[1].nodes["t"]["at"], json!({}));
    let c = patch(&doc, json!([{ "op": "set_text", "node": "t", "text": "Hi" }])).unwrap();
    assert_eq!(c.doc["nodes"]["t"], json!({ "type": "text", "fit": "shrink", "at": { "in": "canvas" }, "text": "Hi" }));
}

#[test]
fn json_patch_and_semantic_ops_mix_and_fail_together() {
    let doc = example();
    let ops = json!([
        { "op": "add", "path": "/nodes/kicker", "value": { "type": "text", "text": "Q3", "at": { "in": "note" } } },
        { "op": "show_node", "node": "kicker", "state": "close" },
        { "op": "replace", "path": "/states/3/hold", "value": 2000 },
    ]);
    let c = patch(&doc, ops).unwrap();
    assert!(matches!(&c.patch[1], JsonOp::Add { path, .. } if path == "/states/3/props/kicker"));
    assert_eq!(c.doc["states"][3]["hold"], 2000);
    // The same, failing at the last op: nothing applies, and `doc` was only read.
    let ops = json!([
        { "op": "add", "path": "/nodes/kicker", "value": { "type": "text", "text": "Q3" } },
        { "op": "show_node", "node": "kicker", "state": "close" },
        { "op": "replace", "path": "/states/9/hold", "value": 2000 },
    ]);
    let err = patch(&doc, ops).unwrap_err();
    assert_eq!((err.index, err.message.as_str()), (2, "`/states/9/hold` is not there"));
    assert_eq!(doc, example());
}

#[test]
fn place_writes_a_placement_where_it_lives() {
    // `revenue` moves the title into the header, and `mix` tracks it from there: a move in
    // `mix` changes `revenue`'s delta, so both show it.
    let c = patch(&example(), json!([{ "op": "place", "node": "title", "state": "mix", "at": { "in": "kicker" } }]))
        .unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "add", "path": "/states/1/props/title/at/in", "value": "kicker" }]));
    let snaps = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    let at = |i: usize| snaps[i].nodes["title"]["at"].clone();
    assert_eq!(
        (at(0), at(1), at(2), at(3)),
        (json!({ "in": "title" }), json!({ "in": "kicker" }), json!({ "in": "kicker" }), json!({ "in": "title" }))
    );
    assert_eq!(errors(&c.doc), Vec::<String>::new());

    // `intro` shows the title's own placement: cells there replace the slot, and the states
    // that set their own keep it.
    let c = patch(
        &example(),
        json!([{ "op": "place", "node": "title", "state": "intro", "at": { "col": [2, 9], "row": [3, 8] } }]),
    )
    .unwrap();
    assert_eq!(c.doc["nodes"]["title"]["at"], json!({ "col": [2, 9], "row": [3, 8] }));
    let snaps = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    assert_eq!(snaps[1].nodes["title"]["at"], json!({ "col": [2, 9], "row": [3, 8], "in": "header" }));
    assert_eq!(errors(&c.doc), Vec::<String>::new());

    // Into a delta whose state tracks a slot: the slot goes with `null`, which the merge takes
    // away from what it tracks.
    let c = patch(
        &example(),
        json!([{ "op": "place", "node": "title", "state": "revenue", "at": { "col": [1, 8], "row": [1, 2] } }]),
    )
    .unwrap();
    assert_eq!(c.doc["states"][1]["props"]["title"]["at"], json!({ "col": [1, 8], "row": [1, 2], "in": null }));
    let snaps = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    assert_eq!(snaps[2].nodes["title"]["at"], json!({ "col": [1, 8], "row": [1, 2] }));
    assert_eq!(errors(&c.doc), Vec::<String>::new());

    // Without a state, the node's own placement, whatever the states set over it.
    let c = patch(&example(), json!([{ "op": "place", "node": "note", "at": { "in": "kicker" } }])).unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "add", "path": "/nodes/note/at/in", "value": "kicker" }]));
}

/// `fork` keeps a placement to the state it names: the state's own delta takes it, and the
/// states that track that state take it with the rest of its props; where the placement lived
/// keeps it for the others (ADR-0013).
#[test]
fn place_forks_a_placement_into_its_state() {
    let before = scaena_core::resolve_states(&deck(&example())).unwrap();
    let at = |snaps: &[scaena_core::Snapshot], i: usize, node: &str| snaps[i].nodes.get(node).map(|p| p["at"].clone());
    // `title` lives in `revenue`'s delta, which `mix` tracks: forked in `mix`, `mix` has its own.
    let c = patch(
        &example(),
        json!([{ "op": "place", "node": "title", "state": "mix", "at": { "in": "kicker" }, "fork": true }]),
    )
    .unwrap();
    let mut title = example()["states"][2]["props"]["title"].clone();
    title["at"] = json!({ "in": "kicker" });
    assert_eq!(c.doc["states"][2]["props"]["title"], title, "beside the text `mix` sets");
    assert_eq!(c.doc["states"][1], example()["states"][1], "where it lived keeps it");
    let after = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    assert_eq!(at(&after, 2, "title"), Some(json!({ "in": "kicker" })));
    for i in [0, 1, 3] {
        assert_eq!(at(&after, i, "title"), at(&before, i, "title"), "state {i}");
    }
    assert_eq!(errors(&c.doc), Vec::<String>::new());

    // `note` lives in its own `at`: forked onto cells in `revenue`, it stands there and in
    // `mix`, which tracks it; its slot is taken away with `null`, and it keeps it elsewhere.
    let c = patch(
        &example(),
        json!([{ "op": "place", "node": "note", "state": "revenue", "at": { "col": [2, 5], "row": 3 }, "fork": true }]),
    )
    .unwrap();
    assert_eq!(c.doc["states"][1]["props"]["note"]["at"], json!({ "col": [2, 5], "row": 3, "in": null }));
    assert_eq!(c.doc["nodes"]["note"], example()["nodes"]["note"]);
    let after = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    for i in [1, 2] {
        assert_eq!(at(&after, i, "note"), Some(json!({ "col": [2, 5], "row": 3 })), "state {i}");
    }
    assert_eq!(at(&after, 0, "note"), at(&before, 0, "note"));
    assert_eq!(errors(&c.doc), Vec::<String>::new());

    // Nothing to keep it to without a state; and over the deck's overrides it would not show.
    let e = patch(&example(), json!([{ "op": "place", "node": "title", "at": { "in": "kicker" }, "fork": true }]))
        .unwrap_err();
    assert!(e.to_string().contains("`fork`"), "{e}");
    let doc = json!({
        "scaena": "0.10", "canvas": { "width": 1920, "height": 1080 },
        "nodes": { "u": { "type": "text", "text": "Yo", "at": { "col": [1, 4] } } },
        "overrides": { "u": { "at": { "rect": [10, 10, 300, 100] } } },
        "states": [{ "id": "a", "props": { "u": {} } }],
    });
    let e = patch(&doc, json!([{ "op": "place", "node": "u", "state": "a", "at": { "col": [2, 3] }, "fork": true }]))
        .unwrap_err();
    assert!(e.to_string().contains("overrides"), "{e}");
}

#[test]
fn place_follows_a_node_out_and_back_and_into_its_overrides() {
    let doc = json!({
        "scaena": "0.10", "canvas": { "width": 1920, "height": 1080 },
        "nodes": {
            "t": { "type": "text", "text": "Hi", "at": { "in": "grid", "align": "center" } },
            "u": { "type": "text", "text": "Yo", "at": { "col": [1, 4] } },
        },
        "overrides": { "u": { "at": { "rect": [10, 10, 300, 100] } } },
        "states": [
            { "id": "a", "props": { "t": { "at": { "in": "canvas" } }, "u": {} } },
            { "id": "b", "remove": ["t"] },
            { "id": "c", "props": { "t": {} } },
        ],
    });
    // `t` leaves in `b` and comes back in `c` with its own placement: that is where it lives.
    let c =
        patch(&doc, json!([{ "op": "place", "node": "t", "state": "c", "at": { "col": [2, 3], "row": 2 } }])).unwrap();
    assert_eq!(c.doc["nodes"]["t"]["at"], json!({ "align": "center", "col": [2, 3], "row": 2 }));
    let snaps = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    assert_eq!(snaps[0].nodes["t"]["at"], json!({ "in": "canvas", "align": "center", "col": [2, 3], "row": 2 }));
    assert_eq!(snaps[2].nodes["t"]["at"], json!({ "align": "center", "col": [2, 3], "row": 2 }));
    // The overrides place `u` in every state: a move changes them, and takes the node's own
    // cells away under them.
    let c = patch(&doc, json!([{ "op": "place", "node": "u", "state": "a", "at": { "in": "grid" } }])).unwrap();
    assert_eq!(
        rfc(&c),
        json!([
            { "op": "add", "path": "/overrides/u/at/col", "value": null },
            { "op": "add", "path": "/overrides/u/at/in", "value": "grid" },
            { "op": "remove", "path": "/overrides/u/at/rect" },
        ])
    );
    let snaps = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    let mut shown = snaps[0].nodes["u"].clone();
    scaena_core::tracking::merge_props(&mut shown, &deck(&c.doc).overrides["u"]);
    assert_eq!(shown["at"], json!({ "in": "grid" }));
    // Overrides that take `at` away whole keep the rest of it away when a placement goes there.
    let mut gone = doc.clone();
    gone["overrides"] = json!({ "t": { "at": null } });
    let c = patch(&gone, json!([{ "op": "place", "node": "t", "state": "a", "at": { "col": 1 } }])).unwrap();
    assert_eq!(c.doc["overrides"]["t"]["at"], json!({ "align": null, "col": 1, "in": null }));
    let snaps = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    let mut shown = snaps[0].nodes["t"].clone();
    scaena_core::tracking::merge_props(&mut shown, &deck(&c.doc).overrides["t"]);
    assert_eq!(shown["at"], json!({ "col": 1 }));
}

#[test]
fn place_says_what_places_a_node() {
    let doc = json!({
        "scaena": "0.10", "canvas": { "width": 1920, "height": 1080 },
        "nodes": {
            "row": { "type": "stack", "axis": "x", "at": { "in": "grid" } },
            "a": { "type": "text", "text": "A", "at": { "parent": "row" } },
            "b": { "type": "text", "text": "B", "at": { "parent": "row", "index": 1 } },
            "board": { "type": "grid", "cols": 2, "rows": 2, "at": { "in": "grid" } },
            "dot": { "type": "shape", "kind": "ellipse", "at": { "parent": "board", "col": 1, "row": 1 } },
            "card": { "type": "frame", "at": { "in": "grid" } },
            "tag": { "type": "text", "text": "T", "at": { "parent": "card" } },
        },
        "states": [{ "id": "s", "props": { "row": {}, "a": {}, "b": {}, "board": {}, "dot": {}, "card": {}, "tag": {} } }],
    });
    // Each container places its children its own way, and the node stays in it.
    let c = patch(
        &doc,
        json!([
            { "op": "place", "node": "a", "state": "s", "at": { "index": 2 } },
            { "op": "place", "node": "dot", "state": "s", "at": { "col": [1, 2], "row": 2 } },
            { "op": "place", "node": "tag", "at": { "rect": [8, 8, 120, 40] } },
        ]),
    )
    .unwrap();
    assert_eq!(c.doc["nodes"]["a"]["at"], json!({ "parent": "row", "index": 2 }));
    assert_eq!(c.doc["nodes"]["dot"]["at"], json!({ "parent": "board", "col": [1, 2], "row": 2 }));
    assert_eq!(c.doc["nodes"]["tag"]["at"], json!({ "parent": "card", "rect": [8.0, 8.0, 120.0, 40.0] }));
    for (op, says) in [
        (
            json!({ "op": "place", "node": "a", "at": { "col": 1 } }),
            "in stack `row`, which places its children in order: by `index`",
        ),
        (json!({ "op": "place", "node": "dot", "at": { "in": "grid" } }), "in grid `board`"),
        (json!({ "op": "place", "node": "tag", "at": { "index": 0 } }), "in frame `card`"),
        (json!({ "op": "place", "node": "row", "at": { "area": "x" } }), "on the theme's grid"),
        (json!({ "op": "place", "node": "row", "at": {} }), "say where"),
        (json!({ "op": "place", "node": "row", "at": { "in": "grid", "col": 1 } }), "one placement"),
        (json!({ "op": "place", "node": "row", "at": { "parent": "board" } }), "unknown field `parent`"),
        (json!({ "op": "place", "node": "nobody", "at": { "in": "grid" } }), "no node `nobody`"),
    ] {
        let err = patch(&doc, json!([op.clone()])).unwrap_err();
        assert!(err.message.contains(says), "{op}: {err}");
    }
}

#[test]
fn replace_text_writes_typing_where_the_text_lives() {
    let text = |doc: &Value, i: usize, node: &str| {
        let snaps = scaena_core::resolve_states(&deck(doc)).unwrap();
        snaps[i].nodes.get(node).map(|p| p["text"].clone())
    };
    // `revenue` sets the title's text: "doubled" (characters 8 to 15) becomes "tripled" there.
    let c = patch(
        &example(),
        json!([{ "op": "replace_text", "node": "title", "state": "revenue", "from": 8, "to": 15, "text": "tripled" }]),
    )
    .unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "add", "path": "/states/1/props/title/text", "value": "Revenue tripled" }]));
    assert_eq!(text(&c.doc, 2, "title"), Some(json!("…and the mix shifted")), "`mix` sets its own");
    assert_eq!(c.doc["nodes"]["title"], example()["nodes"]["title"]);

    // In `intro` the title reads its own text: the node's changes, and the states that set
    // theirs keep them.
    let c = patch(
        &example(),
        json!([{ "op": "replace_text", "node": "title", "state": "intro", "from": 0, "to": 2, "text": "Third-quarter" }]),
    )
    .unwrap();
    assert_eq!(c.doc["nodes"]["title"]["text"], "Third-quarter Review");
    for i in 1..4 {
        assert_eq!(text(&c.doc, i, "title"), text(&example(), i, "title"), "state {i}");
    }

    // `note` reads its own text in `revenue` and in `mix`, which tracks it: the node's
    // changes, so both read it; forked, `revenue`'s delta does, and `mix` takes it from there.
    let c = patch(
        &example(),
        json!([{ "op": "replace_text", "node": "note", "state": "revenue", "from": 0, "to": 7, "text": "Sales" }]),
    )
    .unwrap();
    assert_eq!(c.doc["nodes"]["note"]["text"], "Sales in $M. Enterprise recognized on delivery.");
    let c = patch(
        &example(),
        json!([{ "op": "replace_text", "node": "note", "state": "revenue", "from": 0, "to": 7, "text": "Sales", "fork": true }]),
    )
    .unwrap();
    assert_eq!(c.doc["nodes"]["note"], example()["nodes"]["note"]);
    assert_eq!(c.doc["states"][1]["props"]["note"]["text"], "Sales in $M. Enterprise recognized on delivery.");
    assert_eq!(text(&c.doc, 2, "note"), Some(json!("Sales in $M. Enterprise recognized on delivery.")));
    assert_eq!(errors(&c.doc), Vec::<String>::new());

    // Offsets count characters, not bytes: "…" is one.
    let c = patch(
        &example(),
        json!([{ "op": "replace_text", "node": "title", "state": "mix", "from": 1, "to": 4, "text": "so" }]),
    )
    .unwrap();
    assert_eq!(c.doc["states"][2]["props"]["title"]["text"], "…so the mix shifted");

    // Without a state, the node's own text; past its end, or on what is not a text, refused.
    let c =
        patch(&example(), json!([{ "op": "replace_text", "node": "title", "from": 9, "to": 9, "text": "!" }])).unwrap();
    assert_eq!(c.doc["nodes"]["title"]["text"], "Q3 Review!");
    for (op, says) in [
        (
            json!({ "op": "replace_text", "node": "title", "state": "intro", "from": 3, "to": 10, "text": "" }),
            "9 characters",
        ),
        (
            json!({ "op": "replace_text", "node": "title", "state": "intro", "from": 4, "to": 3, "text": "" }),
            "`from` first",
        ),
        (json!({ "op": "replace_text", "node": "rev", "from": 0, "to": 0, "text": "x" }), "edits a text node's text"),
        (json!({ "op": "replace_text", "node": "title", "from": 0, "to": 0, "text": "x", "fork": true }), "`fork`"),
    ] {
        let e = patch(&example(), json!([op])).unwrap_err();
        assert!(e.to_string().contains(says), "{e}");
    }
}

#[test]
fn replace_text_keeps_runs_and_their_looks() {
    let doc = json!({
        "scaena": "0.10", "canvas": { "width": 1920, "height": 1080 },
        "nodes": {
            "t": { "type": "text", "at": { "in": "title" },
                   "runs": [{ "text": "Hello " }, { "text": "wörld", "emphasis": "strong" }, { "text": "!" }] },
            "u": { "type": "text", "text": "Yo", "at": { "in": "body" } },
        },
        "overrides": { "u": { "text": "Hey" } },
        "states": [{ "id": "a", "props": { "t": {}, "u": {} } }, { "id": "b", "props": { "t": { "runs": [{ "text": "Bye" }] } } }],
    });
    let runs = |c: &Compiled, at: &str| c.doc.pointer(at).unwrap().clone();
    // Typed where two runs meet: into the one before.
    let c =
        patch(&doc, json!([{ "op": "replace_text", "node": "t", "state": "a", "from": 6, "to": 6, "text": "big " }]))
            .unwrap();
    assert_eq!(
        runs(&c, "/nodes/t/runs"),
        json!([{ "text": "Hello big " }, { "text": "wörld", "emphasis": "strong" }, { "text": "!" }])
    );
    // Across runs: each keeps what is left of it, typed into the first; "ö" is one character.
    let c = patch(&doc, json!([{ "op": "replace_text", "node": "t", "state": "a", "from": 4, "to": 9, "text": "p" }]))
        .unwrap();
    assert_eq!(
        runs(&c, "/nodes/t/runs"),
        json!([{ "text": "Hellp" }, { "text": "ld", "emphasis": "strong" }, { "text": "!" }])
    );
    // A run the edit empties goes; at the start, the first run takes it.
    let c = patch(&doc, json!([{ "op": "replace_text", "node": "t", "state": "a", "from": 6, "to": 11, "text": "" }]))
        .unwrap();
    assert_eq!(runs(&c, "/nodes/t/runs"), json!([{ "text": "Hello " }, { "text": "!" }]));
    let c =
        patch(&doc, json!([{ "op": "replace_text", "node": "t", "state": "a", "from": 0, "to": 0, "text": "Oh, " }]))
            .unwrap();
    assert_eq!(runs(&c, "/nodes/t/runs")[0], json!({ "text": "Oh, Hello " }));
    // Everything deleted leaves the run typed into, empty.
    let c = patch(&doc, json!([{ "op": "replace_text", "node": "t", "state": "a", "from": 0, "to": 12, "text": "" }]))
        .unwrap();
    assert_eq!(runs(&c, "/nodes/t/runs"), json!([{ "text": "" }]));
    // `b` sets its own runs: edited there.
    let c = patch(&doc, json!([{ "op": "replace_text", "node": "t", "state": "b", "from": 3, "to": 3, "text": "!" }]))
        .unwrap();
    assert_eq!(runs(&c, "/states/1/props/t/runs"), json!([{ "text": "Bye!" }]));
    assert_eq!(c.doc["nodes"]["t"], doc["nodes"]["t"]);
    // The deck's overrides set `u`'s text in every state: edited there, and never forked.
    let c = patch(&doc, json!([{ "op": "replace_text", "node": "u", "state": "a", "from": 3, "to": 3, "text": "!" }]))
        .unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "add", "path": "/overrides/u/text", "value": "Hey!" }]));
    let e = patch(
        &doc,
        json!([{ "op": "replace_text", "node": "u", "state": "a", "from": 0, "to": 0, "text": "!", "fork": true }]),
    )
    .unwrap_err();
    assert!(e.to_string().contains("overrides"), "{e}");
}
