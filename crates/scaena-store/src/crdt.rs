//! The deck as a CRDT (SPEC §8, ADR-0002; PLAN 1.23).
//!
//! A [`DeckDoc`] holds the logical document in a Loro document, with its history. `deck.json`
//! is an export of it ([`DeckDoc::deck`]), the deck [`Deck::to_json`] writes byte for byte;
//! a deck from a file goes in as the smallest change that makes the document say what the
//! file says ([`DeckDoc::apply`]), authored by whoever made it.
//!
//! The containers (SPEC §8.1):
//! - `deck` (map): `scaena`, `canvas`, `formats`, `theme`, `fonts`, and `_comment`, and
//!   whether the deck has `meta` and a `spine`.
//! - `meta`, `data`, and `overrides` (maps).
//! - `nodes` (map): each node under a key of its own, which nothing outside the CRDT sees,
//!   holding its id, its type, and its props. Renaming a node changes its id and nothing
//!   else, so an edit made meanwhile to the node, by its old id, lands on it. `order`
//!   (movable list) holds the keys in paint order, since a map keeps no order of its keys.
//! - `states` (movable list of maps): the cue list. A state's props are keyed by node key,
//!   and so are `remove`, choreography targets, and `at.parent`, everywhere.
//! - `spine` (tree): sections, with their beats under them.
//! - A text node's `text` and the `notes` of a state or a beat are text, so concurrent edits
//!   to one string merge by character; `runs` is rich text, its runs marked over it.
//!
//! Every other value is JSON text: replaced whole, the last writer winning, with its keys in
//! their order. A map whose keys the deck shows in an order keeps that order beside them.

use loro::{
    CommitOptions, Container, ContainerTrait, ExpandType, ExportMode, ID, LoroDoc, LoroMap, LoroMovableList, LoroText,
    LoroValue, StyleConfig, StyleConfigMap, TreeID, UndoManager, ValueOrContainer,
};
use scaena_core::Deck;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Where a map keeps the order of its keys: JSON text of the list.
const ORDER: &str = "\0order";
/// A node's id, the one deck.json shows.
const ID_KEY: &str = "\0id";
/// A node's type.
const TYPE_KEY: &str = "\0type";
/// Before a node id that names no node: kept as it was written.
const UNRESOLVED: &str = "\0?";
/// The mark a run of rich text carries: JSON text of `[mark id, run]`, the run's `text`
/// null. The id keeps two runs alike two runs.
const RUN: &str = "run";
/// Who made a change that came from a file edited outside Scaena.
pub const FS: &str = "fs";
/// What such a change says.
pub const OUTSIDE: &str = "deck.json changed outside Scaena";

#[derive(Debug, Error)]
pub enum CrdtError {
    #[error("loro: {0}")]
    Loro(#[from] loro::LoroError),
    #[error("loro: {0}")]
    Encode(#[from] loro::LoroEncodeError),
    #[error("deck: {0}")]
    Json(#[from] serde_json::Error),
    #[error("`{0}`: a key may not start with NUL, which the CRDT keeps for itself")]
    Reserved(String),
    #[error("the CRDT document is not a deck: {0}")]
    Shape(String),
}

type Result<T> = std::result::Result<T, CrdtError>;

/// One change to the document, as its history keeps it.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// Who made it: `user`, `agent:<name>`, or `fs` (SPEC §8.2); none if it does not say.
    pub author: Option<String>,
    pub message: Option<String>,
    /// When, in seconds since 1970 (UTC); 0 if it was not recorded.
    pub timestamp: i64,
    /// The editor that made it.
    pub peer: u64,
    /// Its place in the causal order: changes sort by it.
    pub lamport: u32,
    /// How many operations it holds: a character typed is one.
    pub ops: usize,
}

/// Who changes the document, and why.
#[derive(Debug, Clone, Default)]
pub struct Edit<'a> {
    pub author: &'a str,
    pub message: Option<&'a str>,
    /// When, in seconds since 1970; now without it.
    pub timestamp: Option<i64>,
    /// Nodes renamed, `(from, to)`: each stays the node it was, under its new id. A node
    /// whose id changes without one is removed, and another added.
    pub renamed_nodes: &'a [(String, String)],
    /// States renamed, `(from, to)`, likewise.
    pub renamed_states: &'a [(String, String)],
}

impl<'a> Edit<'a> {
    pub fn by(author: &'a str) -> Self {
        Edit { author, ..Edit::default() }
    }
}

/// A change for a history to record, as one module hands it to another (PLAN 2.9): the
/// deck it leaves, as deck.json's text, and the [`Edit`] that made it. A page's engine keeps
/// no CRDT, so it hands its changes, as JSON, to the module that does (`scaena-history`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Recorded {
    pub deck: String,
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// When, in seconds since 1970; now without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub renamed_nodes: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub renamed_states: Vec<(String, String)>,
}

/// The deck as a Loro document, with its history.
pub struct DeckDoc {
    doc: LoroDoc,
}

impl std::fmt::Debug for DeckDoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeckDoc").field("peer", &self.doc.peer_id()).finish()
    }
}

/// What every Loro document of a deck is set to: none of it is saved with the document.
fn configure(doc: &LoroDoc) {
    let mut styles = StyleConfigMap::new();
    // Text typed at the end of a run joins it.
    styles.insert(RUN.into(), StyleConfig { expand: ExpandType::After });
    doc.config_text_style(styles);
    doc.get_tree("spine").enable_fractional_index(0);
    // Each commit is a change of its own, so the history says who made each.
    doc.set_change_merge_interval(-1);
    doc.set_record_timestamp(true);
}

impl DeckDoc {
    fn wrap(doc: LoroDoc) -> Self {
        configure(&doc);
        DeckDoc { doc }
    }

