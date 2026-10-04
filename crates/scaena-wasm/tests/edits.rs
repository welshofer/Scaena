//! Any edit leaves a deck the engine draws, or one it refuses with an error, never one that
//! panics it. In the browser the engine runs in the editor's worker, which compiles the
//! source on every keystroke (PLAN 2.3); a panic there stops the page mid-word. Three kinds
//! of edit, each run through the session as the worker runs it:
//!
//! - a source typed: cut short, a run deleted, a character of any width typed anywhere, a
//!   line copied, two swapped, one indented or not;
//! - a deck changed value by value: a number at the ends of f64, a text empty or long, a
//!   value from elsewhere in the repository's decks, an array emptied, a key dropped;
//! - a patch an assistant might send through `deck_patch` (PLAN 2.6).
//!
//! A deck that compiles is drawn at rest and through its cue, read, painted, and linted and
//! inspected in a state. One that does not is handed to the player as a bundle's `deck.json`
//! would be, unvalidated, and drawn as far as it goes. `SCAENA_FUZZ=<seed>:<count>` runs a
//! longer search, over every deck in the repository, locally.

use scaena_core::Deck;
use scaena_wasm::Session;
use scaena_wasm::assistant::Caller;
use serde_json::{Value, json};
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
        ops.push(match r.below(10) {
            0 => json!({ "op": "add_node", "id": "added", "node": { "type": "text", "text": text(r, &strings) }, "state": state }),
            1 => json!({ "op": "remove_node", "id": node }),
            2 => json!({ "op": "rename_node", "id": node, "to": r.pick(&nodes).clone() }),
            3 => json!({ "op": "show_node", "node": node, "state": state }),
            4 => json!({ "op": "hide_node", "node": node, "state": state }),
            5 => json!({ "op": "set_prop", "node": node, "prop": key, "value": value, "state": state }),
            6 => json!({ "op": "set_text", "node": node, "text": text(r, &strings), "state": state }),
            7 => json!({ "op": "move_state", "id": state, "before": r.pick(&states).clone() }),
            8 => json!({ "op": "remove_state", "id": state }),
            _ => json!({ "op": "replace", "path": r.pick(&pointers).clone(), "value": value }),
        });
    }
    Value::Array(ops)
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
        None => (0x5ca3_ed17_5ca3_ed17_u64, 60, false),
    };
    let bundles = bundles(all);
    let mut values = BTreeMap::new();
    for b in &bundles {
        values_by_key(&b.deck, &mut values);
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
        let ran = catch_unwind(AssertUnwindSafe(|| match case % 3 {
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
