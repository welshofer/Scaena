//! Any edit leaves a deck the engine draws, or one it refuses with an error, never one that
//! panics it. In the browser the engine runs in the editor's worker, which compiles the
//! source on every keystroke (PLAN 2.3); a panic there stops the page mid-word. Five kinds
//! of edit, each run through the session as the worker runs it:
//!
//! - a source typed: cut short, a run deleted, a character of any width typed anywhere, a
//!   line copied, two swapped, one indented or not;
//! - a deck changed value by value: a number at the ends of f64, a text empty or long, a
//!   value from elsewhere in the repository's decks and themes, an array emptied, a key
//!   dropped;
//! - a patch an assistant might send through `deck_patch` (PLAN 2.6);
//! - a theme changed value by value, as a bundle's `theme.json` edited by hand, which the
//!   player opens as it is (PLAN 2.24);
//! - calls to the assistant's tools, each argument there, left out, of the wrong kind, or
//!   past what the tool can do: a state the deck lacks, a raster no painter could hold, a
//!   spine changed, a file that holds no data (PLAN 2.26), a theme edited at any pointer
//!   (ADR-0016).
//!
//! A deck that compiles is drawn at rest and through its cue, read, painted, and linted and
//! inspected in a state. One that does not is handed to the player as a bundle's `deck.json`
//! would be, unvalidated, and drawn as far as it goes. `SCAENA_FUZZ=<seed>:<count>` runs a
//! longer search, over every deck in the repository, locally.

use scaena_core::Deck;
use scaena_session::Session;
use scaena_session::assistant::Caller;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Mutex;

/// A deterministic xorshift generator: the edits are the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// A bundle as the page holds it: its deck, its theme, and its other files by path.
struct Bundle {
    name: &'static str,
    deck: Value,
    theme: String,
    files: Vec<(String, Vec<u8>)>,
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        if entry.file_type().unwrap().is_dir() {
            walk(&entry.path(), &path, out);
        } else {
            out.push((path, std::fs::read(entry.path()).unwrap()));
        }
    }
}

/// The decks: the example, the chart gallery, code (PLAN 2.131), a price series fetched and
/// credited (PLAN O.3a), and B1, or with `all`, every
/// deck in the repository a bundle holds.
fn bundles(all: bool) -> Vec<Bundle> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let examples = root.join("docs/examples");
    let mut shared = Vec::new();
    for dir in ["fonts", "assets", "data", "themes"] {
        walk(&examples.join(dir), dir, &mut shared);
    }
    let mut out = Vec::new();
    let mut add = |name: &'static str, deck: &Path, files: Vec<(String, Vec<u8>)>, dir: &Path| {
        let deck: Value = serde_json::from_slice(&std::fs::read(deck).unwrap()).unwrap();
        let theme = std::fs::read_to_string(dir.join(deck["theme"].as_str().unwrap())).unwrap();
        out.push(Bundle { name, deck, theme, files });
    };
    let examples_named: &[&'static str] = if all {
        &["revenue", "charts", "code", "haiku", "stock", "trails", "ridgeline", "higher-ed"]
    } else {
        &["revenue", "charts", "code", "haiku", "stock"]
    };
    for &name in examples_named {
        add(name, &examples.join(format!("{name}.deck.json")), shared.clone(), &examples);
    }
    // A deck with a map (ADR-0030): the revenue example with the torture deck's map card on its
    // first slide, fetched, pinned, and credited.
    let mut files = shared.clone();
    let card = std::fs::read(root.join("tests/fixtures/torture.scaena/assets/map-card.png")).unwrap();
    files.push(("assets/map-card.png".into(), card));
    add("map", &examples.join("revenue.deck.json"), files, &examples);

    let bundles_named: &[&'static str] = if all {
        &["tests/bench/b1.scaena", "tests/bench/b2.scaena", "tests/bench/b3.scaena", "tests/fixtures/torture.scaena"]
    } else {
        &["tests/bench/b1.scaena"]
    };
    for &dir in bundles_named {
        let dir_path = root.join(dir);
        let mut files = Vec::new();
        walk(&dir_path, "", &mut files);
        files.retain(|(path, _)| path != "deck.json" && path != "README.md");
        add(dir.rsplit('/').next().unwrap(), &dir_path.join("deck.json"), files, &dir_path);
    }
    let map = out.iter_mut().find(|b| b.name == "map").unwrap();
    map.deck["nodes"]["map"] = json!({
        "type": "image", "src": "assets/map-card.png", "at": { "in": "canvas" },
        "fetched": {
            "from": "geoapify", "at": "2026-10-15T20:12:44Z", "asked": "Ferry Building",
            "credit": [{ "text": "© OpenStreetMap contributors", "link": "https://www.openstreetmap.org/copyright" },
                       { "text": " · Powered by Geoapify", "link": "https://www.geoapify.com/" }],
            "map": { "place": { "name": "Ferry Building", "lat": 37.7955, "lon": -122.3937 },
                     "center": { "lat": 37.7955, "lon": -122.3937 }, "zoom": 15, "style": "positron",
                     "size": [800, 450], "scale": 2 }
        },
        "pin": { "at": [0.5, 0.5], "label": "Ferry Building" }
    });
    map.deck["states"][0]["props"]["map"] = json!({});
    out
}

fn session(b: &Bundle) -> Session {
    let mut s = Session::new(&b.deck.to_string(), &b.theme).unwrap();
    for (path, bytes) in &b.files {
        s.add_file(path, bytes.clone());
    }
    s
}