    /// A document holding `deck`: its first change, `edit`'s. Each node is keyed by its id.
    pub fn from_deck(deck: &Deck, edit: &Edit) -> Result<Self> {
        let me = DeckDoc::wrap(LoroDoc::new());
        me.write(deck, edit, true)?;
        Ok(me)
    }

    /// The same, edited as `peer`: for tests that compare documents.
    pub fn from_deck_as(deck: &Deck, edit: &Edit, peer: u64) -> Result<Self> {
        let doc = LoroDoc::new();
        doc.set_peer_id(peer)?;
        let me = DeckDoc::wrap(doc);
        me.write(deck, edit, true)?;
        Ok(me)
    }

    /// A document saved by [`DeckDoc::save`], edited from here as a new peer.
    pub fn load(bytes: &[u8]) -> Result<Self> {
        let doc = LoroDoc::new();
        doc.import(bytes)?;
        Ok(DeckDoc::wrap(doc))
    }

    /// The document and its whole history: what `history/deck.loro` holds.
    pub fn save(&self) -> Result<Vec<u8>> {
        Ok(self.doc.export(ExportMode::Snapshot)?)
    }

    /// The editor this document's changes come from.
    pub fn peer(&self) -> u64 {
        self.doc.peer_id()
    }

    /// Another document with this one's history, edited by a peer of its own: a branch.
    pub fn fork(&self) -> Self {
        DeckDoc::wrap(self.doc.fork())
    }

    /// Takes in what `other` has that this does not: a merge. Nothing is lost; where both
    /// changed one value, one wins, the same one whichever side merges.
    pub fn merge(&self, other: &DeckDoc) -> Result<()> {
        let updates = other.doc.export(ExportMode::updates(&self.doc.oplog_vv()))?;
        self.doc.import(&updates)?;
        Ok(())
    }

    /// Makes the document say what `deck` says, as one change by `edit`'s author, touching
    /// only what differs. Whether anything did.
    pub fn apply(&self, deck: &Deck, edit: &Edit) -> Result<bool> {
        self.write(deck, edit, false)
    }

    /// [`DeckDoc::apply`]s each of `changes` in order: one that leaves the deck as it was is
    /// no change. How many were.
    pub fn record(&self, changes: &[Recorded]) -> Result<usize> {
        let mut recorded = 0;
        for change in changes {
            let edit = Edit {
                author: &change.author,
                message: change.message.as_deref(),
                timestamp: change.timestamp,
                renamed_nodes: &change.renamed_nodes,
                renamed_states: &change.renamed_states,
            };
            recorded += usize::from(self.apply(&Deck::from_json(&change.deck)?, &edit)?);
        }
        Ok(recorded)
    }

    /// Every change, oldest first.
    pub fn changes(&self) -> Vec<Change> {
        let mut out = Vec::new();
        for (&peer, &end) in self.doc.oplog_vv().iter() {
            let mut counter = 0;
            while counter < end {
                let Some(meta) = self.doc.get_change(ID::new(peer, counter)) else { break };
                let (author, message) = match meta.message.as_deref() {
                    Some(text) => match text.split_once('\n') {
                        Some((author, message)) => (Some(author.to_string()), Some(message.to_string())),
                        None => (Some(text.to_string()), None),
                    },
                    None => (None, None),
                };
                let (timestamp, lamport, ops) = (meta.timestamp, meta.lamport, meta.len);
                out.push(Change { author, message, timestamp, peer, lamport, ops });
                counter = meta.id.counter + meta.len as i32;
            }
        }
        out.sort_by_key(|c| (c.lamport, c.peer));
        out
    }

    /// Undo and redo for this document's own changes (SPEC §8.2): each [`DeckDoc::apply`]
    /// is a step. Changes from files (`fs`) and from other peers are never undone.
    pub fn undo_manager(&self) -> Undo {
        let mut manager = UndoManager::new(&self.doc);
        manager.add_exclude_origin_prefix(FS);
        Undo { manager }
    }

    /// The deck the document holds: what deck.json is.
    pub fn deck(&self) -> Result<Deck> {
        Ok(serde_json::from_value(self.read()?)?)
    }

    fn commit(&self, edit: &Edit) -> bool {
        if self.doc.get_pending_txn_len() == 0 {
            return false;
        }
        let message = match edit.message {
            Some(message) => format!("{}\n{message}", edit.author),
            None => edit.author.to_string(),
        };
        let mut options = CommitOptions::new().origin(edit.author).commit_msg(&message);
        if let Some(timestamp) = edit.timestamp {
            options = options.timestamp(timestamp);
        }
        self.doc.commit_with(options);
        true
    }
}

/// Undo and redo of one peer's changes.
pub struct Undo {
    manager: UndoManager,
}

impl Undo {
    /// Undoes the latest change not undone yet; whether there was one. The undoing is a
    /// change too, by `author`.
    pub fn undo(&mut self, doc: &DeckDoc, author: &str) -> Result<bool> {
        doc.doc.set_next_commit_message(&format!("{author}\nundo"));
        Ok(self.manager.undo()?)
    }

    /// Redoes the latest change undone; whether there was one.
    pub fn redo(&mut self, doc: &DeckDoc, author: &str) -> Result<bool> {
        doc.doc.set_next_commit_message(&format!("{author}\nredo"));
        Ok(self.manager.redo()?)
    }
}

// --- values -------------------------------------------------------------------------------

/// What a map holds at a key: a value, as JSON text, or text.
enum Held {
    Json(Value),
    Text(LoroText),
}

fn held(map: &LoroMap, key: &str) -> Result<Option<Held>> {
    match map.get(key) {
        None => Ok(None),
        Some(ValueOrContainer::Value(LoroValue::String(s))) => Ok(Some(Held::Json(serde_json::from_str(&s)?))),
        Some(ValueOrContainer::Container(Container::Text(t))) => Ok(Some(Held::Text(t))),
        Some(other) => Err(CrdtError::Shape(format!("`{}` holds {other:?}", key.escape_default()))),
    }
}

