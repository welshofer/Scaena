//! Patches (SPEC §7.3): JSON Patch (RFC 6902) over a deck's JSON, and the semantic ops that
//! compile to it.
//!
//! - RFC 6902's `add`, `remove`, `replace`, `move`, `copy`, and `test` ([`JsonOp`]), with
//!   JSON Pointer (RFC 6901) paths. A member moved within its object keeps its place, so a
//!   `move` there is a rename: the order of a deck's `nodes` is its paint order at equal `z`.
//! - Semantic ops ([`SemanticOp`]) say what an author means (add a node, set a property in a
//!   state, rename an id everywhere it is used) and compile to RFC 6902, each against the
//!   deck as the ops before it leave it ([`compile`]). The compiled patch is what a store
//!   records (SPEC §8).
//!
//! A patch applies as a whole or not at all: on any failure the document is left as it was.
//! Lint's fixes are RFC 6902 patches (SPEC §7.4, [`apply`]).

mod ops;

use crate::document::{DataSource, Node, Props, State};
use crate::model::values::{Range, Rect};
use crate::model::{Id, StateDeltaRef, ThemeRef};
use crate::validate::BundleFiles;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Why a patch did not apply: which op (from 0), and what was wrong with it.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("op {index}: {message}")]
pub struct PatchError {
    pub index: usize,
    pub message: String,
}

/// A patch (SPEC §7.3): RFC 6902 ops and semantic ops, applied in order, all or none.
// Its schema is `docs/schema/patch.schema.json` (`model::patch_schema`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
#[schemars(title = "Scaena patch")]
pub struct Patch(pub Vec<Op>);

/// One op: RFC 6902's, or a semantic op, which compiles to RFC 6902's.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum Op {
    Json(JsonOp),
    Semantic(Box<SemanticOp>),
}

