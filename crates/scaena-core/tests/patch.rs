//! Patches (SPEC §7.3): semantic ops compile to RFC 6902 against the deck as the ops before
//! them leave it, check what they name, and apply all or none. Against the example deck,
//! whose bundle is `docs/examples/`.

use scaena_core::Deck;
use scaena_core::patch::{Compiled, JsonOp, PatchError, Renamed, Timed, Written, compile, written};
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
        "scaena": "0.12", "canvas": { "width": 1920, "height": 1080 },
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
        "scaena": "0.12", "canvas": { "width": 1920, "height": 1080 },
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
        "scaena": "0.12", "canvas": { "width": 1920, "height": 1080 },
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
        "scaena": "0.12", "canvas": { "width": 1920, "height": 1080 },
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
        (json!({ "op": "place", "node": "row", "at": { "parent": "board" } }), "say where"),
        (json!({ "op": "place", "node": "row", "at": { "anywhere": 1 } }), "unknown field `anywhere`"),
        (json!({ "op": "place", "node": "nobody", "at": { "in": "grid" } }), "no node `nobody`"),
    ] {
        let err = patch(&doc, json!([op.clone()])).unwrap_err();
        assert!(err.message.contains(says), "{op}: {err}");
    }
}

/// `place` with `parent` moves a node into another container, placed as that one places what
/// it holds, or onto the canvas with `null`; written where the node's placement lives, its
/// container with the rest of it (PLAN 2.50).
#[test]
fn place_moves_a_node_into_another_container_or_onto_the_canvas() {
    let doc = json!({
        "scaena": "0.12", "canvas": { "width": 1920, "height": 1080 },
        "nodes": {
            "row": { "type": "stack", "axis": "x", "at": { "col": [1, 6], "row": [1, 3] } },
            "a": { "type": "text", "text": "A", "at": { "parent": "row" } },
            "board": { "type": "grid", "cols": 2, "rows": 2, "at": { "col": [7, 12], "row": [1, 3] } },
            "card": { "type": "frame", "at": { "col": [1, 6], "row": [4, 6] } },
            "tag": { "type": "text", "text": "T", "at": { "parent": "card", "rect": [8, 8, 120, 40] } },
            "lone": { "type": "text", "text": "L", "at": { "col": [7, 9], "row": 5 } },
        },
        "states": [
            { "id": "s", "props": { "row": {}, "a": {}, "board": {}, "card": {}, "tag": {}, "lone": {} } },
            { "id": "t", "mode": "delta", "props": { "lone": { "at": { "col": [10, 12] } } } },
        ],
    });
    let own = |c: &Compiled, node: &str| c.doc["nodes"][node]["at"].clone();
    // Into a stack, by `index`; into a frame, by `rect`; onto the canvas, by cells, the
    // container taken away. The node's own placement, which every state shows.
    let c = patch(
        &doc,
        json!([
            { "op": "place", "node": "tag", "state": "s", "at": { "parent": "row", "index": 1 } },
            { "op": "place", "node": "a", "state": "s", "at": { "parent": "card", "rect": [0, 0, 200, 80] } },
        ]),
    )
    .unwrap();
    assert_eq!(own(&c, "tag"), json!({ "parent": "row", "index": 1 }));
    assert_eq!(own(&c, "a"), json!({ "parent": "card", "rect": [0.0, 0.0, 200.0, 80.0] }));
    let c = patch(
        &doc,
        json!([{ "op": "place", "node": "tag", "state": "s", "at": { "parent": null, "col": [2, 3], "row": 5 } }]),
    )
    .unwrap();
    assert_eq!(own(&c, "tag"), json!({ "col": [2, 3], "row": 5 }));
    // A grid container takes cells, an area, or its flow's `index`.
    let c =
        patch(&doc, json!([{ "op": "place", "node": "lone", "state": "s", "at": { "parent": "board", "index": 3 } }]))
            .unwrap();
    assert_eq!(own(&c, "lone"), json!({ "parent": "board", "index": 3 }));
    // Where `t` places the node, there it goes into the container: the delta takes the cells
    // it merges into away, and `s` keeps the node on the canvas.
    let c = patch(
        &doc,
        json!([{ "op": "place", "node": "lone", "state": "t", "at": { "parent": "card", "rect": [0, 0, 90, 40] } }]),
    )
    .unwrap();
    assert_eq!(
        c.doc["states"][1]["props"]["lone"]["at"],
        json!({ "parent": "card", "rect": [0.0, 0.0, 90.0, 40.0], "col": null, "row": null })
    );
    assert_eq!(own(&c, "lone"), doc["nodes"]["lone"]["at"]);
    let snaps = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    assert_eq!(snaps[1].nodes["lone"]["at"], json!({ "parent": "card", "rect": [0.0, 0.0, 90.0, 40.0] }));
    assert_eq!(snaps[0].nodes["lone"]["at"], doc["nodes"]["lone"]["at"]);
    // And out again from there: `null` takes the container the delta set away.
    let back = patch(
        &c.doc,
        json!([{ "op": "place", "node": "lone", "state": "t", "at": { "parent": null, "col": [10, 12], "row": 5 } }]),
    )
    .unwrap();
    assert_eq!(back.doc["states"][1]["props"]["lone"]["at"], json!({ "col": [10, 12], "row": 5 }));
    // The new container says how the node is placed; what is not a container, the node
    // itself, and what the node holds, it does not go into.
    for (op, says) in [
        (json!({ "op": "place", "node": "lone", "at": { "parent": "row", "col": 1 } }), "in stack `row`"),
        (json!({ "op": "place", "node": "a", "at": { "parent": null, "index": 0 } }), "on the theme's grid"),
        (
            json!({ "op": "place", "node": "a", "at": { "parent": "lone", "col": 1 } }),
            "`lone`, of type `text`, holds nothing",
        ),
        (
            json!({ "op": "place", "node": "card", "at": { "parent": "card", "rect": [0, 0, 9, 9] } }),
            "cannot hold itself",
        ),
        (json!({ "op": "place", "node": "a", "at": { "parent": "ghost", "col": 1 } }), "no node `ghost`"),
    ] {
        let err = patch(&doc, json!([op.clone()])).unwrap_err();
        assert!(err.message.contains(says), "{op}: {err}");
    }
    // Into what it holds, containers nest in a loop: validation finds it, and a patch that
    // makes it is refused (`scaena patch`, SPEC §7.3).
    let nested =
        patch(&doc, json!([{ "op": "place", "node": "card", "state": "s", "at": { "parent": "row", "index": 0 } }]))
            .unwrap();
    assert_eq!(errors(&nested.doc), Vec::<String>::new());
    let looped = patch(
        &nested.doc,
        json!([{ "op": "place", "node": "row", "state": "s", "at": { "parent": "card", "rect": [0, 0, 9, 9] } }]),
    )
    .unwrap();
    assert!(errors(&looped.doc).iter().any(|e| e.contains("containers nest in a loop")), "{:?}", errors(&looped.doc));
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
fn style_text_gives_characters_a_look_where_the_text_lives() {
    let shown = |doc: &Value, i: usize, node: &str| {
        let snaps = scaena_core::resolve_states(&deck(doc)).unwrap();
        snaps[i].nodes[node].clone()
    };
    let style = |doc: &Value, op: Value| patch(doc, json!([op])).unwrap().doc;
    // `revenue` sets the title's text: "doubled" (characters 8 to 15) is bold there, as runs
    // in its delta, and `mix`, which sets a text of its own, still reads it.
    let bold = json!({ "op": "style_text", "node": "title", "state": "revenue", "from": 8, "to": 15, "look": { "style/weight": 700 } });
    let doc = style(&example(), bold.clone());
    assert_eq!(
        doc["states"][1]["props"]["title"],
        json!({ "role": "headline", "semantic": "claim", "at": { "in": "header" },
                "runs": [{ "text": "Revenue " }, { "text": "doubled", "style": { "weight": 700 } }] })
    );
    assert_eq!(doc["nodes"]["title"], example()["nodes"]["title"]);
    assert_eq!(shown(&doc, 2, "title")["text"], "…and the mix shifted");
    assert_eq!(shown(&doc, 2, "title").get("runs"), None);
    assert_eq!(errors(&doc), Vec::<String>::new());

    // Its neighbor made bold too joins it; both made plain again, the delta holds text again.
    let both = style(
        &doc,
        json!({ "op": "style_text", "node": "title", "state": "revenue", "from": 0, "to": 8, "look": { "style/weight": 700 } }),
    );
    assert_eq!(
        both["states"][1]["props"]["title"]["runs"],
        json!([{ "text": "Revenue doubled", "style": { "weight": 700 } }])
    );
    let plain = style(
        &both,
        json!({ "op": "style_text", "node": "title", "state": "revenue", "from": 0, "to": 15, "look": { "style/weight": null } }),
    );
    assert_eq!(plain, example());

    // Italic is a key a run's style takes (PLAN 2.40), and taken away as the others are.
    let italic = json!({ "op": "style_text", "node": "title", "state": "revenue", "from": 8, "to": 15, "look": { "style/italic": true } });
    let doc = style(&example(), italic);
    assert_eq!(
        doc["states"][1]["props"]["title"]["runs"],
        json!([{ "text": "Revenue " }, { "text": "doubled", "style": { "italic": true } }])
    );
    assert_eq!(errors(&doc), Vec::<String>::new());
    let upright = json!({ "op": "style_text", "node": "title", "state": "revenue", "from": 8, "to": 15, "look": { "style/italic": null } });
    assert_eq!(style(&doc, upright), example());

    // A role, a color the theme names, and emphasis; characters, not bytes, counted.
    let doc = json!({
        "scaena": "0.12", "canvas": { "width": 1920, "height": 1080 },
        "nodes": {
            "t": { "type": "text", "role": "body", "at": { "in": "title" },
                   "runs": [{ "text": "Hello " }, { "text": "wörld", "emphasis": "high" }, { "text": "!" }] },
            "u": { "type": "text", "role": "body", "text": "Yo", "at": { "in": "body" } },
        },
        "overrides": { "u": { "text": "Hey" } },
        "states": [{ "id": "a", "props": { "t": {}, "u": {} } }, { "id": "b", "props": { "t": { "runs": [{ "text": "Bye" }] } } }],
    });
    let look = json!({ "role": "caption", "style/color": "accent" });
    let c = style(&doc, json!({ "op": "style_text", "node": "t", "state": "a", "from": 3, "to": 8, "look": look }));
    assert_eq!(
        c["nodes"]["t"]["runs"],
        json!([
            { "text": "Hel" },
            { "text": "lo ", "role": "caption", "style": { "color": "accent" } },
            { "text": "wö", "emphasis": "high", "role": "caption", "style": { "color": "accent" } },
            { "text": "rld", "emphasis": "high" },
            { "text": "!" },
        ])
    );
    assert_eq!(c["states"][1], doc["states"][1], "`b` sets runs of its own");
    let back = json!({ "role": null, "style/color": null });
    let c = style(&c, json!({ "op": "style_text", "node": "t", "state": "a", "from": 3, "to": 8, "look": back }));
    assert_eq!(c, doc);

    // Text the deck's overrides set is styled there, and they hold runs in its place.
    let c = style(
        &doc,
        json!({ "op": "style_text", "node": "u", "state": "a", "from": 0, "to": 3, "look": { "emphasis": "low" } }),
    );
    assert_eq!(c["overrides"]["u"], json!({ "runs": [{ "text": "Hey", "emphasis": "low" }] }));
    assert_eq!(c["nodes"]["u"], doc["nodes"]["u"]);

    // Forked, into the state's own delta.
    let c = style(
        &example(),
        json!({ "op": "style_text", "node": "note", "state": "revenue", "from": 0, "to": 7, "look": { "style/weight": 600 }, "fork": true }),
    );
    assert_eq!(c["nodes"]["note"], example()["nodes"]["note"]);
    assert_eq!(
        c["states"][1]["props"]["note"]["runs"],
        json!([{ "text": "Revenue", "style": { "weight": 600 } }, { "text": " in $M. Enterprise recognized on delivery." }])
    );

    for (op, says) in [
        (
            json!({ "op": "style_text", "node": "title", "from": 0, "to": 2, "look": { "style/size": 90 } }),
            "comes with a role",
        ),
        (
            json!({ "op": "style_text", "node": "title", "from": 0, "to": 2, "look": { "style/color": "#ff0000" } }),
            "written out",
        ),
        (
            json!({ "op": "style_text", "node": "title", "from": 0, "to": 2, "look": { "fit": "shrink" } }),
            "a run's look is",
        ),
        (
            json!({ "op": "style_text", "node": "title", "from": 0, "to": 2, "look": { "style/kerning": 1 } }),
            "a run's look is",
        ),
        (
            json!({ "op": "style_text", "node": "title", "from": 2, "to": 2, "look": { "emphasis": "high" } }),
            "select no characters",
        ),
        (
            json!({ "op": "style_text", "node": "title", "from": 0, "to": 10, "look": { "emphasis": "high" } }),
            "9 characters",
        ),
        (
            json!({ "op": "style_text", "node": "rev", "from": 0, "to": 1, "look": { "emphasis": "high" } }),
            "a text node's characters",
        ),
        (json!({ "op": "style_text", "node": "title", "from": 0, "to": 1, "look": {} }), "look"),
    ] {
        let e = patch(&example(), json!([op])).unwrap_err();
        assert!(e.to_string().contains(says), "{e}");
    }
}

#[test]
fn replace_text_keeps_runs_and_their_looks() {
    let doc = json!({
        "scaena": "0.12", "canvas": { "width": 1920, "height": 1080 },
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

#[test]
fn choose_writes_a_choice_where_the_property_lives() {
    let shown = |doc: &Value, i: usize, node: &str, prop: &str| {
        let snaps = scaena_core::resolve_states(&deck(doc)).unwrap();
        snaps[i].nodes.get(node).and_then(|p| p.get(prop).cloned())
    };
    // `revenue` sets the title's role, and `mix` tracks it from there: a choice in `mix`
    // changes it in `revenue`'s delta, and the node keeps its own.
    let c = patch(
        &example(),
        json!([{ "op": "choose", "node": "title", "prop": "role", "value": "title", "state": "mix" }]),
    )
    .unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "add", "path": "/states/1/props/title/role", "value": "title" }]));
    assert_eq!(shown(&c.doc, 2, "title", "role"), Some(json!("title")));
    assert_eq!(c.doc["nodes"]["title"]["role"], "display");
    assert!(errors(&c.doc).is_empty(), "{:?}", errors(&c.doc));

    // Nothing in the states sets the note's role: the node's changes. Forked, `revenue`'s
    // delta takes it, and `mix` with it.
    let c = patch(
        &example(),
        json!([{ "op": "choose", "node": "note", "prop": "role", "value": "body", "state": "revenue" }]),
    )
    .unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "add", "path": "/nodes/note/role", "value": "body" }]));
    let c = patch(
        &example(),
        json!([{ "op": "choose", "node": "note", "prop": "role", "value": "body", "state": "revenue", "fork": true }]),
    )
    .unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "add", "path": "/states/1/props/note/role", "value": "body" }]));
    assert_eq!(shown(&c.doc, 2, "note", "role"), Some(json!("body")), "`mix` tracks it");

    // One key of an object property: a theme color goes where the style lives, the node.
    let c = patch(
        &example(),
        json!([{ "op": "choose", "node": "title", "prop": "style/color", "value": "accent", "state": "revenue" }]),
    )
    .unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "add", "path": "/nodes/title/style", "value": { "color": "accent" } }]));
    // Taken away, the node's style goes with its last key: the deck is as it was.
    let away = patch(
        &c.doc,
        json!([{ "op": "choose", "node": "title", "prop": "style/color", "value": null, "state": "revenue" }]),
    )
    .unwrap();
    assert_eq!(rfc(&away), json!([{ "op": "remove", "path": "/nodes/title/style" }]));
    assert_eq!(away.doc, example());

    // A color written out is an override: it goes in the deck's `overrides`, the only place
    // it is legal, in every state; it cannot be kept to one.
    let red =
        json!([{ "op": "choose", "node": "title", "prop": "style/color", "value": "#ff3366", "state": "revenue" }]);
    let c = patch(&example(), red).unwrap();
    assert_eq!(
        rfc(&c),
        json!([{ "op": "add", "path": "/overrides", "value": { "title": { "style": { "color": "#ff3366" } } } }])
    );
    assert!(errors(&c.doc).is_empty(), "no W300 in overrides: {:?}", errors(&c.doc));
    let overridden = c.doc;
    let forked = json!([{ "op": "choose", "node": "title", "prop": "style/color", "value": "#ff3366", "state": "revenue", "fork": true }]);
    let e = patch(&example(), forked).unwrap_err().to_string();
    assert!(e.contains("cannot be kept to `revenue`"), "{e}");

    // So is a text size. Where the overrides set the property, a theme name goes there too,
    // since they win in every state; and kept to a state, it would not show.
    let c = patch(
        &overridden,
        json!([
            { "op": "choose", "node": "title", "prop": "style/size", "value": 120, "state": "revenue" },
            { "op": "choose", "node": "title", "prop": "style/color", "value": "accent", "state": "revenue" }
        ]),
    )
    .unwrap();
    assert_eq!(c.doc["overrides"], json!({ "title": { "style": { "color": "accent", "size": 120 } } }));
    let forked = json!([{ "op": "choose", "node": "title", "prop": "style/color", "value": "ink", "state": "revenue", "fork": true }]);
    let e = patch(&overridden, forked).unwrap_err().to_string();
    assert!(e.contains("`overrides` set `title`'s style/color in every state"), "{e}");

    // `null` takes a choice away where it lives: out of the overrides, which go once empty;
    // out of `revenue`'s delta, so the node's own role shows there again.
    let c = patch(
        &overridden,
        json!([{ "op": "choose", "node": "title", "prop": "style/color", "value": null, "state": "revenue" }]),
    )
    .unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "remove", "path": "/overrides" }]));
    let c = patch(
        &example(),
        json!([{ "op": "choose", "node": "title", "prop": "role", "value": null, "state": "revenue" }]),
    )
    .unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "remove", "path": "/states/1/props/title/role" }]));
    assert_eq!(shown(&c.doc, 1, "title", "role"), Some(json!("display")));

    // A name the theme lacks is the deck's error, as any patch's: E102 says what it has.
    let c = patch(
        &example(),
        json!([{ "op": "choose", "node": "note", "prop": "role", "value": "nowhere", "state": "revenue" }]),
    )
    .unwrap();
    assert!(errors(&c.doc).iter().any(|e| e.starts_with("E102 text role `nowhere`")), "{:?}", errors(&c.doc));
    let e = patch(&example(), json!([{ "op": "choose", "node": "title", "prop": "style/color/x", "value": 1 }]))
        .unwrap_err()
        .to_string();
    assert!(e.contains("not a property"), "{e}");
}

