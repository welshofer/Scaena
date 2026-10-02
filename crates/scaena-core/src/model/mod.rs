//! Typed views of the document (SPEC §3) and the JSON schemas generated from them
//! (PLAN 1.1).
//!
//! The runtime document keeps node properties as ordered JSON maps
//! ([`crate::document::Props`]): tracking merges deltas generically, the CRDT (PLAN 1.23)
//! stores maps, and the engine reads what it needs. The types here say what those maps may
//! hold: one struct per node type ([`TypedNode`]), the theme ([`Theme`]), and the values
//! they are made of. With the document's own skeleton types they generate
//! `docs/schema/deck.schema.json` and `theme.schema.json` ([`deck_schema`], [`theme_schema`],
//! written by [`print_schema`]); `tests/schemas.rs` holds the committed files to them.
//!
//! A state's delta is not a type of its own: its schema (`StateDelta`) is derived from the
//! node types, so a property a node type gains is one a delta may set.

pub mod check;
pub mod nodes;
pub mod states;
pub mod theme;
pub mod values;

pub use nodes::TypedNode;
pub use states::{ChoreoItem, Transition};
pub use theme::Theme;
pub use values::{Id, IdList};

use indexmap::IndexMap;
use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::borrow::Cow;

/// `docs/schema/deck.schema.json`.
pub fn deck_schema() -> Value {
    let mut generator = SchemaSettings::draft2020_12().into_generator();
    // Reached only through the shader node's `if`/`then`s, which the generator cannot see.
    shader_params(&mut generator);
    let schema = generator.into_root_schema_for::<crate::document::Deck>();
    let mut v = serde_json::to_value(schema).expect("a schema serializes");
    finish(&mut v, "deck", Some("scaena"), crate::FORMAT_VERSION);
    let defs = v["$defs"].as_object_mut().expect("the deck schema has $defs");
    // Both from the node types as generated, each variant with every property inline.
    let mut deltas = delta_defs(defs);
    let mut nodes = node_defs(defs);
    let in_blocks: Vec<String> = nodes.keys().chain(deltas.keys()).cloned().collect();
    let mut ordered = Map::new();
    for (name, def) in std::mem::take(defs) {
        match name.as_str() {
            "Node" => ordered.append(&mut nodes),
            "StateDelta" => ordered.append(&mut deltas),
            _ if in_blocks.contains(&name) => {} // written with its block
            _ => {
                ordered.insert(name, def);
            }
        }
    }
    *defs = ordered;
    v
}

/// Each shader kind's params, which a shader node and a theme preset type by kind.
fn shader_params(generator: &mut schemars::SchemaGenerator) {
    generator.subschema_for::<nodes::MeshParams>();
    generator.subschema_for::<nodes::GradientParams>();
    generator.subschema_for::<nodes::NoiseParams>();
    generator.subschema_for::<nodes::GrainParams>();
    generator.subschema_for::<nodes::ParticlesParams>();
}

/// `docs/schema/theme.schema.json`.
pub fn theme_schema() -> Value {
    let mut generator = SchemaSettings::draft2020_12().into_generator();
    shader_params(&mut generator);
    let schema = generator.into_root_schema_for::<Theme>();
    let mut v = serde_json::to_value(schema).expect("a schema serializes");
    finish(&mut v, "theme", Some("scaena-theme"), crate::THEME_FORMAT_VERSION);
    v
}

/// `docs/schema/patch.schema.json`: a patch's ops (SPEC §7.3), versioned with the deck
/// format. What an op carries (a node, a state, a delta, a data source, a theme) is typed
/// as `deck.schema.json` types it, those definitions copied in, so the schema stands alone
/// (an MCP tool's input schema must, PLAN 1.17).
pub fn patch_schema() -> Value {
    let generator = SchemaSettings::draft2020_12().into_generator();
    let schema = generator.into_root_schema_for::<crate::patch::Patch>();
    let mut v = serde_json::to_value(schema).expect("a schema serializes");
    finish(&mut v, "patch", None, crate::FORMAT_VERSION);
    let deck = deck_schema();
    let deck_defs = deck["$defs"].as_object().expect("the deck schema has $defs");
    let own = v["$defs"].as_object().expect("the patch schema has $defs");
    // The patch's own definitions, then the deck's, as the deck schema writes them.
    let mut defs: Map<String, Value> =
        own.iter().filter(|(name, _)| !deck_defs.contains_key(*name)).map(|(n, d)| (n.clone(), d.clone())).collect();
    defs.extend(deck_defs.iter().map(|(n, d)| (n.clone(), d.clone())));
    v["$defs"] = Value::Object(defs);
    // Only what the ops reach.
    let mut reached: Vec<String> = Vec::new();
    let mut todo = refs(&v, "$defs");
    while let Some(name) = todo.pop() {
        if !reached.contains(&name) {
            todo.extend(refs(&v["$defs"][&name], ""));
            reached.push(name);
        }
    }
    v["$defs"].as_object_mut().expect("$defs").retain(|name, _| reached.contains(name));
    v
}