impl<'de> Deserialize<'de> for Op {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Op, D::Error> {
        Op::from_value(Value::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

const JSON_OPS: [&str; 6] = ["add", "remove", "replace", "move", "copy", "test"];

impl Op {
    /// An op from its JSON, by its `op`. RFC 6902 ignores members an op does not define; a
    /// semantic op refuses them, so a misspelled `state` is an error rather than a change
    /// to the node's defaults.
    pub fn from_value(v: Value) -> Result<Op, String> {
        let name = v.get("op").and_then(Value::as_str).ok_or_else(|| "an op needs a string `op`".to_string())?;
        let op = if JSON_OPS.contains(&name) {
            serde_json::from_value(v).map(Op::Json)
        } else {
            serde_json::from_value(v).map(|op| Op::Semantic(Box::new(op)))
        };
        op.map_err(|e| e.to_string())
    }
}

/// A JSON Pointer (RFC 6901): empty, the whole document, or `/`-separated tokens, `~1` for a
/// `/` and `~0` for a `~` inside one.
#[derive(JsonSchema)]
#[allow(dead_code)] // the schema's
struct Pointer(#[schemars(regex(pattern = r"^(/.*)?$"))] String);

/// RFC 6902's ops.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum JsonOp {
    /// Put `value` at `path`: a member of an object (one already there is replaced, in its
    /// place), or an item of an array, at an index or at its end (`-`).
    Add {
        #[schemars(with = "Pointer")]
        path: String,
        value: Value,
    },
    /// Take away what is at `path`.
    Remove {
        #[schemars(with = "Pointer")]
        path: String,
    },
    /// Replace what is at `path`, which must be there.
    Replace {
        #[schemars(with = "Pointer")]
        path: String,
        value: Value,
    },
    /// `remove` at `from`, then `add` at `path`. A member moved within its object keeps its
    /// place: a rename.
    Move {
        #[schemars(with = "Pointer")]
        from: String,
        #[schemars(with = "Pointer")]
        path: String,
    },
    /// `add` at `path` what is at `from`.
    Copy {
        #[schemars(with = "Pointer")]
        from: String,
        #[schemars(with = "Pointer")]
        path: String,
    },
    /// Fail the patch unless `path` holds `value` (numbers equal by value: 1 is 1.0).
    Test {
        #[schemars(with = "Pointer")]
        path: String,
        value: Value,
    },
}

/// What an author means, compiled to RFC 6902 against the deck as the ops before it leave
/// it (SPEC §7.3). `state` names the state an op acts in, from it on; without one, an op
/// acts on the node's own properties, its defaults. An op on a node in a state needs the
/// node on screen there.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticOp {
    /// A node new to the scene graph, last in its order (painted over the others at its
    /// `z`). With `state`, it enters there, `props` its delta in that state.
    AddNode {
        #[schemars(with = "Id")]
        id: String,
        node: Node,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        state: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<StateDeltaRef>")]
        props: Option<Props>,
    },
    /// A node, and everything that names it: its deltas, its exits, its choreography, and
    /// its overrides. A container must be emptied first.
    RemoveNode {
        #[schemars(with = "Id")]
        id: String,
    },
    /// A node's id, everywhere it is used: the node keeps its place, its type, and every
    /// property, so it stays the same node to tracking and to a morph.
    RenameNode {
        #[schemars(with = "Id")]
        id: String,
        #[schemars(with = "Id")]
        to: String,
    },
    /// A node already in the scene graph enters in a state, `props` its delta there: one
    /// that left comes back.
    ShowNode {
        #[schemars(with = "Id")]
        node: String,
        #[schemars(with = "Id")]
        state: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<StateDeltaRef>")]
        props: Option<Props>,
    },
    /// A node leaves in a state: it exits there, or, if it would enter there, it does not.
    HideNode {
        #[schemars(with = "Id")]
        node: String,
        #[schemars(with = "Id")]
        state: String,
    },
    /// A new group, `id`, holding `nodes` where they stand (ADR-0008, ADR-0013). They are
    /// children of one container, the slide or a group, as `state` shows them (else as their
    /// own `at` places them), and take the group as their container wherever that one is
    /// written. A group lays nothing out, so nothing moves. It sits where they sat, at the
    /// highest `z` among them (0 for one that sets none), last among what stands there; and
    /// it shows in each state that shows one of them in it, and only there.
    Group {
        #[schemars(with = "Id")]
        id: String,
        #[schemars(with = "Vec<Id>", length(min = 1))]
        nodes: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        state: Option<String>,
    },
    /// A group's children taken out to the group's own container where they stand, each
    /// keeping its own `z`, and the group gone with everything that names it: its deltas, its
    /// exits, its choreography, and its overrides. Its look goes with it.
    Ungroup {
        #[schemars(with = "Id")]
        group: String,
    },
    /// One property (`kind`), or one key of an object property (`at/in`), set to `value`:
    /// in a state, as its delta, or in the node's defaults. `null` takes it away: from the
    /// node's defaults, or, in a state, from what the node tracks (SPEC §2.2).
    SetProp {
        #[schemars(with = "Id")]
        node: String,
        #[schemars(regex(pattern = r"^[^/]+(/[^/]+)?$"))]
        prop: String,
        value: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        state: Option<String>,
    },
    /// A node's place (ADR-0013), as a drag in an editor moves it: the keys of its `at` that
    /// say where it goes (`col`, `row`, `in`, `rect`, `area`, `index`) become those of `at`,
    /// and the rest of them go. They are written where the placement lives: in the deck's
    /// `overrides` if they set it; else in the latest delta that sets it, from `state` back
    /// along what it tracks; else in the node's own `at`. Without `state`, in the node's own
    /// unless the overrides set it. The node stays in its container (`at.parent`), which says
    /// what places it: the theme's grid places a root or a group's member by cells, slot, or
    /// `rect`; a grid container by cells or `area`; a stack by `index`; a frame by `rect`.
    Place {
        #[schemars(with = "Id")]
        node: String,
        at: Spot,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        state: Option<String>,
        /// Write it into `state`'s own props, wherever the placement lives now: the node goes
        /// there in that state, and in the states that track it as they take the rest of its
        /// props, and stays where it was in the others. Refused where the deck's `overrides`
        /// place the node, in every state.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        fork: bool,
    },
    /// A text node's `text`, in a state or in its defaults. `runs` there go, so the text
    /// is what shows.
    SetText {
        #[schemars(with = "Id")]
        node: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        state: Option<String>,
    },
    /// Text typed into a text node where it stands (ADR-0013): the characters from `from` to
    /// `to` of its text as `state` shows it (its `text`, or its runs' texts end to end, as the
    /// deck's `overrides` leave them) become `text`. Offsets count characters (Unicode scalar
    /// values). Runs keep their looks: what is typed takes the look of the run it is typed
    /// into, the one before where two meet, and a run the edit leaves with no text goes. It is
    /// written where the
    /// text lives: in the deck's `overrides` if they set it; else in the latest delta that
    /// sets it, from `state` back along what it tracks; else in the node's own. Without
    /// `state`, in the node's own unless the overrides set it.
    ReplaceText {
        #[schemars(with = "Id")]
        node: String,
        from: u32,
        to: u32,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        state: Option<String>,
        /// Write it into `state`'s own props, wherever the text lives now: it reads so there,
        /// and in the states that track it as they take the rest of its props, and as it did
        /// in the others. Refused where the deck's `overrides` set the text, in every state.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        fork: bool,
    },
    /// The look of characters selected in a text node (ADR-0013, PLAN 2.38): characters
    /// `from` to `to` of its text as `state` shows it, counted as `replace_text` counts them,
    /// take each key of `look`: a run's `role`, `emphasis`, or `lang`, or one key of its
    /// `style` (`style/weight`, `style/color`); a key set to `null` is taken away, so the
    /// node's own look shows there. The text becomes runs split at `from` and `to`; neighbors
    /// left alike are joined, and runs that all read as the node does are its `text` again.
    /// It is written where the text lives, as `replace_text` writes typing. A size, and a
    /// color written out, are refused: a size comes with a role, and a run takes the theme's
    /// names.
    StyleText {
        #[schemars(with = "Id")]
        node: String,
        from: u32,
        to: u32,
        #[schemars(extend("minProperties" = 1))]
        look: Props,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        state: Option<String>,
        /// Write it into `state`'s own props, wherever the text lives now, as `replace_text`'s
        /// `fork` does.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        fork: bool,
    },
    /// One property of a node (`prop`: a property, or one key of an object property,
    /// `style/color`), as an inspector chooses it (ADR-0013): `value` is written where the
    /// property lives. A value written out where the theme has names (a color, a text size, a
    /// length in canvas units: lint W300's) goes in the deck's `overrides`, the only place it
    /// is legal, and so does any value where the overrides set the property, since they win
    /// in every state. Else it goes in the latest delta that sets it, from `state` back along
    /// what it tracks, else in the node's own; without `state`, in the node's own. `null`
    /// takes it away where it lives, so what is under it shows.
    Choose {
        #[schemars(with = "Id")]
        node: String,
        #[schemars(regex(pattern = r"^[^/]+(/[^/]+)?$"))]
        prop: String,
        value: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        state: Option<String>,
        /// Write it into `state`'s own props, wherever it lives now: it shows there, and in
        /// the states that track it as they take the rest of its props, and the others show
        /// what they did. Refused for a value that goes in the deck's `overrides`, which hold
        /// in every state.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        fork: bool,
    },
    /// A chart or a table reads the data source `data` (`q4` or `@q4`), in a state (a data
    /// update, morphed by key) or in its defaults. `source` declares it, or replaces it if
    /// the deck has it.
    BindData {
        #[schemars(with = "Id")]
        node: String,
        #[schemars(regex(pattern = r"^@?[a-z][a-z0-9_-]{0,63}$"))]
        data: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<DataSource>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        state: Option<String>,
    },
    /// A theme preset. With `motion`, a motion preset becomes the node's `enter`, `exit`,
    /// or `emphasis`. Without, a shader takes a shader preset whole: the preset's kind, and
    /// none of its own `palette` or `params`, which would win over the preset's.
    ApplyPreset {
        #[schemars(with = "Id")]
        node: String,
        #[schemars(length(min = 1))]
        preset: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        motion: Option<Motion>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        state: Option<String>,
    },
    /// A state, `after` or `before` another, else last. With `beat`, it joins that beat's
    /// states.
    AddState {
        state: State,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        after: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        before: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        beat: Option<String>,
    },
    /// A state, moved `after` or `before` another. A state in delta mode tracks from the
    /// state before it, wherever that is now.
    MoveState {
        #[schemars(with = "Id")]
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        after: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "Option<Id>")]
        before: Option<String>,
    },
    /// A state, and its place in the spine's beats. The state after it tracks from the one
    /// before it. A state that others build on (`slide`) or track from (`from`) stays.
    RemoveState {
        #[schemars(with = "Id")]
        id: String,
    },
    /// A state's id, everywhere it is used: the states that build on it or track from it,
    /// and the spine's beats.
    RenameState {
        #[schemars(with = "Id")]
        id: String,
        #[schemars(with = "Id")]
        to: String,
    },
    /// One property of state `id`, as an inspector chooses it (PLAN 2.36): its `layout`, its
    /// `transition` or one key of it (`transition/ease`), its `hold`, or its `notes`. A layout
    /// tracks (SPEC §2.2), so it is written where it lives: in the latest state that sets it,
    /// from `id` back along what it tracks, and each state that takes it from there changes
    /// with it; in `id` where none sets it. The rest are `id`'s own. A transition that is a
    /// bare duration, or none, stays bare while its duration is all it sets, and becomes an
    /// object to take another key. `null` takes a value away where it lives: a layout taken
    /// away shows what is under it, and a transition left with nothing cuts.
    SetState {
        #[schemars(with = "Id")]
        id: String,
        #[schemars(regex(pattern = r"^(layout|hold|notes|transition(/(duration|ease|spring|match))?)$"))]
        prop: String,
        value: Value,
        /// Write a layout into `id` itself, wherever it lives now: it shows there, and in the
        /// states that track `id`, and the others show what they did. The rest are `id`'s own
        /// with or without it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        fork: bool,
    },
    /// The deck's theme: a theme file in the bundle (`scaena theme --apply` copies one in),
    /// or a theme inline.
    Retheme {
        #[schemars(with = "ThemeRef")]
        theme: Value,
    },
}

