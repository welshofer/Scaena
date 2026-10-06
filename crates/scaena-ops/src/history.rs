//! A bundle's versions (PLAN 2.60, SPEC §8): each change its history keeps, by author and
//! time; the deck as it was just after one; two compared, state by state; and one made the
//! deck again, as one change that undoes as any other.

use crate::inspect::Change;
use crate::lint::{View, Why, Write, errors, lint, lint_in, write};
use crate::{Bundle, Context, OpsError};
use indexmap::IndexMap;
use scaena_core::document::Props;
use scaena_core::format::DateTime;
use scaena_core::lint::{Delta, delta};
use scaena_core::validate::validate_bundle;
use scaena_core::{Deck, Finding};
use scaena_engine::cascade::with_overrides;
use scaena_store::crdt::DeckDoc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// A version of the deck: as it was just after one change its history keeps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Version {
    /// Its place in the history, oldest first, from 1.
    pub n: usize,
    /// What names it for as long as the history lasts: its change's id, `counter@peer`.
    pub id: String,
    /// Who made the change: `user`, `agent:<name>`, or `fs`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// What the change did, as what made it says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// When it was made, in RFC 3339, UTC.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// How many operations it holds: a character typed is one.
    pub ops: usize,
}

/// The versions `doc` holds, oldest first.
pub fn listed(doc: &DeckDoc) -> Vec<Version> {
    let version = |(i, c): (usize, scaena_store::crdt::Change)| Version {
        n: i + 1,
        id: c.id,
        author: c.author,
        message: c.message,
        at: (c.timestamp > 0).then(|| rfc3339(c.timestamp)),
        ops: c.ops,
    };
    doc.changes().into_iter().enumerate().map(version).collect()
}

/// `secs` since 1970 in RFC 3339, UTC.
pub fn rfc3339(secs: i64) -> String {
    let c = DateTime(secs).civil();
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", c.year, c.month, c.day, c.hour, c.minute, c.second)
}

/// The bundle's history, with the deck and the files it is drawn from as they are now taken in;
/// an error that says how to begin one where it keeps none.
fn kept(b: &Bundle) -> Result<DeckDoc, OpsError> {
    b.history()
        .context("reading the bundle's history")?
        .ok_or_else(|| OpsError::new("the bundle keeps no history: `scaena save --history` begins one (SPEC §8)"))
}

/// The bundle's versions, oldest first.
pub fn versions(b: &Bundle) -> Result<Vec<Version>, OpsError> {
    Ok(listed(&kept(b)?))
}

/// The version of `versions` that `name` names: its number as listed (`3`, or `#3`), or its id.
pub fn named<'a>(versions: &'a [Version], name: &str) -> Result<&'a Version, OpsError> {
    let n = name.strip_prefix('#').unwrap_or(name).parse::<usize>().ok();
    versions.iter().find(|v| Some(v.n) == n || v.id == name).ok_or_else(|| {
        OpsError::new(format!(
            "no version `{name}`: the history holds {} (`scaena history` lists them)",
            versions.len()
        ))
    })
}

/// The deck in `doc`'s version `v`, and the files it is drawn from, its data files and its
/// theme, as the history held them then (ADR-0014, ADR-0016): a history from before it kept the
/// theme holds none.
pub fn then(doc: &DeckDoc, v: &Version) -> Result<(Deck, BTreeMap<String, Vec<u8>>), OpsError> {
    let at =
        doc.at(&v.id).context("reading the history")?.ok_or_else(|| OpsError::new(format!("no version {}", v.id)))?;
    let deck = at.deck().with_context(|| format!("the deck in version {}", v.n))?;
    let mut held = at.files();
    let named = scaena_store::kept_paths(&deck);
    held.retain(|path, _| named.contains(&path.as_str()));
    Ok((deck, held))
}

/// A version, and the deck as it was then: what `scaena history --at` prints.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Seen {
    pub version: Version,
    /// The deck as it was, canonical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deck: Option<Value>,
    /// With `scn`, the deck as it was, as `.scn` (SPEC §4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scn: Option<String>,
    /// The files it was drawn from that the history held then, its data files and its theme,
    /// by their paths.
    pub files: Vec<String>,
}

/// The deck as it was in the version `name` names.
pub fn at(b: &Bundle, name: &str, scn: bool) -> Result<Seen, OpsError> {
    let doc = kept(b)?;
    let all = listed(&doc);
    let version = named(&all, name)?.clone();
    let (deck, files) = then(&doc, &version)?;
    let files = files.into_keys().collect();
    Ok(match scn {
        true => Seen { version, deck: None, scn: Some(scaena_core::dsl::decompile(&deck)), files },
        false => Seen { version, deck: Some(deck.to_value()?), scn: None, files },
    })
}

