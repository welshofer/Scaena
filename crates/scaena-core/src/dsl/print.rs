//! A deck as canonical `.scn` source (SPEC §4.3).
//!
//! Every shorthand is used only where it compiles back to exactly what the deck holds:
//! keys keep their order, a node is declared in a state only where that keeps `nodes` in
//! its order, and a time takes a unit only where the number reads back the same. Anything
//! else is written out as `key:value`.

use super::parse::{DECK_KEYS, KINDED, STATE_WORDS, TYPES};
use crate::document::Deck;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Lines run to 100 columns; props that would pass it go on continuation lines.
const WIDTH: usize = 100;

/// The keys that hold milliseconds in a state and its choreography.
const TIMES: [&str; 4] = ["hold", "delay", "stagger", "duration"];

/// `deck` as canonical source.
pub fn decompile(deck: &Deck) -> String {
    let doc = serde_json::to_value(deck).expect("a deck serializes");
    let mut p = Printer::default();
    p.document(doc.as_object().expect("a deck is an object"));
    p.out
}

#[derive(Default)]
struct Printer {
    out: String,
    /// The kind of the last top-level declaration.
    last: &'static str,
}

impl Printer {
    /// A blank line before a top-level declaration of another kind than the last, and
    /// before every section and state. Blank lines mean nothing to the compiler.
    fn gap(&mut self, kind: &'static str) {
        if !self.out.is_empty() && (kind != self.last || kind == "section" || kind == "state") {
            self.out.push('\n');
        }
        self.last = kind;
    }