/// Every value in `v` by the key it sits under: what a changed value is drawn from.
fn values_by_key(v: &Value, into: &mut BTreeMap<String, Vec<Value>>) {
    match v {
        Value::Object(map) => {
            for (k, x) in map {
                let seen = into.entry(k.clone()).or_default();
                if seen.len() < 40 && !seen.contains(x) {
                    seen.push(x.clone());
                }
                values_by_key(x, into);
            }
        }
        Value::Array(items) => items.iter().for_each(|x| values_by_key(x, into)),
        _ => {}
    }
}

/// Every JSON pointer in `v`, and every string it holds.
fn walk_json(v: &Value, at: String, pointers: &mut Vec<String>, strings: &mut Vec<String>) {
    pointers.push(at.clone());
    match v {
        Value::Object(map) => {
            for (k, x) in map {
                strings.push(k.clone());
                walk_json(x, format!("{at}/{}", k.replace('~', "~0").replace('/', "~1")), pointers, strings);
            }
        }
        Value::Array(items) => {
            for (i, x) in items.iter().enumerate() {
                walk_json(x, format!("{at}/{i}"), pointers, strings);
            }
        }
        Value::String(s) => strings.push(s.clone()),
        _ => {}
    }
}

/// What a keystroke or a paste puts in a source, besides the language's own words.
const TYPED: &[&str] = &[
    "\"", "\"\"\"", "{", "}", "[", "]", "(", ")", ":", ",", "@", "#", "\n", "  ", "-", ".", "0", "9", "ms", "%", "x",
    "state ", "node ", "text ", "at:", "choreo ", "é", "漢", "🇺🇸", "\u{301}", "\r\n", "\\", "1e999", "-1",
];

/// Texts at the edges of what text holds.
const TEXTS: &[&str] = &[
    "",
    " ",
    "\n",
    "\n\n",
    "Ünïcödé ✓",
    "שלום עולם",
    "مرحبا",
    "👩‍👩‍👧‍👦",
    "e\u{301}\u{301}",
    "\u{200B}",
    "\u{0}",
    "\u{FEFF}",
    "@color.accent",
    "#00000000",
    "1e9ms",
    "0:0",
    "16:0",
    "1:100000",
    "-50%",
    "auto",
    "0fr",
    "2026-02-30",
];

fn number(r: &mut Rng, was: f64) -> Value {
    match r.below(3) {
        0 => json!(*r.pick(&[0i64, -1, 1, 13, 65_536, i64::MAX, i64::MIN])),
        1 => json!(*r.pick(&[0.0f64, -0.0, 1e-9, 1e300, -1e300, f64::MAX, f64::MIN_POSITIVE, 0.5])),
        _ => json!(was * *r.pick(&[0.0f64, -1.0, 1e3, 1e-3, 1e9])),
    }
}

fn text(r: &mut Rng, strings: &[String]) -> Value {
    Value::String(match r.below(5) {
        0 if !strings.is_empty() => r.pick(strings).clone(),
        1 => "lorem ipsum ".repeat(1 + r.below(300)),
        2 => String::new(),
        _ => r.pick(TEXTS).to_string(),
    })
}

/// `source` edited once, said in a line.
fn type_into(r: &mut Rng, source: &mut String) -> String {
    let at = |r: &mut Rng, s: &str| s.floor_char_boundary(r.below(s.len() + 1));
    let mut lines: Vec<String> = source.lines().map(str::to_string).collect();
    let said = match r.below(7) {
        0 => {
            let i = at(r, source);
            source.truncate(i);
            return format!("cut at {i}");
        }
        1 | 2 => {
            let i = at(r, source);
            let j = source.ceil_char_boundary(i + 1 + r.below(24));
            let gone: String = source.drain(i..j).collect();
            return format!("deleted {gone:?} at {i}");
        }
        3 | 4 => {
            let (i, t) = (at(r, source), r.pick(TYPED).to_string());
            source.insert_str(i, &t);
            return format!("typed {t:?} at {i}");
        }
        5 => {
            let (a, b) = (r.below(lines.len()), r.below(lines.len()));
            lines.swap(a, b);
            format!("lines {a} and {b} swapped")
        }
        _ => {
            let k = r.below(lines.len());
            lines[k].insert_str(0, "  ");
            format!("line {k} indented")
        }
    };
    *source = lines.join("\n") + "\n";
    said
}

/// The states a change at `pointer` touches: the state it is in, and the one after, which
/// tracks from it.
fn touched_by(doc: &Value, pointer: &str) -> Vec<String> {
    let Some(i) = pointer.strip_prefix("/states/").and_then(|p| p.split('/').next()?.parse::<usize>().ok()) else {
        return Vec::new();
    };
    (i..i + 2).filter_map(|k| doc["states"].get(k)?["id"].as_str().map(String::from)).collect()
}

