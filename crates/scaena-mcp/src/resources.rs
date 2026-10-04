//! The resources (SPEC §7.2): what an agent reads to learn the format without the docs.
//!
//! Claude Code keeps an MCP result over 25,000 tokens in a file (`MAX_MCP_OUTPUT_TOKENS`),
//! out of reach of an agent with no tools for files. So every resource arrives whole, under
//! [`LIMIT`] as `resources/read` returns it: SPEC is served by section, a schema too large
//! for that in parts, and JSON without its whitespace.

use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

/// The most a resource may weigh as `resources/read` returns it, in bytes of JSON with its
/// text escaped. Gate 1's agent got a 45 KB result whole and a 74 KB one as a file; at two
/// and a half bytes a token, the densest of these texts, 40 KB is 16,000 tokens.
pub const LIMIT: usize = 40_000;

/// A resource the server serves.
#[derive(Debug)]
pub struct Served {
    pub uri: String,
    pub name: String,
    pub mime: &'static str,
    pub text: String,
    /// Whether `resources/list` names it. A subsection of SPEC whose section arrives whole is
    /// read by the uri SPEC's index gives it.
    pub listed: bool,
}

/// Every resource, built once.
pub fn all() -> &'static [Served] {
    &ALL
}

/// A resource's text, by its uri.
pub fn resource(uri: &str) -> Option<&'static str> {
    ALL.iter().find(|r| r.uri == uri).map(|r| r.text.as_str())
}

const SCHEMA: &str = "application/schema+json";
const MARKDOWN: &str = "text/markdown";
const JSON: &str = "application/json";

static ALL: LazyLock<Vec<Served>> = LazyLock::new(|| {
    let mut out = vec![];
    let deck = split(
        include_str!("../../../docs/schema/deck.schema.json"),
        ("scaena://schema/deck", "The deck format"),
        &[
            Part { name: "nodes", about: "The node types, and what each holds.", roots: &["/properties/nodes"] },
            Part {
                name: "deltas",
                about: "What a state sets on a node (`StateDelta`), prop by prop.",
                roots: &["/$defs/StateDelta"],
            },
        ],
        Some(Part {
            name: "values",
            about: "The values nodes and deltas share: colors, lengths, paints, styles.",
            roots: &[],
        }),
    );
    out.push(patch(include_str!("../../../docs/schema/patch.schema.json"), &deck));
    out.extend(deck.served);
    out.extend(
        split(
            include_str!("../../../docs/schema/theme.schema.json"),
            ("scaena://schema/theme", "The theme format"),
            &[
                Part {
                    name: "charts",
                    about: "How a theme draws charts and tables.",
                    roots: &["/properties/charts", "/properties/tables"],
                },
                Part { name: "shaders", about: "A theme's shader presets.", roots: &["/properties/shaders"] },
                Part {
                    name: "motion",
                    about: "A theme's motion: durations, curves, presets.",
                    roots: &["/properties/motion"],
                },
            ],
            None,
        )
        .served,
    );
    out.push(whole(
        include_str!("../../../docs/schema/spine.schema.json"),
        ("scaena://schema/spine", "The spine projection: what `spine.json` holds"),
    ));
    out.push(listed("scaena://lint/catalog", "The lint catalog", MARKDOWN, catalog()));
    out.extend(spec(include_str!("../../../docs/SPEC.md")));
    out.push(listed(
        "scaena://spec/format",
        "Number and date formats: `format` and `parse`",
        MARKDOWN,
        include_str!("../../../docs/spec/format.md"),
    ));
    out.push(listed(
        "scaena://spec/expr",
        "Data expressions: `dataTransform`'s `filter` and `calculate`",
        MARKDOWN,
        include_str!("../../../docs/spec/expr.md"),
    ));
    for (name, about, text) in [
        ("author-deck", "How to author a deck", include_str!("../../../skills/author-deck/SKILL.md")),
        (
            "chart-from-data",
            "How to make a chart or table from data",
            include_str!("../../../skills/chart-from-data/SKILL.md"),
        ),
        ("motion-pass", "How to set a deck's motion", include_str!("../../../skills/motion-pass/SKILL.md")),
        ("retheme", "How to apply another theme", include_str!("../../../skills/retheme/SKILL.md")),
        ("tighten-copy", "How to tighten a deck's words", include_str!("../../../skills/tighten-copy/SKILL.md")),
    ] {
        out.push(listed(&format!("scaena://skills/{name}"), about, MARKDOWN, text));
    }
    for (name, about, mime, text) in [
        ("revenue.deck.json", "An example deck", JSON, include_str!("../../../docs/examples/revenue.deck.json")),
        (
            "trails.deck.json",
            "A fifteen-slide example: text, a stat, a photograph, five kinds of chart, a table, cards, and a quote",
            JSON,
            include_str!("../../../docs/examples/trails.deck.json"),
        ),
        (
            "charts.deck.json",
            "Every kind of chart, and a table, with no style set: what a theme draws",
            JSON,
            include_str!("../../../docs/examples/charts.deck.json"),
        ),
        (
            "revenue.deck.scn",
            "The example deck as .scn",
            "text/plain",
            include_str!("../../../docs/examples/revenue.deck.scn"),
        ),
        ("revenue.patch.json", "An example patch", JSON, include_str!("../../../docs/examples/revenue.patch.json")),
        ("dusk.theme.json", "A theme: Dusk, dark", JSON, include_str!("../../../docs/examples/themes/dusk.theme.json")),
        (
            "daybreak.theme.json",
            "A theme: Daybreak, Dusk's light twin",
            JSON,
            include_str!("../../../docs/examples/authorability/themes/daybreak.theme.json"),
        ),
        (
            "ember.theme.json",
            "A theme: Ember, near-black with one signal orange",
            JSON,
            include_str!("../../../docs/examples/themes/ember.theme.json"),
        ),
    ] {
        let text = if mime == JSON { compact(text) } else { text.to_string() };
        out.push(listed(&format!("scaena://examples/{name}"), about, mime, &text));
    }
    out
});