    fn line(&mut self, indent: usize, text: &str) {
        self.out.push_str(&" ".repeat(indent));
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// `head` and its props, filled to the width; props that do not fit continue on lines
    /// two deeper.
    fn item(&mut self, indent: usize, head: &str, props: &[String]) {
        let mut line = format!("{}{head}", " ".repeat(indent));
        let mut alone = head.is_empty();
        for prop in props {
            if !alone && width(&line) + 1 + width(prop) > WIDTH {
                self.out.push_str(&line);
                self.out.push('\n');
                line = format!("{}{prop}", " ".repeat(indent + 2));
            } else if alone {
                line.push_str(prop);
                alone = false;
            } else {
                line.push(' ');
                line.push_str(prop);
            }
        }
        self.out.push_str(&line);
        self.out.push('\n');
    }

    fn comments(&mut self, indent: usize, text: &str) {
        for line in text.split('\n') {
            if line.is_empty() {
                self.line(indent, "#");
            } else {
                self.line(indent, &format!("# {line}"));
            }
        }
    }

    fn document(&mut self, d: &Map<String, Value>) {
        let mut props = Vec::new();
        match d.get("_comment") {
            Some(Value::String(c)) if commentable(c) => self.comments(0, c),
            Some(c) => props.push(format!("_comment:{}", val(c))),
            None => {}
        }
        let meta = d.get("meta").and_then(Value::as_object);
        let mut head = "deck".to_string();
        if let Some(Value::String(title)) = meta.and_then(|m| m.get("title")) {
            head = format!("deck {}", quoted(title));
        }
        if d["scaena"] != Value::String(crate::FORMAT_VERSION.into()) {
            props.push(format!("scaena:{}", val(&d["scaena"])));
        }
        if let Some(theme) = d.get("theme") {
            props.push(format!("theme:{}", val(theme)));
        }
        props.push(canvas(&d["canvas"]));
        if let Some(formats) = d.get("formats") {
            props.push(format!("formats:{}", val(formats)));
        }
        if let Some(meta) = meta {
            let rest: Map<String, Value> =
                meta.iter().filter(|(k, _)| *k != "title").map(|(k, v)| (k.clone(), v.clone())).collect();
            let clash = rest.keys().any(|k| DECK_KEYS.contains(&k.as_str()) || k == "title");
            if (rest.is_empty() && !meta.contains_key("title")) || clash {
                props.push(format!("meta:{}", val(&Value::Object(rest))));
            } else {
                props.extend(rest.iter().map(|(k, v)| format!("{}:{}", key(k), val(v))));
            }
        }
        let sections = d.get("spine").and_then(|s| s.get("sections")).and_then(Value::as_array);
        if let Some(spine) = d.get("spine")
            && !spine_as_sections(spine)
        {
            props.push(format!("spine:{}", val(spine)));
        }
        self.item(0, &head, &props);

        self.last = "deck";
        for font in d.get("fonts").and_then(Value::as_array).into_iter().flatten() {
            self.gap("font");
            self.font(font.as_object().expect("a font is an object"));
        }
        for (id, source) in d.get("data").and_then(Value::as_object).into_iter().flatten() {
            self.gap("data");
            self.data(id, source.as_object().expect("a data source is an object"));
        }
        let nodes = d["nodes"].as_object().expect("nodes are a map");
        let states = d["states"].as_array().expect("states are a list");
        let decls = declarations(nodes, states);
        for id in &decls.before[0] {
            self.gap("node");
            self.node(0, &format!("node {}", word(id)), &nodes[*id]);
        }
        for (id, props) in d.get("overrides").and_then(Value::as_object).into_iter().flatten() {
            self.gap("override");
            let mut keys = Keys::new(props.as_object().expect("overrides are maps"));
            if let Some(c) = keys.comment() {
                self.comments(0, &c);
            }
            self.item(0, &format!("override {}", word(id)), &keys.rest(""));
        }
        if let Some(sections) = sections
            && d.get("spine").is_some_and(spine_as_sections)
        {
            for section in sections {
                self.gap("section");
                self.section(section.as_object().expect("a section is an object"));
            }
        }
        for (k, state) in states.iter().enumerate() {
            if k > 0 {
                for id in &decls.before[k] {
                    self.gap("node");
                    self.node(0, &format!("node {}", word(id)), &nodes[*id]);
                }
            }
            self.gap("state");
            self.state(k, state.as_object().expect("a state is an object"), nodes, &decls.inline);
        }
        if !states.is_empty() {
            for id in &decls.before[states.len()] {
                self.gap("node");
                self.node(0, &format!("node {}", word(id)), &nodes[*id]);
            }
        }
    }

    fn font(&mut self, f: &Map<String, Value>) {
        let mut head = "font".to_string();
        let mut props = Vec::new();
        if let (Some(Value::String(family)), Some(Value::String(file))) = (f.get("family"), f.get("file")) {
            head = format!("font {} {}", word(family), quoted(file));
        }
        for (k, v) in f {
            if !(head != "font" && (k == "family" || k == "file")) {
                // `axes` and `weight` are numbers in the deck's types, so `100` reads as `100.0`.
                props.push(format!("{}:{}", key(k), val(&whole(v))));
            }
        }
        self.item(0, &head, &props);
    }

    fn data(&mut self, id: &str, s: &Map<String, Value>) {
        let mut head = format!("data {}", word(id));
        let mut props = Vec::new();
        for (k, v) in s {
            match (k.as_str(), v) {
                ("source", Value::String(path)) => head = format!("{head} {}", quoted(path)),
                ("source", Value::Object(o)) if o.len() == 1 && o.contains_key("inline") => {
                    props.push(format!("inline:{}", val(&o["inline"])));
                }
                _ => props.push(format!("{}:{}", key(k), val(v))),
            }
        }
        self.item(0, &head, &props);
    }

    /// A node's declaration: `head` (`node id`, or the id in a state), its type, and its
    /// props.
    fn node(&mut self, indent: usize, head: &str, node: &Value) {
        let node = node.as_object().expect("a node is an object");
        let ty = node["type"].as_str().expect("a node's type is a word");
        let props: Map<String, Value> =
            node.iter().filter(|(k, _)| *k != "type").map(|(k, v)| (k.clone(), v.clone())).collect();
        let mut keys = Keys::new(&props);
        if let Some(c) = keys.comment() {
            self.comments(indent, &c);
        }
        let mut head = format!("{head} {ty}");
        if KINDED.contains(&ty)
            && let Some(kind) = keys.take_if("kind", |v| v.as_str().is_some_and(is_word))
        {
            head = format!("{head}:{}", kind.as_str().expect("a word"));
        }
        self.item(indent, &head, &keys.rest(ty));
    }

    fn section(&mut self, s: &Map<String, Value>) {
        let mut head = format!("section {}", word(s["id"].as_str().expect("a section's id is a string")));
        if let Some(Value::String(title)) = s.get("title") {
            head = format!("{head} {}", quoted(title));
        }
        self.item(0, &head, &[]);
        for beat in s["beats"].as_array().expect("beats are a list") {
            let b = beat.as_object().expect("a beat is an object");
            let head = format!(
                "beat {} {}",
                word(b["id"].as_str().expect("a beat's id is a string")),
                quoted(b["claim"].as_str().expect("a claim is a string"))
            );
            let props: Vec<String> = b
                .iter()
                .filter(|(k, v)| !matches!(k.as_str(), "id" | "claim" | "notes") && !(*k == "media" && v.is_object()))
                .map(|(k, v)| match (k.as_str(), whole(v)) {
                    // A beat's duration is in seconds.
                    ("duration", n @ Value::Number(_)) => format!("duration:{n}s"),
                    _ => format!("{}:{}", key(k), val(v)),
                })
                .collect();
            self.item(2, &head, &props);
            if let Some(Value::Object(media)) = b.get("media") {
                let props: Vec<String> = media.iter().map(|(k, v)| format!("{}:{}", key(k), val(v))).collect();
                self.item(4, "media", &props);
            }
            if let Some(Value::String(notes)) = b.get("notes") {
                self.notes(4, notes);
            }
        }
    }

    fn state(&mut self, k: usize, s: &Map<String, Value>, nodes: &Map<String, Value>, inline: &HashMap<&str, usize>) {
        let mut props = Vec::new();
        match s.get("_comment") {
            Some(Value::String(c)) if commentable(c) => self.comments(0, c),
            Some(c) => props.push(format!("_comment:{}", val(c))),
            None => {}
        }
        for key in ["name", "slide", "from", "mode", "layout", "transition", "hold"] {
            match (key, s.get(key)) {
                ("mode", Some(Value::String(m))) if m == "delta" => {}
                // `hold` is a float in the deck's types, so `4000` reads back as `4000.0`.
                ("hold", Some(v)) => props.push(format!("hold:{}", timed("hold", &whole(v)))),
                ("transition", Some(v)) => props.push(format!("transition:{}", timed("duration", v))),
                (_, Some(v)) => props.push(format!("{key}:{}", val(v))),
                (_, None) => {}
            }
        }
        self.item(0, &format!("state {}", word(s["id"].as_str().expect("a state's id is a string"))), &props);
        for id in s.get("remove").and_then(Value::as_array).into_iter().flatten() {
            self.line(2, &format!("-{}", word(id.as_str().expect("an id"))));
        }
        for (id, delta) in s.get("props").and_then(Value::as_object).into_iter().flatten() {
            if inline.get(id.as_str()) == Some(&k) {
                self.node(2, &body_word(id), &nodes[id]);
                continue;
            }
            let ty = nodes.get(id).and_then(|n| n["type"].as_str()).unwrap_or("");
            let mut keys = Keys::new(delta.as_object().expect("a delta is a map"));
            if let Some(c) = keys.comment() {
                self.comments(2, &c);
            }
            let mut rest = keys.rest(ty);
            // A first prop named like a type would read as one: quote its key.
            if let Some(first) = rest.first_mut()
                && let Some((k, v)) = first.split_once(':')
                && TYPES.contains(&k)
            {
                *first = format!("{}:{v}", quoted(k));
            }
            self.item(2, &body_word(id), &rest);
        }
        for item in s.get("choreography").and_then(Value::as_array).into_iter().flatten() {
            self.choreo(2, item);
        }
        if let Some(Value::String(notes)) = s.get("notes") {
            self.notes(2, notes);
        }
    }

    /// One choreography item: `choreo target …`, a `sequence` or `parallel` block, or
    /// `choreo = value` for an item neither can say.
    fn choreo(&mut self, indent: usize, item: &Value) {
        let Some(map) = item.as_object() else {
            return self.line(indent, &format!("choreo = {}", val(item)));
        };
        let first = map.keys().next().map(String::as_str);
        let grouped = map.contains_key("sequence") || map.contains_key("parallel");
        match first {
            Some("target") if !grouped && target_word(&map["target"]).is_some() => {
                let props: Vec<String> =
                    map.iter().skip(1).map(|(k, v)| format!("{}:{}", key(k), timed(k, v))).collect();
                self.item(indent, &format!("choreo {}", target_word(&map["target"]).expect("checked")), &props);
            }
            Some(kind @ ("sequence" | "parallel"))
                if map[kind].as_array().is_some_and(|items| items.iter().all(Value::is_object))
                    && !(map.contains_key("sequence") && map.contains_key("parallel")) =>
            {
                let props: Vec<String> =
                    map.iter().skip(1).map(|(k, v)| format!("{}:{}", key(k), timed(k, v))).collect();
                self.item(indent, kind, &props);
                for inner in map[kind].as_array().expect("checked") {
                    self.choreo(indent + 2, inner);
                }
            }
            _ => self.line(indent, &format!("choreo = {}", val(item))),
        }
    }

    fn notes(&mut self, indent: usize, notes: &str) {
        if notes.contains('\n') && triple_quotable(notes) {
            self.line(indent, "notes \"\"\"");
            for line in notes.split('\n') {
                if line.is_empty() {
                    self.out.push('\n');
                } else {
                    self.line(indent + 2, line);
                }
            }
            self.line(indent + 2, "\"\"\"");
        } else {
            self.line(indent, &format!("notes {}", quoted(notes)));
        }
    }
}

/// A node line's or an override's props, read off in the order shorthand may take them.
struct Keys<'a> {
    map: &'a Map<String, Value>,
    at: usize,
}