/// `doc` changed once, said in a line.
fn change(r: &mut Rng, doc: &mut Value, values: &BTreeMap<String, Vec<Value>>) -> String {
    let (mut pointers, mut strings) = (Vec::new(), Vec::new());
    walk_json(doc, String::new(), &mut pointers, &mut strings);
    pointers.retain(|p| !p.is_empty() && !p.starts_with("/fonts") && p != "/theme");
    // Half the time a state's delta, where most of what an author changes is.
    let deltas: Vec<&String> = pointers.iter().filter(|p| p.starts_with("/states/") && p.contains("/props/")).collect();
    let at =
        if r.below(2) == 0 && !deltas.is_empty() { r.pick(&deltas).to_string() } else { r.pick(&pointers).clone() };
    let key = at.rsplit('/').next().unwrap().replace("~1", "/").replace("~0", "~");
    let target = doc.pointer_mut(&at).unwrap();
    let new = match (r.below(4), &*target) {
        (0, _) if values.contains_key(&key) => r.pick(&values[&key]).clone(),
        (_, Value::Number(n)) => number(r, n.as_f64().unwrap_or(1.0)),
        (_, Value::String(_)) => text(r, &strings),
        (_, Value::Bool(b)) => Value::Bool(!b),
        (_, Value::Array(items)) if r.below(2) == 0 && !items.is_empty() => {
            let mut items = items.clone();
            let i = r.below(items.len());
            items.insert(i, items[i].clone());
            Value::Array(items)
        }
        (_, Value::Array(_)) => json!([]),
        (_, Value::Object(map)) if !map.is_empty() => {
            let mut map = map.clone();
            let k = map.keys().nth(r.below(map.len())).unwrap().clone();
            map.shift_remove(&k);
            Value::Object(map)
        }
        _ => number(r, 1.0),
    };
    let said = format!("{at} = {}", short(&new));
    *target = new;
    said
}

/// A patch an assistant might send: one to three ops over the deck's own ids.
fn patch(r: &mut Rng, doc: &Value, values: &BTreeMap<String, Vec<Value>>) -> Value {
    let nodes: Vec<String> = doc["nodes"].as_object().unwrap().keys().cloned().collect();
    let states: Vec<String> =
        doc["states"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap().into()).collect();
    let keys: Vec<&String> = values.keys().collect();
    let (mut pointers, mut strings) = (Vec::new(), Vec::new());
    walk_json(doc, String::new(), &mut pointers, &mut strings);
    let mut ops = Vec::new();
    for _ in 0..1 + r.below(3) {
        // A node the state shows, most of the time: one its delta names.
        let k = r.below(states.len());
        let shown: Vec<&String> = doc["states"][k]["props"].as_object().map(|p| p.keys().collect()).unwrap_or_default();
        let node =
            if r.below(4) > 0 && !shown.is_empty() { r.pick(&shown).to_string() } else { r.pick(&nodes).clone() };
        let state = states[k].clone();
        let key = r.pick(&keys).to_string();
        let value = r.pick(&values[&key]).clone();
        ops.push(match r.below(13) {
            0 => json!({ "op": "add_node", "id": "added", "node": { "type": "text", "text": text(r, &strings) }, "state": state }),
            1 => json!({ "op": "remove_node", "id": node }),
            2 => json!({ "op": "rename_node", "id": node, "to": r.pick(&nodes).clone() }),
            3 => json!({ "op": "show_node", "node": node, "state": state }),
            4 => json!({ "op": "hide_node", "node": node, "state": state }),
            5 => json!({ "op": "set_prop", "node": node, "prop": key, "value": value, "state": state }),
            6 => json!({ "op": "set_text", "node": node, "text": text(r, &strings), "state": state }),
            7 => json!({ "op": "move_state", "id": state, "before": r.pick(&states).clone() }),
            8 => json!({ "op": "remove_state", "id": state }),
            // Typing and a look for characters, over offsets in the text and past it.
            9 => json!({ "op": "replace_text", "node": node, "state": state, "from": r.below(8), "to": r.below(24), "text": text(r, &strings) }),
            10 => {
                let look = *r.pick(&LOOKS);
                json!({ "op": "style_text", "node": node, "state": state, "from": r.below(8), "to": r.below(24), "look": { look: value } })
            }
            // A text's paragraphs as a list (PLAN 2.69): a kind, none, or levels in and out.
            11 => match r.below(3) {
                0 => json!({ "op": "list", "node": node, "state": state, "from": r.below(8), "to": r.below(24), "kind": *r.pick(&["bullet", "number", "none"]) }),
                1 => json!({ "op": "list", "node": node, "state": state, "from": r.below(8), "to": r.below(24), "by": r.below(5) as i64 - 2 }),
                _ => json!({ "op": "list", "node": node, "state": state, "from": r.below(8), "to": r.below(24), "kind": "number", "level": r.below(10) }),
            },
            _ => json!({ "op": "replace", "path": r.pick(&pointers).clone(), "value": value }),
        });
    }
    Value::Array(ops)
}

/// RFC 6902 operations on a theme, as `theme_edit` takes them (ADR-0016): a value replaced by
/// another the repository's decks and themes hold under its key, or a member added, taken out,
/// moved, copied, or tested, at pointers the theme has and one it does not.
fn theme_ops(r: &mut Rng, theme: &Value, values: &BTreeMap<String, Vec<Value>>) -> Value {
    let (mut pointers, mut strings) = (Vec::new(), Vec::new());
    walk_json(theme, String::new(), &mut pointers, &mut strings);
    pointers.retain(|p| !p.is_empty());
    let keys: Vec<&String> = values.keys().collect();
    let mut ops = Vec::new();
    for _ in 0..1 + r.below(3) {
        let path = if r.below(8) == 0 { "/no/such".to_string() } else { r.pick(&pointers).clone() };
        // A value its key holds elsewhere, most of the time.
        let key = match path.rsplit('/').next().filter(|k| values.contains_key(*k) && r.below(4) > 0) {
            Some(key) => key.to_string(),
            None => r.pick(&keys).to_string(),
        };
        let value = r.pick(&values[&key]).clone();
        let to = r.pick(&pointers).clone();
        ops.push(match r.below(8) {
            0 => json!({ "op": "remove", "path": path }),
            1 => json!({ "op": "add", "path": path, "value": value }),
            2 => json!({ "op": "move", "from": path, "path": to }),
            3 => json!({ "op": "copy", "from": path, "path": to }),
            4 => json!({ "op": "test", "path": path, "value": value }),
            _ => json!({ "op": "replace", "path": path, "value": value }),
        });
    }
    Value::Array(ops)
}