fn json_at(map: &LoroMap, key: &str) -> Result<Option<Value>> {
    match held(map, key)? {
        Some(Held::Json(v)) => Ok(Some(v)),
        Some(Held::Text(t)) => Ok(Some(Value::String(t.to_string()))),
        None => Ok(None),
    }
}

/// Puts `value` at `key` as JSON text, unless it is there.
fn put_json(map: &LoroMap, key: &str, value: &Value) -> Result<()> {
    let text = serde_json::to_string(value)?;
    if let Some(ValueOrContainer::Value(LoroValue::String(s))) = map.get(key)
        && *s == text
    {
        return Ok(());
    }
    map.insert(key, text)?;
    Ok(())
}

/// Puts `s` at `key` as text, editing the text there by the least it takes.
fn put_text(map: &LoroMap, key: &str, s: &str) -> Result<()> {
    let text = match map.get(key) {
        Some(ValueOrContainer::Container(Container::Text(t))) => t,
        _ => map.insert_container(key, LoroText::new())?,
    };
    if text.to_string() != s {
        text.update(s, Default::default()).map_err(|e| CrdtError::Shape(format!("{e:?}")))?;
    }
    Ok(())
}

fn put_opt(map: &LoroMap, key: &str, value: Option<&Value>) -> Result<()> {
    match value {
        Some(value) => put_json(map, key, value),
        None => drop_key(map, key),
    }
}

fn drop_key(map: &LoroMap, key: &str) -> Result<()> {
    if map.get(key).is_some() {
        map.delete(key)?;
    }
    Ok(())
}

/// The keys of a map, in the order it keeps; any it does not list come after, sorted.
fn ordered_keys(map: &LoroMap) -> Result<Vec<String>> {
    let listed: Vec<String> = match json_at(map, ORDER)? {
        Some(Value::Array(keys)) => keys.into_iter().filter_map(|k| k.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    };
    let present: HashSet<String> = map.keys().map(|k| k.to_string()).filter(|k| !k.starts_with('\0')).collect();
    let mut out: Vec<String> = Vec::new();
    for key in listed {
        if present.contains(&key) && !out.contains(&key) {
            out.push(key);
        }
    }
    let mut rest: Vec<String> = present.into_iter().filter(|k| !out.contains(k)).collect();
    rest.sort();
    out.extend(rest);
    Ok(out)
}

fn put_order(map: &LoroMap, keys: &[String]) -> Result<()> {
    put_json(map, ORDER, &Value::Array(keys.iter().cloned().map(Value::String).collect()))
}

fn no_reserved<'a>(keys: impl IntoIterator<Item = &'a String>) -> Result<()> {
    match keys.into_iter().find(|k| k.starts_with('\0')) {
        Some(key) => Err(CrdtError::Reserved(key.escape_default().to_string())),
        None => Ok(()),
    }
}

/// A child map at `key`, made if it is not there.
fn child_map(map: &LoroMap, key: &str) -> Result<LoroMap> {
    match map.get(key) {
        Some(ValueOrContainer::Container(Container::Map(m))) => Ok(m),
        _ => Ok(map.insert_container(key, LoroMap::new())?),
    }
}

// --- node keys ----------------------------------------------------------------------------

/// Node ids and the keys the CRDT holds nodes by, both ways.
#[derive(Default)]
struct Keys {
    by_id: HashMap<String, String>,
    by_key: HashMap<String, String>,
}

impl Keys {
    /// The key a reference to node `id` is held by.
    fn key(&self, id: &str) -> String {
        self.by_id.get(id).cloned().unwrap_or_else(|| format!("{UNRESOLVED}{id}"))
    }

    /// The node id a reference held by `key` is to: the node's id now, or for a node that
    /// is gone, the id it was keyed by.
    fn id(&self, key: &str) -> String {
        if let Some(id) = key.strip_prefix(UNRESOLVED) {
            return id.to_string();
        }
        match self.by_key.get(key) {
            Some(id) => id.clone(),
            None => key.split('~').next().unwrap_or(key).to_string(),
        }
    }
}

/// The node references in a prop's value, mapped by `f`: `at.parent`.
fn map_prop_refs(key: &str, value: &Value, f: &dyn Fn(&str) -> String) -> Value {
    let mut value = value.clone();
    if key == "at"
        && let Some(Value::String(parent)) = value.get_mut("parent")
    {
        *parent = f(parent);
    }
    value
}

/// Choreography's targets, mapped by `f`, at every depth.
fn map_choreography(items: &Value, f: &dyn Fn(&str) -> String) -> Value {
    let Value::Array(items) = items else { return items.clone() };
    Value::Array(
        items
            .iter()
            .map(|item| {
                let mut item = item.clone();
                if let Some(fields) = item.as_object_mut() {
                    match fields.get_mut("target") {
                        Some(Value::String(t)) => *t = f(t),
                        Some(Value::Array(ts)) => {
                            for t in ts.iter_mut() {
                                if let Value::String(s) = t {
                                    *s = f(s);
                                }
                            }
                        }
                        _ => {}
                    }
                    for key in ["sequence", "parallel"] {
                        if let Some(inner) = fields.get(key).cloned() {
                            fields.insert(key.into(), map_choreography(&inner, f));
                        }
                    }
                }
                item
            })
            .collect(),
    )
}

fn map_ids(items: &Value, f: &dyn Fn(&str) -> String) -> Value {
    match items {
        Value::Array(items) => Value::Array(
            items.iter().map(|x| x.as_str().map(|s| Value::String(f(s))).unwrap_or_else(|| x.clone())).collect(),
        ),
        other => other.clone(),
    }
}

// --- runs ---------------------------------------------------------------------------------