impl SemanticOp {
    /// Its `op`.
    pub fn name(&self) -> &'static str {
        match self {
            SemanticOp::AddNode { .. } => "add_node",
            SemanticOp::RemoveNode { .. } => "remove_node",
            SemanticOp::RenameNode { .. } => "rename_node",
            SemanticOp::ShowNode { .. } => "show_node",
            SemanticOp::HideNode { .. } => "hide_node",
            SemanticOp::Group { .. } => "group",
            SemanticOp::Ungroup { .. } => "ungroup",
            SemanticOp::SetProp { .. } => "set_prop",
            SemanticOp::Place { .. } => "place",
            SemanticOp::SetText { .. } => "set_text",
            SemanticOp::ReplaceText { .. } => "replace_text",
            SemanticOp::StyleText { .. } => "style_text",
            SemanticOp::Choose { .. } => "choose",
            SemanticOp::BindData { .. } => "bind_data",
            SemanticOp::ApplyPreset { .. } => "apply_preset",
            SemanticOp::AddState { .. } => "add_state",
            SemanticOp::MoveState { .. } => "move_state",
            SemanticOp::RemoveState { .. } => "remove_state",
            SemanticOp::RenameState { .. } => "rename_state",
            SemanticOp::SetState { .. } => "set_state",
            SemanticOp::Retheme { .. } => "retheme",
        }
    }
}

