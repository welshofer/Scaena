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
use scaena_wasm::Session;
use scaena_wasm::assistant::Caller;
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

/// The decks: the example, B1, and the chart gallery, or with `all`, every deck in the
/// repository a bundle holds.
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
    let examples_named: &[&'static str] =
        if all { &["revenue", "charts", "trails", "ridgeline", "higher-ed"] } else { &["revenue", "charts"] };
    for &name in examples_named {
        add(name, &examples.join(format!("{name}.deck.json")), shared.clone(), &examples);
    }
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
    let mut tools = scaena_wasm::assistant::TOOLS.to_vec();
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
    if compiled {
        let _ = s.lint(Some(one));
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