/// Whether `runs` can be rich text: runs, each with text to mark.
fn runs_as_text(runs: &Value) -> Option<&Vec<Value>> {
    let runs = runs.as_array()?;
    let fits =
        !runs.is_empty() && runs.iter().all(|r| r.get("text").and_then(Value::as_str).is_some_and(|t| !t.is_empty()));
    fits.then_some(runs)
}

fn read_runs(text: &LoroText) -> Result<Value> {
    let mut runs: Vec<(Option<String>, String)> = Vec::new();
    for segment in text.to_delta() {
        let loro::TextDelta::Insert { insert, attributes } = segment else { continue };
        let mark = attributes.and_then(|a| a.get(RUN).and_then(|v| v.as_string().map(|s| s.to_string())));
        match runs.last_mut() {
            Some((last, s)) if *last == mark && mark.is_some() => s.push_str(&insert),
            _ => runs.push((mark, insert)),
        }
    }
    let mut out = Vec::with_capacity(runs.len());
    for (mark, s) in runs {
        let mut run = match mark {
            Some(mark) => match serde_json::from_str::<Value>(&mark)? {
                Value::Array(mut pair) if pair.len() == 2 => pair.pop().unwrap_or_default(),
                other => return Err(CrdtError::Shape(format!("a run marked {other}"))),
            },
            None => Value::Object(Map::new()),
        };
        match run.as_object_mut() {
            Some(fields) => {
                fields.insert("text".into(), Value::String(s));
            }
            None => return Err(CrdtError::Shape(format!("a run marked {run}"))),
        }
        out.push(run);
    }
    Ok(Value::Array(out))
}

/// A stretch of rich text, by character, and the run mark it carries.
struct Stretch {
    start: usize,
    end: usize,
    mark: Option<(Value, Value)>,
}

fn stretches(text: &LoroText) -> Vec<Stretch> {
    let mut out: Vec<Stretch> = Vec::new();
    let mut at = 0;
    for segment in text.to_delta() {
        let loro::TextDelta::Insert { insert, attributes } = segment else { continue };
        let len = insert.chars().count();
        let mark =
            attributes.and_then(|a| a.get(RUN).and_then(|v| v.as_string().map(|s| s.to_string()))).and_then(|m| {
                match serde_json::from_str::<Value>(&m) {
                    Ok(Value::Array(mut pair)) if pair.len() == 2 => {
                        let run = pair.pop()?;
                        Some((pair.pop()?, run))
                    }
                    _ => None,
                }
            });
        match out.last_mut() {
            Some(last) if last.mark.is_some() && last.mark == mark => last.end += len,
            _ => out.push(Stretch { start: at, end: at + len, mark }),
        }
        at += len;
    }
    out
}

/// A mark id no other run has: this peer's, and its next operation's counter.
fn new_mark_id(text: &LoroText) -> Value {
    let Some(doc) = text.doc() else { return Value::Null };
    let peer = doc.peer_id();
    let next = doc.oplog_vv().get(&peer).copied().unwrap_or(0) as usize + doc.get_pending_txn_len();
    Value::String(format!("{peer:x}:{next}"))
}

/// Writes runs as rich text: their characters edited by the least it takes, then each run
/// marked where its range or its look changed, and only there, so concurrent edits to
/// different runs both stand.
fn put_runs(map: &LoroMap, key: &str, runs: &[Value]) -> Result<()> {
    let text = match map.get(key) {
        Some(ValueOrContainer::Container(Container::Text(t))) => t,
        _ => map.insert_container(key, LoroText::new())?,
    };
    if read_runs(&text)? == Value::Array(runs.to_vec()) {
        return Ok(());
    }
    let whole: String = runs.iter().filter_map(|r| r["text"].as_str()).collect();
    if text.to_string() != whole {
        text.update(&whole, Default::default()).map_err(|e| CrdtError::Shape(format!("{e:?}")))?;
    }
    let now = stretches(&text);
    let (mut at, mut before): (usize, Option<Value>) = (0, None);
    for run in runs {
        let len = run["text"].as_str().unwrap_or_default().chars().count();
        let mut look = run.clone();
        look["text"] = Value::Null;
        // The stretch the run starts in now: its id stays the run's, unless the run before
        // took it.
        let here = now.iter().find(|s| s.start <= at && at < s.end);
        let held = here.and_then(|s| s.mark.clone());
        let id = match &held {
            Some((id, _)) if before.as_ref() != Some(id) => id.clone(),
            _ => new_mark_id(&text),
        };
        let exact = here.is_some_and(|s| s.start == at && s.end == at + len);
        if !(exact && held.as_ref() == Some(&(id.clone(), look.clone()))) {
            text.mark(at..at + len, RUN, serde_json::to_string(&Value::Array(vec![id.clone(), look]))?)?;
        }
        before = Some(id);
        at += len;
    }
    Ok(())
}

// --- props --------------------------------------------------------------------------------

/// Writes a node's props, or a delta, into `map`: text as text, runs as rich text, the
/// rest as JSON, references by key.
fn put_props(map: &LoroMap, props: &Map<String, Value>, keys: &Keys) -> Result<()> {
    no_reserved(props.keys())?;
    let to_key = |id: &str| keys.key(id);
    for (key, value) in props {
        match (key.as_str(), value) {
            ("text", Value::String(s)) => put_text(map, key, s)?,
            ("runs", runs) if runs_as_text(runs).is_some() => put_runs(map, key, runs_as_text(runs).unwrap())?,
            _ => {
                if matches!(held(map, key)?, Some(Held::Text(_))) {
                    map.delete(key)?;
                }
                put_json(map, key, &map_prop_refs(key, value, &to_key))?;
            }
        }
    }
    for key in map.keys().map(|k| k.to_string()).collect::<Vec<_>>() {
        if !key.starts_with('\0') && !props.contains_key(&key) {
            map.delete(&key)?;
        }
    }
    let order: Vec<String> = props.keys().cloned().collect();
    put_order(map, &order)
}