#[test]
fn set_state_writes_a_layout_where_it_lives_and_the_rest_in_the_state() {
    let layouts = |doc: &Value| -> Vec<Option<String>> {
        scaena_core::resolve_states(&deck(doc)).unwrap().into_iter().map(|s| s.layout).collect()
    };
    let figure = || Some("figure".to_string());
    let title = || Some("title".to_string());
    // `mix` builds on `revenue` and takes its layout from there: a layout chosen in `mix` is
    // written in `revenue`, and both show it.
    let c = patch(&example(), json!([{ "op": "set_state", "id": "mix", "prop": "layout", "value": "full" }])).unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "add", "path": "/states/1/layout", "value": "full" }]));
    let full = || Some("full".to_string());
    assert_eq!(layouts(&c.doc), [title(), full(), full(), title()]);
    assert!(errors(&c.doc).is_empty(), "{:?}", errors(&c.doc));
    // Forked, it is `mix`'s own, and `revenue` keeps its layout.
    let c =
        patch(&example(), json!([{ "op": "set_state", "id": "mix", "prop": "layout", "value": "full", "fork": true }]))
            .unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "add", "path": "/states/2/layout", "value": "full" }]));
    assert_eq!(layouts(&c.doc), [title(), figure(), full(), title()]);
    // Taken away where it lives, what is under it shows: `revenue` tracks `intro`'s.
    let c = patch(&example(), json!([{ "op": "set_state", "id": "mix", "prop": "layout", "value": null }])).unwrap();
    assert_eq!(rfc(&c), json!([{ "op": "remove", "path": "/states/1/layout" }]));
    assert_eq!(layouts(&c.doc), [title(), title(), title(), title()]);
    // A name the theme lacks is the deck's error, as any patch's: E102 says what it has.
    let c =
        patch(&example(), json!([{ "op": "set_state", "id": "close", "prop": "layout", "value": "nowhere" }])).unwrap();
    assert!(errors(&c.doc).iter().any(|e| e.starts_with("E102 layout `nowhere`")), "{:?}", errors(&c.doc));

    // A transition is the state's own. One that is a bare duration stays bare while its
    // duration is all it sets, and becomes an object to take another key; one that is an
    // object stays one.
    let set = |id: &str, prop: &str, value: Value| {
        let c = patch(&example(), json!([{ "op": "set_state", "id": id, "prop": prop, "value": value }])).unwrap();
        assert!(errors(&c.doc).is_empty(), "{id} {prop}: {:?}", errors(&c.doc));
        (rfc(&c), c.doc)
    };
    let (ops, doc) = set("intro", "transition/duration", json!("fast"));
    assert_eq!(ops, json!([{ "op": "add", "path": "/states/0/transition", "value": "fast" }]));
    assert_eq!(doc["states"][0]["transition"], "fast", "a cut takes a duration bare");
    let (ops, _) = set("mix", "transition/ease", json!("out"));
    assert_eq!(
        ops,
        json!([{ "op": "add", "path": "/states/2/transition", "value": { "duration": "slow", "ease": "out" } }])
    );
    let (ops, _) = set("revenue", "transition/duration", json!("slow"));
    assert_eq!(ops, json!([{ "op": "add", "path": "/states/1/transition/duration", "value": "slow" }]));
    let (ops, _) = set("revenue", "transition/spring", json!("gentle"));
    assert_eq!(ops, json!([{ "op": "add", "path": "/states/1/transition/spring", "value": "gentle" }]));
    // `null` takes a key away; a transition left with nothing goes, and the state cuts.
    let (ops, _) = set("revenue", "transition/ease", Value::Null);
    assert_eq!(ops, json!([{ "op": "remove", "path": "/states/1/transition/ease" }]));
    let (ops, _) = set("mix", "transition/duration", Value::Null);
    assert_eq!(ops, json!([{ "op": "remove", "path": "/states/2/transition" }]));
    let (ops, _) = set("mix", "transition/ease", Value::Null);
    assert_eq!(ops, json!([]), "a key it does not set is not there to take away: `slow` stays bare");
    let (ops, _) = set("revenue", "transition", Value::Null);
    assert_eq!(ops, json!([{ "op": "remove", "path": "/states/1/transition" }]));
    // Its hold and its notes.
    let (ops, _) = set("close", "hold", json!(4500));
    assert_eq!(ops, json!([{ "op": "add", "path": "/states/3/hold", "value": 4500 }]));
    let (ops, _) = set("revenue", "notes", json!("Let the bars land."));
    assert_eq!(ops, json!([{ "op": "add", "path": "/states/1/notes", "value": "Let the bars land." }]));
    let (ops, _) = set("intro", "notes", Value::Null);
    assert_eq!(ops, json!([{ "op": "remove", "path": "/states/0/notes" }]));
    // A duration the theme lacks is E102.
    let c = patch(
        &example(),
        json!([{ "op": "set_state", "id": "mix", "prop": "transition/duration", "value": "glacial" }]),
    )
    .unwrap();
    assert!(errors(&c.doc).iter().any(|e| e.starts_with("E102 duration `glacial`")), "{:?}", errors(&c.doc));

    // What it does not set, it says so, and what does.
    for (prop, says) in [
        ("mode", "not one `set_state` sets"),
        ("transition/delay", "not one `set_state` sets"),
        ("props", "not one `set_state` sets"),
        ("layout/x/y", "not a property"),
    ] {
        let e = patch(&example(), json!([{ "op": "set_state", "id": "mix", "prop": prop, "value": 1 }]))
            .unwrap_err()
            .to_string();
        assert!(e.contains(says), "{prop}: {e}");
    }
    let e = patch(&example(), json!([{ "op": "set_state", "id": "nowhere", "prop": "hold", "value": 1 }]))
        .unwrap_err()
        .to_string();
    assert!(e.contains("no state `nowhere`"), "{e}");
}

