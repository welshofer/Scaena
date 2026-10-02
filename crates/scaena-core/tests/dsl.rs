//! The DSL's round trip (SPEC §4, PLAN 1.5): every deck in the repository decompiles to
//! source that compiles back to the same deck, byte for byte as canonical JSON, and
//! decompiling again gives the same source.

use scaena_core::Deck;
use scaena_core::dsl::{compile, decompile};

fn corpus() -> Vec<(String, String)> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut files = vec![
        "docs/examples/revenue.deck.json".to_string(),
        "docs/examples/trails.deck.json".into(),
        "docs/examples/authorability/deck.json".into(),
        "tests/fixtures/torture.scaena/deck.json".into(),
        "tests/bench/b1.scaena/deck.json".into(),
    ];
    for dir in [
        "docs/examples/authorability/history",
        "tests/lint/E102",
        "tests/lint/E104",
        "tests/lint/E105",
        "tests/lint/E106",
    ] {
        let mut more: Vec<String> = std::fs::read_dir(format!("{root}/{dir}"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".json"))
            .map(|n| format!("{dir}/{n}"))
            .collect();
        more.sort();
        files.extend(more);
    }
    files
        .into_iter()
        .map(|f| {
            let text = std::fs::read_to_string(format!("{root}/{f}")).unwrap();
            (f, text)
        })
        .collect()
}

/// `docs/examples/revenue.deck.scn` is the example deck's canonical source: what the
/// decompiler writes, and what compiles to the deck. `SCAENA_BLESS=1` rewrites it.
#[test]
fn the_example_source_is_its_decks_canonical_source() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/examples");
    let deck = Deck::from_json(&std::fs::read_to_string(format!("{root}/revenue.deck.json")).unwrap()).unwrap();
    let path = format!("{root}/revenue.deck.scn");
    let source = decompile(&deck);
    // SPEC §4.1 shows it whole.
    let spec_path = format!("{root}/../SPEC.md");
    let shown = |spec: &str| -> (String, String, String) {
        let (before, rest) = spec.split_once("```scn\n").unwrap();
        let (shown, after) = rest.split_once("```").unwrap();
        (before.into(), shown.into(), after.into())
    };
    if std::env::var_os("SCAENA_BLESS").is_some() {
        std::fs::write(&path, &source).unwrap();
        let (before, _, after) = shown(&std::fs::read_to_string(&spec_path).unwrap());
        std::fs::write(&spec_path, format!("{before}```scn\n{source}```{after}")).unwrap();
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source, "SCAENA_BLESS=1 rewrites it");
    assert_eq!(compile(&source).unwrap().to_json().unwrap(), deck.to_json().unwrap());
    let (_, in_spec, _) = shown(&std::fs::read_to_string(&spec_path).unwrap());
    assert_eq!(in_spec, source, "SPEC §4.1 shows the example as it is; SCAENA_BLESS=1 rewrites it");
}

#[test]
fn every_deck_in_the_repository_round_trips() {
    for (name, json) in corpus() {
        let deck = Deck::from_json(&json).unwrap_or_else(|e| panic!("{name}: {e}"));
        let source = decompile(&deck);
        let back = compile(&source).unwrap_or_else(|e| panic!("{name}: {e}\n{source}"));
        assert_eq!(back.to_json().unwrap(), deck.to_json().unwrap(), "{name}:\n{source}");
        assert_eq!(decompile(&back), source, "{name}: decompiling is a fixed point");
    }
}

