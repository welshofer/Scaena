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
        "docs/examples/higher-ed.deck.json".into(),
        "docs/examples/code.deck.json".into(),
        "docs/examples/weather.deck.json".into(),
        "docs/examples/haiku.deck.json".into(),
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

/// Each example's `.scn` is its deck's canonical source: what the decompiler writes, and what
/// compiles to the deck. `revenue.deck.scn` is the one SPEC §4.1 shows whole, and
/// `code.deck.scn` writes its code as blocks (PLAN 2.143). `SCAENA_BLESS=1` rewrites them.
#[test]
fn the_example_source_is_its_decks_canonical_source() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/examples");
    let bless = std::env::var_os("SCAENA_BLESS").is_some();
    let mut sources = Vec::new();
    for name in ["revenue", "code"] {
        let deck = Deck::from_json(&std::fs::read_to_string(format!("{root}/{name}.deck.json")).unwrap()).unwrap();
        let path = format!("{root}/{name}.deck.scn");
        let source = decompile(&deck);
        if bless {
            std::fs::write(&path, &source).unwrap();
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source, "{name}: SCAENA_BLESS=1 rewrites it");
        assert_eq!(compile(&source).unwrap().to_json().unwrap(), deck.to_json().unwrap(), "{name}");
        sources.push(source);
    }
    assert!(sources[1].contains(" code:{language: rust} \"\"\"\n    // A talk on the program"), "{}", sources[1]);
    // SPEC §4.1 shows the first whole.
    let spec_path = format!("{root}/../SPEC.md");
    let shown = |spec: &str| -> (String, String, String) {
        let (before, rest) = spec.split_once("```scn\n").unwrap();
        let (shown, after) = rest.split_once("```").unwrap();
        (before.into(), shown.into(), after.into())
    };
    if bless {
        let (before, _, after) = shown(&std::fs::read_to_string(&spec_path).unwrap());
        std::fs::write(&spec_path, format!("{before}```scn\n{}```{after}", sources[0])).unwrap();
    }
    let (_, in_spec, _) = shown(&std::fs::read_to_string(&spec_path).unwrap());
    assert_eq!(in_spec, sources[0], "SPEC §4.1 shows the example as it is; SCAENA_BLESS=1 rewrites it");
}

/// A deck of one code text in `language`: shown as it is in state `one`, and as `then` in
/// state `two`, beside a text of two lines that is no code.
fn code_deck(language: &str, text: &str, then: &str) -> Deck {
    let doc = serde_json::json!({
        "scaena": scaena_core::FORMAT_VERSION,
        "canvas": { "width": 1920, "height": 1080 },
        "nodes": {
            "snippet": { "type": "text", "role": "code", "code": { "language": language }, "text": text },
            "note": { "type": "text", "text": "Two lines\nof prose" }
        },
        "states": [
            { "id": "one", "props": { "snippet": {}, "note": {} } },
            { "id": "two", "props": { "snippet": { "text": then } } }
        ]
    });
    Deck::from_json(&doc.to_string()).unwrap()
}

/// `deck`'s source, which compiles back to it and decompiles again to itself.
fn round_trip(deck: &Deck) -> String {
    let source = decompile(deck);
    let back = compile(&source).unwrap_or_else(|e| panic!("{e}\n{source}"));
    assert_eq!(back.to_json().unwrap(), deck.to_json().unwrap(), "\n{source}");
    assert_eq!(decompile(&back), source, "decompiling is a fixed point");
    source
}