/// `group` puts nodes in a new group where they stand, shown in each state that shows one of
/// them in it, and `ungroup` gives the deck back as it was (PLAN 2.43, ADR-0008).
#[test]
fn group_holds_nodes_where_they_stand_and_ungroup_gives_the_deck_back() {
    let original = example();
    // The title and subtitle: the title shows in every state, so the group does too.
    let c =
        patch(&original, json!([{ "op": "group", "id": "heading", "nodes": ["title", "subtitle"], "state": "intro" }]))
            .unwrap();
    assert_eq!(c.doc["nodes"]["heading"], json!({ "type": "group" }));
    assert_eq!(c.doc["nodes"]["title"]["at"], json!({ "in": "title", "parent": "heading" }));
    assert_eq!(c.doc["nodes"]["subtitle"]["at"], json!({ "in": "subtitle", "parent": "heading" }));
    assert_eq!(c.doc["states"][0]["props"]["heading"], json!({}), "it enters with them in `intro`, and stays");
    assert!(c.doc["states"].as_array().unwrap()[1..].iter().all(|s| s["props"].get("heading").is_none()));
    assert_eq!(errors(&c.doc), Vec::<String>::new());
    let back = patch(&c.doc, json!([{ "op": "ungroup", "group": "heading" }])).unwrap();
    assert_eq!(back.doc.to_string(), original.to_string(), "ungroup gives the deck back, key for key");

    // The chart and its note, in `revenue` and `mix`: the group enters in `revenue` and
    // leaves with them in `close`.
    let c =
        patch(&original, json!([{ "op": "group", "id": "figure", "nodes": ["rev", "note"], "state": "mix" }])).unwrap();
    assert_eq!(c.doc["states"][1]["props"]["figure"], json!({}));
    assert_eq!(c.doc["states"][3]["remove"], json!(["rev", "note", "figure"]));
    assert_eq!(errors(&c.doc), Vec::<String>::new());
    let snapshots = scaena_core::resolve_states(&deck(&c.doc)).unwrap();
    let shows: Vec<bool> = snapshots.iter().map(|s| s.nodes.contains_key("figure")).collect();
    assert_eq!(shows, [false, true, true, false]);
    let back = patch(&c.doc, json!([{ "op": "ungroup", "group": "figure" }])).unwrap();
    assert_eq!(back.doc.to_string(), original.to_string());

    // A group sits where its highest member sat: the backdrop stays behind, the title in front.
    let c = patch(&original, json!([{ "op": "group", "id": "cover", "nodes": ["bg", "title"], "state": "intro" }]))
        .unwrap();
    assert_eq!(c.doc["nodes"]["cover"], json!({ "type": "group" }), "the title sets no z: 0, over the backdrop's -100");
    let c = patch(&original, json!([{ "op": "group", "id": "back", "nodes": ["bg"], "state": "intro" }])).unwrap();
    assert_eq!(c.doc["nodes"]["back"], json!({ "type": "group", "z": -100 }));
    assert_eq!(c.doc["nodes"]["bg"]["z"], json!(-100), "each keeps its own");
}