/// Where `place` puts a node: one placement, keyed as `at` keys it (SPEC §3.4). Cells (`col`
/// and `row`, each 1-based and inclusive, either alone spanning its grid), a slot of the
/// state's layout template (`in`), a `rect`, a grid container's `area`, or a stack's `index`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Spot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<Range>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<Range>,
    /// A slot of the state's layout template, or `canvas` or `grid`.
    #[serde(rename = "in", default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    /// `[x, y, w, h]`, canvas units: on the canvas, an override (W301), or from a frame's
    /// padding edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<Rect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
}

impl Spot {
    /// The keys of `at` that say where a node goes: what `place` replaces.
    pub const KEYS: [&'static str; 6] = ["col", "row", "in", "rect", "area", "index"];
}

/// The motion a motion preset is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Motion {
    Enter,
    Exit,
    Emphasis,
}

impl Motion {
    /// The node property it is.
    pub fn key(self) -> &'static str {
        match self {
            Motion::Enter => "enter",
            Motion::Exit => "exit",
            Motion::Emphasis => "emphasis",
        }
    }
}

/// An id a patch renamed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Renamed {
    Node { from: String, to: String },
    State { from: String, to: String },
}

/// A patch compiled to RFC 6902, and what it does.
#[derive(Debug, Clone)]
pub struct Compiled {
    /// The deck with every op applied.
    pub doc: Value,
    /// Every op as RFC 6902, in order.
    pub patch: Vec<JsonOp>,
    /// The ids renamed, in order.
    pub renamed: Vec<Renamed>,
}