/// A deterministic xorshift generator: the test's mutations are the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const STRINGS: &[&str] = &[
    "",
    " ",
    "a b",
    "true",
    "null",
    "false",
    "123",
    "1.5",
    "50%",
    "05%",
    "-5%",
    "1e3%",
    "@x",
    "@",
    "@3d",
    "#hash",
    "a#b",
    "\"quoted\"",
    "line1\nline2",
    "trailing \nx",
    "\n",
    "\n\n",
    "tab\there",
    "uni ✓ é 漢字",
    "\"\"\"",
    "back\\slash",
    "x-height",
    "space.4",
    "_x",
    "-x",
    "a:b",
    "{",
    "[",
    "(",
    ")",
    "choreo",
    "sequence",
    "parallel",
    "notes",
    "text",
    "chart",
    "shader",
    "kind",
    "at",
    "col",
    "deck",
    "node",
    "state",
    "1920x1080",
    "1-7",
    "1-",
    "ms",
    "\r\n",
    "  lead",
    "a\u{2028}b",
    "\u{0}",
    "emoji 🇺🇸",
    "16:9",
    "1.85:1",
    "016:9",
    "16:",
    ":9",
    "1:2:3",
    "4s",
    "40ms",
    "media",
];
const KEYS: &[&str] = &[
    "a",
    "with space",
    "_comment",
    "text",
    "src",
    "chart",
    "shader",
    "kind",
    "at",
    "type",
    "role",
    "true",
    "notes",
    "col",
    "in",
    "x",
    "1x",
    "",
    "-",
    "@k",
    "fit",
    "params",
    "delay",
    "stagger",
    "duration",
    "hold",
    "media",
];

fn random_value(r: &mut Rng, depth: usize) -> serde_json::Value {
    use serde_json::{Value, json};
    match r.below(if depth > 2 { 6 } else { 9 }) {
        0 => Value::Null,
        1 => Value::Bool(r.below(2) == 0),
        2 => json!(*r.pick(&[0i64, 1, -1, 7, 100, -9_223_372_036_854_775_808])),
        3 => json!(*r.pick(&[0.0f64, 1.0, -2.5, 1e300, std::f64::consts::PI, 0.1, -0.0])),
        4 => json!(u64::MAX),
        5 => Value::String(r.pick(STRINGS).to_string()),
        6 => Value::Array((0..r.below(4)).map(|_| random_value(r, depth + 1)).collect()),
        7 => {
            let mut m = serde_json::Map::new();
            for _ in 0..r.below(4) {
                m.insert(r.pick(KEYS).to_string(), random_value(r, depth + 1));
            }
            Value::Object(m)
        }
        _ => {
            // An `at` the call shorthand might take.
            let mut m = serde_json::Map::new();
            for k in ["col", "row", "in", "rect", "align"] {
                if r.below(2) == 0 {
                    m.insert(k.into(), random_value(r, depth + 1));
                }
            }
            Value::Object(m)
        }
    }
}

/// `map` with `key` set to `value` at a random place among its keys.
fn insert_at(r: &mut Rng, map: &mut serde_json::Map<String, serde_json::Value>, key: String, value: serde_json::Value) {
    map.shift_remove(&key);
    let at = r.below(map.len() + 1);
    let mut entries: Vec<(String, serde_json::Value)> = std::mem::take(map).into_iter().collect();
    entries.insert(at, (key, value));
    map.extend(entries);
}