impl<'a> Keys<'a> {
    fn new(map: &'a Map<String, Value>) -> Keys<'a> {
        Keys { map, at: 0 }
    }

    fn next(&self) -> Option<(&'a String, &'a Value)> {
        self.map.iter().nth(self.at)
    }

    fn take_if(&mut self, k: &str, ok: impl Fn(&Value) -> bool) -> Option<&'a Value> {
        match self.next() {
            Some((key, v)) if key == k && ok(v) => {
                self.at += 1;
                Some(v)
            }
            _ => None,
        }
    }

    /// A first `_comment` that comment lines can hold.
    fn comment(&mut self) -> Option<String> {
        self.take_if("_comment", |v| v.as_str().is_some_and(commentable))
            .map(|v| v.as_str().expect("a string").to_string())
    }

    /// Every prop not yet taken, as `key:value`, except a text node's `text` or an image's
    /// `src`: a bare string where it stands.
    fn rest(&self, ty: &str) -> Vec<String> {
        let primary = match ty {
            "text" => Some("text"),
            "image" => Some("src"),
            _ => None,
        };
        self.map
            .iter()
            .skip(self.at)
            .map(|(k, v)| match v {
                Value::String(s) if primary == Some(k.as_str()) => quoted(s),
                _ => prop(k, v),
            })
            .collect()
    }
}

