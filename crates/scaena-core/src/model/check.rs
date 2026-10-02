//! A document checked against its generated schema (PLAN 1.2).
//!
//! The generated schemas use a handful of JSON Schema 2020-12 keywords, and this checks
//! exactly those; a test fails if the schemas start using another. Knowing the schemas'
//! shape lets a violation say what the author meant. A node is checked as its own `type`,
//! so a chart's mistake reads as a chart's, not as "matches none of nine node types". An
//! unknown property names the node types that have it, or the closest known name.

use super::{deck_schema, theme_schema};
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};

/// The keywords this checker understands. Annotations (`description`, `default`, `format`,
/// …) say nothing about validity.
pub const KEYWORDS: &[&str] = &[
    "$ref",
    "type",
    "const",
    "enum",
    "pattern",
    "minLength",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "items",
    "minItems",
    "maxItems",
    "uniqueItems",
    "required",
    "properties",
    "additionalProperties",
    "unevaluatedProperties",
    "propertyNames",
    "minProperties",
    "anyOf",
    "oneOf",
    "if",
    "then",
];

/// The annotation keywords the generated schemas carry.
pub const ANNOTATIONS: &[&str] = &["$schema", "$id", "$defs", "title", "description", "default", "format"];

/// One way a document breaks its schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// JSON pointer to the offending value; for a missing property, to where it belongs.
    pub path: String,
    pub kind: Kind,
    pub message: String,
    /// The innermost named definition the value failed, such as `Id` or `TextNode`.
    pub def: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A property the schema does not have (`key` is its name).
    Unknown,
    /// A required property is absent.
    Missing,
    /// The wrong kind of JSON value.
    Type,
    /// A value outside what the schema allows: constant, enumeration, pattern, range, or
    /// length.
    Value,
    /// Too few or too many items or entries.
    Count,
    /// An item that appears twice in a list of unique items.
    Repeated,
    /// An object key that is not a valid name (`propertyNames`).
    Name,
    /// A value that matches none of a union's forms, or more than one of a `oneOf`.
    Form,
}

/// A generated schema, ready to check documents against.
pub struct Checker {
    root: Value,
    defs: Map<String, Value>,
    patterns: Mutex<HashMap<String, regex_lite::Regex>>,
}