/// A group in a group, and what `group` and `ungroup` refuse (PLAN 2.43).
#[test]
fn group_nests_in_a_group_and_refuses_what_it_cannot_hold() {
    let original = example();
    let c = patch(
        &original,
        json!([
            { "op": "group", "id": "heading", "nodes": ["title", "subtitle"], "state": "intro" },
            { "op": "group", "id": "words", "nodes": ["subtitle"], "state": "intro" },
        ]),
    )
    .unwrap();
    assert_eq!(c.doc["nodes"]["words"], json!({ "type": "group", "at": { "parent": "heading" } }));
    assert_eq!(c.doc["nodes"]["subtitle"]["at"], json!({ "in": "subtitle", "parent": "words" }));
    assert_eq!(errors(&c.doc), Vec::<String>::new());
    // Out of the inner group, the subtitle is the outer group's again.
    let out = patch(&c.doc, json!([{ "op": "ungroup", "group": "words" }])).unwrap();
    assert_eq!(out.doc["nodes"]["subtitle"]["at"], json!({ "in": "subtitle", "parent": "heading" }));
    assert!(out.doc["nodes"].get("words").is_none());
    assert_eq!(errors(&out.doc), Vec::<String>::new());

    let refused = |ops: Value| patch(&original, ops).unwrap_err().message;
    let two = refused(json!([
        { "op": "group", "id": "heading", "nodes": ["title"], "state": "intro" },
        { "op": "group", "id": "pair", "nodes": ["title", "bg"], "state": "intro" },
    ]));
    assert!(two.contains("a group holds what one container holds"), "{two}");
    let stack = refused(json!([
        { "op": "add_node", "id": "row", "node": { "type": "stack", "at": { "in": "main" } } },
        { "op": "set_prop", "node": "note", "prop": "at", "value": { "parent": "row" } },
        { "op": "group", "id": "pair", "nodes": ["note"] },
    ]));
    assert!(stack.contains("places what it holds itself"), "{stack}");
    assert!(
        refused(json!([{ "op": "group", "id": "title", "nodes": ["note"] }]))
            .contains("there is a node `title` already")
    );
    assert!(refused(json!([{ "op": "group", "id": "pair", "nodes": ["note", "note"] }])).contains("named twice"));
    assert!(
        refused(json!([{ "op": "group", "id": "pair", "nodes": ["rev"], "state": "intro" }])).contains("not on screen")
    );
    assert!(refused(json!([{ "op": "group", "id": "pair", "nodes": [] }])).contains("one node or more"));
    assert!(refused(json!([{ "op": "ungroup", "group": "title" }])).contains("takes a group's children out"));
}