fn read_props(map: &LoroMap, keys: &Keys) -> Result<Map<String, Value>> {
    let to_id = |key: &str| keys.id(key);
    let mut out = Map::new();
    for key in ordered_keys(map)? {
        let value = match held(map, &key)? {
            Some(Held::Text(t)) if key == "runs" => read_runs(&t)?,
            Some(Held::Text(t)) => Value::String(t.to_string()),
            Some(Held::Json(v)) => map_prop_refs(&key, &v, &to_id),
            None => continue,
        };
        out.insert(key, value);
    }
    Ok(out)
}

/// Writes a map of deltas by node id (a state's `props`, `overrides`) into `map`, by key.
fn put_deltas(map: &LoroMap, deltas: &Map<String, Value>, keys: &Keys) -> Result<()> {
    no_reserved(deltas.keys())?;
    let mut order = Vec::with_capacity(deltas.len());
    for (id, delta) in deltas {
        let key = keys.key(id);
        let Value::Object(delta) = delta else {
            return Err(CrdtError::Shape(format!("`{id}`'s delta is not an object")));
        };
        put_props(&child_map(map, &key)?, delta, keys)?;
        order.push(key);
    }
    for key in map.keys().map(|k| k.to_string()).collect::<Vec<_>>() {
        if (!key.starts_with('\0') || key.starts_with(UNRESOLVED)) && !order.contains(&key) {
            map.delete(&key)?;
        }
    }
    put_order(map, &order)
}

fn read_deltas(map: &LoroMap, keys: &Keys) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    let listed: Vec<String> = match json_at(map, ORDER)? {
        Some(Value::Array(k)) => k.into_iter().filter_map(|k| k.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    };
    let mut present: Vec<String> =
        map.keys().map(|k| k.to_string()).filter(|k| !k.starts_with('\0') || k.starts_with(UNRESOLVED)).collect();
    present.sort();
    let order = listed.iter().filter(|k| present.contains(k)).chain(present.iter().filter(|k| !listed.contains(k)));
    for key in order {
        let Some(ValueOrContainer::Container(Container::Map(delta))) = map.get(key) else { continue };
        out.insert(keys.id(key), Value::Object(read_props(&delta, keys)?));
    }
    Ok(out)
}

// --- writing ------------------------------------------------------------------------------

/// The fields of a state the CRDT holds as JSON (its `notes` are text, its `props` a map).
const STATE_FIELDS: [&str; 10] =
    ["id", "name", "slide", "from", "mode", "layout", "transition", "remove", "choreography", "hold"];
/// The fields of a beat the CRDT holds as JSON (its `notes` are text).
const BEAT_FIELDS: [&str; 6] = ["id", "claim", "evidence", "states", "duration", "media"];
/// The deck's own fields, as JSON.
const DECK_FIELDS: [&str; 6] = ["scaena", "canvas", "formats", "theme", "fonts", "_comment"];

impl DeckDoc {
    fn write(&self, deck: &Deck, edit: &Edit, fresh: bool) -> Result<bool> {
        let Value::Object(v) = serde_json::to_value(deck)? else {
            return Err(CrdtError::Shape("a deck is an object".into()));
        };
        let field = |k: &str| v.get(k);
        let root = self.doc.get_map("deck");
        for key in DECK_FIELDS {
            put_opt(&root, key, field(key))?;
        }
        self.write_meta(field("meta"))?;
        let empty = Map::new();
        let data = field("data").and_then(Value::as_object).unwrap_or(&empty);
        let data_map = self.doc.get_map("data");
        no_reserved(data.keys())?;
        for (id, source) in data {
            put_json(&data_map, id, source)?;
        }
        for key in data_map.keys().map(|k| k.to_string()).collect::<Vec<_>>() {
            if !key.starts_with('\0') && !data.contains_key(&key) {
                data_map.delete(&key)?;
            }
        }
        put_order(&data_map, &data.keys().cloned().collect::<Vec<_>>())?;
        let nodes = field("nodes").and_then(Value::as_object).unwrap_or(&empty);
        let keys = self.write_nodes(nodes, edit.renamed_nodes, fresh)?;
        let states = field("states").and_then(Value::as_array).cloned().unwrap_or_default();
        self.write_states(&states, edit.renamed_states, &keys)?;
        let overrides = field("overrides").and_then(Value::as_object).unwrap_or(&empty);
        put_deltas(&self.doc.get_map("overrides"), overrides, &keys)?;
        self.write_spine(field("spine"))?;
        Ok(self.commit(edit))
    }

    fn write_meta(&self, meta: Option<&Value>) -> Result<()> {
        let root = self.doc.get_map("deck");
        let map = self.doc.get_map("meta");
        let fields = match meta {
            Some(Value::Object(fields)) => {
                put_json(&root, "meta", &Value::Bool(true))?;
                fields.clone()
            }
            _ => {
                drop_key(&root, "meta")?;
                Map::new()
            }
        };
        no_reserved(fields.keys())?;
        for (key, value) in &fields {
            put_json(&map, key, value)?;
        }
        for key in map.keys().map(|k| k.to_string()).collect::<Vec<_>>() {
            if !key.starts_with('\0') && !fields.contains_key(&key) {
                map.delete(&key)?;
            }
        }
        put_order(&map, &fields.keys().cloned().collect::<Vec<_>>())
    }