fn mutate(r: &mut Rng, doc: &mut serde_json::Value) {
    use serde_json::Value;
    let node_ids: Vec<String> = doc["nodes"].as_object().unwrap().keys().cloned().collect();
    match r.below(21) {
        17 => {
            // Shuffle `nodes`: declarations in states must keep their order.
            let nodes = doc["nodes"].as_object_mut().unwrap();
            let mut entries: Vec<(String, Value)> = std::mem::take(nodes).into_iter().collect();
            for i in (1..entries.len()).rev() {
                entries.swap(i, r.below(i + 1));
            }
            nodes.extend(entries);
        }
        18 => {
            // A node first shown unchanged, so a state can declare it.
            let id = r.pick(&node_ids).clone();
            let states = doc["states"].as_array_mut().unwrap();
            let i = r.below(states.len());
            let state = states[i].as_object_mut().unwrap();
            let props = state.entry("props").or_insert_with(|| Value::Object(Default::default()));
            insert_at(r, props.as_object_mut().unwrap(), id, Value::Object(Default::default()));
        }
        19 => {
            // Times, in milliseconds, whole seconds, and floats.
            let time = |r: &mut Rng| serde_json::json!(*r.pick(&[0i64, 40, 1000, 1500, 4000, -2000, 60000]));
            let states = doc["states"].as_array_mut().unwrap();
            let i = r.below(states.len());
            states[i]["hold"] = time(r);
            states[i]["transition"] = match r.below(4) {
                0 => time(r),
                1 => serde_json::json!(2.5),
                2 => serde_json::json!({ "duration": time(r), "ease": "standard" }),
                _ => serde_json::json!({ "duration": 1000.0 }),
            };
            let item = serde_json::json!({ "target": r.pick(&node_ids).clone(), "delay": time(r), "stagger": 12.5,
                "duration": time(r), "enter": { "preset": "grow", "stagger": time(r), "delay": 0.5 } });
            let list = states[i].as_object_mut().unwrap().entry("choreography").or_insert_with(|| Value::Array(vec![]));
            list.as_array_mut().unwrap().push(item);
        }
        20 => {
            // A beat's seconds and its media.
            let duration = *r.pick(&[0.0f64, 0.5, 15.0, 1e-3, 90.25]);
            let media = match r.below(3) {
                0 => serde_json::json!({}),
                1 => serde_json::json!({ "podcast": { "script": "x" }, "with space": [1, 2] }),
                _ => random_value(r, 1),
            };
            let beat = serde_json::json!({ "id": "b", "claim": "c", "duration": duration, "media": media });
            doc["spine"] = serde_json::json!({ "sections": [{ "id": "s", "beats": [beat] }] });
        }
        10 => {
            let id = r.pick(&node_ids).clone();
            let mut props = serde_json::Map::new();
            for _ in 0..1 + r.below(3) {
                let key = r.pick(KEYS).to_string();
                let v = random_value(r, 1);
                insert_at(r, &mut props, key, v);
            }
            doc.as_object_mut()
                .unwrap()
                .entry("overrides")
                .or_insert_with(|| Value::Object(Default::default()))
                .as_object_mut()
                .unwrap()
                .insert(id, Value::Object(props));
        }
        11 => {
            let notes = r.pick(STRINGS).to_string();
            let claim = r.pick(STRINGS).to_string();
            let beat = serde_json::json!({ "id": r.pick(STRINGS).to_string(), "claim": claim, "notes": notes,
                "evidence": [r.pick(STRINGS).to_string()], "duration": 2.5, "media": random_value(r, 1) });
            let section = serde_json::json!({ "id": r.pick(STRINGS).to_string(), "title": r.pick(STRINGS).to_string(), "beats": [beat] });
            let spine =
                doc.as_object_mut().unwrap().entry("spine").or_insert_with(|| serde_json::json!({ "sections": [] }));
            if r.below(4) == 0 {
                spine["sections"] = serde_json::json!([]);
            } else {
                spine["sections"].as_array_mut().unwrap().push(section);
            }
        }
        12 => {
            let source = match r.below(3) {
                0 => Value::String(r.pick(STRINGS).to_string()),
                1 => serde_json::json!({ "inline": [random_value(r, 1), { "q": "Q1", "v": 1.5 }] }),
                _ => random_value(r, 1),
            };
            let mut ds = serde_json::json!({ "source": source });
            if r.below(2) == 0 {
                ds["schema"] = serde_json::json!({ "q": "string", "with space": "number" });
            }
            doc.as_object_mut()
                .unwrap()
                .entry("data")
                .or_insert_with(|| Value::Object(Default::default()))
                .as_object_mut()
                .unwrap()
                .insert(r.pick(STRINGS).to_string(), ds);
        }
        13 => {
            let font = serde_json::json!({ "family": r.pick(STRINGS).to_string(), "file": r.pick(STRINGS).to_string(),
                "weight": 400, "style": r.pick(STRINGS).to_string(), "axes": { "wght": [100.0, 900.5] } });
            doc.as_object_mut()
                .unwrap()
                .entry("fonts")
                .or_insert_with(|| Value::Array(vec![]))
                .as_array_mut()
                .unwrap()
                .push(font);
        }
        14 => {
            let states = doc["states"].as_array_mut().unwrap();
            let i = r.below(states.len());
            states[i]["id"] = Value::String(r.pick(STRINGS).to_string());
            if r.below(3) == 0 {
                states[i]["mode"] = Value::String("absolute".into());
            }
            if r.below(3) == 0 {
                states[i]["hold"] = serde_json::json!(1234.5);
            }
        }
        15 => {
            doc["theme"] = match r.below(3) {
                0 => Value::String(r.pick(STRINGS).to_string()),
                _ => random_value(r, 0),
            };
        }
        16 => {
            doc["canvas"] = serde_json::json!({ "width": 1920.5, "height": 1080 });
            doc["formats"] = serde_json::json!([r.pick(STRINGS).to_string()]);
        }
        0 | 1 => {
            let id = r.pick(&node_ids).clone();
            let node = doc["nodes"][&id].as_object_mut().unwrap();
            let key = r.pick(KEYS).to_string();
            if key != "type" {
                let v = random_value(r, 0);
                insert_at(r, node, key, v);
            }
        }
        2 | 3 => {
            let states = doc["states"].as_array_mut().unwrap();
            let i = r.below(states.len());
            let state = states[i].as_object_mut().unwrap();
            let props = state.entry("props").or_insert_with(|| Value::Object(Default::default()));
            let id = if r.below(5) == 0 { r.pick(STRINGS).to_string() } else { r.pick(&node_ids).clone() };
            let delta = props.as_object_mut().unwrap().entry(id).or_insert_with(|| Value::Object(Default::default()));
            let key = r.pick(KEYS).to_string();
            let v = random_value(r, 0);
            insert_at(r, delta.as_object_mut().unwrap(), key, v);
        }
        4 => {
            let states = doc["states"].as_array_mut().unwrap();
            let i = r.below(states.len());
            let field = *r.pick(&["notes", "_comment", "transition", "name", "layout", "slide", "from"]);
            let v = match field {
                "notes" | "_comment" | "name" | "layout" | "slide" | "from" => {
                    Value::String(r.pick(STRINGS).to_string())
                }
                _ => random_value(r, 1),
            };
            states[i][field] = v;
        }
        5 => {
            let states = doc["states"].as_array_mut().unwrap();
            let i = r.below(states.len());
            let item = match r.below(3) {
                0 => serde_json::json!({ "target": r.pick(STRINGS).to_string(), "delay": 10 }),
                1 => serde_json::json!({ "sequence": [{ "target": "a" }, random_value(r, 1)], "timing": "with" }),
                _ => random_value(r, 1),
            };
            let list = states[i].as_object_mut().unwrap().entry("choreography").or_insert_with(|| Value::Array(vec![]));
            list.as_array_mut().unwrap().push(item);
        }
        6 => {
            let states = doc["states"].as_array_mut().unwrap();
            let i = r.below(states.len());
            let id = r.pick(STRINGS).to_string();
            states[i]
                .as_object_mut()
                .unwrap()
                .entry("remove")
                .or_insert_with(|| Value::Array(vec![]))
                .as_array_mut()
                .unwrap()
                .push(Value::String(id));
        }
        7 => {
            let meta = doc.as_object_mut().unwrap().entry("meta").or_insert_with(|| Value::Object(Default::default()));
            let key = r.pick(KEYS).to_string();
            let v = random_value(r, 1);
            meta.as_object_mut().unwrap().insert(key, v);
        }
        8 => {
            let comment = r.pick(STRINGS).to_string();
            doc["_comment"] = Value::String(comment);
        }
        _ => {
            // A new node, with an id anything might be.
            let id = r.pick(STRINGS).to_string();
            let ty = *r.pick(&["text", "image", "chart", "shader", "group"]);
            let mut node = serde_json::Map::new();
            node.insert("type".into(), Value::String(ty.into()));
            for _ in 0..r.below(3) {
                let key = r.pick(KEYS).to_string();
                if key != "type" {
                    let v = random_value(r, 1);
                    node.insert(key, v);
                }
            }
            doc["nodes"].as_object_mut().unwrap().insert(id, Value::Object(node));
        }
    }
}