/// Where each node is declared: `inline` in the state that first shows it (the state's
/// index), or by a `node` line in `before[k]`, before state k (k = the number of states:
/// after the last).
struct Declarations<'a> {
    inline: HashMap<&'a str, usize>,
    before: Vec<Vec<&'a str>>,
}

/// A node is declared in the state that first shows it when that state shows it unchanged
/// (`{}`), and when declaring it there keeps `nodes` in its order: declarations compile in
/// the order the source writes them.
fn declarations<'a>(nodes: &'a Map<String, Value>, states: &'a [Value]) -> Declarations<'a> {
    let mut first: HashMap<&str, (usize, usize)> = HashMap::new();
    for (s, state) in states.iter().enumerate() {
        for (j, id) in state.get("props").and_then(Value::as_object).into_iter().flatten().map(|(id, _)| id).enumerate()
        {
            first.entry(id.as_str()).or_insert((s, j));
        }
    }
    let mut out = Declarations { inline: HashMap::new(), before: vec![Vec::new(); states.len() + 1] };
    // Where the last declaration went: a line in a state, or (k, None) for one before state k.
    let mut last: (usize, Option<usize>) = (0, None);
    for (id, node) in nodes {
        let here = first.get(id.as_str()).copied().filter(|&(s, _)| {
            states[s]["props"][id].as_object().is_some_and(Map::is_empty)
                && node["type"].as_str().is_some_and(|ty| TYPES.contains(&ty))
        });
        match here {
            Some((s, j)) if (s, Some(j)) > last => {
                out.inline.insert(id, s);
                last = (s, Some(j));
            }
            _ => {
                let k = match last {
                    (k, None) => k,
                    (k, Some(_)) => k + 1,
                };
                out.before[k].push(id);
                last = (k, None);
            }
        }
    }
    out
}