/// A code text's lines print as a block, in a node's declaration and in a state's change,
/// fenced by three quotes where no line is three quotes alone: Python's docstrings among them
/// (PLAN 2.143). Prose and a line of code stay JSON strings.
#[test]
fn a_code_text_prints_as_a_block_and_reads_back() {
    let docstring = "def area(r):\n    \"\"\"The area of a circle of radius r.\"\"\"\n\n    return math.pi * r**2";
    let one_line = "area = lambda r: math.pi * r**2";
    let source = round_trip(&code_deck("python", docstring, one_line));
    let q = "\"\"\"";
    assert_eq!(
        source,
        format!(
            "deck canvas:1920x1080\n\nstate one\n  snippet text role:code code:{{language: python}} {q}\n    def area(r):\n        \
             {q}The area of a circle of radius r.{q}\n\n        return math.pi * r**2\n    {q}\n  \
             note text \"Two lines\\nof prose\"\n\nstate two\n  snippet \"{one_line}\"\n"
        )
    );
    // The block stands wherever the text does among its props, which go on after it.
    let mut doc = serde_json::to_value(code_deck("rust", "fn main() {\n    run();\n}", "")).unwrap();
    doc["nodes"]["snippet"]["semantic"] = "evidence".into();
    doc["states"][1]["props"]["snippet"] =
        serde_json::json!({ "text": "fn main() {}\n", "alt": "Rust: an empty main." });
    let source = round_trip(&Deck::from_json(&doc.to_string()).unwrap());
    let snippet = "  snippet text role:code code:{language: rust} \"\"\"\n    fn main() {\n        run();\n    }\n    \"\"\"\n    semantic:evidence\n";
    assert!(source.contains(snippet), "{source}");
    let change = "  snippet \"\"\"\n    fn main() {}\n\n    \"\"\"\n    alt:\"Rust: an empty main.\"\n";
    assert!(source.contains(change), "{source}");
    // A fence past the width starts the next line, and the block's lines stand two deeper.
    let alt = format!("{} code", "x".repeat(77));
    doc["states"][1]["props"]["snippet"] = serde_json::json!({ "alt": alt, "text": "a\nb" });
    let source = round_trip(&Deck::from_json(&doc.to_string()).unwrap());
    let change = format!("  snippet alt:\"{alt}\"\n    \"\"\"\n      a\n      b\n      \"\"\"\n");
    assert!(source.ends_with(&change), "{source}");
}

/// The fence is the shortest run of quotes, three or more, that no line of the code is after
/// its indentation: a line of three quotes takes four, and lines of three and four take five.
#[test]
fn a_block_holding_its_fence_takes_a_longer_one() {
    let q = |n: usize| "\"".repeat(n);
    let docstring =
        format!("def area(r):\n    {}\n    The area of a circle.\n    {}\n    return math.pi * r**2", q(3), q(3));
    let quotes = format!("{}\n{}\nx = {}", q(3), q(4), q(5));
    let source = round_trip(&code_deck("python", &docstring, &quotes));
    let first = format!("code:{{language: python}} {}\n    def area(r):\n        {}\n", q(4), q(3));
    assert!(source.contains(&first), "{source}");
    assert!(source.contains(&format!("    return math.pi * r**2\n    {}\n", q(4))), "{source}");
    let then = format!("  snippet {}\n    {}\n    {}\n    x = {}\n    {}\n", q(5), q(3), q(4), q(5), q(5));
    assert!(source.contains(&then), "{source}");
    // A line of four alone leaves three free.
    let source = round_trip(&code_deck("plain", &format!("{}\nfour", q(4)), ""));
    assert!(source.contains(&format!(" {}\n    {}\n    four\n    {}\n", q(3), q(4), q(3))), "{source}");
}

/// SPEC §4.2's block is canonical source: a docstring's quotes alone on lines fence the code
/// with four.
#[test]
fn the_specs_block_is_canonical() {
    let spec = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/SPEC.md")).unwrap();
    let (_, rest) = spec.split_once("\n  ```scn\n").expect("SPEC §4.2 shows a block");
    let (shown, _) = rest.split_once("\n  ```\n").unwrap();
    let state: String = shown.lines().map(|l| format!("{}\n", l.strip_prefix("  ").unwrap_or(l))).collect();
    let source = format!("deck canvas:1920x1080\n\n{state}");
    let deck = compile(&source).unwrap_or_else(|e| panic!("{e}\n{source}"));
    assert_eq!(decompile(&deck), source);
    let text = &deck.nodes["sort"].props["text"];
    assert_eq!(text.as_str().unwrap().lines().nth(1), Some("    \"\"\""), "{text}");
}

/// Notes keep their fence on the `notes` line, which holds their string, however far past the
/// width: lines of every run from 3 to 99 quotes take a fence of 100, in a state's notes and in
/// a beat's.
#[test]
fn notes_keep_their_fence_on_their_line_past_the_width() {
    let q = |n: usize| "\"".repeat(n);
    let notes = (3..100).map(q).collect::<Vec<_>>().join("\n");
    let doc = serde_json::json!({
        "scaena": scaena_core::FORMAT_VERSION,
        "canvas": { "width": 1920, "height": 1080 },
        "spine": { "sections": [{ "id": "open", "beats": [{ "id": "b", "claim": "C", "notes": notes }] }] },
        "nodes": {},
        "states": [{ "id": "a", "notes": notes }]
    });
    let source = round_trip(&Deck::from_json(&doc.to_string()).unwrap());
    assert!(source.contains(&format!("\n    notes {}\n      {}\n", q(100), q(3))), "{source}");
    assert!(source.contains(&format!("\n  notes {}\n    {}\n", q(100), q(3))), "{source}");
}