/// Each state that changed, by id.
pub type StateChanges = IndexMap<String, StateChange>;

/// Fields that changed, each as the later version has it: null where it is gone.
pub type Fields = IndexMap<String, Value>;

/// What changed in one state from one version of the deck to another.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum StateChange {
    /// The later version has it, and the earlier did not.
    Added(bool),
    /// The earlier version had it, and the later does not.
    Removed(bool),
    /// Both have it, and it shows otherwise.
    Changed(Changed),
}

/// What changed in a state both versions have.
#[derive(Debug, Clone, PartialEq, Default, Serialize, JsonSchema)]
pub struct Changed {
    /// Its own fields that changed (its layout, transition, choreography, hold, notes, …),
    /// each as the later version has it; null where it is gone.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub fields: Fields,
    /// The nodes it shows that changed, resolved with the deck's overrides, as `diff` says them:
    /// one that enters, with its props; one that exits; one whose props change, each as the
    /// later version has it, and null where it is gone.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub nodes: IndexMap<String, Change>,
}

/// What changed from one version of the deck to another: what `scaena history --diff` prints.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Compared {
    pub from: Version,
    /// The later version; none for the deck as it is now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Version>,
    /// Each state that changed, by id: in the later version's order, then those it lacks.
    pub states: StateChanges,
    /// The deck's own fields that changed (its theme, canvas, formats, fonts, data sources,
    /// spine, …), each as the later version has it; null where it is gone. `order` is the
    /// later version's states, where those both have run in another order.
    pub deck: Fields,
    /// The files it is drawn from whose bytes changed, data files and the theme, by their
    /// paths.
    pub files: Vec<String>,
}

/// What changed from `from` to `to`, state by state, and in the deck's own fields.
pub fn compare(from: &Deck, to: &Deck) -> Result<(StateChanges, Fields), OpsError> {
    let was = scaena_core::resolve_states(from).context("tracking the earlier version")?;
    let is = scaena_core::resolve_states(to).context("tracking the later version")?;
    // A state's own fields: what it says of itself, beside what its nodes show.
    let own = |deck: &Deck, id: &str| -> serde_json::Map<String, Value> {
        let state = deck.states.iter().find(|s| s.id == id).and_then(|s| serde_json::to_value(s).ok());
        let mut fields = state.and_then(|v| v.as_object().cloned()).unwrap_or_default();
        for drawn in ["id", "props", "remove"] {
            fields.remove(drawn);
        }
        fields
    };
    let mut states = IndexMap::new();
    for snap in &is {
        let Some(then) = was.iter().find(|w| w.state_id == snap.state_id) else {
            states.insert(snap.state_id.clone(), StateChange::Added(true));
            continue;
        };
        let changed = Changed {
            fields: changes(&own(from, &then.state_id), &own(to, &snap.state_id)),
            nodes: nodes(&with_overrides(from, then).nodes, &with_overrides(to, snap).nodes),
        };
        if changed != Changed::default() {
            states.insert(snap.state_id.clone(), StateChange::Changed(changed));
        }
    }
    for gone in was.iter().filter(|w| !is.iter().any(|s| s.state_id == w.state_id)) {
        states.insert(gone.state_id.clone(), StateChange::Removed(true));
    }
    let deck_fields = |deck: &Deck| -> Result<serde_json::Map<String, Value>, OpsError> {
        let mut fields = deck.to_value()?.as_object().cloned().unwrap_or_default();
        for drawn in ["nodes", "states", "overrides"] {
            fields.remove(drawn);
        }
        Ok(fields)
    };
    let mut deck = changes(&deck_fields(from)?, &deck_fields(to)?);
    let both = |a: &Deck, b: &Deck| -> Vec<String> {
        a.states.iter().filter(|s| b.states.iter().any(|t| t.id == s.id)).map(|s| s.id.clone()).collect()
    };
    if both(from, to) != both(to, from) {
        deck.insert("order".into(), to.states.iter().map(|s| Value::from(s.id.clone())).collect());
    }
    Ok((states, deck))
}