#[test]
fn any_deck_round_trips() {
    let bases: Vec<serde_json::Value> =
        corpus().into_iter().map(|(_, json)| serde_json::from_str(&json).unwrap()).collect();
    // `SCAENA_FUZZ=<seed>:<count>` runs a longer search locally.
    let (seed, count) = std::env::var("SCAENA_FUZZ")
        .ok()
        .and_then(|s| s.split_once(':').map(|(a, b)| (a.parse().unwrap(), b.parse().unwrap())))
        .unwrap_or((0x5ca3_a5ca_3a5c_a3a5, 4000));
    let mut r = Rng(seed);
    let mut tried = 0;
    for _ in 0..count {
        let mut doc = r.pick(&bases).clone();
        for _ in 0..1 + r.below(4) {
            mutate(&mut r, &mut doc);
        }
        let Ok(deck) = serde_json::from_value::<Deck>(doc) else { continue };
        tried += 1;
        let source = decompile(&deck);
        let back = compile(&source).unwrap_or_else(|e| panic!("{e}\n{source}"));
        assert_eq!(back.to_json().unwrap(), deck.to_json().unwrap(), "\n{source}");
        assert_eq!(decompile(&back), source, "decompiling is a fixed point");
    }
    assert!(tried > count / 2, "only {tried} mutants were decks");
}