    /// The nodes the document holds, by key, in paint order, each with its id. Two that came
    /// to share an id (made apart, then merged) are told apart: the later in order takes a
    /// suffix, `-2`, as deck.json shows it.
    fn node_ids(&self) -> Result<Vec<(String, String)>> {
        let held = self.node_keys()?;
        let mut taken: HashSet<String> = held.iter().map(|(_, id)| id.clone()).collect();
        let mut claimed: HashSet<String> = HashSet::new();
        let mut out = Vec::with_capacity(held.len());
        for (key, id) in held {
            let mut unique = id.clone();
            if !claimed.insert(id.clone()) {
                let mut n = 2;
                while taken.contains(&format!("{id}-{n}")) {
                    n += 1;
                }
                unique = format!("{id}-{n}");
                taken.insert(unique.clone());
                claimed.insert(unique.clone());
            }
            out.push((key, unique));
        }
        Ok(out)
    }

    /// The node ids the document holds now, by key, in paint order, as each node says.
    fn node_keys(&self) -> Result<Vec<(String, String)>> {
        let nodes = self.doc.get_map("nodes");
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let order = self.doc.get_movable_list("order");
        let listed: Vec<String> =
            order.to_vec().into_iter().filter_map(|v| v.as_string().map(|s| s.to_string())).collect();
        let mut rest: Vec<String> = nodes.keys().map(|k| k.to_string()).filter(|k| !listed.contains(k)).collect();
        rest.sort();
        for key in listed.into_iter().chain(rest) {
            if !seen.insert(key.clone()) {
                continue;
            }
            let Some(ValueOrContainer::Container(Container::Map(node))) = nodes.get(&key) else { continue };
            let id = match json_at(&node, ID_KEY)? {
                Some(Value::String(id)) => id,
                _ => key.split('~').next().unwrap_or(&key).to_string(),
            };
            out.push((key, id));
        }
        Ok(out)
    }

    fn write_nodes(&self, nodes: &Map<String, Value>, renamed: &[(String, String)], fresh: bool) -> Result<Keys> {
        no_reserved(nodes.keys())?;
        let map = self.doc.get_map("nodes");
        let mut current: HashMap<String, String> = HashMap::new();
        for (key, id) in self.node_ids()? {
            current.entry(id).or_insert(key);
        }
        // In order, so a rename of a rename follows the node.
        for (from, to) in renamed {
            if !nodes.contains_key(from)
                && !current.contains_key(to)
                && let Some(key) = current.remove(from)
            {
                current.insert(to.clone(), key);
            }
        }
        let peer = self.doc.peer_id();
        let mut keys = Keys::default();
        for id in nodes.keys() {
            let key = match current.get(id) {
                Some(key) => key.clone(),
                None if fresh => id.clone(),
                None => {
                    let mut key = format!("{id}~{peer:x}");
                    let mut n = 2;
                    while map.get(&key).is_some() || keys.by_key.contains_key(&key) {
                        key = format!("{id}~{peer:x}~{n}");
                        n += 1;
                    }
                    key
                }
            };
            keys.by_id.insert(id.clone(), key.clone());
            keys.by_key.insert(key, id.clone());
        }
        for key in map.keys().map(|k| k.to_string()).collect::<Vec<_>>() {
            if !keys.by_key.contains_key(&key) {
                map.delete(&key)?;
            }
        }
        for (id, node) in nodes {
            let Value::Object(fields) = node else {
                return Err(CrdtError::Shape(format!("node `{id}` is not an object")));
            };
            let node_map = child_map(&map, &keys.by_id[id])?;
            put_json(&node_map, ID_KEY, &Value::String(id.clone()))?;
            put_opt(&node_map, TYPE_KEY, fields.get("type"))?;
            // Not `remove`: with `preserve_order` it swaps the last prop into its place.
            let props: Map<String, Value> =
                fields.iter().filter(|(k, _)| *k != "type").map(|(k, v)| (k.clone(), v.clone())).collect();
            put_props(&node_map, &props, &keys)?;
        }
        let target: Vec<String> = nodes.keys().map(|id| keys.by_id[id].clone()).collect();
        sync_values(&self.doc.get_movable_list("order"), &target)?;
        Ok(keys)
    }

    fn write_states(&self, states: &[Value], renamed: &[(String, String)], keys: &Keys) -> Result<()> {
        let list = self.doc.get_movable_list("states");
        // The id a state had, following renames back.
        let back: HashMap<&str, &str> = renamed.iter().map(|(a, b)| (b.as_str(), a.as_str())).collect();
        let was = |id: &str| -> String {
            let (mut id, mut seen) = (id, HashSet::new());
            while let Some(&from) = back.get(id) {
                if !seen.insert(from) {
                    break;
                }
                id = from;
            }
            id.to_string()
        };
        // The states there are, by id; each matched to at most one state of the deck.
        let mut there: Vec<(LoroMap, String)> = Vec::new();
        for i in 0..list.len() {
            let Some(ValueOrContainer::Container(Container::Map(m))) = list.get(i) else { continue };
            let id = json_at(&m, "id")?.and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            there.push((m, id));
        }
        let mut used = vec![false; there.len()];
        let mut target: Vec<Option<usize>> = Vec::with_capacity(states.len());
        for state in states {
            let id = state.get("id").and_then(Value::as_str).unwrap_or_default();
            let was = was(id);
            let found = (0..there.len()).find(|&j| !used[j] && there[j].1 == was);
            if let Some(j) = found {
                used[j] = true;
            }
            target.push(found);
        }
        let maps = sync_maps(&list, &there.iter().map(|(m, _)| m.id()).collect::<Vec<_>>(), &target)?;
        for (state, map) in states.iter().zip(&maps) {
            let Value::Object(fields) = state else {
                return Err(CrdtError::Shape("a state is not an object".into()));
            };
            let to_key = |id: &str| keys.key(id);
            for key in STATE_FIELDS {
                let value = match (key, fields.get(key)) {
                    ("remove", Some(v)) => Some(map_ids(v, &to_key)),
                    ("choreography", Some(v)) => Some(map_choreography(v, &to_key)),
                    (_, v) => v.cloned(),
                };
                put_opt(map, key, value.as_ref())?;
            }
            put_opt(map, "_comment", fields.get("_comment"))?;
            match fields.get("notes") {
                Some(Value::String(notes)) => put_text(map, "notes", notes)?,
                Some(other) => put_json(map, "notes", other)?,
                None => drop_key(map, "notes")?,
            }
            let empty = Map::new();
            let props = fields.get("props").and_then(Value::as_object).unwrap_or(&empty);
            put_deltas(&child_map(map, "props")?, props, keys)?;
        }
        Ok(())
    }