/// The keys whose values differ from `before` to `after`, each as `after` has it: null where
/// it is gone.
fn changes(before: &serde_json::Map<String, Value>, after: &serde_json::Map<String, Value>) -> Fields {
    let mut out: Fields =
        after.iter().filter(|(k, v)| before.get(*k) != Some(*v)).map(|(k, v)| (k.clone(), v.clone())).collect();
    for gone in before.keys().filter(|k| !after.contains_key(*k)) {
        out.insert(gone.clone(), Value::Null);
    }
    out
}

/// The nodes that change from `before` to `after`, as `diff` says them, a prop gone as null.
fn nodes(before: &IndexMap<String, Props>, after: &IndexMap<String, Props>) -> IndexMap<String, Change> {
    let mut out = IndexMap::new();
    for (id, props) in after {
        match before.get(id) {
            None => drop(out.insert(id.clone(), Change::Enter(props.clone()))),
            Some(was) if was != props => {
                let mut delta: Props = props
                    .iter()
                    .filter(|(k, v)| was.get(*k) != Some(*v))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for gone in was.keys().filter(|k| !props.contains_key(*k)) {
                    delta.insert(gone.clone(), Value::Null);
                }
                out.insert(id.clone(), Change::Change(delta));
            }
            _ => {}
        }
    }
    for id in before.keys().filter(|id| !after.contains_key(*id)) {
        out.insert(id.clone(), Change::Exit(true));
    }
    out
}

/// The files whose bytes differ between `a`, a deck and the files it is drawn from, and `b`, by
/// their paths: one either holds and the other does not, too. A file a deck names whose bytes
/// its side does not hold is not compared: a history from before it kept the theme (ADR-0016)
/// says nothing of how the theme was.
pub fn differing(a: (&Deck, &BTreeMap<String, Vec<u8>>), b: (&Deck, &BTreeMap<String, Vec<u8>>)) -> Vec<String> {
    let unknown = |(deck, files): (&Deck, &BTreeMap<String, Vec<u8>>), path: &str| {
        !files.contains_key(path) && scaena_store::kept_paths(deck).contains(&path)
    };
    let paths: BTreeSet<&String> = a.1.keys().chain(b.1.keys()).collect();
    (paths.into_iter()).filter(|p| !unknown(a, p) && !unknown(b, p) && a.1.get(*p) != b.1.get(*p)).cloned().collect()
}

/// What changed from the version `from` names to the one `to` names, or, without `to`, to the
/// deck and the files it is drawn from as they are now.
pub fn diff(b: &Bundle, from: &str, to: Option<&str>) -> Result<Compared, OpsError> {
    let doc = kept(b)?;
    let all = listed(&doc);
    let earlier = named(&all, from)?.clone();
    let (then_deck, then_files) = then(&doc, &earlier)?;
    let (later, now_deck, now_files) = match to {
        Some(name) => {
            let later = named(&all, name)?.clone();
            let (deck, files) = then(&doc, &later)?;
            (Some(later), deck, files)
        }
        None => (None, b.deck.clone(), b.kept_files(&b.deck, &BTreeMap::new()).into_iter().collect()),
    };
    let (states, deck) = compare(&then_deck, &now_deck)?;
    let files = differing((&then_deck, &then_files), (&now_deck, &now_files));
    Ok(Compared { from: earlier, to: later, states, deck, files })
}

/// What restoring a version did, or would do: what `scaena history --restore` prints.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Restored {
    pub version: Version,
    /// Whether the bundle was written: not on a dry run, nor when the version is refused.
    pub applied: bool,
    /// The files written as the version held them, data files and the theme, by their paths.
    pub files: Vec<String>,
    /// What `validate` and `lint` find after that they did not before.
    pub added: Vec<Finding>,
    /// What they found before that they do not after.
    pub removed: Vec<Finding>,
    /// The findings that are errors, after.
    pub errors: usize,
    /// The states it changes what shows in, by id, as `patch` says them.
    pub states: Vec<String>,
    /// Refused: the version's deck would not validate in the bundle as it is now (in `added`).
    #[serde(skip)]
    pub refused: bool,
}

/// Make the version `name` names the deck again, with the files it was drawn from, its data
/// files and its theme, as the history held them: one change, by the bundle's author, refused
/// as `patch` refuses one where the deck would not validate in the bundle as it is now (a file
/// it names gone). A dry run writes nothing.
pub fn restore(b: &Bundle, name: &str, dry_run: bool) -> Result<Restored, OpsError> {
    let (mut restored, write_it) = restoring(b, name)?;
    match write_it {
        Some(w) if !dry_run => write(b, w)?,
        _ => restored.applied &= !dry_run,
    }
    Ok(restored)
}