/// The error `source` compiles to, as (line, column, message, pointer).
fn error(source: &str) -> (usize, usize, String, Option<String>) {
    let e = compile(source).unwrap_err();
    (e.line, e.col, e.message, e.pointer)
}

#[test]
fn errors_say_where_and_what() {
    let head = "deck \"T\" canvas:1920x1080\n";
    let (line, col, message, _) = error(&format!("{head}nod title text\n"));
    assert_eq!((line, col), (2, 1));
    assert!(message.contains("does not start a declaration"), "{message}");

    assert!(error("node t text \"x\"\n").2.contains("needs a canvas"));

    let (line, col, message, pointer) = error(&format!("{head}node t text role:body role:title\n"));
    assert_eq!((line, col), (2, 23));
    assert_eq!(message, "`role` is written twice");
    assert_eq!(pointer.as_deref(), Some("/nodes/t/role"));

    let (line, _, message, pointer) = error(&format!("{head}node t text\nstate a\n  t chart:bar\n"));
    assert_eq!(line, 4);
    assert!(message.contains("a node's type never changes (E104)"), "{message}");
    assert_eq!(pointer.as_deref(), Some("/states/0/props/t"));

    assert!(error(&format!("{head}node c chart:bar \"label\"\n")).2.contains("this is a chart node"));
    assert!(error(&format!("{head}node t\n")).2.contains("needs a type"));
    assert!(error(&format!("{head}node t text at:col()\n")).2.contains("needs a value"));
    assert!(error(&format!("{head}state a\n\tt\n")).2.contains("tabs"));
    assert!(error(&format!("{head}node t text size:12px\n")).2.contains("unit"));
    assert!(error(&format!("{head}node t text x:1-7\n")).2.contains("`at:` calls only"));
    assert!(error("  node t text\n").2.contains("indented"));
    assert!(error(&format!("{head}deck \"again\"\n")).2.contains("one `deck` header"));
}