/// What a look for characters names: a run's own keys, and keys it does not take.
const LOOKS: [&str; 10] = [
    "role",
    "emphasis",
    "lang",
    "style/weight",
    "style/italic",
    "style/color",
    "style/family",
    "style/size",
    "fit",
    "link",
];

/// Values no tool takes where they are put.
const JUNK: &[&str] = &["null", "[]", "{}", "-1", "1e308", "\"\"", "true", "18446744073709551615", "[[[[[[]]]]]]"];

/// Sizes to render at: the canvas's ratio from a pixel up, past what a raster holds, and not
/// sizes at all.
const SIZES: &[&str] = &[
    "1x1",
    "2x1",
    "16x9",
    "17x9",
    "0x0",
    "1920x0",
    "960x540",
    "1920x1081",
    "8193x4609",
    "16384x9216",
    "65535x36864",
    "4294967295x2415919104",
    "4294967296x1",
    "-1920x-1080",
    "1e3x1e3",
    "1920x1080x1",
    "１９２０x1080",
    "1080x1920",
];

/// A call the assistant might make: one of its tools, or a tool it does not have, with
/// each argument there, left out, of the wrong kind, or at an edge of what the tool does.
fn tool_call(r: &mut Rng, b: &Bundle, states: &[String], values: &BTreeMap<String, Vec<Value>>) -> (String, Value) {
    let junk = |r: &mut Rng| serde_json::from_str::<Value>(r.pick(JUNK)).unwrap();
    let state = |r: &mut Rng| match r.below(6) {
        0 => json!("no-such-state"),
        1 => junk(r),
        _ => json!(r.pick(states)),
    };
    let flag = |r: &mut Rng| if r.below(6) == 0 { junk(r) } else { json!(r.below(2) == 0) };
    let mut tools = scaena_session::assistant::TOOLS.to_vec();
    tools.push("deck_paint");
    let name = *r.pick(&tools);
    let mut args = Map::new();
    // Each argument is there four times in five.
    macro_rules! set {
        ($key:expr, $value:expr) => {{
            let value = $value;
            if r.below(5) > 0 {
                args.insert(String::from($key), value);
            }
        }};
    }
    match name {
        "deck_read" => set!("scn", flag(r)),
        "deck_patch" => {
            set!("ops", if r.below(4) == 0 { junk(r) } else { patch(r, &b.deck, values) });
            set!("dry_run", flag(r));
        }
        "deck_lint" => {
            set!("state", state(r));
            set!("severity", json!(r.pick(&["error", "warning", "info", "fatal", ""])));
            set!("fix", flag(r));
        }
        "deck_inspect" => {
            set!("state", state(r));
            for view in ["resolved", "timeline", "data"] {
                set!(view, flag(r));
            }
        }
        "deck_diff" => {
            set!("from", state(r));
            set!("to", state(r));
        }
        "deck_render" => {
            set!("state", state(r));
            set!("t", r.pick(&[json!(0), json!(-1), json!(5e-324), json!(1e308), json!(u64::MAX)]).clone());
            let format = [json!("9:16"), json!("16:9"), json!("nope"), json!("1:0"), junk(r)];
            set!("format", r.pick(&format).clone());
            set!("size", json!(r.pick(SIZES)));
        }
        "spine_update" => {
            let mut spine = b.deck.get("spine").cloned().unwrap_or_else(|| json!({ "sections": [] }));
            for _ in 0..r.below(3) {
                change(r, &mut spine, values);
            }
            set!("spine", spine);
            set!("dry_run", flag(r));
        }
        "data_attach" => {
            let id = [json!("attached"), json!("q3"), json!(""), json!("Not An Id"), junk(r)];
            set!("id", r.pick(&id).clone());
            let files: Vec<&String> = b.files.iter().map(|(path, _)| path).collect();
            let data: Vec<&String> = files.iter().copied().filter(|f| f.ends_with(".csv")).collect();
            set!(
                "file",
                match r.below(4) {
                    0 => json!("data/none.csv"),
                    1 => json!(r.pick(&files)),
                    _ if data.is_empty() => json!("deck.json"),
                    _ => json!(r.pick(&data)),
                }
            );
            let column = *r.pick(&["quarter", "revenue", "", "nope"]);
            set!("schema", json!({ column: *r.pick(&["number", "date", "boolean", "nope"]) }));
            set!("parse", json!({ column: *r.pick(&["%Y", "%", "%Q", "%Y%Y%Y%Y%Y%Y", ""]) }));
        }
        "data_edit" => {
            let sources: Vec<Value> = match b.deck.get("data").and_then(Value::as_object) {
                Some(data) => data.keys().map(|k| json!(k)).collect(),
                None => Vec::new(),
            };
            let source = match r.below(5) {
                0 => json!("no-such-source"),
                1 => junk(r),
                _ if sources.is_empty() => json!("q3"),
                _ => r.pick(&sources).clone(),
            };
            set!("source", source);
            // Rows at an edge, columns there or not, values a column reads or does not.
            let rows = [json!(0), json!(1), json!(11), json!(12), json!(4096), json!(-1), json!(1.5), junk(r)];
            let columns = [json!("quarter"), json!("product"), json!("revenue"), json!(""), json!("nope"), junk(r)];
            let cells = [
                json!("19.8"),
                json!(19.8),
                json!("n/a"),
                json!(""),
                json!(null),
                json!(true),
                json!("2026-Q1"),
                json!("Core"),
                json!("a,\"quoted\"\nvalue"),
                json!("1e308"),
                junk(r),
            ];
            let mut edits = Vec::new();
            for _ in 0..r.below(4) {
                let mut edit = Map::new();
                let op = *r.pick(&["set", "add", "remove", "move", ""]);
                edit.insert("op".into(), json!(op));
                if r.below(5) > 0 {
                    edit.insert("row".into(), r.pick(&rows).clone());
                }
                if op == "set" || r.below(8) == 0 {
                    edit.insert("column".into(), r.pick(&columns).clone());
                    edit.insert("value".into(), r.pick(&cells).clone());
                }
                if op == "add" && r.below(2) == 0 {
                    edit.insert(
                        "values".into(),
                        json!({ "quarter": r.pick(&cells).clone(), "revenue": r.pick(&cells).clone() }),
                    );
                }
                edits.push(Value::Object(edit));
            }
            set!("edits", if r.below(8) == 0 { junk(r) } else { Value::Array(edits) });
            set!("dry_run", flag(r));
        }
        "theme_edit" => {
            let theme: Value = serde_json::from_str(&b.theme).unwrap();
            set!("ops", if r.below(6) == 0 { junk(r) } else { theme_ops(r, &theme, values) });
            set!("dry_run", flag(r));
        }
        "icon_find" => {
            set!("query", json!(r.pick(&["rocket", "", "-", "arrow up", "\u{1F680}", "zzqx"])));
            set!("weight", json!(r.pick(&["regular", "fill", "bold", ""])));
            set!("limit", r.pick(&[json!(0), json!(3), json!(u64::MAX), json!(-1), json!(null)]).clone());
            set!("paths", r.pick(&[json!(true), json!(false), json!("yes")]).clone());
        }
        "slide_write" => {
            let slide = json!({
                "slideTitle": r.pick(&["Three reasons", "", "\u{1F680}"]),
                "layoutSuggestion": r.pick(&["Bullet List", "Big Number", "Quote", "Nothing like it"]),
                "keyBullets": r.pick(&[json!(["Fast: drafts in seconds", "Safe: never sends alone"]), json!([]), json!(["42%"]), json!(null)]).clone(),
            });
            set!("slide", if r.below(6) == 0 { junk(r) } else { slide });
            if r.below(2) == 0 {
                set!("after", state(r));
            } else {
                set!("replace", state(r));
            }
            set!("dry_run", flag(r));
        }
        _ => set!("state", state(r)),
    }
    if r.below(12) == 0 {
        args.insert(r.pick(&["bundle", "out", "painter"]).to_string(), junk(r));
    }
    (name.into(), if r.below(16) == 0 { junk(r) } else { Value::Object(args) })
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 60 { format!("{}… ({} bytes)", &s[..s.floor_char_boundary(60)], s.len()) } else { s }
}