/// The definitions `v` names by `$ref`, leaving out what is under the key `skip`.
fn refs(v: &Value, skip: &str) -> Vec<String> {
    match v {
        Value::Object(map) => map
            .iter()
            .filter(|(key, _)| *key != skip)
            .flat_map(|(key, value)| match (key.as_str(), ref_name(v)) {
                ("$ref", Some(name)) => vec![name.to_string()],
                _ => refs(value, skip),
            })
            .collect(),
        Value::Array(items) => items.iter().flat_map(|item| refs(item, skip)).collect(),
        _ => Vec::new(),
    }
}

/// `docs/schema/manifest.schema.json`.
pub fn manifest_schema() -> Value {
    let generator = SchemaSettings::draft2020_12().into_generator();
    let schema = generator.into_root_schema_for::<crate::document::Manifest>();
    let mut v = serde_json::to_value(schema).expect("a schema serializes");
    finish(&mut v, "manifest", Some("scaena"), crate::FORMAT_VERSION);
    v
}

/// The node types as named definitions, `Node` one of them, with the properties every type
/// has written once (`NodeProps`) rather than once per type. A definition only a type's
/// `then` names (a shader kind's params) follows that type.
fn node_defs(defs: &Map<String, Value>) -> Map<String, Value> {
    let node = defs["Node"].as_object().expect("Node is an object schema");
    let variants = node["oneOf"].as_array().expect("Node is one of the node types");
    let first = variants[0]["properties"].as_object().expect("a node type has properties");
    let shared: Map<String, Value> = first
        .iter()
        .filter(|(name, schema)| *name != "type" && variants.iter().all(|v| v["properties"].get(*name) == Some(schema)))
        .map(|(name, schema)| (name.clone(), schema.clone()))
        .collect();
    let mut types = Map::new();
    let mut refs = Vec::new();
    for variant in variants {
        let variant = variant.as_object().expect("a node type is an object schema");
        let tag = variant["properties"]["type"]["const"].as_str().expect("a node type has a `type` tag");
        let name = format!("{}{}Node", tag[..1].to_uppercase(), &tag[1..]);
        let mut properties = Map::new();
        properties.insert("type".into(), variant["properties"]["type"].clone());
        for (prop, schema) in variant["properties"].as_object().into_iter().flatten() {
            if prop != "type" && !shared.contains_key(prop) {
                properties.insert(prop.clone(), schema.clone());
            }
        }
        let mut def: Map<String, Value> = variant
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), "properties" | "additionalProperties"))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        def.insert("$ref".into(), json!("#/$defs/NodeProps"));
        def.insert("properties".into(), Value::Object(properties));
        // `additionalProperties` would not see the shared properties behind the `$ref`.
        def.insert("unevaluatedProperties".into(), json!(false));
        order_keys(&mut def);
        refs.push(json!({"$ref": format!("#/$defs/{name}")}));
        let then = def.get("then").cloned();
        types.insert(name, Value::Object(def));
        for prop in then.as_ref().and_then(|t| t["properties"].as_object()).into_iter().flat_map(|p| p.values()) {
            if let Some(param_def) = ref_name(prop) {
                types.insert(param_def.into(), defs[param_def].clone());
            }
        }
    }
    let mut out = Map::new();
    let mut node = node.clone();
    node.insert("oneOf".into(), Value::Array(refs));
    out.insert("Node".into(), Value::Object(node));
    out.insert(
        "NodeProps".into(),
        json!({
            "description": "The properties every node type has (SPEC §3.3); each type adds its own.",
            "type": "object",
            "properties": shared,
        }),
    );
    out.append(&mut types);
    out
}