/// A JSON example without its whitespace, as the schemas are served: the same document in
/// fewer tokens. Printed as its file is, the trails deck would not arrive whole.
fn compact(text: &str) -> String {
    let doc: Value = serde_json::from_str(text).expect("an example is JSON");
    serde_json::to_string(&doc).expect("JSON prints")
}

fn listed(uri: &str, name: &str, mime: &'static str, text: &str) -> Served {
    Served { uri: uri.into(), name: name.into(), mime, text: text.into(), listed: true }
}

/// The lint catalog: SPEC §7.5, as SPEC writes it.
fn catalog() -> &'static str {
    let spec = include_str!("../../../docs/SPEC.md");
    let start = spec.find("### 7.5").unwrap_or(0);
    let end = spec[start..].find("### 7.6").map_or(spec.len(), |i| start + i);
    &spec[start..end]
}

/// How many bytes `resources/read` returns for a text, at most: the text as a JSON string,
/// and room for its uri, its MIME type, and the cache hints.
fn weight(text: &str) -> usize {
    serde_json::to_string(text).map_or(usize::MAX, |s| s.len()) + 256
}

// --- the schemas, in parts ------------------------------------------------------------

/// A part of a schema: its name, the last segment of its uri; what it holds; and the JSON
/// pointers whose definitions it takes.
struct Part {
    name: &'static str,
    about: &'static str,
    roots: &'static [&'static str],
}

/// A schema, served in parts.
struct Split {
    served: Vec<Served>,
    /// The uri of the part that holds each definition.
    home: BTreeMap<String, String>,
    defs: Map<String, Value>,
}

/// Serves a schema in parts. A part takes the definitions its roots reach and no other
/// part's roots do; one that two parts reach goes to `shared` when there is one, and the
/// root keeps the rest. Each part is a JSON Schema whose `$id` is its uri, and a `$ref` to a
/// definition another part holds names that part's uri.
fn split(text: &str, (uri, name): (&str, &str), parts: &[Part], shared: Option<Part>) -> Split {
    let schema: Map<String, Value> = serde_json::from_str(text).expect("the schema is JSON");
    let defs = schema.get("$defs").and_then(Value::as_object).cloned().unwrap_or_default();
    let whole = Value::Object(schema.clone());
    let reach: Vec<BTreeSet<String>> = parts
        .iter()
        .map(|p| {
            let mut seen = BTreeSet::new();
            for root in p.roots {
                if let Some(def) = root.strip_prefix("/$defs/") {
                    visit(def, &defs, &mut seen);
                } else {
                    for def in refs(whole.pointer(root).expect("a part's root is in its schema")) {
                        visit(&def, &defs, &mut seen);
                    }
                }
            }
            seen
        })
        .collect();
    let at = |p: &Part| format!("{uri}/{}", p.name);
    let home: BTreeMap<String, String> = defs
        .keys()
        .map(|def| {
            let owners: Vec<&Part> =
                parts.iter().zip(&reach).filter(|(_, r)| r.contains(def)).map(|(p, _)| p).collect();
            let part = match owners[..] {
                [] => None,
                [one] => Some(one),
                _ => shared.as_ref(),
            };
            (def.clone(), part.map_or_else(|| uri.to_string(), at))
        })
        .collect();
    let held = |part: &str| -> Map<String, Value> {
        defs.iter().filter(|(d, _)| home[*d] == part).map(|(d, v)| (d.clone(), v.clone())).collect()
    };
    let all: Vec<&Part> = parts.iter().chain(shared.as_ref()).collect();
    let canonical = schema.get("$id").and_then(Value::as_str).unwrap_or(uri);
    let comment = format!(
        "{canonical}, served in parts, each a resource: this one, {}. A $ref names the part that holds its definition.",
        all.iter().map(|p| at(p)).collect::<Vec<_>>().join(", ")
    );
    let mut docs = vec![(uri.to_string(), name.to_string(), rooted(&schema, uri, comment, held(uri)))];
    let title = schema.get("title").and_then(Value::as_str).unwrap_or(name);
    for p in all {
        let part = at(p);
        let doc = json!({
            "$schema": schema.get("$schema"),
            "$id": part,
            "title": format!("{title}: {}", p.name),
            "description": format!("{} A part of {uri}.", p.about),
            "$defs": held(&part),
        });
        docs.push((part, format!("{name}: {}", p.name), doc));
    }
    let served = docs
        .into_iter()
        .map(|(part, name, mut doc)| {
            relink(&mut doc, &|def| home.get(def).filter(|h| **h != part).cloned());
            Served {
                uri: part,
                name,
                mime: SCHEMA,
                text: serde_json::to_string(&doc).expect("a schema prints"),
                listed: true,
            }
        })
        .collect();
    Split { served, home, defs }
}