/// What the page asks of a deck it shows: the states an edit touched and two more, each at
/// rest and through its cue, and read; one painted, and drawn in each format; then, for a
/// deck that compiled, linted and inspected.
fn exercise(r: &mut Rng, s: &mut Session, compiled: bool, touched: &[String]) {
    let Ok(timeline) = s.timeline() else { return };
    let states = s.states();
    if states.is_empty() {
        return;
    }
    let mut chosen: Vec<String> = touched.iter().filter(|t| states.contains(t)).cloned().collect();
    chosen.extend((0..2).map(|_| r.pick(&states).clone()));
    for state in &chosen {
        let span = timeline.slot(state).map_or(0.0, |slot| slot.span);
        let _ = s.frame(state, f64::INFINITY);
        if span > 0.0 && span.is_finite() {
            let _ = s.frame(state, 0.0);
            let _ = s.frame(state, span * 0.5);
        }
        let _ = s.reading(state);
    }
    let one = &chosen[0];
    let _ = s.pixels(one, f64::INFINITY, 64);
    for format in s.formats() {
        if s.set_format(Some(&format)).is_ok() {
            let _ = s.frame(one, f64::INFINITY);
        }
    }
    let _ = s.set_format(None);
    // Each format beside the canvas, as the editor paints them (PLAN 2.62): at rest and inside the
    // cue, a format the deck no longer lists among them.
    let span = timeline.slot(one).map_or(0.0, |slot| slot.span);
    for format in s.formats().iter().map(|f| Some(f.as_str())).chain([None, Some("4:5")]) {
        let _ = s.pixels_in(format, one, f64::INFINITY, 24);
        if span > 0.0 && span.is_finite() {
            let _ = s.pixels_in(format, one, span * 0.5, 24);
        }
    }
    // The mark a point names, and what a source's rows draw (PLAN 2.64): at points on and off
    // the canvas, and for each source's first rows and a row past any.
    for _ in 0..4 {
        let point = [r.below(2400) as f32 - 200.0, r.below(1400) as f32 - 200.0];
        let _ = s.mark_at(one, point);
    }
    for (source, _) in s.data_sources() {
        let _ = s.marks_of(one, &source, &[0, 1, r.below(40), usize::MAX]);
    }
    // The layout it uses, as its slots (PLAN 2.71).
    let _ = s.layout(one);
    // The link at points on and off the canvas (PLAN 2.70).
    for _ in 0..2 {
        let _ = s.link_at(one, [r.below(2400) as f32 - 200.0, r.below(1400) as f32 - 200.0]);
    }
    // Each shape's outline, and a node that is none (PLAN 2.68).
    for b in s.boxes(one).unwrap_or_default().iter().take(8) {
        let _ = s.outline(one, &b.node);
    }
    let _ = s.outline(one, "nowhere");
    // Each node's framing, an image's or none, and a node that is not there (PLAN 2.74).
    for b in s.boxes(one).unwrap_or_default().iter().take(8) {
        let _ = s.framing(one, &b.node);
    }
    let _ = s.framing(one, "nowhere");
    // Each chart's marks and annotations, and a node that is none (PLAN 2.75).
    for b in s.boxes(one).unwrap_or_default().iter().take(8) {
        let _ = s.marks_in(one, &b.node);
    }
    if compiled {
        let _ = s.lint(Some(one), None);
        let _ = s.inspect(one);
    }
}