/// `StateDelta`, the delta a state applies to one node (SPEC §2.2), derived from the node
/// types: every property any node type has, in any of its forms, or `null` to delete it. An
/// object value merges one level into the tracked one, so its keys are optional and `null`
/// deletes one; each named object form gets a `…Delta` definition that says so, returned
/// with `StateDelta`. Whether a property belongs to the node's type is checked on the
/// resolved snapshot (PLAN 1.2), where the type is known.
fn delta_defs(defs: &Map<String, Value>) -> Map<String, Value> {
    let mut partials = Partials { defs, out: Map::new() };
    let mut forms: IndexMap<String, Vec<Value>> = IndexMap::new();
    for variant in defs["Node"]["oneOf"].as_array().expect("Node is one of the node types") {
        for (name, schema) in resolve(variant, defs)["properties"].as_object().into_iter().flatten() {
            if name == "type" {
                continue;
            }
            let form = undescribed(partials.partial(schema));
            let seen = forms.entry(name.clone()).or_default();
            if !seen.contains(&form) {
                seen.push(form);
            }
        }
    }
    let properties: Map<String, Value> = forms
        .into_iter()
        .map(|(name, mut forms)| match forms.len() {
            1 => (name, nullable(forms.remove(0))),
            _ => {
                forms.push(json!({"type": "null"}));
                (name, json!({"anyOf": forms}))
            }
        })
        .collect();
    let mut out = Map::new();
    out.insert(
        "StateDelta".into(),
        json!({
            "description": "The delta a state applies to one node: any node type's property, or null to delete it. An object value merges one level into the tracked one (the …Delta forms), so its keys are optional and null deletes one. An empty delta shows the node with its defaults (SPEC §2.2). Whether a property belongs to the node's type is checked on the resolved state (PLAN 1.2).",
            "type": "object",
            "additionalProperties": false,
            "properties": properties,
        }),
    );
    out.append(&mut partials.out);
    out
}

/// The forms a delta gives object values in, as `…Delta` definitions named after the
/// definition each one relaxes.
struct Partials<'a> {
    defs: &'a Map<String, Value>,
    out: Map<String, Value>,
}

impl Partials<'_> {
    /// `schema` as a delta may give it: an object's keys optional and nullable, a map's
    /// values nullable, a union's branches likewise. Any other value replaces the tracked
    /// one whole, so its schema stands.
    fn partial(&mut self, schema: &Value) -> Value {
        if !self.merges(schema) {
            return schema.clone();
        }
        let Some(name) = ref_name(schema) else {
            return self.relax(schema);
        };
        let delta = format!("{name}Delta");
        if !self.out.contains_key(&delta) {
            self.out.insert(delta.clone(), Value::Null); // its place, ahead of the forms it names
            let defs = self.defs;
            let target = &defs[name];
            let mut form = self.relax(target);
            form["description"] = json!(if union(target).is_some() {
                format!(
                    "`{name}` in a state's delta: a whole value, or an object that merges one level into the tracked one (SPEC §2.2)."
                )
            } else if target.get("properties").is_some() {
                format!(
                    "`{name}` in a state's delta: it merges one level into the tracked value, so every key is optional and null deletes one (SPEC §2.2)."
                )
            } else {
                format!(
                    "`{name}` in a state's delta: it merges one level into the tracked value, so null deletes a key (SPEC §2.2)."
                )
            });
            let mut form = form.as_object().cloned().expect("a delta form is an object schema");
            order_keys(&mut form);
            self.out.insert(delta.clone(), Value::Object(form));
        }
        json!({"$ref": format!("#/$defs/{delta}")})
    }

    /// Whether a delta merges into values of `schema` rather than replacing them: an object
    /// with known keys, a map, or a union with either among its branches.
    fn merges(&self, schema: &Value) -> bool {
        let schema = resolve(schema, self.defs);
        match union(schema) {
            Some((_, branches)) => branches.iter().any(|b| self.merges(b)),
            None => {
                is_object(schema)
                    && (schema.get("properties").is_some()
                        || schema.get("additionalProperties").is_some_and(Value::is_object))
            }
        }
    }

    fn relax(&mut self, schema: &Value) -> Value {
        let schema = resolve(schema, self.defs);
        if let Some((_, branches)) = union(schema) {
            // Relaxed branches may overlap, so one of them no longer says enough.
            let mut out = schema.as_object().cloned().expect("a union is an object schema");
            out.remove("oneOf");
            out.insert("anyOf".into(), branches.iter().map(|b| self.partial(b)).collect());
            return Value::Object(out);
        }
        let mut out = schema.as_object().cloned().expect("an object schema");
        // What a whole value needs is not what a delta to one needs.
        for key in ["required", "minProperties", "anyOf", "if", "then"] {
            out.remove(key);
        }
        if let Some(Value::Object(props)) = out.get_mut("properties") {
            for value in props.values_mut() {
                *value = nullable(value.take());
            }
        }
        if let Some(extra @ Value::Object(_)) = out.get_mut("additionalProperties") {
            *extra = nullable(extra.take());
        }
        Value::Object(out)
    }
}