/// `time_motion` sets a motion's delay and duration where the engine reads them: the
/// choreography item that moves the node so, else the node's own preset (PLAN 2.44).
#[test]
fn time_motion_writes_where_the_motion_is_written() {
    let original = example();
    // The subtitle's rise in `intro`, a choreography item with a delay of its own.
    let c = patch(&original, json!([{ "op": "time_motion", "node": "subtitle", "motion": "enter", "state": "intro", "delay": 300, "duration": 600 }]))
        .unwrap();
    assert_eq!(
        c.doc["states"][0]["choreography"][1],
        json!({ "target": "subtitle", "enter": "rise", "delay": 300, "duration": 600 })
    );
    assert_eq!(errors(&c.doc), Vec::<String>::new());
    // A delay of 0 is no delay; a theme duration is written by its name.
    let c = patch(&original, json!([{ "op": "time_motion", "node": "subtitle", "motion": "enter", "state": "intro", "delay": 0, "duration": "slow" }]))
        .unwrap();
    assert_eq!(
        c.doc["states"][0]["choreography"][1],
        json!({ "target": "subtitle", "enter": "rise", "duration": "slow" })
    );

    // The chart's grow is a call: its delay goes on the item, which wins over the call.
    let c = patch(
        &original,
        json!([{ "op": "time_motion", "node": "rev", "motion": "enter", "state": "revenue", "delay": 120 }]),
    )
    .unwrap();
    assert_eq!(c.doc["states"][1]["choreography"][0]["delay"], json!(120));
    assert_eq!(c.doc["states"][1]["choreography"][0]["enter"], original["states"][1]["choreography"][0]["enter"]);
    // It runs on a spring, which lasts as long as it settles.
    let sprung = patch(
        &original,
        json!([{ "op": "time_motion", "node": "rev", "motion": "enter", "state": "revenue", "duration": 500 }]),
    )
    .unwrap_err()
    .message;
    assert!(sprung.contains("runs on the spring `snappy`"), "{sprung}");

    // A node's own entrance, where it lives: a name becomes a call, and back again.
    let own = patch(
        &original,
        json!([{ "op": "apply_preset", "node": "bg", "preset": "fade", "motion": "enter", "state": "close" }]),
    )
    .unwrap()
    .doc;
    let c = patch(&own, json!([{ "op": "time_motion", "node": "bg", "motion": "enter", "state": "close", "delay": 120, "duration": "fast" }]))
        .unwrap();
    assert_eq!(
        c.doc["states"][3]["props"]["bg"]["enter"],
        json!({ "preset": "fade", "delay": 120, "duration": "fast" })
    );
    assert_eq!(errors(&c.doc), Vec::<String>::new());
    let back =
        patch(&c.doc, json!([{ "op": "time_motion", "node": "bg", "motion": "enter", "state": "close", "delay": 0 }]))
            .unwrap();
    assert_eq!(back.doc["states"][3]["props"]["bg"]["enter"], json!({ "preset": "fade", "duration": "fast" }));

    // An exit is read from the state the node leaves: the subtitle leaves `intro` for `revenue`.
    let own = patch(
        &original,
        json!([{ "op": "apply_preset", "node": "subtitle", "preset": "fade", "motion": "exit", "state": "intro" }]),
    )
    .unwrap()
    .doc;
    let c = patch(
        &own,
        json!([{ "op": "time_motion", "node": "subtitle", "motion": "exit", "state": "revenue", "duration": 200 }]),
    )
    .unwrap();
    assert_eq!(c.doc["states"][0]["props"]["subtitle"]["exit"], json!({ "preset": "fade", "duration": 200 }));
}