static PANIC: Mutex<Option<String>> = Mutex::new(None);

#[test]
fn any_edit_leaves_a_deck_the_engine_draws_or_refuses() {
    let (seed, count, all) = match std::env::var("SCAENA_FUZZ").ok().and_then(|s| {
        let (a, b) = s.split_once(':')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    }) {
        Some((seed, count)) => (seed, count, true),
        None => (0x5ca3_ed17_5ca3_ed17_u64, 100, false),
    };
    let bundles = bundles(all);
    let mut values = BTreeMap::new();
    for b in &bundles {
        values_by_key(&b.deck, &mut values);
        values_by_key(&serde_json::from_str(&b.theme).unwrap(), &mut values);
    }
    let sources: Vec<String> =
        bundles.iter().map(|b| scaena_core::dsl::decompile(&Deck::from_json(&b.deck.to_string()).unwrap())).collect();
    std::panic::set_hook(Box::new(|info| {
        let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let what = (info.payload().downcast_ref::<&str>().map(|s| s.to_string()))
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        *PANIC.lock().unwrap() = Some(format!("{what} at {at}"));
    }));
    let mut r = Rng(seed | 1);
    let mut sessions: Vec<Option<Session>> = bundles.iter().map(|_| None).collect();
    let mut failed = Vec::new();
    for case in 0..count {
        let i = r.below(bundles.len());
        let b = &bundles[i];
        let s = sessions[i].get_or_insert_with(|| session(b));
        let mut said = Vec::new();
        let ran = catch_unwind(AssertUnwindSafe(|| match case % 5 {
            // A source typed into the editor: what compiles replaces the deck.
            0 => {
                let mut source = sources[i].clone();
                for _ in 0..1 + r.below(3) {
                    said.push(type_into(&mut r, &mut source));
                }
                let compiled = s.compile(&source).valid;
                exercise(&mut r, s, compiled, &[]);
            }
            // A deck changed: the editor's when it compiles, else the player's as it is.
            1 => {
                let mut doc = b.deck.clone();
                for _ in 0..1 + r.below(3) {
                    said.push(change(&mut r, &mut doc, &values));
                }
                let Ok(deck) = Deck::from_json(&doc.to_string()) else { return };
                let touched: Vec<String> =
                    said.iter().flat_map(|line| touched_by(&doc, line.split(' ').next().unwrap_or(""))).collect();
                let compiled = s.compile(&scaena_core::dsl::decompile(&deck)).valid;
                if !compiled {
                    s.set_deck(deck);
                }
                exercise(&mut r, s, compiled, &touched);
            }
            // A theme edited by hand, under the deck as it was opened.
            3 => {
                let mut theme: Value = serde_json::from_str(&b.theme).unwrap();
                for _ in 0..1 + r.below(3) {
                    said.push(change(&mut r, &mut theme, &values));
                }
                let Ok(mut themed) = Session::new(&b.deck.to_string(), &theme.to_string()) else { return };
                for (path, bytes) in &b.files {
                    themed.add_file(path, bytes.clone());
                }
                let compiled = themed.compile(&sources[i]).valid;
                exercise(&mut r, &mut themed, compiled, &[]);
            }
            // Calls the assistant makes, one after another, on the deck as it was opened. A
            // call refused leaves the session as it was; one that edits shows its deck.
            4 => {
                if !s.compile(&sources[i]).valid {
                    return;
                }
                // The user's calls are the Data panel's (PLAN 2.55), which it undoes and redoes.
                let by = Caller { author: if r.below(3) == 0 { "user" } else { "agent:test" }, at: None };
                for _ in 0..1 + r.below(3) {
                    let states = s.states();
                    if states.is_empty() {
                        break;
                    }
                    let (name, args) = tool_call(&mut r, b, &states, &values);
                    said.push(format!("{name} {}", short(&args)));
                    if let Ok(called) = s.tool(&name, args, by) {
                        if let Some(frame) = &called.frame {
                            assert!(frame.width as u64 * frame.height as u64 <= scaena_paint::MAX_PIXELS);
                        }
                        // A theme edited draws every state in it from now on.
                        if called.edited || !called.rewritten.is_empty() {
                            let compiled = s.compile(&s.source()).valid;
                            exercise(&mut r, s, compiled, &[]);
                        }
                    }
                    if r.below(4) == 0 {
                        let redo = r.below(2) == 0;
                        said.push(format!("data {}", if redo { "redo" } else { "undo" }));
                        if s.data_undo(redo, by).is_ok_and(|undone| undone.is_some()) {
                            let compiled = s.compile(&s.source()).valid;
                            exercise(&mut r, s, compiled, &[]);
                        }
                    }
                }
                let compiled = s.compile(&s.source()).valid;
                exercise(&mut r, s, compiled, &[]);
            }
            // A patch the assistant sends, to the deck as it was opened.
            _ => {
                let ops = patch(&mut r, &b.deck, &values);
                said.push(short(&ops));
                if !s.compile(&sources[i]).valid {
                    return;
                }
                let touched: Vec<String> = (ops.as_array().into_iter().flatten())
                    .filter_map(|op| op.get("state").or_else(|| op.get("before")).and_then(Value::as_str))
                    .map(String::from)
                    .collect();
                let by = Caller { author: "agent:test", at: None };
                if s.tool("deck_patch", json!({ "ops": ops }), by).is_ok_and(|called| called.edited) {
                    let compiled = s.compile(&s.source()).valid;
                    exercise(&mut r, s, compiled, &touched);
                }
            }
        }));
        if ran.is_err() {
            let why = PANIC.lock().unwrap().take().unwrap_or_default();
            failed.push(format!("case {case} ({}): {}\n  {why}", b.name, said.join("; ")));
            sessions[i] = None;
        }
    }
    let _ = std::panic::take_hook();
    assert!(failed.is_empty(), "{} of {count} edits panicked the engine:\n{}", failed.len(), failed.join("\n"));
}