/// The patch schema: its ops, and by reference the deck's definitions it shares.
fn patch(text: &str, deck: &Split) -> Served {
    let schema: Map<String, Value> = serde_json::from_str(text).expect("the schema is JSON");
    let defs = schema.get("$defs").and_then(Value::as_object).cloned().unwrap_or_default();
    // A definition the deck has too, word for word, is the deck's.
    let own: Map<String, Value> = defs.into_iter().filter(|(d, v)| deck.defs.get(d.as_str()) != Some(v)).collect();
    let canonical = schema.get("$id").and_then(Value::as_str).unwrap_or_default();
    let comment = format!(
        "{canonical}, with the definitions it shares with the deck by reference: a $ref names the part of \
         scaena://schema/deck that holds its definition."
    );
    let mut doc = rooted(&schema, "scaena://schema/patch", comment, own.clone());
    relink(&mut doc, &|def| if own.contains_key(def) { None } else { deck.home.get(def).cloned() });
    Served {
        uri: "scaena://schema/patch".into(),
        name: "A patch's ops".into(),
        mime: SCHEMA,
        text: serde_json::to_string(&doc).expect("a schema prints"),
        listed: true,
    }
}

/// A schema small enough to serve whole, its `$id` its uri as the parts' are.
fn whole(text: &str, (uri, name): (&str, &str)) -> Served {
    let schema: Map<String, Value> = serde_json::from_str(text).expect("the schema is JSON");
    let defs = schema.get("$defs").and_then(Value::as_object).cloned().unwrap_or_default();
    let canonical = schema.get("$id").and_then(Value::as_str).unwrap_or_default();
    let doc = rooted(&schema, uri, format!("{canonical}, served whole."), defs);
    Served {
        uri: uri.into(),
        name: name.into(),
        mime: SCHEMA,
        text: serde_json::to_string(&doc).expect("a schema prints"),
        listed: true,
    }
}