/// Code a block would not keep as it is stays a JSON string: a tab, a carriage return, or a
/// line that ends in white space, which editors change.
#[test]
fn code_with_a_tab_a_return_or_trailing_space_is_a_json_string() {
    for kept in ["fn f() {\n\treturn;\n}", "a\r\nb", "x = 1 \ny = 2", "x\n  \ny", "a\u{a0}\nb"] {
        let source = round_trip(&code_deck("rust", kept, kept));
        assert!(!source.contains("\"\"\""), "{kept:?}:\n{source}");
        assert!(source.contains(&serde_json::to_string(kept).unwrap()), "{kept:?}:\n{source}");
    }
}

/// Source cut anywhere, or with a quote typed anywhere, compiles or says why: a block left open,
/// fenced by any run, never stops the compiler.
#[test]
fn a_source_cut_or_quoted_anywhere_compiles_or_says_why() {
    let q = |n: usize| "\"".repeat(n);
    let deck = code_deck("python", &format!("def f():\n    {}\n    Docs.\n    {}\n", q(3), q(3)), &q(4));
    let source = decompile(&deck);
    for (at, _) in source.char_indices() {
        for typed in [source[..at].to_string(), format!("{}\"{}", &source[..at], &source[at..])] {
            if let Err(e) = scaena_core::dsl::compile_json(&typed) {
                let end = e.offset + e.len;
                assert!(typed.is_char_boundary(e.offset) && typed.is_char_boundary(end), "{e:?}\n{typed}");
            }
        }
    }
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
    // Code (PLAN 2.143): docstrings on a line and alone on lines, lines of quotes alone,
    // indented, and at a line's end, tabs, trailing spaces, CRLFs, a lone CR, and blank lines.
    "def f():\n    \"\"\"Docs.\"\"\"\n    pass",
    "def f():\n    \"\"\"\n    Docs.\n    \"\"\"\n    pass",
    "\"\"\"\n\"\"\"\"",
    "x\n\"\"\"\"\"\n\"\"\"\"",
    "  \"\"\"\n    \"\"\"\"\n",
    "print(\"\"\"\nhi\"\"\")",
    "\"\"\" # no comment\nx",
    "\u{a0}\"\"\"\nx",
    "fn f() {\n\treturn;\n}",
    "x = 1  \ny = 2",
    "a = 1\r\nb = 2\r\n",
    "a\rb\nc",
    "\n\nx\n\n",
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
    "code",
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
    match r.below(22) {
        21 => {
            // A code text, its lines in its node and in a state's change, under an id a line of
            // a state may start with.
            let lines: Vec<&str> = STRINGS.iter().copied().filter(|s| s.contains('\n')).collect();
            let id = r.pick(&["snippet", "notes", "text", "code"]).to_string();
            let code = serde_json::json!({ "language": *r.pick(&["python", "rust", "plain"]) });
            let node = serde_json::json!({ "type": "text", "role": "code", "code": code, "text": *r.pick(&lines) });
            doc["nodes"].as_object_mut().unwrap().insert(id.clone(), node);
            let states = doc["states"].as_array_mut().unwrap();
            let i = r.below(states.len());
            let change = match r.below(3) {
                0 => serde_json::json!({}),
                1 => serde_json::json!({ "text": *r.pick(&lines) }),
                _ => serde_json::json!({ "alt": "Code.", "text": *r.pick(STRINGS), "role": "code" }),
            };
            let props =
                states[i].as_object_mut().unwrap().entry("props").or_insert_with(|| Value::Object(Default::default()));
            insert_at(r, props.as_object_mut().unwrap(), id, change);
        }
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
    // Sources that wrote a block, and a block fenced by more than three quotes.
    let (mut blocks, mut longer) = (0, 0);
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
        blocks += usize::from(source.contains("\"\"\"\n"));
        longer += usize::from(source.contains("\"\"\"\"\n"));
    }
    assert!(tried > count / 2, "only {tried} mutants were decks");
    assert!(blocks > count / 10 && longer > count / 50, "{blocks} sources wrote a block, {longer} a longer fence");
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
    // A line short of the closing quotes' indentation, a letter of any width in its way.
    for c in ['x', 'é', '🇺'] {
        let notes = format!("{head}state a\n  notes \"\"\"\n    one\n   {c}two\n    \"\"\"\n");
        assert!(error(&notes).2.contains("indented less than the closing"), "{c}");
    }
    // A block that a line of its own run never closes, where it opened: a line of three quotes
    // is a line of a block fenced by four.
    let (line, col, message, _) = error(&format!("{head}state a\n  notes \"\"\"\"\n    one\n    \"\"\"\n"));
    assert_eq!((line, col), (3, 9));
    assert_eq!(message, "this `\"\"\"\"` string is not closed: a line of `\"\"\"\"` alone closes it");
    assert!(error(&format!("{head}deck \"again\"\n")).2.contains("one `deck` header"));
}