/// Compile `ops` against `doc`, a deck's JSON, each against the deck as the ops before it
/// leave it, and apply them: all or none. `files` is the bundle, from which a `retheme`
/// and a shader preset read the theme.
pub fn compile(doc: &Value, ops: &[Value], files: &dyn BundleFiles) -> Result<Compiled, PatchError> {
    let mut work = doc.clone();
    let mut patch = Vec::new();
    let mut renamed = Vec::new();
    for (index, raw) in ops.iter().enumerate() {
        let fail = |message: String| PatchError { index, message };
        match Op::from_value(raw.clone()).map_err(fail)? {
            Op::Json(op) => {
                one(&mut work, &op).map_err(fail)?;
                patch.push(op);
            }
            Op::Semantic(op) => {
                let name = op.name();
                let out = ops::compile(&work, &op, files).map_err(|m| fail(format!("`{name}`: {m}")))?;
                for json in out.ops {
                    // A semantic op compiles against the deck it applies to: a failure here
                    // is a bug in the compiler, reported as such.
                    one(&mut work, &json)
                        .map_err(|m| fail(format!("`{name}` compiled to {json:?}, which failed: {m}")))?;
                    patch.push(json);
                }
                renamed.extend(out.renamed);
            }
        }
    }
    Ok(Compiled { doc: work, patch, renamed })
}

/// Apply RFC 6902 `ops` to `doc`, all or none.
pub fn apply(doc: &mut Value, ops: &[Value]) -> Result<(), PatchError> {
    let mut work = doc.clone();
    for (index, op) in ops.iter().enumerate() {
        let fail = |message: String| PatchError { index, message };
        let op: JsonOp = match op.get("op").and_then(Value::as_str) {
            Some(name) if !JSON_OPS.contains(&name) => return Err(fail(format!("`{name}` is not a JSON Patch op"))),
            _ => serde_json::from_value(op.clone()).map_err(|e| fail(e.to_string()))?,
        };
        one(&mut work, &op).map_err(fail)?;
    }
    *doc = work;
    Ok(())
}