/// `v` as the value of `k`, where `k` may hold milliseconds: an integer with its unit,
/// `4s` or `240ms`, and maps within it likewise. A float stays a plain number, since a
/// unit would read it back as an integer.
fn timed(k: &str, v: &Value) -> String {
    match v {
        Value::Number(n) if TIMES.contains(&k) => match n.as_i64() {
            Some(ms) if ms != 0 && ms % 1000 == 0 => format!("{}s", ms / 1000),
            Some(ms) => format!("{ms}ms"),
            None => val(v),
        },
        Value::Object(map) if !map.is_empty() => {
            format!(
                "{{{}}}",
                map.iter().map(|(k, v)| format!("{}: {}", key(k), timed(k, v))).collect::<Vec<_>>().join(", ")
            )
        }
        _ => val(v),
    }
}

/// `key:value`, and `at` as calls where it can be.
fn prop(k: &str, v: &Value) -> String {
    if k == "at"
        && let Some(calls) = at_calls(v)
    {
        return format!("at:{calls}");
    }
    format!("{}:{}", key(k), val(v))
}

/// `col(1-7) row(1) in(title)`: an `at` map as calls, when every key is a word.
fn at_calls(v: &Value) -> Option<String> {
    let map = v.as_object()?;
    if map.is_empty() || !map.keys().all(|k| is_word(k)) {
        return None;
    }
    let calls: Vec<String> = map
        .iter()
        .map(|(k, v)| {
            let args = match (k.as_str(), v) {
                ("col" | "row", Value::Array(a)) if a.len() == 2 && a.iter().all(Value::is_u64) => {
                    format!("{}-{}", a[0], a[1])
                }
                (_, Value::Array(a)) if a.len() >= 2 => a.iter().map(val).collect::<Vec<_>>().join(", "),
                _ => val(v),
            };
            format!("{k}({args})")
        })
        .collect();
    Some(calls.join(" "))
}

/// `canvas:1920x1080`, or the canvas as a map when it holds more than a size.
fn canvas(c: &Value) -> String {
    let map = c.as_object().expect("a canvas is a map");
    let only_size = map.keys().all(|k| matches!(k.as_str(), "width" | "height" | "unit"))
        && map.get("unit").is_none_or(|u| u == "cu");
    match (map.get("width").and_then(Value::as_f64), map.get("height").and_then(Value::as_f64)) {
        (Some(w), Some(h)) if only_size => format!("canvas:{}x{}", dim(w), dim(h)),
        _ => format!("canvas:{}", val(c)),
    }
}