/// `schema` or `null`.
fn nullable(schema: Value) -> Value {
    let schema = undescribed(schema);
    match schema.as_object().map(|s| (s.len(), s.get("type"))) {
        Some((1, Some(Value::String(t)))) => json!({"type": [t, "null"]}),
        _ => json!({"anyOf": [schema, {"type": "null"}]}),
    }
}

/// A delta's forms leave their descriptions to the node types they come from.
fn undescribed(mut schema: Value) -> Value {
    if let Some(obj) = schema.as_object_mut() {
        obj.remove("description");
    }
    schema
}

/// A union's keyword and branches; an object schema whose `anyOf` names the keys it needs
/// (a text node's `text` or `runs`) is not one.
fn union(schema: &Value) -> Option<(&'static str, &Vec<Value>)> {
    if schema.get("properties").is_some() {
        return None;
    }
    ["anyOf", "oneOf"].into_iter().find_map(|key| schema.get(key).and_then(Value::as_array).map(|b| (key, b)))
}

fn ref_name(schema: &Value) -> Option<&str> {
    schema.get("$ref").and_then(Value::as_str).and_then(|r| r.strip_prefix("#/$defs/"))
}

/// `schema` with a `$ref` to a definition followed.
fn resolve<'a>(schema: &'a Value, defs: &'a Map<String, Value>) -> &'a Value {
    match ref_name(schema) {
        Some(name) => defs.get(name).map_or(schema, |d| resolve(d, defs)),
        None => schema,
    }
}

fn is_object(schema: &Value) -> bool {
    schema.get("type").and_then(Value::as_str) == Some("object")
}

/// What schemars writes that the document format does not say: numeric `format`s, `null`
/// on every optional property (a node's absent property is absent, not null; only a delta
/// deletes with null, and `StateDelta` says so itself), and doc comments' line breaks and
/// links. Then the root's `$id` and its version key's pattern (a document that has one), both
/// from `version`, and every schema's keywords in one order.
fn finish(v: &mut Value, name: &str, version_key: Option<&str>, version: &str) {
    v["$id"] = json!(format!("https://scaena.dev/schema/{name}-{version}.json"));
    if let Some(key) = version_key {
        v["properties"][key]["pattern"] = json!(format!("^{}(\\.[0-9]+)?$", version.replace('.', "\\.")));
    }
    each_schema(v, &mut |schema| {
        if schema.get("format").and_then(Value::as_str).is_some_and(is_numeric_format) {
            schema.remove("format");
        }
        if let Some(Value::String(text)) = schema.get_mut("description") {
            *text = plain(text);
        }
        let required: Vec<String> = match schema.get("required") {
            Some(Value::Array(r)) => r.iter().filter_map(|k| k.as_str().map(str::to_string)).collect(),
            _ => Vec::new(),
        };
        if let Some(Value::Object(props)) = schema.get_mut("properties") {
            for (name, prop) in props.iter_mut() {
                if !required.contains(name) {
                    drop_null(prop);
                }
            }
        }
        order_keys(schema);
    });
}

/// `f` on every schema in `v`, parents before children; instance values (`const`, `enum`,
/// `default`) and maps of schemas are not schemas themselves.
fn each_schema(v: &mut Value, f: &mut impl FnMut(&mut Map<String, Value>)) {
    let Value::Object(schema) = v else { return };
    f(schema);
    for (key, child) in schema.iter_mut() {
        match key.as_str() {
            "properties" | "patternProperties" | "dependentSchemas" | "$defs" => {
                child.as_object_mut().into_iter().flat_map(|m| m.values_mut()).for_each(|s| each_schema(s, f));
            }
            "anyOf" | "oneOf" | "allOf" | "prefixItems" => {
                child.as_array_mut().into_iter().flatten().for_each(|s| each_schema(s, f));
            }
            "items"
            | "additionalProperties"
            | "unevaluatedProperties"
            | "propertyNames"
            | "contains"
            | "not"
            | "if"
            | "then"
            | "else" => each_schema(child, f),
            _ => {}
        }
    }
}