/// `anim` tracks keep their keys spaced as they were: a delay moves them, a duration stretches
/// them from the first (PLAN 2.44).
#[test]
fn time_motion_moves_and_stretches_anim_keys() {
    let tracks = json!({ "opacity": [{ "t": 100, "v": 0 }, { "t": 500, "v": 1, "ease": "out" }], "rotate": [{ "t": 300, "v": 4 }] });
    let animated = patch(
        &example(),
        json!([{ "op": "set_prop", "node": "title", "state": "mix", "prop": "anim", "value": tracks }]),
    )
    .unwrap()
    .doc;
    let c = patch(&animated, json!([{ "op": "time_motion", "node": "title", "motion": "anim", "state": "mix", "delay": 300, "duration": 200 }]))
        .unwrap();
    assert_eq!(
        c.doc["states"][2]["props"]["title"]["anim"],
        json!({ "opacity": [{ "t": 300, "v": 0 }, { "t": 500, "v": 1, "ease": "out" }], "rotate": [{ "t": 400, "v": 4 }] })
    );
    assert_eq!(errors(&c.doc), Vec::<String>::new());

    // In choreography, an item's delay is the item's, in a sequence too.
    let sequenced = patch(
        &animated,
        json!([{ "op": "add", "path": "/states/2/choreography", "value": [
            { "sequence": [ { "target": "rev", "emphasis": "pulse" }, { "target": ["title", "rev"], "anim": tracks } ] }
        ] }]),
    )
    .unwrap()
    .doc;
    let c = patch(
        &sequenced,
        json!([{ "op": "time_motion", "node": "rev", "motion": "anim", "state": "mix", "delay": 80, "duration": 800 }]),
    )
    .unwrap();
    let item = &c.doc["states"][2]["choreography"][0]["sequence"][1];
    assert_eq!(item["delay"], json!(80));
    assert_eq!(item["anim"]["opacity"], json!([{ "t": 100, "v": 0 }, { "t": 900, "v": 1, "ease": "out" }]));
    assert_eq!(item["anim"]["rotate"], json!([{ "t": 500, "v": 4 }]));
    assert_eq!(
        c.doc["states"][2]["props"]["title"]["anim"], tracks,
        "the title's own, which the item's gives way to, stays"
    );
    let pulse = patch(
        &sequenced,
        json!([{ "op": "time_motion", "node": "rev", "motion": "emphasis", "state": "mix", "delay": 50 }]),
    )
    .unwrap();
    assert_eq!(pulse.doc["states"][2]["choreography"][0]["sequence"][0]["delay"], json!(50));

    let refused = |doc: &Value, ops: Value| patch(doc, ops).unwrap_err().message;
    let flat = json!({ "opacity": [{ "t": 0, "v": 0 }] });
    let one = patch(
        &example(),
        json!([{ "op": "set_prop", "node": "title", "state": "mix", "prop": "anim", "value": flat }]),
    )
    .unwrap()
    .doc;
    let stretch = refused(
        &one,
        json!([{ "op": "time_motion", "node": "title", "motion": "anim", "state": "mix", "duration": 400 }]),
    );
    assert!(stretch.contains("no time between them"), "{stretch}");
    let none = refused(
        &example(),
        json!([{ "op": "time_motion", "node": "title", "motion": "emphasis", "state": "mix", "delay": 10 }]),
    );
    assert!(none.contains("is not there to time: `apply_preset` gives it one"), "{none}");
    let nothing =
        refused(&example(), json!([{ "op": "time_motion", "node": "subtitle", "motion": "enter", "state": "intro" }]));
    assert!(nothing.contains("name the `delay` or the `duration`"), "{nothing}");
    let named = refused(
        &example(),
        json!([{ "op": "time_motion", "node": "subtitle", "motion": "enter", "state": "intro", "duration": "glacial" }]),
    );
    assert!(named.contains("no duration `glacial`; it has `fast`, `standard`, `slow`"), "{named}");
}