#[test]
fn shorthand_compiles_to_what_it_stands_for() {
    let deck = compile(
        "# Opening deck\ndeck \"Q3\" canvas:16x9\nstate intro layout:title transition:1.5s\n  # the title\n  title text \"Q3 Review\" role:display at:col(1-7) row(1) in(title) rect(1, 2, 3, 4)\n  bg shader:mesh seed:7\n  choreo [title, bg] enter:rise stagger:40ms\n  sequence delay:100\n    choreo title enter:fade\n  -gone\n  notes \"\"\"\n    First line.\n      Indented.\n    \"\"\"\n",
    )
    .unwrap();
    let json: serde_json::Value = serde_json::to_value(&deck).unwrap();
    assert_eq!(json["_comment"], "Opening deck");
    assert_eq!(json["canvas"]["width"], 16.0);
    assert_eq!(
        json["nodes"]["title"],
        serde_json::json!({
            "type": "text", "_comment": "the title", "text": "Q3 Review", "role": "display",
            "at": { "col": [1, 7], "row": 1, "in": "title", "rect": [1, 2, 3, 4] }
        }),
        "the line that first gives a node's type declares it"
    );
    assert_eq!(json["nodes"]["bg"], serde_json::json!({ "type": "shader", "kind": "mesh", "seed": 7 }));
    let state = &json["states"][0];
    assert_eq!(state["transition"], 1500);
    assert_eq!(state["props"]["title"], serde_json::json!({}), "and the state shows it as it is");
    assert_eq!(state["props"]["bg"], serde_json::json!({}));
    assert_eq!(
        state["choreography"],
        serde_json::json!([
            { "target": ["title", "bg"], "enter": "rise", "stagger": 40 },
            { "sequence": [{ "target": "title", "enter": "fade" }], "delay": 100 }
        ])
    );
    assert_eq!(state["remove"], serde_json::json!(["gone"]));
    assert_eq!(state["notes"], "First line.\n  Indented.");
}

#[test]
fn the_source_map_finds_where_each_part_came_from() {
    let src = "deck \"T\" canvas:1920x1080 author:jay\nfont \"Inter\" \"fonts/Inter.ttf\"\ndata q3 \"data/q3.csv\"\n\
               data rows inline:[{a: 1}]\nnode title text \"Hi\" role:display\nstate intro\n  title at:col(1-7) row(1)\n\
               \x20 bg shader:mesh seed:7\n  -gone\n  choreo [title, bg] enter:rise\n  notes \"Say it.\"\n";
    let (_, map) = scaena_core::dsl::compile_json(src).unwrap();
    let at = |p: &str| map.locate(p).map(|(o, l)| &src[o..o + l]);
    assert_eq!(at("/canvas"), Some("canvas:1920x1080"));
    assert_eq!(at("/meta/author"), Some("author:jay"), "a header key the deck does not have is meta");
    assert_eq!(at("/meta/title"), Some("\"T\""));
    assert_eq!(at("/fonts/0"), Some("font"));
    assert_eq!(at("/fonts/0/file"), Some("\"fonts/Inter.ttf\""));
    assert_eq!(at("/data/q3/source"), Some("\"data/q3.csv\""));
    assert_eq!(at("/data/rows/source/inline/0/a"), Some("a: 1"));
    assert_eq!(at("/nodes/title/role"), Some("role:display"));
    assert_eq!(at("/nodes/title/text"), Some("\"Hi\""));
    assert_eq!(at("/nodes/title/size"), Some("title"), "a key that is not there: its object");
    assert_eq!(at("/states/0"), Some("intro"));
    assert_eq!(at("/states/0/props/title/at"), Some("at:col(1-7) row(1)"));
    assert_eq!(at("/states/0/props/title/at/col"), Some("col(1-7)"));
    assert_eq!(at("/nodes/bg"), Some("bg"), "a first mention declares the node");
    assert_eq!(at("/nodes/bg/kind"), Some("mesh"));
    assert_eq!(at("/nodes/bg/seed"), Some("seed:7"));
    assert_eq!(at("/states/0/props/bg"), Some("bg"));
    assert_eq!(at("/states/0/remove/0"), Some("-gone"));
    assert_eq!(at("/states/0/choreography/0"), Some("choreo"));
    assert_eq!(at("/states/0/choreography/0/target/1"), Some("bg"));
    assert_eq!(at("/states/0/choreography/0/enter"), Some("enter:rise"));
    assert_eq!(at("/states/0/notes"), Some("notes \"Say it.\""));
    assert_eq!(at("/nodes"), Some("deck"), "the deck itself: its header");
}

#[test]
fn a_source_that_is_not_a_deck_says_why_in_the_decks_terms() {
    let e = compile("deck \"T\" canvas:big\n").unwrap_err();
    assert_eq!((e.line, e.col, e.pointer.as_deref()), (1, 10, Some("/canvas")), "{e}");
}