fn is_numeric_format(f: &str) -> bool {
    matches!(
        f,
        "double"
            | "float"
            | "int"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "uint"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
    )
}

/// A doc comment as plain text: each paragraph on one line, intra-doc links unlinked.
fn plain(text: &str) -> String {
    text.split("\n\n")
        .map(|paragraph| paragraph.lines().map(str::trim).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n\n")
        .replace("[`", "`")
        .replace("`]", "`")
}

/// `prop` without its `null`: from `"type": [T, "null"]`, or from an `anyOf` branch.
fn drop_null(prop: &mut Value) {
    let Some(obj) = prop.as_object_mut() else { return };
    if let Some(Value::Array(types)) = obj.get_mut("type") {
        types.retain(|t| t != "null");
        if types.len() == 1 {
            let only = types[0].clone();
            obj.insert("type".into(), only);
        }
    }
    if let Some(Value::Array(branches)) = obj.get("anyOf") {
        let kept: Vec<Value> = branches.iter().filter(|b| **b != json!({"type": "null"})).cloned().collect();
        if kept.len() < branches.len() {
            obj.remove("anyOf");
            if kept.len() == 1 {
                let only = kept.into_iter().next().expect("one branch");
                for (k, v) in only.as_object().cloned().unwrap_or_default() {
                    obj.entry(k).or_insert(v);
                }
            } else {
                obj.insert("anyOf".into(), Value::Array(kept));
            }
        }
    }
}

/// The order a schema's keywords are written in: what it is, then what it allows, then how
/// it combines; anything else after, as it was.
const KEYWORD_ORDER: &[&str] = &[
    "$schema",
    "$id",
    "title",
    "description",
    "$ref",
    "type",
    "const",
    "enum",
    "format",
    "pattern",
    "minimum",
    "exclusiveMinimum",
    "maximum",
    "exclusiveMaximum",
    "minLength",
    "maxLength",
    "default",
    "items",
    "prefixItems",
    "minItems",
    "maxItems",
    "uniqueItems",
    "propertyNames",
    "minProperties",
    "maxProperties",
    "required",
    "additionalProperties",
    "unevaluatedProperties",
    "properties",
    "anyOf",
    "oneOf",
    "allOf",
    "if",
    "then",
    "else",
    "$defs",
];

fn order_keys(schema: &mut Map<String, Value>) {
    let rank = |key: &str| KEYWORD_ORDER.iter().position(|k| *k == key).unwrap_or(KEYWORD_ORDER.len());
    let mut entries: Vec<(String, Value)> = std::mem::take(schema).into_iter().collect();
    entries.sort_by_key(|(key, _)| rank(key));
    schema.extend(entries);
}

/// A schema as the repository writes it: two-space indents, and any object or array whose
/// line fits in 120 columns on that line.
pub fn print_schema(schema: &Value) -> String {
    let mut out = String::new();
    print_value(schema, 0, 0, &mut out);
    out.push('\n');
    out
}

const WIDTH: usize = 120;

/// `v` at `indent`, its first line already `lead` columns in.
fn print_value(v: &Value, indent: usize, lead: usize, out: &mut String) {
    let flat = flat(v);
    let (open, close, entries): (char, char, Vec<(Option<&String>, &Value)>) = match v {
        Value::Object(map) if !map.is_empty() => ('{', '}', map.iter().map(|(k, v)| (Some(k), v)).collect()),
        Value::Array(items) if !items.is_empty() => ('[', ']', items.iter().map(|v| (None, v)).collect()),
        _ => (' ', ' ', Vec::new()),
    };
    // Short of the width by one: room for a trailing comma.
    if entries.is_empty() || lead + flat.chars().count() < WIDTH {
        out.push_str(&flat);
        return;
    }
    let pad = " ".repeat(indent + 2);
    out.push(open);
    out.push('\n');
    let last = entries.len() - 1;
    for (i, (key, value)) in entries.into_iter().enumerate() {
        out.push_str(&pad);
        let key = key.map(|k| format!("{}: ", Value::String(k.clone()))).unwrap_or_default();
        out.push_str(&key);
        print_value(value, indent + 2, indent + 2 + key.chars().count(), out);
        if i < last {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(&" ".repeat(indent));
    out.push(close);
}

/// `v` on one line: `{ "a": 1 }`, `["a", "b"]`.
fn flat(v: &Value) -> String {
    match v {
        Value::Object(map) if map.is_empty() => "{}".into(),
        Value::Object(map) => {
            let entries: Vec<String> =
                map.iter().map(|(k, v)| format!("{}: {}", Value::String(k.clone()), flat(v))).collect();
            format!("{{ {} }}", entries.join(", "))
        }
        Value::Array(items) => format!("[{}]", items.iter().map(flat).collect::<Vec<_>>().join(", ")),
        scalar => scalar.to_string(),
    }
}

/// Where the generated schema's `StateDelta` definition goes: a state's `props` refer to it
/// by name, and [`deck_schema`] writes it from the node types.
pub struct StateDeltaRef;

impl JsonSchema for StateDeltaRef {
    fn schema_name() -> Cow<'static, str> {
        "StateDelta".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({"type": "object"})
    }
}

/// Additional projections rendered from the same spine and nodes with other layout
/// template sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum Format {
    #[serde(rename = "16:9")]
    Wide,
    #[serde(rename = "4:3")]
    Standard,
    #[serde(rename = "9:16")]
    Tall,
    #[serde(rename = "1:1")]
    Square,
    A4,
    Letter,
}

impl Format {
    pub const ALL: [Format; 6] =
        [Format::Wide, Format::Standard, Format::Tall, Format::Square, Format::A4, Format::Letter];

    /// As a deck writes it: `16:9`, `A4`.
    pub fn name(self) -> &'static str {
        match self {
            Format::Wide => "16:9",
            Format::Standard => "4:3",
            Format::Tall => "9:16",
            Format::Square => "1:1",
            Format::A4 => "A4",
            Format::Letter => "Letter",
        }
    }

    /// The format a deck writes as `name`.
    pub fn parse(name: &str) -> Option<Format> {
        Format::ALL.into_iter().find(|f| f.name() == name)
    }

    /// Width over height. The paper sizes are upright: A4 is 210:297 and Letter 8.5:11.
    pub fn aspect(self) -> f64 {
        match self {
            Format::Wide => 16.0 / 9.0,
            Format::Standard => 4.0 / 3.0,
            Format::Tall => 9.0 / 16.0,
            Format::Square => 1.0,
            Format::A4 => 210.0 / 297.0,
            Format::Letter => 8.5 / 11.0,
        }
    }

    /// The canvas of a deck whose own canvas is `canvas` (`[width, height]`), in this
    /// format: the shorter side kept, the longer one set by the aspect, in whole canvas
    /// units. A 1920 × 1080 deck is 1080 × 1920 in `9:16`.
    pub fn canvas(self, canvas: [f64; 2]) -> [f64; 2] {
        let short = canvas[0].min(canvas[1]);
        match self.aspect() {
            a if a >= 1.0 => [(short * a).round(), short],
            a => [short, (short / a).round()],
        }
    }
}