pub(super) fn one(doc: &mut Value, op: &JsonOp) -> Result<(), String> {
    match op {
        JsonOp::Add { path, value } => add(doc, path, value.clone()),
        JsonOp::Remove { path } => remove(doc, path).map(drop),
        JsonOp::Replace { path, value } => {
            let target = doc.pointer_mut(path).ok_or_else(|| format!("`{path}` is not there"))?;
            *target = value.clone();
            Ok(())
        }
        JsonOp::Move { from, path } => {
            if path.starts_with(&format!("{from}/")) {
                return Err(format!("cannot move `{from}` into itself"));
            }
            if from == path {
                return doc.pointer(from).map(drop).ok_or_else(|| format!("`{from}` is not there"));
            }
            if rename(doc, from, path) {
                return Ok(());
            }
            let v = remove(doc, from)?;
            add(doc, path, v)
        }
        JsonOp::Copy { from, path } => {
            let v = doc.pointer(from).cloned().ok_or_else(|| format!("`{from}` is not there"))?;
            add(doc, path, v)
        }
        JsonOp::Test { path, value } => match doc.pointer(path) {
            Some(v) if equal(v, value) => Ok(()),
            Some(v) => Err(format!("`{path}` is {v}, not {value}")),
            None => Err(format!("`{path}` is not there")),
        },
    }
}

/// A move from one member of an object to a new member of the same object, done in place:
/// `false` when the move is not one.
fn rename(doc: &mut Value, from: &str, path: &str) -> bool {
    let (Ok((parent, old)), Ok((to_parent, new))) = (split(from), split(path)) else { return false };
    let Some(Value::Object(map)) = doc.pointer_mut(parent) else { return false };
    if parent != to_parent || !map.contains_key(&old) || map.contains_key(&new) {
        return false;
    }
    *map = std::mem::take(map).into_iter().map(|(k, v)| if k == old { (new.clone(), v) } else { (k, v) }).collect();
    true
}

/// JSON equality as RFC 6902's `test` reads it: numbers by value, objects regardless of
/// member order.
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_i64(), y.as_i64(), x.as_u64(), y.as_u64()) {
            (Some(x), Some(y), _, _) => x == y,
            (_, _, Some(x), Some(y)) => x == y,
            _ => x.as_f64() == y.as_f64(),
        },
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| equal(x, y)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| equal(v, w)))
        }
        _ => a == b,
    }
}

/// The parent of `path` and its last token, unescaped.
fn split(path: &str) -> Result<(&str, String), String> {
    let (parent, last) = path.rsplit_once('/').ok_or_else(|| format!("`{path}` is not a JSON pointer"))?;
    Ok((parent, last.replace("~1", "/").replace("~0", "~")))
}

fn add(doc: &mut Value, path: &str, value: Value) -> Result<(), String> {
    if path.is_empty() {
        *doc = value;
        return Ok(());
    }
    let (parent, key) = split(path)?;
    match doc.pointer_mut(parent) {
        Some(Value::Object(m)) => {
            m.insert(key, value);
            Ok(())
        }
        Some(Value::Array(a)) => {
            let i = if key == "-" { a.len() } else { index(&key, a.len() + 1)? };
            a.insert(i, value);
            Ok(())
        }
        Some(_) => Err(format!("`{parent}` holds neither an object nor an array")),
        None => Err(format!("`{parent}` is not there")),
    }
}

fn remove(doc: &mut Value, path: &str) -> Result<Value, String> {
    let (parent, key) = split(path)?;
    match doc.pointer_mut(parent) {
        Some(Value::Object(m)) => m.shift_remove(&key).ok_or_else(|| format!("`{path}` is not there")),
        Some(Value::Array(a)) => {
            let i = index(&key, a.len())?;
            Ok(a.remove(i))
        }
        _ => Err(format!("`{path}` is not there")),
    }
}

/// An array index token below `len`: digits, no leading zero.
fn index(key: &str, len: usize) -> Result<usize, String> {
    let ok = !key.is_empty() && key.bytes().all(|b| b.is_ascii_digit()) && (key == "0" || !key.starts_with('0'));
    let i: usize = key.parse().ok().filter(|_| ok).ok_or_else(|| format!("`{key}` is not an array index"))?;
    if i < len { Ok(i) } else { Err(format!("index {i} is past the end")) }
}