/// [`restore`] with nothing written: what restoring does, and what to write, if it changes
/// the deck or a file it is drawn from and is not refused. A client that keeps its bundle in
/// memory writes it there.
pub fn restoring(b: &Bundle, name: &str) -> Result<(Restored, Option<Write>), OpsError> {
    let doc = kept(b)?;
    let all = listed(&doc);
    let version = named(&all, name)?.clone();
    let (deck, held) = then(&doc, &version)?;
    restored(b, version, deck, held)
}

/// `deck`, version `version`'s, made the bundle's deck again with `held`, the files it was
/// drawn from.
pub fn restored(
    b: &Bundle,
    version: Version,
    deck: Deck,
    held: BTreeMap<String, Vec<u8>>,
) -> Result<(Restored, Option<Write>), OpsError> {
    let files: BTreeMap<String, Vec<u8>> =
        held.into_iter().filter(|(path, bytes)| b.files.read(path).ok().as_ref() != Some(bytes)).collect();
    let mut view = View::of(b);
    for (path, bytes) in &files {
        view = view.with(path.clone(), bytes.clone());
    }
    let was: Vec<&str> = b.deck.states.iter().map(|s| s.id.as_str()).collect();
    let is: Vec<&str> = deck.states.iter().map(|s| s.id.as_str()).collect();
    let text = deck.to_json()?;
    let invalid = validate_bundle(&b.deck.to_json()?, &b.files)?;
    let invalid_after = validate_bundle(&text, &view)?;
    let refused = !delta(&invalid, &was, &invalid_after, &is, &[]).added.is_empty();
    let (before, after, states) = match refused {
        true => (invalid, invalid_after, Vec::new()),
        false => (lint(b)?.findings, lint_in(&deck, &view)?.findings, crate::patch::changed(&b.deck, &deck)?),
    };
    let Delta { added, removed } = delta(&before, &was, &after, &is, &[]);
    let restored = Restored {
        applied: !refused,
        files: files.keys().cloned().collect(),
        added: added.into_iter().cloned().collect(),
        removed: removed.into_iter().cloned().collect(),
        errors: errors(&after),
        states,
        refused,
        version,
    };
    let changes = text != b.deck.to_json()? || !restored.files.is_empty();
    let write_it = (!refused && changes).then(|| Write {
        why: Why::new(format!("history --restore {}", restored.version.id)),
        deck,
        files,
    });
    Ok((restored, write_it))
}

/// What is asked of a bundle's history: one of `at`, `compare`, and `restore`, or none of them,
/// for its versions. What `scaena history` and `deck_history` take.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct Ask {
    /// The deck as it was in this version: its number as listed (`3`), or its id.
    #[serde(default)]
    pub at: Option<String>,
    /// With `at`, the deck as `.scn` source (SPEC §4).
    #[serde(default)]
    pub scn: bool,
    /// What changed from the first version to the second, or, with one, to the deck as it is
    /// now, state by state.
    #[serde(default)]
    pub compare: Vec<String>,
    /// Make this version the deck again, with its data files and its theme as they were: one
    /// change.
    #[serde(default)]
    pub restore: Option<String>,
    /// With `restore`, say what it would change, and write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// What a bundle's history answers: its versions, one seen, two compared, or one restored.
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct History {
    /// Its versions, oldest first, when nothing else is asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versions: Option<Vec<Version>>,
    /// With `at`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen: Option<Seen>,
    /// With `compare`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compared: Option<Compared>,
    /// With `restore`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restored: Option<Restored>,
}

/// What `ask` asks of the bundle's history.
pub fn history(b: &Bundle, ask: &Ask) -> Result<History, OpsError> {
    let asked = [ask.at.is_some(), !ask.compare.is_empty(), ask.restore.is_some()];
    if asked.iter().filter(|a| **a).count() > 1 {
        return Err(OpsError::new("ask one of `at`, `compare`, and `restore` at a time"));
    }
    if let Some(name) = &ask.at {
        return Ok(History { seen: Some(at(b, name, ask.scn)?), ..History::default() });
    }
    if let Some(name) = &ask.restore {
        return Ok(History { restored: Some(restore(b, name, ask.dry_run)?), ..History::default() });
    }
    let compared = match ask.compare.as_slice() {
        [] => return Ok(History { versions: Some(versions(b)?), ..History::default() }),
        [from] => diff(b, from, None)?,
        [from, to] => diff(b, from, Some(to))?,
        _ => return Err(OpsError::new("compare one version with the deck as it is now, or two versions")),
    };
    Ok(History { compared: Some(compared), ..History::default() })
}