/// A theme file inside the bundle, or an inline theme object (schema: theme.schema.json).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ThemeRef {
    Path(#[schemars(length(min = 1))] String),
    Inline(Map<String, Value>),
}

/// A data file in the bundle, or rows inline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SourceRef {
    Path(#[schemars(regex(pattern = r"^data/"))] String),
    Inline(InlineRows),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InlineRows {
    pub inline: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    Number,
    String,
    Date,
    Boolean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FontStyle {
    Normal,
    Italic,
}

#[cfg(test)]
mod tests {
    use super::Format;

    #[test]
    fn a_format_keeps_the_shorter_side_and_its_name_round_trips() {
        let hd = [1920.0, 1080.0];
        assert_eq!(Format::Wide.canvas(hd), hd);
        assert_eq!(Format::Tall.canvas(hd), [1080.0, 1920.0]);
        assert_eq!(Format::Standard.canvas(hd), [1440.0, 1080.0]);
        assert_eq!(Format::Square.canvas(hd), [1080.0, 1080.0]);
        assert_eq!(Format::A4.canvas(hd), [1080.0, 1527.0]);
        assert_eq!(Format::Letter.canvas(hd), [1080.0, 1398.0]);
        assert_eq!(Format::Wide.canvas([1080.0, 1920.0]), hd, "from an upright deck too");
        for f in Format::ALL {
            assert_eq!(Format::parse(f.name()), Some(f));
            assert_eq!(serde_json::to_value(f).unwrap(), f.name());
        }
    }
}