    fn write_spine(&self, spine: Option<&Value>) -> Result<()> {
        let root = self.doc.get_map("deck");
        let tree = self.doc.get_tree("spine");
        let sections: Vec<Value> = match spine {
            Some(spine) => {
                put_json(&root, "spine", &Value::Bool(true))?;
                spine.get("sections").and_then(Value::as_array).cloned().unwrap_or_default()
            }
            None => {
                drop_key(&root, "spine")?;
                Vec::new()
            }
        };
        let id_of = |node: TreeID| -> Result<String> {
            Ok(json_at(&tree.get_meta(node)?, "id")?.and_then(|v| v.as_str().map(String::from)).unwrap_or_default())
        };
        // Sections and beats there are, by id; each matched at most once.
        let mut there_sections: Vec<(TreeID, String)> = Vec::new();
        let mut there_beats: Vec<(TreeID, String)> = Vec::new();
        for section in tree.roots() {
            there_sections.push((section, id_of(section)?));
            for beat in tree.children(section).unwrap_or_default() {
                there_beats.push((beat, id_of(beat)?));
            }
        }
        let mut used_sections = HashSet::new();
        let mut used_beats = HashSet::new();
        let mut placed: Vec<(TreeID, Vec<TreeID>)> = Vec::new();
        for section in &sections {
            let id = section.get("id").and_then(Value::as_str).unwrap_or_default();
            let node = match there_sections.iter().find(|(n, s)| s == id && !used_sections.contains(n)) {
                Some((n, _)) => *n,
                None => tree.create(None::<TreeID>)?,
            };
            used_sections.insert(node);
            let mut beats = Vec::new();
            for beat in section.get("beats").and_then(Value::as_array).into_iter().flatten() {
                let id = beat.get("id").and_then(Value::as_str).unwrap_or_default();
                let found = there_beats.iter().find(|(n, b)| b == id && !used_beats.contains(n)).map(|(n, _)| *n);
                let node = match found {
                    Some(n) => n,
                    None => tree.create(node)?,
                };
                used_beats.insert(node);
                beats.push(node);
            }
            placed.push((node, beats));
        }
        // Beats go where the deck puts them before anything is deleted, so none is lost with
        // the section it leaves.
        for (section, beats) in &placed {
            for (i, beat) in beats.iter().enumerate() {
                let here = tree.children(*section).unwrap_or_default();
                if here.get(i) != Some(beat) {
                    tree.mov_to(*beat, *section, i)?;
                }
            }
        }
        for (beat, _) in &there_beats {
            if !used_beats.contains(beat) && tree.contains(*beat) && !tree.is_node_deleted(beat)? {
                tree.delete(*beat)?;
            }
        }
        for (section, _) in &there_sections {
            if !used_sections.contains(section) {
                tree.delete(*section)?;
            }
        }
        for (i, (section, _)) in placed.iter().enumerate() {
            if tree.roots().get(i) != Some(section) {
                tree.mov_to(*section, None::<TreeID>, i)?;
            }
        }
        for ((section, beats), value) in placed.iter().zip(&sections) {
            let meta = tree.get_meta(*section)?;
            put_opt(&meta, "id", value.get("id"))?;
            put_opt(&meta, "title", value.get("title"))?;
            let values = value.get("beats").and_then(Value::as_array).cloned().unwrap_or_default();
            for (beat, value) in beats.iter().zip(&values) {
                let meta = tree.get_meta(*beat)?;
                for key in BEAT_FIELDS {
                    put_opt(&meta, key, value.get(key))?;
                }
                match value.get("notes") {
                    Some(Value::String(notes)) => put_text(&meta, "notes", notes)?,
                    Some(other) => put_json(&meta, "notes", other)?,
                    None => drop_key(&meta, "notes")?,
                }
            }
        }
        Ok(())
    }
}

/// Makes a movable list of strings read `target`: removes what it does not hold, moves what
/// it does into place, and inserts the rest.
fn sync_values(list: &LoroMovableList, target: &[String]) -> Result<()> {
    let mut now: Vec<String> =
        list.to_vec().into_iter().map(|v| v.as_string().map(|s| s.to_string()).unwrap_or_default()).collect();
    let wanted: HashSet<&String> = target.iter().collect();
    for i in (0..now.len()).rev() {
        // Each key once, the first time it is listed.
        let first = now.iter().position(|k| *k == now[i]) == Some(i);
        if !wanted.contains(&now[i]) || !first {
            list.delete(i, 1)?;
            now.remove(i);
        }
    }
    for (i, key) in target.iter().enumerate() {
        match now.iter().position(|k| k == key) {
            Some(j) if j == i => {}
            Some(j) => {
                list.mov(j, i)?;
                let moved = now.remove(j);
                now.insert(i, moved);
            }
            None => {
                list.insert(i, key.as_str())?;
                now.insert(i, key.clone());
            }
        }
    }
    Ok(())
}