/// `written` says where `time_motion` writes a motion, and the delay it reads there (PLAN 2.44).
#[test]
fn written_finds_a_motion_where_time_motion_writes_it() {
    let doc = example();
    let at = |doc: &Value, state: &str, node: &str, motion: Timed| written(doc, state, node, motion).unwrap();
    let item = |pointer: &str, delay: f64| Written { pointer: pointer.into(), delay };
    assert_eq!(at(&doc, "intro", "subtitle", Timed::Enter), item("/states/0/choreography/1", 240.0));
    assert_eq!(at(&doc, "revenue", "rev", Timed::Enter), item("/states/1/choreography/0", 0.0));
    let timed = patch(
        &doc,
        json!([
            { "op": "apply_preset", "node": "bg", "preset": "fade", "motion": "enter", "state": "close" },
            { "op": "time_motion", "node": "bg", "motion": "enter", "state": "close", "delay": 120 },
            { "op": "set_prop", "node": "title", "state": "mix", "prop": "anim", "value": { "opacity": [{ "t": 100, "v": 0 }, { "t": 400, "v": 1 }] } },
        ]),
    )
    .unwrap()
    .doc;
    assert_eq!(at(&timed, "close", "bg", Timed::Enter), item("/states/3/props/bg/enter", 120.0));
    assert_eq!(at(&timed, "mix", "title", Timed::Anim), item("/states/2/props/title/anim", 100.0));
    assert!(written(&doc, "mix", "title", Timed::Exit).unwrap_err().contains("is not there to time"));
}

#[test]
fn annotate_writes_a_charts_annotations_where_they_live() {
    let original = example();
    let callout = json!({ "kind": "callout", "at": { "x": "2026-Q3", "series": "Pro" }, "text": "Pro nearly doubled" });
    // No state sets the chart's annotations: one made in `revenue` goes on the chart, and `mix`
    // shows it too.
    let c = patch(&original, json!([{ "op": "annotate", "node": "rev", "state": "revenue", "annotation": callout }]))
        .unwrap();
    assert_eq!(c.doc["nodes"]["rev"]["annotations"], json!([callout]));
    assert_eq!(errors(&c.doc), Vec::<String>::new());

    // A rule after it; then the callout moved and its text changed, and the rule's text taken
    // away, each merged into the one at its index.
    let c = patch(
        &c.doc,
        json!([
            { "op": "annotate", "node": "rev", "state": "revenue", "annotation": { "kind": "rule", "at": { "y": 20 }, "text": "Target" } },
            { "op": "annotate", "node": "rev", "state": "revenue", "index": 0, "annotation": { "at": { "x": "2026-Q2", "series": "Pro" }, "text": "Pro took off" } },
            { "op": "annotate", "node": "rev", "state": "revenue", "index": 1, "annotation": { "text": null } }
        ]),
    )
    .unwrap();
    assert_eq!(
        c.doc["nodes"]["rev"]["annotations"],
        json!([
            { "kind": "callout", "at": { "x": "2026-Q2", "series": "Pro" }, "text": "Pro took off" },
            { "kind": "rule", "at": { "y": 20 } }
        ])
    );
    assert_eq!(errors(&c.doc), Vec::<String>::new());

    // Kept to `mix`, they are its own: `revenue` shows the chart's two.
    let kept = patch(
        &c.doc,
        json!([{ "op": "annotate", "node": "rev", "state": "mix", "fork": true, "annotation": { "kind": "highlight", "at": { "series": "Enterprise" } } }]),
    )
    .unwrap();
    assert_eq!(kept.doc["states"][2]["props"]["rev"]["annotations"].as_array().unwrap().len(), 3);
    assert_eq!(kept.doc["nodes"]["rev"]["annotations"], c.doc["nodes"]["rev"]["annotations"]);
    assert_eq!(errors(&kept.doc), Vec::<String>::new());
    // There, each change goes where they now live, and emptied, `mix` keeps an empty list, so
    // the chart's do not show again.
    let away = json!({ "op": "annotate", "node": "rev", "state": "mix", "index": 0, "annotation": null });
    let emptied = patch(&kept.doc, json!([away, away, away])).unwrap();
    assert_eq!(emptied.doc["states"][2]["props"]["rev"]["annotations"], json!([]));
    assert_eq!(emptied.doc["nodes"]["rev"]["annotations"], c.doc["nodes"]["rev"]["annotations"]);

    // On the chart itself, emptied, the list goes.
    let gone = patch(
        &c.doc,
        json!([
            { "op": "annotate", "node": "rev", "index": 1, "annotation": null },
            { "op": "annotate", "node": "rev", "index": 0, "annotation": null }
        ]),
    )
    .unwrap();
    assert!(gone.doc["nodes"]["rev"].get("annotations").is_none(), "{}", gone.doc["nodes"]["rev"]);

    // Where the deck's overrides set them, they are written there, over every state, and an
    // index counts them as the overrides have them.
    let mut over = c.doc.clone();
    over["overrides"] = json!({ "rev": { "annotations": [{ "kind": "highlight", "at": { "series": "Pro" } }] } });
    let o = patch(
        &over,
        json!([
            { "op": "annotate", "node": "rev", "state": "revenue", "annotation": callout },
            { "op": "annotate", "node": "rev", "state": "revenue", "index": 0, "annotation": null }
        ]),
    )
    .unwrap();
    assert_eq!(o.doc["overrides"]["rev"]["annotations"], json!([callout]));
    assert_eq!(o.doc["nodes"]["rev"]["annotations"], c.doc["nodes"]["rev"]["annotations"]);
    assert_eq!(errors(&o.doc), Vec::<String>::new());

    // What it cannot do, it refuses, saying why.
    let refused = |ops: Value| patch(&c.doc, ops).unwrap_err().message;
    for (ops, why) in [
        (json!([{ "op": "annotate", "node": "title", "annotation": callout }]), "a chart takes annotations"),
        (
            json!([{ "op": "annotate", "node": "rev", "index": 5, "annotation": null }]),
            "2 annotations, at `index` 0 to 1",
        ),
        (json!([{ "op": "annotate", "node": "rev", "annotation": { "kind": "rule" } }]), "a `kind` and an `at`"),
        (
            json!([{ "op": "annotate", "node": "rev", "annotation": { "kind": "callout", "at": { "x": "2026-Q2" } } }]),
            "give it `text`",
        ),
        (json!([{ "op": "annotate", "node": "rev", "annotation": null }]), "say which by its `index`"),
        (json!([{ "op": "annotate", "node": "rev", "index": 0 }]), "give `annotation`"),
        (
            json!([{ "op": "annotate", "node": "rev", "index": 1, "annotation": { "at": { "y": [1, 2] } } }]),
            "one value",
        ),
        (json!([{ "op": "annotate", "node": "rev", "fork": true, "annotation": callout }]), "name it (`state`)"),
    ] {
        let message = refused(ops.clone());
        assert!(message.contains(why), "{ops}: {message}");
    }
}