impl Checker {
    /// `docs/schema/deck.schema.json`.
    pub fn deck() -> &'static Checker {
        static DECK: OnceLock<Checker> = OnceLock::new();
        DECK.get_or_init(|| Checker::new(deck_schema()))
    }

    /// `docs/schema/theme.schema.json`.
    pub fn theme() -> &'static Checker {
        static THEME: OnceLock<Checker> = OnceLock::new();
        THEME.get_or_init(|| Checker::new(theme_schema()))
    }

    fn new(root: Value) -> Checker {
        let defs = root["$defs"].as_object().cloned().unwrap_or_default();
        Checker { root, defs, patterns: Mutex::default() }
    }

    /// Every violation in `doc`.
    pub fn check(&self, doc: &Value) -> Vec<Violation> {
        self.run(&self.root, doc, "", None).errors
    }

    /// The violations of `value`, which sits at `path`, against the definition `def`: a
    /// resolved node against its type's (`TextNode`).
    pub fn check_def(&self, def: &str, value: &Value, path: &str) -> Vec<Violation> {
        assert!(self.defs.contains_key(def), "no definition `{def}`");
        self.run(&json!({ "$ref": format!("#/$defs/{def}") }), value, path, None).errors
    }

    /// The `type` tag of each node type, by definition name (`TextNode` → `text`).
    pub fn node_types(&self) -> Vec<(String, String)> {
        let branches = self.defs.get("Node").and_then(|n| n["oneOf"].as_array()).cloned().unwrap_or_default();
        self.tags(&branches)
            .map(|tags| {
                branches
                    .iter()
                    .filter_map(|b| b.get("$ref").and_then(Value::as_str))
                    .zip(tags)
                    .map(|(r, t)| (def_of(r).into(), t))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn run(&self, schema: &Value, value: &Value, path: &str, def: Option<&str>) -> Outcome {
        let mut out = Outcome::default();
        let schema = match schema {
            Value::Bool(true) => return out,
            Value::Bool(false) => {
                out.push(path, Kind::Value, def, "nothing is allowed here".into());
                return out;
            }
            Value::Object(schema) => schema,
            _ => panic!("a schema is an object or a boolean"),
        };
        if let Some(types) = schema.get("type")
            && !type_matches(types, value)
        {
            let message = format!("expected {}, found {}", type_names(types), kind_of(value));
            out.push(path, Kind::Type, def, message);
            return out;
        }
        if let Some(target) = schema.get("$ref").and_then(Value::as_str).map(def_of) {
            let target_schema = self.defs.get(target).unwrap_or_else(|| panic!("no $defs/{target}"));
            out.absorb(self.run(target_schema, value, path, Some(target)));
        }
        if let Some(constant) = schema.get("const")
            && !same(constant, value)
        {
            out.push(path, Kind::Value, def, format!("expected {}, found {}", show(constant), show(value)));
        }
        if let Some(Value::Array(options)) = schema.get("enum")
            && !options.iter().any(|o| same(o, value))
        {
            let options: Vec<String> = options.iter().map(plain).collect();
            out.push(path, Kind::Value, def, format!("{} is not one of: {}", show(value), options.join(", ")));
        }
        match value {
            Value::String(s) => self.strings(schema, s, path, def, &mut out),
            Value::Number(n) => numbers(schema, n.as_f64().unwrap_or(f64::NAN), path, def, &mut out),
            Value::Array(items) => self.arrays(schema, items, path, def, &mut out),
            Value::Object(map) => self.objects(schema, map, path, def, &mut out),
            _ => {}
        }
        for key in ["anyOf", "oneOf"] {
            if let Some(Value::Array(branches)) = schema.get(key) {
                self.union(key, branches, value, path, def, &mut out);
            }
        }
        if let Some(condition) = schema.get("if") {
            let condition = self.run(condition, value, path, def);
            if condition.errors.is_empty() {
                out.claimed.extend(condition.claimed);
                if let Some(then) = schema.get("then") {
                    out.absorb(self.run(then, value, path, def));
                }
            }
        }
        if schema.get("unevaluatedProperties") == Some(&Value::Bool(false))
            && let Value::Object(map) = value
        {
            let known = self.declared(&Value::Object(schema.clone()));
            let unclaimed: Vec<&String> = map.keys().filter(|k| !out.claimed.contains(*k)).collect();
            for key in unclaimed {
                let message = self.unknown(key, def, &known);
                out.push(&child(path, key), Kind::Unknown, def, message);
                out.claimed.insert(key.clone());
            }
        }
        out
    }

    fn strings(&self, schema: &Map<String, Value>, s: &str, path: &str, def: Option<&str>, out: &mut Outcome) {
        if let Some(pattern) = schema.get("pattern").and_then(Value::as_str)
            && !self.matches(pattern, s)
        {
            let message = match self.described(def) {
                Some((name, about)) => format!("{} is not a valid `{name}`: {about}", show(&Value::from(s))),
                None => format!("{} does not match `{pattern}`", show(&Value::from(s))),
            };
            out.push(path, Kind::Value, def, message);
        }
        if let Some(min) = schema.get("minLength").and_then(Value::as_u64)
            && (s.chars().count() as u64) < min
        {
            let message =
                if min == 1 { "must not be empty".to_string() } else { format!("needs at least {min} characters") };
            out.push(path, Kind::Value, def, message);
        }
    }

    fn arrays(&self, schema: &Map<String, Value>, items: &[Value], path: &str, def: Option<&str>, out: &mut Outcome) {
        if let Some(item) = schema.get("items") {
            for (i, value) in items.iter().enumerate() {
                out.errors.extend(self.run(item, value, &child(path, &i.to_string()), None).errors);
            }
        }
        if let Some(min) = schema.get("minItems").and_then(Value::as_u64)
            && (items.len() as u64) < min
        {
            out.push(
                path,
                Kind::Count,
                def,
                format!("needs at least {min} {}, has {}", plural(min, "item"), items.len()),
            );
        }
        if let Some(max) = schema.get("maxItems").and_then(Value::as_u64)
            && (items.len() as u64) > max
        {
            out.push(
                path,
                Kind::Count,
                def,
                format!("takes at most {max} {}, has {}", plural(max, "item"), items.len()),
            );
        }
        if schema.get("uniqueItems") == Some(&Value::Bool(true)) {
            for (i, value) in items.iter().enumerate() {
                if items[..i].iter().any(|earlier| same(earlier, value)) {
                    out.push(
                        &child(path, &i.to_string()),
                        Kind::Repeated,
                        def,
                        format!("{} appears more than once", show(value)),
                    );
                }
            }
        }
    }

    fn objects(
        &self,
        schema: &Map<String, Value>,
        map: &Map<String, Value>,
        path: &str,
        def: Option<&str>,
        out: &mut Outcome,
    ) {
        if let Some(Value::Array(required)) = schema.get("required") {
            for key in required.iter().filter_map(Value::as_str).filter(|k| !map.contains_key(*k)) {
                out.push(&child(path, key), Kind::Missing, def, format!("missing `{key}`"));
            }
        }
        if let Some(names) = schema.get("propertyNames") {
            for key in map.keys() {
                for v in self.run(names, &Value::from(key.as_str()), &child(path, key), None).errors {
                    out.errors.push(Violation { kind: Kind::Name, ..v });
                }
            }
        }
        if let Some(min) = schema.get("minProperties").and_then(Value::as_u64)
            && (map.len() as u64) < min
        {
            out.push(
                path,
                Kind::Count,
                def,
                format!("needs at least {min} {}, has {}", plural(min, "entry"), map.len()),
            );
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        for (key, value) in map {
            if let Some(property) = properties.and_then(|p| p.get(key)) {
                out.claimed.insert(key.clone());
                out.errors.extend(self.run(property, value, &child(path, key), None).errors);
                continue;
            }
            match schema.get("additionalProperties") {
                Some(Value::Bool(false)) => {
                    let known = self.declared(&Value::Object(schema.clone()));
                    let message = self.unknown(key, def, &known);
                    out.push(&child(path, key), Kind::Unknown, def, message);
                    out.claimed.insert(key.clone());
                }
                Some(extra) => {
                    out.claimed.insert(key.clone());
                    out.errors.extend(self.run(extra, value, &child(path, key), None).errors);
                }
                None => {}
            }
        }
    }

    /// `anyOf` or `oneOf`. When no form matches, the violations reported are those of the
    /// form the author most likely meant: the node type the value names, else the one form
    /// that takes this kind of value, else the object form closest to matching.
    fn union(&self, key: &str, branches: &[Value], value: &Value, path: &str, def: Option<&str>, out: &mut Outcome) {
        let results: Vec<Outcome> = branches.iter().map(|b| self.run(b, value, path, def)).collect();
        let passing: Vec<&Outcome> = results.iter().filter(|r| r.errors.is_empty()).collect();
        if key == "oneOf" && passing.len() > 1 {
            out.push(path, Kind::Form, def, format!("{} matches more than one form of {}", show(value), name_of(def)));
            return;
        }
        if !passing.is_empty() {
            for result in passing {
                out.claimed.extend(result.claimed.iter().cloned());
            }
            return;
        }
        if let Some(tags) = self.tags(branches) {
            let tag = value.get("type");
            match tag.and_then(Value::as_str).and_then(|t| tags.iter().position(|x| x == t)) {
                Some(i) => out.absorb(results.into_iter().nth(i).expect("a result per branch")),
                None if tag.is_none() => {
                    out.push(path, Kind::Missing, def, format!("missing `type`: one of {}", tags.join(", ")))
                }
                None => {
                    let message = format!(
                        "{} is not a node type: one of {}",
                        show(&tag.cloned().unwrap_or_default()),
                        tags.join(", ")
                    );
                    out.push(&child(path, "type"), Kind::Value, def, message);
                }
            }
            return;
        }
        let fitting: Vec<usize> = (0..branches.len()).filter(|&i| self.takes(&branches[i], value)).collect();
        let best = match fitting.as_slice() {
            [only] => Some(*only),
            [] => None,
            many if value.is_object() => many.iter().copied().min_by_key(|&i| results[i].errors.len()),
            _ => None,
        };
        match best {
            Some(i) => out.absorb(results.into_iter().nth(i).expect("a result per branch")),
            None => {
                let options: Option<Vec<String>> =
                    branches.iter().map(|b| self.options(b)).collect::<Option<Vec<_>>>().map(|o| o.concat());
                let message = match (options, self.described(def)) {
                    (Some(options), _) => format!("{} is not one of: {}", show(value), options.join(", ")),
                    (None, Some((name, about))) => format!("{} is not a valid `{name}`: {about}", show(value)),
                    (None, None) => {
                        let forms: Vec<String> = branches.iter().map(|b| self.summary(b)).collect();
                        format!("{} is not {}", show(value), either(&forms))
                    }
                };
                out.push(path, Kind::Form, def, message);
            }
        }
    }

    /// Whether `schema` takes values of `value`'s JSON kind at all.
    fn takes(&self, schema: &Value, value: &Value) -> bool {
        let Some(schema) = schema.as_object() else { return schema != &Value::Bool(false) };
        if let Some(types) = schema.get("type")
            && !type_matches(types, value)
        {
            return false;
        }
        if let Some(constant) = schema.get("const")
            && kind_of(constant) != kind_of(value)
        {
            return false;
        }
        if let Some(Value::Array(options)) = schema.get("enum")
            && !options.iter().any(|o| kind_of(o) == kind_of(value))
        {
            return false;
        }
        if let Some(target) = schema.get("$ref").and_then(Value::as_str).map(def_of)
            && !self.takes(&self.defs[target], value)
        {
            return false;
        }
        ["anyOf", "oneOf"].iter().all(|key| match schema.get(*key) {
            Some(Value::Array(branches)) => branches.iter().any(|b| self.takes(b, value)),
            _ => true,
        })
    }

    /// The `type` tags of a union of node types, if `branches` is one.
    fn tags(&self, branches: &[Value]) -> Option<Vec<String>> {
        if branches.is_empty() {
            return None;
        }
        branches
            .iter()
            .map(|b| {
                let target = self.defs.get(def_of(b.get("$ref")?.as_str()?))?;
                target.get("properties")?.get("type")?.get("const")?.as_str().map(String::from)
            })
            .collect()
    }

    /// The property names `schema` declares, through its `$ref`s.
    fn declared(&self, schema: &Value) -> Vec<String> {
        let mut names: Vec<String> =
            schema.get("properties").and_then(Value::as_object).into_iter().flat_map(|p| p.keys().cloned()).collect();
        if let Some(target) = schema.get("$ref").and_then(Value::as_str).map(def_of) {
            names.extend(self.declared(&self.defs[target]));
        }
        names
    }

    /// Why `key` is unknown: which node types have it, or the known name it is closest to.
    fn unknown(&self, key: &str, def: Option<&str>, known: &[String]) -> String {
        if let Some(def) = def
            && let Some(this) = self.node_types().into_iter().find(|(d, _)| d == def).map(|(_, tag)| tag)
        {
            let owners: Vec<String> = self
                .node_types()
                .into_iter()
                .filter(|(d, _)| self.declared(&self.defs[d]).iter().any(|k| k == key))
                .map(|(_, tag)| tag)
                .collect();
            if !owners.is_empty() {
                return format!(
                    "`{key}` is not a property of {} node; {} nodes have it",
                    article(&this),
                    list(&owners)
                );
            }
        }
        let closest = known.iter().map(|k| (distance(k, key), k)).min();
        match closest {
            Some((d, k)) if d <= (key.chars().count() / 3).max(1) => {
                format!("unknown property `{key}`; did you mean `{k}`?")
            }
            _ => format!("unknown property `{key}`"),
        }
    }

    /// A definition's name and description, for a value that fails it whole.
    fn described<'a>(&'a self, def: Option<&'a str>) -> Option<(&'a str, &'a str)> {
        let def = def?;
        Some((def, self.defs.get(def)?.get("description")?.as_str()?))
    }

    fn matches(&self, pattern: &str, s: &str) -> bool {
        let mut patterns = self.patterns.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let regex = patterns.entry(pattern.to_string()).or_insert_with(|| {
            regex_lite::Regex::new(pattern).unwrap_or_else(|e| panic!("schema pattern `{pattern}`: {e}"))
        });
        regex.is_match(s)
    }

    /// Every value `schema` takes, when it takes a short list of them: an enumeration, a
    /// constant, `null`, or a reference to one of those.
    fn options(&self, schema: &Value) -> Option<Vec<String>> {
        let s = schema.as_object()?;
        if let Some(target) = s.get("$ref").and_then(Value::as_str).map(def_of) {
            return self.options(&self.defs[target]);
        }
        if let Some(constant) = s.get("const") {
            return Some(vec![plain(constant)]);
        }
        if let Some(Value::Array(options)) = s.get("enum") {
            return Some(options.iter().map(plain).collect());
        }
        match (s.get("type"), s.len()) {
            (Some(t), 1) if t == "null" => Some(vec!["null".into()]),
            _ => ["anyOf", "oneOf"].iter().find_map(|key| {
                let branches = s.get(*key)?.as_array()?;
                branches.iter().map(|b| self.options(b)).collect::<Option<Vec<_>>>().map(|o| o.concat())
            }),
        }
    }

    /// A few words for what `schema` takes.
    fn summary(&self, schema: &Value) -> String {
        let Some(s) = schema.as_object() else { return "anything".into() };
        if let Some(target) = s.get("$ref").and_then(Value::as_str).map(def_of) {
            return format!("{} `{target}`", if target.starts_with(['A', 'E', 'I', 'O', 'U']) { "an" } else { "a" });
        }
        if let Some(constant) = s.get("const") {
            return show(constant);
        }
        if let Some(Value::Array(options)) = s.get("enum") {
            return format!("one of {}", options.iter().map(plain).collect::<Vec<_>>().join(", "));
        }
        if let Some(pattern) = s.get("pattern").and_then(Value::as_str) {
            return format!("a string matching `{pattern}`");
        }
        s.get("type").map(type_names).unwrap_or_else(|| "anything".into())
    }
}

/// What checking a value against a schema found: violations, and the properties the schema
/// accounted for (for `unevaluatedProperties`). A property is accounted for even when its
/// value fails, so one mistake is reported once.
#[derive(Default)]
struct Outcome {
    errors: Vec<Violation>,
    claimed: BTreeSet<String>,
}

impl Outcome {
    fn push(&mut self, path: &str, kind: Kind, def: Option<&str>, message: String) {
        self.errors.push(Violation { path: path.into(), kind, message, def: def.map(String::from) });
    }

    fn absorb(&mut self, other: Outcome) {
        self.errors.extend(other.errors);
        self.claimed.extend(other.claimed);
    }
}

fn numbers(schema: &Map<String, Value>, n: f64, path: &str, def: Option<&str>, out: &mut Outcome) {
    let bound = |key: &str| schema.get(key).and_then(Value::as_f64);
    if let Some(min) = bound("minimum")
        && n < min
    {
        out.push(path, Kind::Value, def, format!("{} is below the minimum, {}", number(n), number(min)));
    }
    if let Some(max) = bound("maximum")
        && n > max
    {
        out.push(path, Kind::Value, def, format!("{} is above the maximum, {}", number(n), number(max)));
    }
    if let Some(min) = bound("exclusiveMinimum")
        && n <= min
    {
        out.push(path, Kind::Value, def, format!("{} must be above {}", number(n), number(min)));
    }
}

/// Whether a JSON Schema `type` (a name or a list of names) takes `value`. An integer is
/// any number without a fractional part.
fn type_matches(types: &Value, value: &Value) -> bool {
    let one = |t: &str| match t {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_f64().is_some_and(|n| n.fract() == 0.0),
        _ => false,
    };
    match types {
        Value::String(t) => one(t),
        Value::Array(ts) => ts.iter().filter_map(Value::as_str).any(one),
        _ => true,
    }
}

fn type_names(types: &Value) -> String {
    let name = |t: &str| match t {
        "null" => "null",
        "boolean" => "true or false",
        "object" => "an object",
        "array" => "an array",
        "string" => "a string",
        "number" => "a number",
        "integer" => "a whole number",
        _ => "something else",
    };
    match types {
        Value::String(t) => name(t).into(),
        Value::Array(ts) => {
            either(&ts.iter().filter_map(Value::as_str).map(|t| name(t).to_string()).collect::<Vec<_>>())
        }
        _ => "anything".into(),
    }
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "true or false",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// JSON equality, with numbers equal by value (`1` is `1.0`).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

fn def_of(reference: &str) -> &str {
    reference.strip_prefix("#/$defs/").unwrap_or_else(|| panic!("a $ref into $defs, not `{reference}`"))
}

/// `path` extended by one JSON-pointer token.
fn child(path: &str, token: &str) -> String {
    format!("{path}/{}", token.replace('~', "~0").replace('/', "~1"))
}

/// A value as a message quotes it, cut short if long.
fn show(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() <= 48 {
        format!("`{text}`")
    } else {
        format!("`{}…`", text.chars().take(45).collect::<String>())
    }
}

/// A string bare, anything else as JSON.
fn plain(value: &Value) -> String {
    value.as_str().map(String::from).unwrap_or_else(|| value.to_string())
}

fn number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

fn plural(n: u64, word: &str) -> String {
    match (n, word) {
        (1, _) => word.into(),
        (_, "entry") => "entries".into(),
        _ => format!("{word}s"),
    }
}

fn name_of(def: Option<&str>) -> String {
    def.map(|d| format!("`{d}`")).unwrap_or_else(|| "this value".into())
}

fn article(tag: &str) -> String {
    if tag.starts_with(['a', 'e', 'i', 'o', 'u']) { format!("an {tag}") } else { format!("a {tag}") }
}

/// `a`, `a or b`, `a, b, or c`.
fn either(items: &[String]) -> String {
    match items {
        [] => "anything".into(),
        [one] => one.clone(),
        [a, b] => format!("{a} or {b}"),
        [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
    }
}

/// `a`, `a and b`, `a, b, and c`.
fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// Levenshtein distance, for "did you mean".
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitute = previous + usize::from(ca != *cb);
            previous = row[j + 1];
            row[j + 1] = substitute.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[b.len()]
}