/// What a shared deck may carry that once stopped the engine (lift-up STAB-1, STAB-5): a
/// chart's expression nested thousands deep, or a chain of thousands of operators; a source's
/// value nested thousands deep; and a forecast whose first date is no day. Each is drawn,
/// linted, or refused with why, never a stack overflow or a panic.
#[test]
fn deep_nesting_and_a_forecasts_short_date_are_drawn_or_refused() {
    let bundles = bundles(false);
    let shared = &bundles.iter().find(|b| b.name == "revenue").unwrap().files;
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples");
    let open = |name: &str, edit: &dyn Fn(&mut Value)| {
        let mut deck: Value =
            serde_json::from_slice(&std::fs::read(examples.join(format!("{name}.deck.json"))).unwrap()).unwrap();
        let theme = std::fs::read_to_string(examples.join(deck["theme"].as_str().unwrap())).unwrap();
        edit(&mut deck);
        session(&Bundle { name: "edited", deck, theme, files: shared.clone() })
    };
    let deep = 50_000;
    let filters = [
        (format!("{}quarter == '2026-Q3'{}", "(".repeat(deep), ")".repeat(deep)), true),
        (format!("{}(quarter == '2026-Q3')", "!".repeat(deep)), true),
        (format!("{}1 > 0", "-".repeat(deep)), true),
        (format!("quarter == '2026-Q3'{}", " && true".repeat(deep)), false),
    ];
    for (filter, refused) in &filters {
        let mut s = open("charts", &|deck| {
            for node in deck["nodes"].as_object_mut().unwrap().values_mut() {
                if let Some(t) = node.get_mut("dataTransform").filter(|t| t.is_array()) {
                    for step in t.as_array_mut().unwrap() {
                        if step.get("filter").is_some() {
                            step["filter"] = filter.as_str().into();
                        }
                    }
                }
            }
        });
        let source = s.source();
        s.compile(&source);
        for state in s.states() {
            let _ = s.frame(&state, f64::INFINITY);
        }
        let found = s.lint(None, Some(1_800_000_000)).unwrap();
        let nested = found.findings.iter().any(|f| f.finding.message.contains("nests more than 64 deep"));
        assert_eq!(nested, *refused, "{:#?}", found.findings);
    }
    // A source typed with a value nested past what deck.json holds.
    let mut s = open("revenue", &|_| {});
    let source = s.source().replacen("axes:{", &format!("axes:{{deep: {}1{}, ", "[".repeat(deep), "]".repeat(deep)), 1);
    let compiled = s.compile(&source);
    assert!(!compiled.valid);
    // And one nested just past what deck.json holds, counted from the deck: 124 to 127 deep here
    // compiled, then panicked the compile, which stops the editor's worker on a keystroke (the
    // 20261024 plan's STAB-1). Each compiles, its findings said, or is refused at its line.
    for n in 120..=128 {
        let mut s = open("revenue", &|_| {});
        let source = s.source().replacen("axes:{", &format!("axes:{{deep: {}1{}, ", "[".repeat(n), "]".repeat(n)), 1);
        let compiled = s.compile(&source);
        let said = serde_json::to_string(&compiled).unwrap();
        let refused = said.contains("nests more than 127 deep in the deck");
        assert!(refused == (n >= 123), "{n} deep: {}", &said[..said.len().min(300)]);
    }
    // A forecast whose first date is too short to be a day, or is cut inside a character, as the
    // assistant's lint reads the deck opened, which validation refuses.
    for first in ["2026", "2026-10-1é", ""] {
        let mut s = open("weather", &|deck| {
            deck["data"]["boston"]["fetched"]["kept"] = false.into();
            deck["data"]["boston"]["source"]["inline"][0]["date"] = first.into();
        });
        let by = Caller { author: "agent:test", at: None };
        let called = s.tool("deck_lint", json!({ "now": "2027-01-15" }), by).unwrap();
        assert!(called.result.contains("W428"), "{first:?}: {}", called.result);
    }
}