#[test]
fn list_marks_a_texts_paragraphs_and_replace_text_keeps_them_in_step() {
    // ADR-0018, PLAN 2.69: `t`'s three paragraphs, its list on the node; `b` forks its text.
    let doc = json!({
        "scaena": "0.12", "canvas": { "width": 1920, "height": 1080 },
        "nodes": {
            "t": { "type": "text", "role": "body", "text": "One\nTwo\nThree", "at": { "in": "body" } },
        },
        "states": [{ "id": "a", "props": { "t": {} } }, { "id": "b", "props": { "t": { "text": "Uno\nDos" } } }],
    });
    let run = |doc: &Value, op: Value| patch(doc, json!([op])).map(|c| c.doc);
    // Bullets on the first two, from a range across them.
    let c =
        run(&doc, json!({ "op": "list", "node": "t", "state": "a", "from": 1, "to": 5, "kind": "bullet" })).unwrap();
    assert_eq!(c["nodes"]["t"]["list"], json!([{ "kind": "bullet" }, { "kind": "bullet" }]));
    assert_eq!(errors(&c), Vec::<String>::new());
    // Tab on the second: a level deeper. Numbers on the third, at level 1.
    let c = run(&c, json!({ "op": "list", "node": "t", "state": "a", "from": 5, "to": 5, "by": 1 })).unwrap();
    let c =
        run(&c, json!({ "op": "list", "node": "t", "state": "a", "from": 9, "to": 9, "kind": "number", "level": 1 }))
            .unwrap();
    assert_eq!(
        c["nodes"]["t"]["list"],
        json!([{ "kind": "bullet" }, { "kind": "bullet", "level": 1 }, { "kind": "number", "level": 1 }])
    );
    // Enter at the end of the first item: the new paragraph is an item like it.
    let typed =
        run(&c, json!({ "op": "replace_text", "node": "t", "state": "a", "from": 3, "to": 3, "text": "\n" })).unwrap();
    assert_eq!(typed["nodes"]["t"]["text"], "One\n\nTwo\nThree");
    assert_eq!(
        typed["nodes"]["t"]["list"],
        json!([{ "kind": "bullet" }, { "kind": "bullet" }, { "kind": "bullet", "level": 1 }, { "kind": "number", "level": 1 }])
    );
    // Words typed inside an item leave the list as it is.
    let words =
        run(&c, json!({ "op": "replace_text", "node": "t", "state": "a", "from": 2, "to": 2, "text": "ly" })).unwrap();
    assert_eq!(words["nodes"]["t"]["list"], c["nodes"]["t"]["list"]);
    // None takes the paragraphs out of the list.
    let none =
        run(&c, json!({ "op": "list", "node": "t", "state": "a", "from": 0, "to": 13, "kind": "none" })).unwrap();
    assert_eq!(none["nodes"]["t"].get("list"), None, "no items left, `list` goes");
    // In `b`, which sets text of its own, `fork` keeps the list there.
    let b = run(
        &doc,
        json!({ "op": "list", "node": "t", "state": "b", "from": 0, "to": 7, "kind": "number", "fork": true }),
    )
    .unwrap();
    assert_eq!(b["states"][1]["props"]["t"]["list"], json!([{ "kind": "number" }, { "kind": "number" }]));
    assert_eq!(b["nodes"]["t"].get("list"), None);
    // What it refuses.
    for (op, says) in [
        (json!({ "op": "list", "node": "t", "from": 0, "to": 1, "kind": "bullet", "by": 1 }), "not both"),
        (json!({ "op": "list", "node": "t", "from": 0, "to": 1 }), "say how"),
        (json!({ "op": "list", "node": "t", "from": 0, "to": 99, "kind": "bullet" }), "characters"),
    ] {
        let e = run(&doc, op).unwrap_err().to_string();
        assert!(e.contains(says), "{e}");
    }
}