/// A JSON Pointer token for `key`.
fn esc(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_op_applies_as_rfc_6902_says() {
        let mut doc = json!({ "a": { "b": 1 }, "list": [1, 2, 3], "a/b": 0 });
        let ops = [
            json!({ "op": "add", "path": "/a/c", "value": 2 }),
            json!({ "op": "add", "path": "/list/1", "value": 9 }),
            json!({ "op": "add", "path": "/list/-", "value": 4 }),
            json!({ "op": "replace", "path": "/a/b", "value": 10 }),
            json!({ "op": "remove", "path": "/list/0" }),
            json!({ "op": "copy", "from": "/a", "path": "/copied" }),
            json!({ "op": "move", "from": "/a/c", "path": "/moved" }),
            json!({ "op": "test", "path": "/a~1b", "value": 0 }),
            json!({ "op": "test", "path": "/a/b", "value": 10.0, "ignored": "members an op does not define" }),
        ];
        apply(&mut doc, &ops).unwrap();
        assert_eq!(
            doc,
            json!({ "a": { "b": 10 }, "list": [9, 2, 3, 4], "a/b": 0, "copied": { "b": 10, "c": 2 }, "moved": 2 })
        );
    }

    #[test]
    fn a_move_within_an_object_keeps_its_place() {
        let mut doc = json!({ "nodes": { "a": 1, "b": 2, "c": 3 } });
        apply(&mut doc, &[json!({ "op": "move", "from": "/nodes/b", "path": "/nodes/x" })]).unwrap();
        let keys: Vec<&String> = doc["nodes"].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["a", "x", "c"]);
        // Onto a member already there: it takes the value, in its own place.
        apply(&mut doc, &[json!({ "op": "move", "from": "/nodes/a", "path": "/nodes/c" })]).unwrap();
        assert_eq!(doc, json!({ "nodes": { "x": 2, "c": 1 } }));
        let keys: Vec<&String> = doc["nodes"].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["x", "c"]);
    }

    #[test]
    fn a_patch_that_fails_changes_nothing() {
        let before = json!({ "a": 1, "list": [1] });
        for ops in [
            vec![json!({ "op": "replace", "path": "/a", "value": 2 }), json!({ "op": "remove", "path": "/missing" })],
            vec![json!({ "op": "test", "path": "/a", "value": 2 })],
            vec![json!({ "op": "add", "path": "/list/5", "value": 0 })],
            vec![json!({ "op": "add", "path": "/list/01", "value": 0 })],
            vec![json!({ "op": "move", "from": "/list", "path": "/list/0" })],
            vec![json!({ "op": "frobnicate", "path": "/a" })],
            vec![json!({ "op": "add", "path": "/nowhere/x", "value": 0 })],
            vec![json!({ "op": "add", "path": "/b" })],
            vec![json!({ "op": "add_node", "id": "x", "node": { "type": "text", "text": "x" } })],
        ] {
            let mut doc = before.clone();
            assert!(apply(&mut doc, &ops).is_err(), "{ops:?}");
            assert_eq!(doc, before, "{ops:?}");
        }
        let mut doc = before.clone();
        let err = apply(
            &mut doc,
            &[json!({ "op": "add", "path": "/b", "value": 1 }), json!({ "op": "remove", "path": "/c" })],
        );
        assert_eq!(err.unwrap_err().index, 1);
    }

    #[test]
    fn test_compares_numbers_by_value_and_objects_by_members() {
        assert!(equal(&json!(1), &json!(1.0)));
        assert!(equal(&json!({ "a": 1, "b": [2.0] }), &json!({ "b": [2], "a": 1.0 })));
        assert!(!equal(&json!(1), &json!(1.5)));
        assert!(!equal(&json!([1, 2]), &json!([2, 1])));
        assert!(!equal(&json!("1"), &json!(1)));
    }
}