/// `v` with every float that is a whole number written as an integer: for a field the
/// deck's types hold as a float anyway, where `4000` reads back as `4000.0`.
fn whole(v: &Value) -> Value {
    match v {
        Value::Number(n) if n.is_f64() => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => Value::from(f as i64),
            _ => v.clone(),
        },
        Value::Array(items) => Value::Array(items.iter().map(whole).collect()),
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), whole(v))).collect()),
        other => other.clone(),
    }
}

fn dim(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

/// Whether a spine is written as `section` lines: a spine of sections, each a map with an
/// id, beats, and nothing else a section line cannot say.
fn spine_as_sections(spine: &Value) -> bool {
    let Some(map) = spine.as_object() else { return false };
    let Some(sections) = map.get("sections").and_then(Value::as_array) else { return false };
    map.len() == 1 && !sections.is_empty()
}

/// A target as `choreo` writes it: an id, or a list.
fn target_word(target: &Value) -> Option<String> {
    match target {
        Value::String(s) => Some(body_word(s)),
        Value::Array(_) => Some(val(target)),
        _ => None,
    }
}

/// A value as source.
fn val(v: &Value) -> String {
    match v {
        Value::String(s) if is_bare(s) => s.clone(),
        Value::String(s) => quoted(s),
        Value::Array(items) => format!("[{}]", items.iter().map(val).collect::<Vec<_>>().join(", ")),
        Value::Object(map) if map.is_empty() => "{}".into(),
        Value::Object(map) => {
            format!("{{{}}}", map.iter().map(|(k, v)| format!("{}: {}", key(k), val(v))).collect::<Vec<_>>().join(", "))
        }
        other => other.to_string(),
    }
}

fn quoted(s: &str) -> String {
    serde_json::to_string(s).expect("a string serializes")
}

fn key(k: &str) -> String {
    if is_word(k) { k.to_string() } else { quoted(k) }
}

/// An id at a line's head, where a word cannot be one that starts some other line.
fn word(id: &str) -> String {
    if is_word(id) { id.to_string() } else { quoted(id) }
}

/// An id at the head of a line in a state's body.
fn body_word(id: &str) -> String {
    if is_word(id) && !STATE_WORDS.contains(&id) { id.to_string() } else { quoted(id) }
}

/// `[A-Za-z_][A-Za-z0-9_.-]*`.
fn is_word(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// A string that reads back as itself unquoted: a word that is not `true`, `false`, or
/// `null`; `@` and a word; a percentage; or a ratio.
fn is_bare(s: &str) -> bool {
    if is_word(s) {
        return !matches!(s, "true" | "false" | "null");
    }
    if let Some(name) = s.strip_prefix('@') {
        return !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    }
    if let Some(n) = s.strip_suffix('%') {
        return is_json_number(n) && !n.contains(['e', 'E']);
    }
    if let Some((a, b)) = s.split_once(':') {
        // `16:9`: two numbers as JSON writes them, without a sign or an exponent.
        let numeral = |n: &str| is_json_number(n) && !n.contains(['-', 'e', 'E']);
        return numeral(a) && numeral(b);
    }
    false
}

/// `-?(0|[1-9][0-9]*)(.[0-9]+)?`, as JSON writes a number.
fn is_json_number(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = s.split_once('.').map_or((s, None), |(i, f)| (i, Some(f)));
    let int_ok = int == "0" || (!int.is_empty() && !int.starts_with('0') && int.bytes().all(|b| b.is_ascii_digit()));
    int_ok && frac.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()))
}

/// Text comment lines can hold.
fn commentable(c: &str) -> bool {
    !c.contains('\r')
}

/// Text a `"""` block holds exactly.
fn triple_quotable(s: &str) -> bool {
    !s.contains('\r') && !s.contains("\"\"\"") && s.split('\n').all(|l| l == l.trim_end() && !l.contains('\t'))
}

fn width(s: &str) -> usize {
    s.chars().count()
}