/// The deepest deck a person may write, containers nested `MAX_NESTING` deep, is compiled,
/// drawn, and linted on a thread with the stack a Mac's dispatch queue gives its workers,
/// 512 KB, where the editor lints off the main thread (PLAN 4.13); a debug build, which the
/// Swift tests link, takes twice that. The browser's worker holds it too, in the editor, the
/// player, and a single file, and one deeper is E106.
#[test]
fn the_deepest_deck_a_person_may_write_fits_a_workers_stack() {
    let bundles = bundles(false);
    let revenue = bundles.iter().find(|b| b.name == "revenue").unwrap();
    let mut deck = revenue.deck.clone();
    let depth = scaena_core::document::MAX_NESTING;
    for i in 1..=depth {
        let at = if i == 1 { json!({ "in": "canvas" }) } else { json!({ "parent": format!("s{}", i - 1) }) };
        deck["nodes"][format!("s{i}")] = json!({ "type": "stack", "at": at, "semantic": "decoration" });
        deck["states"][0]["props"][format!("s{i}")] = json!({});
    }
    deck["nodes"]["deep"] =
        json!({ "type": "text", "role": "body", "text": "64 deep", "at": { "parent": format!("s{depth}") } });
    deck["states"][0]["props"]["deep"] = json!({});
    let bundle = Bundle { name: "deep", deck, theme: revenue.theme.clone(), files: revenue.files.clone() };
    let stack = if cfg!(debug_assertions) { 1 << 20 } else { 512 << 10 };
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            let mut s = session(&bundle);
            let source = s.source();
            assert!(s.compile(&source).valid, "the deck compiles");
            for state in s.states() {
                s.frame(&state, f64::INFINITY).unwrap();
            }
            let found = s.lint(None, Some(1_800_000_000)).unwrap();
            assert!(!found.findings.iter().any(|f| f.finding.code == "E106"), "{:#?}", found.findings);
        })
        .unwrap()
        .join()
        .expect("drawn and linted within the stack");
}

/// A theme whose grid cannot be laid out, four billion columns or margins past the canvas, says
/// why wherever the deck is used: validation, lint, a render, and the session's frames, as the
/// editor's worker draws them; never a panic, and never the engine run out of memory (lift-up
/// 10-24b STAB-3).
#[test]
fn a_grid_that_cannot_be_laid_out_says_why_wherever_the_deck_is_used() {
    let bundles = bundles(false);
    let revenue = bundles.iter().find(|b| b.name == "revenue").unwrap();
    let state = revenue.deck["states"][0]["id"].as_str().unwrap().to_string();
    let by = || Caller { author: "agent:test", at: None };
    for (key, value, said) in [
        ("columns", json!(4_000_000_000u32), "4000000000 columns: a grid has 1 to 1000 of them"),
        ("margin", json!(1200), "margins leave its columns no room"),
    ] {
        let mut theme: Value = serde_json::from_str(&revenue.theme).unwrap();
        theme["grid"][key] = value;
        let theme = theme.to_string();
        let mut files: BTreeMap<String, Vec<u8>> = revenue.files.iter().cloned().collect();
        files.insert("deck.json".into(), revenue.deck.to_string().into_bytes());
        files.insert(revenue.deck["theme"].as_str().unwrap().into(), theme.clone().into_bytes());
        let bundle = scaena_store::Bundle::in_memory(files).unwrap();
        let found = scaena_core::validate::validate_bundle(&revenue.deck.to_string(), &bundle.files).unwrap();
        let grid = found.iter().find(|f| f.code == "E106" && f.message.contains(said));
        assert!(
            grid.is_some_and(|f| f.path.as_deref().is_some_and(|p| p.ends_with(&format!("/grid/{key}")))),
            "{key}: {found:?}"
        );
        let mut s = Session::new(&revenue.deck.to_string(), &theme).unwrap();
        let named = revenue.deck["theme"].as_str().unwrap();
        for (path, bytes) in revenue.files.iter().filter(|(path, _)| path != named) {
            s.add_file(path, bytes.clone());
        }
        let linted = s.tool("deck_lint", json!({}), by());
        let linted = linted.map(|c| c.result).unwrap_or_else(|e| e.to_string());
        assert!(linted.contains(said), "{key}: lint {linted}");
        let rendered = s.tool("deck_render", json!({ "state": state }), by());
        let rendered = rendered.map(|c| c.result).unwrap_or_else(|e| e.to_string());
        assert!(rendered.contains(said), "{key}: render {rendered}");
        let frame = s.frame(&state, 0.0).map(|_| ()).unwrap_err().to_string();
        assert!(frame.contains(&format!("grid.{key}")) && frame.contains(said), "{key}: frame {frame}");
    }
}