/// A schema's root as a part: its `$id` the part's uri, with a `$comment` after it, and
/// `$defs` the definitions it holds.
fn rooted(schema: &Map<String, Value>, uri: &str, comment: String, defs: Map<String, Value>) -> Value {
    let mut out = Map::new();
    for (k, v) in schema {
        match k.as_str() {
            "$defs" => {}
            "$id" => {
                out.insert(k.clone(), json!(uri));
                out.insert("$comment".into(), json!(comment));
            }
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out.insert("$defs".into(), Value::Object(defs));
    Value::Object(out)
}

/// The definitions a value names in its `$ref`s.
fn refs(v: &Value) -> BTreeSet<String> {
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::Object(o) => {
                if let Some(def) = o.get("$ref").and_then(Value::as_str).and_then(|r| r.strip_prefix("#/$defs/")) {
                    out.insert(def.to_string());
                }
                o.values().for_each(|v| walk(v, out));
            }
            Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(v, &mut out);
    out
}

/// A definition, and every one it reaches.
fn visit(def: &str, defs: &Map<String, Value>, seen: &mut BTreeSet<String>) {
    if seen.insert(def.to_string())
        && let Some(v) = defs.get(def)
    {
        for next in refs(v) {
            visit(&next, defs, seen);
        }
    }
}

/// Points each `$ref` at the part that holds its definition: `elsewhere` gives that part's
/// uri, or `None` for a definition the document holds itself.
fn relink(v: &mut Value, elsewhere: &dyn Fn(&str) -> Option<String>) {
    match v {
        Value::Object(o) => {
            if let Some(Value::String(r)) = o.get_mut("$ref")
                && let Some(def) = r.strip_prefix("#/$defs/")
                && let Some(part) = elsewhere(def)
            {
                *r = format!("{part}#/$defs/{def}");
            }
            o.values_mut().for_each(|v| relink(v, elsewhere));
        }
        Value::Array(a) => a.iter_mut().for_each(|v| relink(v, elsewhere)),
        _ => {}
    }
}

// --- SPEC, by section -----------------------------------------------------------------

/// A numbered heading of SPEC: its number (`3`, `3.7`), its title, and its text, the
/// heading's line first.
struct Heading {
    number: String,
    title: String,
    text: String,
}

/// A section of SPEC: its heading, its numbered subsections, and how much of its text comes
/// before the first of them.
struct Section {
    heading: Heading,
    subsections: Vec<Heading>,
    intro: usize,
}

/// What comes before SPEC's first section, and its sections.
fn sections(spec: &str) -> (String, Vec<Section>) {
    let mut preamble = String::new();
    let mut out: Vec<Section> = vec![];
    let mut fenced = false;
    for line in spec.split_inclusive('\n') {
        if line.starts_with("```") {
            fenced = !fenced;
        }
        if !fenced && let Some((number, title)) = numbered(line, "## ") {
            let text = line.to_string();
            out.push(Section { intro: text.len(), heading: Heading { number, title, text }, subsections: vec![] });
            continue;
        }
        let Some(section) = out.last_mut() else {
            preamble.push_str(line);
            continue;
        };
        section.heading.text.push_str(line);
        if !fenced
            && let Some((number, title)) = numbered(line, "### ")
            && number.starts_with(&format!("{}.", section.heading.number))
        {
            section.subsections.push(Heading { number, title, text: line.into() });
        } else if let Some(sub) = section.subsections.last_mut() {
            sub.text.push_str(line);
        } else {
            section.intro = section.heading.text.len();
        }
    }
    (preamble, out)
}

/// A heading's number and title, when the line is a numbered heading at this level:
/// `## 3. Document model`, `### 3.7 Charts`.
fn numbered(line: &str, level: &str) -> Option<(String, String)> {
    let (number, title) = line.strip_prefix(level)?.trim_end().split_once(' ')?;
    let number = number.trim_end_matches('.');
    number
        .split('.')
        .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .then(|| (number.to_string(), title.trim().to_string()))
}

/// SPEC by section: `scaena://spec` is its index, `scaena://spec/3` its §3, and
/// `scaena://spec/3.7` its §3.7. A section too large to arrive whole holds its text up to
/// its first subsection, then the uris of its subsections, which are listed too.
fn spec(text: &str) -> Vec<Served> {
    let (preamble, sections) = sections(text);
    let mut index = preamble.trim_end().to_string();
    index.push_str(
        "\n\nThis is the index of the specification. Each section is a resource, and so is each numbered \
         subsection: `scaena://spec/3.7` is §3.7.\n\n",
    );
    let mut out = vec![];
    for Section { heading, subsections, intro } in &sections {
        let uri = format!("scaena://spec/{}", heading.number);
        index.push_str(&format!("- §{} {}: `{uri}`\n", heading.number, heading.title));
        for sub in subsections {
            index.push_str(&format!("  - §{} {}: `scaena://spec/{}`\n", sub.number, sub.title, sub.number));
        }
        let whole = weight(&heading.text) <= LIMIT || subsections.is_empty();
        let text = if whole {
            heading.text.clone()
        } else {
            let mut text = heading.text[..*intro].to_string();
            text.push_str("This section is served by subsection:\n\n");
            for sub in subsections {
                text.push_str(&format!("- §{} {}: `scaena://spec/{}`\n", sub.number, sub.title, sub.number));
            }
            text
        };
        out.push(Served {
            uri,
            name: format!("SPEC §{} {}", heading.number, heading.title),
            mime: MARKDOWN,
            text,
            listed: true,
        });
        out.extend(subsections.iter().map(|sub| Served {
            uri: format!("scaena://spec/{}", sub.number),
            name: format!("SPEC §{} {}", sub.number, sub.title),
            mime: MARKDOWN,
            text: sub.text.clone(),
            listed: !whole,
        }));
    }
    out.insert(
        0,
        listed("scaena://spec", "The specification: its index, and a resource per section", MARKDOWN, &index),
    );
    out
}