/// A canvas of any size the deck holds decompiles to a size that compiles back to it, and a
/// size no number holds, typed into a source, is an error where it is typed.
#[test]
fn canvas_sizes_round_trip_at_the_ends_of_f64() {
    let base = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/examples/revenue.deck.json"));
    let mut doc: serde_json::Value = serde_json::from_str(&base.unwrap()).unwrap();
    for (w, h) in [(f64::MAX, 1e-9), (1e20, 0.5), (1920.5, 1e15)] {
        doc["canvas"] = serde_json::json!({ "width": w, "height": h });
        let deck = Deck::from_json(&doc.to_string()).unwrap();
        let source = decompile(&deck);
        let back = compile(&source).unwrap_or_else(|e| panic!("{w}x{h}: {e}"));
        assert_eq!((back.canvas.width, back.canvas.height), (w, h));
    }
    for typed in ["canvas:1920x1e999", "canvas:1e999x1080"] {
        let e = compile(&format!("deck \"T\" {typed}\n")).unwrap_err();
        assert!(e.message.contains("is not a number"), "{typed}: {e}");
    }
    let e = compile("deck \"T\" canvas:16x9\nnode t text at:col(1-1e999)\n").unwrap_err();
    assert!(e.message.contains("is not a number"), "{e}");
}

/// An error about a character spans all of its bytes, so an editor can slice the source at
/// either end: a letter typed where none goes, in any script.
#[test]
fn an_error_about_a_character_spans_all_of_it() {
    let head = "deck \"T\" canvas:1920x1080\n";
    for c in ['é', '漢', '🇺', '\u{301}'] {
        let source = format!("{head}state a transition:{c}standard\n");
        let e = compile(&source).unwrap_err();
        assert_eq!(&source[e.offset..e.offset + e.len], c.to_string(), "{e:?}");
        assert_eq!((e.line, e.col), (2, 20), "{e:?}");
    }
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

/// A long deck's source compiles in time that grows with it, not with its square: each `at:` call
/// moves only the marks its value just made under the call's place, never every mark the source
/// made before it, which cost a 38-slide deck most of each compile (the editor compiles one
/// after every gesture).
#[test]
fn a_long_source_compiles_in_time_that_grows_with_it() {
    let n = 3000;
    let mut src = String::from("deck \"Long\" canvas:1920x1080 author:jay\n");
    for i in 0..n {
        src += &format!("node n{i} text \"Words {i}\" role:body\n");
    }
    src += "state all\n";
    for i in 0..n {
        src += &format!("  n{i} at:col({}-{}) row({}) in(body)\n", 1 + i % 6, 7 + i % 6, 1 + i % 12);
    }
    let started = std::time::Instant::now();
    let (json, map) = scaena_core::dsl::compile_json(&src).unwrap();
    let took = started.elapsed();
    let at = |p: &str| map.locate(p).map(|(o, l)| &src[o..o + l]);
    let last = format!("/states/0/props/n{}/at", n - 1);
    assert_eq!(at(&format!("{last}/col")), Some("col(6-12)"));
    assert_eq!(at(&format!("{last}/row")), Some("row(12)"));
    assert_eq!(at(&format!("{last}/in")), Some("in(body)"));
    assert_eq!(json["states"][0]["props"]["n0"]["at"], serde_json::json!({ "col": [1, 7], "row": 1, "in": "body" }));
    assert!(took < std::time::Duration::from_secs(3), "{n} nodes compiled in {took:?}");
}