/// Makes a movable list of maps hold, in order, the maps `target` names by their index in
/// `there` (the maps it holds now), and a new map for each `None`. Returns the maps, in order.
fn sync_maps(list: &LoroMovableList, there: &[loro::ContainerID], target: &[Option<usize>]) -> Result<Vec<LoroMap>> {
    let kept: HashSet<usize> = target.iter().flatten().copied().collect();
    let mut now: Vec<Option<loro::ContainerID>> = (0..list.len())
        .map(|i| match list.get(i) {
            Some(ValueOrContainer::Container(Container::Map(m))) => Some(m.id()),
            _ => None,
        })
        .collect();
    for i in (0..now.len()).rev() {
        let keep = now[i].as_ref().and_then(|id| there.iter().position(|t| t == id)).is_some_and(|j| kept.contains(&j));
        if !keep {
            list.delete(i, 1)?;
            now.remove(i);
        }
    }
    let mut out = Vec::with_capacity(target.len());
    for (i, want) in target.iter().enumerate() {
        match want {
            Some(j) => {
                let id = &there[*j];
                let at = now
                    .iter()
                    .position(|n| n.as_ref() == Some(id))
                    .ok_or_else(|| CrdtError::Shape("a state went missing".into()))?;
                if at != i {
                    list.mov(at, i)?;
                    let moved = now.remove(at);
                    now.insert(i, moved);
                }
            }
            None => {
                let map = list.insert_container(i, LoroMap::new())?;
                now.insert(i, Some(map.id()));
            }
        }
        match list.get(i) {
            Some(ValueOrContainer::Container(Container::Map(m))) => out.push(m),
            _ => return Err(CrdtError::Shape("a state is not a map".into())),
        }
    }
    Ok(out)
}

// --- reading ------------------------------------------------------------------------------

impl DeckDoc {
    /// The deck as JSON, in the shape deck.json has.
    fn read(&self) -> Result<Value> {
        let root = self.doc.get_map("deck");
        let mut deck = Map::new();
        let field = |key: &str| json_at(&root, key);
        if let Some(v) = field("scaena")? {
            deck.insert("scaena".into(), v);
        }
        if root.get("meta").is_some() {
            let map = self.doc.get_map("meta");
            let mut meta = Map::new();
            for key in ordered_keys(&map)? {
                if let Some(v) = json_at(&map, &key)? {
                    meta.insert(key, v);
                }
            }
            deck.insert("meta".into(), Value::Object(meta));
        }
        for key in ["canvas", "formats", "theme", "fonts"] {
            if let Some(v) = field(key)? {
                deck.insert(key.into(), v);
            }
        }
        let data_map = self.doc.get_map("data");
        let mut data = Map::new();
        for key in ordered_keys(&data_map)? {
            if let Some(v) = json_at(&data_map, &key)? {
                data.insert(key, v);
            }
        }
        deck.insert("data".into(), Value::Object(data));
        if root.get("spine").is_some() {
            deck.insert("spine".into(), self.read_spine()?);
        }
        let mut keys = Keys::default();
        let held_nodes = self.node_ids()?;
        for (key, id) in &held_nodes {
            keys.by_id.insert(id.clone(), key.clone());
            keys.by_key.insert(key.clone(), id.clone());
        }
        let nodes_map = self.doc.get_map("nodes");
        let mut nodes = Map::new();
        for (key, _) in &held_nodes {
            let Some(ValueOrContainer::Container(Container::Map(node))) = nodes_map.get(key) else { continue };
            let mut fields = Map::new();
            if let Some(t) = json_at(&node, TYPE_KEY)? {
                fields.insert("type".into(), t);
            }
            fields.extend(read_props(&node, &keys)?);
            nodes.insert(keys.by_key[key].clone(), Value::Object(fields));
        }
        deck.insert("nodes".into(), Value::Object(nodes));
        let list = self.doc.get_movable_list("states");
        let mut states = Vec::with_capacity(list.len());
        for i in 0..list.len() {
            let Some(ValueOrContainer::Container(Container::Map(map))) = list.get(i) else { continue };
            states.push(read_state(&map, &keys)?);
        }
        deck.insert("states".into(), Value::Array(states));
        deck.insert("overrides".into(), Value::Object(read_deltas(&self.doc.get_map("overrides"), &keys)?));
        if let Some(v) = field("_comment")? {
            deck.insert("_comment".into(), v);
        }
        Ok(Value::Object(deck))
    }

    fn read_spine(&self) -> Result<Value> {
        let tree = self.doc.get_tree("spine");
        let mut sections = Vec::new();
        for section in tree.roots() {
            let meta = tree.get_meta(section)?;
            let mut s = Map::new();
            for key in ["id", "title"] {
                if let Some(v) = json_at(&meta, key)? {
                    s.insert(key.into(), v);
                }
            }
            let mut beats = Vec::new();
            for beat in tree.children(section).unwrap_or_default() {
                let meta = tree.get_meta(beat)?;
                let mut b = Map::new();
                for key in BEAT_FIELDS.iter().copied().chain(["notes"]) {
                    if let Some(v) = json_at(&meta, key)? {
                        b.insert(key.into(), v);
                    }
                }
                beats.push(Value::Object(b));
            }
            s.insert("beats".into(), Value::Array(beats));
            sections.push(Value::Object(s));
        }
        let mut spine = Map::new();
        spine.insert("sections".into(), Value::Array(sections));
        Ok(Value::Object(spine))
    }
}

fn read_state(map: &LoroMap, keys: &Keys) -> Result<Value> {
    let to_id = |key: &str| keys.id(key);
    let mut state = Map::new();
    for key in STATE_FIELDS.iter().copied().chain(["notes", "_comment"]) {
        let Some(value) = json_at(map, key)? else { continue };
        let value = match key {
            "remove" => map_ids(&value, &to_id),
            "choreography" => map_choreography(&value, &to_id),
            _ => value,
        };
        state.insert(key.into(), value);
    }
    if let Some(ValueOrContainer::Container(Container::Map(props))) = map.get("props") {
        state.insert("props".into(), Value::Object(read_deltas(&props, keys)?));
    }
    Ok(Value::Object(state))
}
