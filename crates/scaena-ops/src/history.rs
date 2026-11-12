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
use scaena_store::crdt::{DeckDoc, Edit};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

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
        .ok_or_else(|| {
            OpsError::new("the bundle keeps no history: `deck_history` with `begin`, or `scaena save --history`, begins one (SPEC §8)")
        })
}

/// The bundle's versions, oldest first.
pub fn versions(b: &Bundle) -> Result<Vec<Version>, OpsError> {
    Ok(listed(&kept(b)?))
}

/// The bundle's history, begun where it keeps none (SPEC §8): its first change, by the bundle's
/// author, holds the deck and the files it is drawn from as they are, and every write after is
/// recorded in it. Nothing else of the bundle changes: no file is renamed or subset, as a save
/// would. `message` says what the first change is: `history begins`, or `deck_create` for a
/// bundle just made. Whether it was begun here, and the history.
pub fn begin(b: &Bundle, message: &str) -> Result<(bool, DeckDoc), OpsError> {
    if let Some(doc) = b.history().context("reading the bundle's history")? {
        return Ok((false, doc));
    }
    let edit = Edit { message: Some(message), ..Edit::by(&b.author) };
    let doc = DeckDoc::begin(&b.deck, &b.kept_files(&b.deck, &BTreeMap::new()), &edit)
        .context("beginning the bundle's history")?;
    let bytes = doc.save().context("writing the bundle's history")?;
    b.write(&BTreeMap::from([(scaena_store::HISTORY.to_string(), bytes)]))
        .with_context(|| format!("writing {}", b.root.display()))?;
    Ok((true, doc))
}

/// A version as `deck_history` and `scaena history` list it: as the history keeps it, with what
/// its change did and who made it in a person's words, as the editors say them
/// (`scaena_core::words::change`, `author`), and what it changed, slide by slide, each object by
/// what it shows: "Slide 3: “Revenue doubled”".
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Told {
    #[serde(flatten)]
    pub version: Version,
    pub said: String,
    pub by: String,
    /// What it changed from the version before, as an editor names it: for the last [`TOLD`]
    /// versions alone, each read by checking two versions out; none for the first, which holds the
    /// deck whole.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<String>,
}

/// At most this many of what a version changed are named.
const NAMED: usize = 8;

/// The most recent versions whose changes are told: telling one checks it and the one before it
/// out and compares them, and a listing of 200 versions of B1 so took about 1.9 s (review on #585).
const TOLD: usize = 25;

/// Each version of `doc`, told: what each change did, by whom, and what it changed from the one
/// before it.
pub fn told(doc: &DeckDoc) -> Result<Vec<Told>, OpsError> {
    let versions = listed(doc);
    // The version before the first told, its deck the base of the first comparison.
    let first = versions.len().saturating_sub(TOLD);
    let mut out = Vec::new();
    let mut before: Option<Deck> = match first.checked_sub(1) {
        Some(base) => Some(then(doc, &versions[base])?.0),
        None => None,
    };
    for (i, version) in versions.into_iter().enumerate() {
        let mut said = scaena_core::words::change(version.message.as_deref().unwrap_or_default());
        let by = scaena_core::words::author(version.author.as_deref().unwrap_or_default());
        let mut changed = Vec::new();
        if i >= first {
            let (deck, _) = then(doc, &version)?;
            if let Some(was) = &before {
                let summary;
                (changed, summary) = touched(was, &deck)?;
                // A patch that changed nothing but what slides say of themselves, their notes or
                // their advance, says so, never "A slide changed".
                if let Some(summary) =
                    summary.filter(|_| version.message.as_deref().is_some_and(|m| m.starts_with("patch: ")))
                {
                    said = summary;
                }
            }
            before = Some(deck);
        }
        out.push(Told { version, said, by, changed });
    }
    Ok(out)
}

/// What changed from `was` to `is`, as an editor names it (R1's agent dogfood 6): each slide added,
/// taken away, or written again in its place; each object changed on a slide, by what it shows
/// there, "Slide 3: “Revenue doubled”"; and what else a slide changed, its own or its beat's,
/// each with every slide it changed on, "Presenter Notes on Slide 1, Slide 2, and Slide 4" and
/// "Advance on Slide 9". The spine is named only for what no beat's slides tell. Each once,
/// [`NAMED`] at most, "and N more" past them; and, where it changed nothing but such fields, a
/// sentence that says so for the version's `said`.
fn touched(was: &Deck, is: &Deck) -> Result<(Vec<String>, Option<String>), OpsError> {
    let (states, deck) = compare(was, is)?;
    let snaps = scaena_core::resolve_states(is).context("tracking the later version")?;
    let mut said: Vec<String> = Vec::new();
    let push = |said: &mut Vec<String>, what: String| {
        if !said.contains(&what) {
            said.push(what);
        }
    };
    // Each field a slide changed, by its label, with the slides it changed on, in the deck's order.
    let mut fields: Vec<(String, Vec<String>)> = Vec::new();
    let mut on = |label: String, slide: String| match fields.iter_mut().find(|(l, _)| *l == label) {
        Some((_, slides)) if slides.contains(&slide) => {}
        Some((_, slides)) => slides.push(slide),
        None => fields.push((label, vec![slide])),
    };
    let added: Vec<String> = states
        .iter()
        .filter(|(_, c)| matches!(c, StateChange::Added(_)))
        .map(|(id, _)| scaena_core::words::slide(is, id))
        .collect();
    let removed: Vec<String> = states
        .iter()
        .filter(|(_, c)| matches!(c, StateChange::Removed(_)))
        .map(|(id, _)| scaena_core::words::slide(was, id))
        .collect();
    let mut others = false;
    for (state, change) in &states {
        match change {
            // A slide taken away and one added where it stood: the slide written again.
            StateChange::Added(_) => {
                let slide = scaena_core::words::slide(is, state);
                if removed.contains(&slide) {
                    push(&mut said, format!("{slide} written again"));
                } else {
                    push(&mut said, format!("{slide} added"));
                }
                others = true;
            }
            StateChange::Removed(_) => {
                let slide = scaena_core::words::slide(was, state);
                if !added.contains(&slide) {
                    push(&mut said, format!("{slide} taken away"));
                }
                others = true;
            }
            StateChange::Changed(changed) => {
                let slide = scaena_core::words::slide(is, state);
                let shown = snaps.iter().find(|s| s.state_id == *state).map(|s| with_overrides(is, s).nodes);
                for field in changed.fields.keys() {
                    on(scaena_core::words::label(field), slide.clone());
                }
                for id in changed.nodes.keys() {
                    let object = match &shown {
                        Some(shown) if is.nodes.contains_key(id) => scaena_core::words::object_in(is, shown, id),
                        _ => scaena_core::words::kind(was.nodes.get(id).map(|n| n.node_type)).to_string(),
                    };
                    push(&mut said, format!("{slide}: \u{201c}{object}\u{201d}"));
                    others = true;
                }
            }
        }
    }
    // A beat's own fields, its notes and its claim among them, on each slide it tells.
    let mut spine = false;
    for field in deck.keys() {
        if field == "spine" {
            let (told, rest) = beats(was, is);
            for (label, state) in told {
                if is.states.iter().any(|s| s.id == state) {
                    // A beat's field is its slide's, whichever of its steps the beat names.
                    let slide = scaena_core::words::slide(is, &state);
                    on(label, slide.split(", step ").next().unwrap_or(&slide).to_string());
                }
            }
            spine = rest;
            continue;
        }
        push(&mut said, format!("the deck's {}", scaena_core::words::label(field).to_lowercase()));
        others = true;
    }
    if spine {
        push(&mut said, "the deck's spine".into());
        others = true;
    }
    let summary = (!others && !fields.is_empty()).then(|| {
        let each: Vec<String> = fields
            .iter()
            .map(|(label, slides)| match slides.len() {
                1 => format!("{label} changed on {}", slides[0]),
                n => format!("{label} changed on {n} slides"),
            })
            .collect();
        each.join("; ")
    });
    let order = scaena_core::words::slides(is);
    for (label, mut slides) in fields {
        scaena_core::sort::by_key(&mut slides, |slide| order.iter().position(|o| o == slide));
        push(&mut said, format!("{label} on {}", and(&slides)));
    }
    if said.len() > NAMED {
        let more = said.len() - NAMED;
        said.truncate(NAMED);
        said.push(format!("and {more} more"));
    }
    Ok((said, summary))
}

/// `items` in a sentence: "a", "a and b", "a, b, and c".
fn and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// What changed in the spine's beats from `was` to `is`, as a slide's: each field of a beat both
/// have that changed, its label with each state the beat tells in `is`, "Presenter Notes" for its
/// notes; and whether anything else changed, a beat or a section added, taken away, moved, or
/// retitled, or the states a beat tells.
fn beats(was: &Deck, is: &Deck) -> (Vec<(String, String)>, bool) {
    let all = |deck: &Deck| -> Vec<scaena_core::document::Beat> {
        deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| s.beats.iter().cloned()).collect()
    };
    let (then, now) = (all(was), all(is));
    let mut told = Vec::new();
    for beat in &now {
        let Some(old) = then.iter().find(|b| b.id == beat.id) else { continue };
        let mut changed = Vec::new();
        if old.notes != beat.notes {
            changed.push(scaena_core::words::label("notes"));
        }
        if old.claim != beat.claim {
            changed.push("Claim".to_string());
        }
        if old.duration != beat.duration {
            changed.push("Time".to_string());
        }
        if old.evidence != beat.evidence {
            changed.push("Evidence".to_string());
        }
        if old.media != beat.media {
            changed.push("Plan".to_string());
        }
        for label in changed {
            told.extend(beat.states.iter().map(|state| (label.clone(), state.clone())));
        }
    }
    // The spine with each beat's own fields taken away: what changed of it but those.
    let bare = |deck: &Deck| -> Option<Value> {
        let mut spine = serde_json::to_value(deck.spine.as_ref()?).ok()?;
        for section in spine.get_mut("sections")?.as_array_mut()? {
            for beat in section.get_mut("beats").and_then(Value::as_array_mut).into_iter().flatten() {
                if let Some(beat) = beat.as_object_mut() {
                    for field in ["claim", "notes", "duration", "evidence", "media"] {
                        beat.remove(field);
                    }
                }
            }
        }
        Some(spine)
    };
    let rest = bare(was) != bare(is) || told.is_empty();
    (told, rest)
}

/// The version of `versions` that `name` names: its number as listed (`3`, or `#3`), its id, or
/// how far back from the latest it is (`-1`, or `~1`, the one before the latest, which restored
/// undoes the last change; lift-up 20261025c FUNC-5).
pub fn named<'a>(versions: &'a [Version], name: &str) -> Result<&'a Version, OpsError> {
    let back = name.strip_prefix('-').or_else(|| name.strip_prefix('~')).and_then(|n| n.parse::<usize>().ok());
    if let Some(back) = back.filter(|&n| n > 0) {
        let latest = versions.iter().map(|v| v.n).max().unwrap_or(0);
        let n = latest.checked_sub(back).filter(|&n| n >= 1);
        return n.and_then(|n| versions.iter().find(|v| v.n == n)).ok_or_else(|| {
            OpsError::new(format!(
                "no version `{name}`: the history holds {latest}, so `-{}` is the furthest back",
                latest.saturating_sub(1)
            ))
        });
    }
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
    Changed(Box<Changed>),
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
    /// What each of those was in the earlier version: its own fields that changed, each as the
    /// earlier version had it, null where it had none; and each node's props that changed or
    /// left, as it had them, null where it had none.
    #[serde(skip_serializing_if = "Before::is_empty")]
    pub before: Before,
}

/// What the earlier version had of what changed.
#[derive(Debug, Clone, PartialEq, Default, Serialize, JsonSchema)]
pub struct Before {
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub fields: Fields,
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub nodes: IndexMap<String, Props>,
}

impl Before {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.nodes.is_empty()
    }
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
    /// later version's states, where those both have run in another order. The spine is said by
    /// what changed of it, each beat's fields that changed by its id under `beats` and the
    /// outline under `sections` where it changed; the fonts by the files added, `before`
    /// holding those taken away.
    pub deck: Fields,
    /// Each of those as the earlier version had it; null where it had none.
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub before: Fields,
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
        let (own_was, own_is) = (own(from, &then.state_id), own(to, &snap.state_id));
        let (shown_was, shown_is) = (with_overrides(from, then).nodes, with_overrides(to, snap).nodes);
        let fields = changes(&own_was, &own_is);
        let nodes = nodes(&shown_was, &shown_is);
        let before = Before { fields: had(&own_was, fields.keys()), nodes: had_nodes(&shown_was, &nodes) };
        let changed = Changed { fields, nodes, before };
        if changed != Changed::default() {
            states.insert(snap.state_id.clone(), StateChange::Changed(Box::new(changed)));
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

/// Each of `keys` as `before` has it: null where it has none.
fn had<'k>(before: &serde_json::Map<String, Value>, keys: impl Iterator<Item = &'k String>) -> Fields {
    keys.map(|k| (k.clone(), before.get(k).cloned().unwrap_or(Value::Null))).collect()
}

/// What each node of `changed` had in `before`: the props that changed or left, as it had them,
/// null where it had none; a node that entered had nothing.
fn had_nodes(before: &IndexMap<String, Props>, changed: &IndexMap<String, Change>) -> IndexMap<String, Props> {
    let mut out = IndexMap::new();
    for (id, change) in changed {
        let Some(was) = before.get(id) else { continue };
        let props = match change {
            Change::Change(delta) => {
                delta.keys().map(|k| (k.clone(), was.get(k).cloned().unwrap_or(Value::Null))).collect()
            }
            Change::Exit(_) => was.clone(),
            _ => continue,
        };
        out.insert(id.clone(), props);
    }
    out
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
    let paths = scaena_core::sort::set(a.1.keys().chain(b.1.keys()));
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
    let mut was = then_deck.to_value()?.as_object().cloned().unwrap_or_default();
    was.insert("order".into(), then_deck.states.iter().map(|s| Value::from(s.id.clone())).collect());
    let mut before = had(&was, deck.keys());
    let mut deck = deck;
    // The spine and the fonts by what changed of them, never each whole twice (R1's agent
    // dogfood 6: 3 KB of ridgeline's 7.4 KB was its spine, before and after).
    if deck.contains_key("spine") {
        let (now, then) = spine_changed(&then_deck, &now_deck);
        deck.insert("spine".into(), now);
        before.insert("spine".into(), then);
    }
    if deck.contains_key("fonts") {
        let (added, gone) = fonts_changed(&then_deck, &now_deck);
        deck.insert("fonts".into(), added);
        before.insert("fonts".into(), gone);
    }
    Ok(Compared { from: earlier, to: later, states, deck, before, files })
}

/// What changed of the spine from `was` to `is`, as the later and the earlier versions have it:
/// under `beats`, each beat by its id, with each of its fields that changed (its claim, its notes,
/// the states it tells, …), a beat added whole and null for one taken away, each as the other
/// version has it; and under `sections`, where a section was added, taken away, moved, retitled,
/// or given other beats, each section's id, title, and beats' ids, the spine's outline.
fn spine_changed(was: &Deck, is: &Deck) -> (Value, Value) {
    let beats = |deck: &Deck| -> serde_json::Map<String, Value> {
        let all = deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats);
        all.filter_map(|b| Some((b.id.clone(), serde_json::to_value(b).ok()?))).collect()
    };
    // Each section as it stands, its beats by their ids: its id, its title, and any field it
    // carries beside them.
    let outline = |deck: &Deck| -> Value {
        let sections = deck.spine.iter().flat_map(|s| &s.sections);
        sections
            .map(|s| {
                let mut section = serde_json::to_value(s).unwrap_or_default();
                let beats: Vec<Value> = s.beats.iter().map(|b| Value::from(b.id.clone())).collect();
                if let Some(section) = section.as_object_mut() {
                    section.insert("beats".into(), Value::from(beats));
                }
                section
            })
            .collect()
    };
    // The spine's own fields beside its sections.
    let own = |deck: &Deck| -> serde_json::Map<String, Value> {
        let mut fields = deck.spine.as_ref().and_then(|s| serde_json::to_value(s).ok());
        let mut fields = fields.as_mut().and_then(Value::as_object_mut).map(std::mem::take).unwrap_or_default();
        fields.remove("sections");
        fields
    };
    let (then, now) = (beats(was), beats(is));
    let (mut after, mut before) = (serde_json::Map::new(), serde_json::Map::new());
    for (id, beat) in &now {
        match then.get(id) {
            None => {
                after.insert(id.clone(), beat.clone());
                before.insert(id.clone(), Value::Null);
            }
            Some(old) => {
                let empty = serde_json::Map::new();
                let (old, new) = (old.as_object().unwrap_or(&empty), beat.as_object().unwrap_or(&empty));
                let fields = changes(old, new);
                if !fields.is_empty() {
                    let had = had(old, fields.keys());
                    after.insert(id.clone(), Value::Object(fields.into_iter().collect()));
                    before.insert(id.clone(), Value::Object(had.into_iter().collect()));
                }
            }
        }
    }
    for (id, beat) in then.iter().filter(|(id, _)| !now.contains_key(*id)) {
        after.insert(id.clone(), Value::Null);
        before.insert(id.clone(), beat.clone());
    }
    let (mut later, mut earlier) = (serde_json::Map::new(), serde_json::Map::new());
    if !after.is_empty() {
        later.insert("beats".into(), Value::Object(after));
        earlier.insert("beats".into(), Value::Object(before));
    }
    let (from, to) = (outline(was), outline(is));
    if from != to {
        later.insert("sections".into(), to);
        earlier.insert("sections".into(), from);
    }
    let (then_own, now_own) = (own(was), own(is));
    let fields = changes(&then_own, &now_own);
    let had = had(&then_own, fields.keys());
    later.extend(fields);
    earlier.extend(had);
    // A spine missing on one side, or changed in what none of the above says, is given whole:
    // a comparison never shows a spine that changed as `{}`.
    if was.spine.is_none() || is.spine.is_none() || later.is_empty() {
        let whole = |d: &Deck| d.spine.as_ref().map_or(Value::Null, |s| serde_json::to_value(s).unwrap_or_default());
        return (whole(is), whole(was));
    }
    (Value::Object(later), Value::Object(earlier))
}

/// The fonts added from `was` to `is`, and those taken away, each by its file: a re-theme's
/// fonts named, never every font's axes twice.
fn fonts_changed(was: &Deck, is: &Deck) -> (Value, Value) {
    let files = |deck: &Deck| -> Vec<Value> {
        deck.to_value()
            .ok()
            .and_then(|v| v.get("fonts").cloned())
            .and_then(|f| f.as_array().cloned())
            .unwrap_or_default()
    };
    let (then, now) = (files(was), files(is));
    let named = |font: &Value| font.get("file").cloned().unwrap_or_else(|| font.clone());
    let added: Vec<Value> = now.iter().filter(|f| !then.contains(f)).map(named).collect();
    let gone: Vec<Value> = then.iter().filter(|f| !now.contains(f)).map(named).collect();
    (Value::from(added), Value::from(gone))
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
    /// With `slide`, the slide put back, by its number and its headline in the deck it makes
    /// ("Slide 2: Revenue doubled"), so a dry run says which slide it brings back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slide: Option<String>,
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

/// [`restoring_slide`], written unless `dry_run`.
pub fn restore_slide(b: &Bundle, name: &str, slide: &str, dry_run: bool) -> Result<Restored, OpsError> {
    let (mut restored, write_it) = restoring_slide(b, name, slide)?;
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

/// [`restoring`] of one slide alone (agent dogfood 7): the slide that holds `slide`, a state of
/// version `name`, put back into the deck as it is now, with everything else as it is. Its
/// states as the version held them; each node they show, where the deck lacks it now, or shows
/// it on no other slide, as the version held it (a node another slide shows stays as it is, and
/// one the slide alone showed whose id another slide gives a node of its own now comes back under
/// a fresh id, [`fresh_ids`]); each data source they read that the deck lacks, with its file; and
/// the beat that told it: one the deck has keeps its own claim, notes, and states, the slide's
/// states put back among them, and one the deck lacks comes back with its claim and notes, in its
/// part of the talk. A slide the deck still has is put back
/// where it stands; one taken away goes after the nearest slide before it in the version that
/// the deck still has, else first. Refused as a patch is where the deck would not validate.
pub fn restoring_slide(b: &Bundle, name: &str, slide: &str) -> Result<(Restored, Option<Write>), OpsError> {
    let doc = kept(b)?;
    let all = listed(&doc);
    let version = named(&all, name)?.clone();
    let (was, held) = then(&doc, &version)?;
    let (deck, files) = slide_back(&b.deck, &was, &held, slide, version.n)?;
    let said = put_back(&deck, slide);
    let (mut restored, write_it) = restored(b, version, deck, files)?;
    restored.slide = said;
    let id = restored.version.id.clone();
    let write_it = write_it.map(|w| Write { why: Why::new(format!("history --restore {id} --slide {slide}")), ..w });
    Ok((restored, write_it))
}

/// `now` with the slide of `was` that holds `state` put back ([`restoring_slide`]), and the
/// files of `held` the data sources it brings back read.
fn slide_back(
    now: &Deck,
    was: &Deck,
    held: &BTreeMap<String, Vec<u8>>,
    state: &str,
    n: usize,
) -> Result<(Deck, BTreeMap<String, Vec<u8>>), OpsError> {
    let slides = was.slides();
    let Some(k) = (slides.iter()).position(|s| s.iter().any(|&i| was.states[i].id == state)) else {
        let near = scaena_core::patch::nearest("states", state, was.states.iter().map(|s| s.id.as_str()));
        return Err(OpsError::new(format!("version {n} has no state `{state}`{near}")));
    };
    let was = &fresh_ids(now, was, &slides[k])?;
    let back: Vec<scaena_core::document::State> = slides[k].iter().map(|&i| was.states[i].clone()).collect();
    let ids: Vec<&str> = back.iter().map(|s| s.id.as_str()).collect();
    let first = was.slide_of(&was.states[slides[k][0]]).to_string();
    let mut deck = now.clone();
    // Where it stands now, if the deck still has it, and the deck without it, each other state
    // showing what it showed, as a slide taken away leaves them.
    let ours = |s: &scaena_core::document::State| deck_slide(now, s) == first;
    let stood = deck.states.iter().position(ours);
    let rest: Vec<scaena_core::document::State> = deck.states.iter().filter(|s| !ours(s)).cloned().collect();
    if let Some(taken) = rest.iter().find(|s| ids.contains(&s.id.as_str())) {
        return Err(OpsError::new(format!("the deck has a state `{}` of another slide now", taken.id)));
    }
    if rest.len() != deck.states.len() {
        deck.states = scaena_core::tracking::keep_looks(&deck, rest, &[]).map_err(OpsError::new)?;
    }
    let at = stood.unwrap_or_else(|| {
        // After the nearest slide before it that the deck still has, else first.
        let before = slides[..k].iter().rev().find_map(|earlier| {
            let id = was.slide_of(&was.states[earlier[0]]);
            deck.states.iter().rposition(|s| deck_slide(&deck, s) == id)
        });
        before.map_or(0, |i| i + 1)
    });
    // Its nodes: each the deck lacks, or shows on no other slide, as the version held it.
    let shown: Vec<&String> = back.iter().flat_map(|s| s.props.keys()).collect();
    for id in shown {
        let Some(node) = was.nodes.get(id) else { continue };
        let elsewhere =
            (deck.states.iter()).filter(|s| !ids.contains(&s.id.as_str())).any(|s| s.props.contains_key(id));
        if !deck.nodes.contains_key(id) || !elsewhere {
            deck.nodes.insert(id.clone(), node.clone());
            match was.overrides.get(id) {
                Some(o) => {
                    deck.overrides.insert(id.clone(), o.clone());
                }
                None => {
                    deck.overrides.shift_remove(id);
                }
            }
        }
    }
    // The data it reads that the deck lacks, with the files the history kept of it.
    let written = serde_json::to_string(&(
        &back,
        back.iter().flat_map(|s| s.props.keys()).filter_map(|id| was.nodes.get(id)).collect::<Vec<_>>(),
    ))
    .unwrap_or_default();
    let mut files = BTreeMap::new();
    for (id, source) in &was.data {
        let named = |id: &str| written.contains(&format!("\"{id}\"")) || written.contains(&format!("\"@{id}\""));
        if deck.data.contains_key(id) || !named(id) {
            continue;
        }
        deck.data.insert(id.clone(), source.clone());
        // A source is written by its file's path (`"source": "data/…"`, SPEC §3.10), its rows
        // inline otherwise (W1's review of #1172).
        if let Some(file) = source.source.as_str()
            && let Some(bytes) = held.get(file)
        {
            files.insert(file.to_string(), bytes.clone());
        }
    }
    // Its states put back, each other state showing what it showed: the slide after it, which
    // tracked from another, keeps its looks.
    let mut order = deck.states.clone();
    order.splice(at..at, back.iter().cloned());
    deck.states = scaena_core::tracking::keep_looks(&deck, order, &[]).map_err(OpsError::new)?;
    // The beat that told it, in its part of the talk, telling its states alone.
    let told = was.spine.iter().flat_map(|sp| &sp.sections).find_map(|sec| {
        let beat = sec.beats.iter().position(|b| b.states.iter().any(|s| ids.contains(&s.as_str())))?;
        Some((sec, beat))
    });
    if let Some((sec, j)) = told {
        let states: Vec<String> = deck.states.iter().map(|s| s.id.clone()).collect();
        let spine = deck.spine.get_or_insert_with(|| scaena_core::document::Spine { sections: Vec::new() });
        for other in spine.sections.iter_mut().flat_map(|s| s.beats.iter_mut()) {
            other.states.retain(|s| !ids.contains(&s.as_str()));
        }
        let mut beat = sec.beats[j].clone();
        let present = |spine: &scaena_core::document::Spine| {
            (spine.sections.iter().enumerate())
                .find_map(|(a, s)| s.beats.iter().position(|b| b.id == beat.id).map(|b| (a, b)))
        };
        // A beat the deck has keeps its own claim, notes, and states, edits made since and all,
        // the slide's states going back where the version had them among its states; one the deck
        // lacks comes back as the version held it, telling only states the deck has that no beat
        // tells now (W1's review of #1172).
        let told: Vec<String> =
            spine.sections.iter().flat_map(|s| &s.beats).flat_map(|b| b.states.iter().cloned()).collect();
        if let Some((a, i)) = present(spine) {
            let kept = &mut spine.sections[a].beats[i].states;
            *kept = among(&beat.states, kept, &ids);
        } else {
            beat.states.retain(|s| ids.contains(&s.as_str()) || (states.contains(s) && !told.contains(s)));
            if let Some(a) = spine.sections.iter().position(|s| s.id == sec.id) {
                let near = (sec.beats[..j].iter().rev())
                    .find_map(|e| spine.sections[a].beats.iter().position(|b| b.id == e.id));
                let i = near.map_or(0, |i| i + 1);
                spine.sections[a].beats.insert(i, beat);
            } else {
                let sections = &was.spine.as_ref().map(|s| s.sections.as_slice()).unwrap_or_default();
                let k = sections.iter().position(|s| s.id == sec.id).unwrap_or_default();
                let near = sections[..k].iter().rev().find_map(|e| spine.sections.iter().position(|s| s.id == e.id));
                let i = near.map_or(0, |i| i + 1);
                spine.sections.insert(i, scaena_core::document::Section { beats: vec![beat], ..sec.clone() });
            }
        }
        spine.sections.retain(|s| !s.beats.is_empty());
    }
    Ok((deck, files))
}

/// `kept`, a beat's states now, with `back`, the slide's states put back, each where `version`,
/// that beat's states as the version held them, had it: after the nearest state before it there
/// that `kept` still has, else first.
fn among(version: &[String], kept: &[String], back: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = kept.iter().filter(|s| !back.contains(&s.as_str())).cloned().collect();
    for (k, id) in version.iter().enumerate().filter(|(_, id)| back.contains(&id.as_str())) {
        let at = version[..k].iter().rev().find_map(|e| out.iter().position(|o| o == e)).map_or(0, |i| i + 1);
        out.insert(at, id.clone());
    }
    out
}

/// `was` with each node `slide`, its states, alone showed renamed to an id new to both decks, where
/// another slide of `now` shows a node of that id (W1's review of #1172): the id was given again
/// since, so the version's node comes back under a fresh one and the other slide keeps its own. A
/// node the version showed on other slides too is the one node, edited since, and keeps its id.
fn fresh_ids(now: &Deck, was: &Deck, slide: &[usize]) -> Result<Deck, OpsError> {
    let ids: Vec<&str> = slide.iter().map(|&i| was.states[i].id.as_str()).collect();
    let then = scaena_core::resolve_states(was).context("tracking")?;
    let since = scaena_core::resolve_states(now).context("tracking")?;
    let elsewhere = |snaps: &[scaena_core::Snapshot], node: &str| {
        snaps.iter().any(|s| !ids.contains(&s.state_id.as_str()) && s.nodes.contains_key(node))
    };
    let shown: Vec<&String> =
        then.iter().filter(|s| ids.contains(&s.state_id.as_str())).flat_map(|s| s.nodes.keys()).collect();
    let mut renames: Vec<Value> = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    for id in shown {
        let given = now.nodes.contains_key(id) && elsewhere(&since, id) && !elsewhere(&then, id);
        if !given || renames.iter().any(|r| r["id"] == id.as_str()) {
            continue;
        }
        let to = (2..)
            .map(|n| format!("{id}-{n}"))
            .find(|to| !now.nodes.contains_key(to) && !was.nodes.contains_key(to) && !taken.contains(to))
            .expect("some number is free");
        taken.push(to.clone());
        renames.push(serde_json::json!({ "op": "rename_node", "id": id, "to": to }));
    }
    if renames.is_empty() {
        return Ok(was.clone());
    }
    struct Bare;
    impl scaena_core::validate::BundleFiles for Bare {
        fn exists(&self, _: &str) -> bool {
            false
        }
        fn read_text(&self, _: &str) -> Option<String> {
            None
        }
    }
    let doc =
        scaena_core::patch::compile_alone(&was.to_value()?, &renames, &Bare).map_err(|e| OpsError::new(e.message))?;
    Deck::from_value(&doc.doc).map_err(|e| OpsError::new(e.to_string()))
}

/// The slide of `deck` that holds `state`, by its number and the headline it shows at its last
/// step, as a person names it.
fn put_back(deck: &Deck, state: &str) -> Option<String> {
    let slides = deck.slides();
    let k = slides.iter().position(|s| s.iter().any(|&i| deck.states[i].id == state))?;
    let snaps = scaena_core::tracking::resolve_states(deck).ok()?;
    let last = *slides[k].last()?;
    let headline = snaps.get(last).and_then(|snap| scaena_core::reading::headline(deck, snap));
    Some(match headline {
        Some(h) => format!("Slide {}: {h}", k + 1),
        None => format!("Slide {}", k + 1),
    })
}

/// The slide `state` of `deck` belongs to, by its first state's id.
fn deck_slide<'a>(deck: &Deck, state: &'a scaena_core::document::State) -> &'a str {
    deck.slide_of(state)
}

/// `deck`, version `version`'s, made the bundle's deck again with `held`, the files it was
/// drawn from.
pub fn restored(
    b: &Bundle,
    version: Version,
    deck: Deck,
    held: BTreeMap<String, Vec<u8>>,
) -> Result<(Restored, Option<Write>), OpsError> {
    let mut files = held;
    files.retain(|path, bytes| b.files.read(path).ok().as_ref() != Some(bytes));
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
        false => {
            let places = crate::patch::placed(b.theme_json.as_deref(), &b.deck, &deck);
            (
                lint(b)?.findings,
                lint_in(&deck, &view)?.findings,
                crate::patch::changed(&b.deck, &deck, places.as_ref())?,
            )
        }
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
        slide: None,
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
    /// With `restore`, the one slide that holds this state, put back as the version held it
    /// into the deck as it is now, with everything else as it is: its steps, the objects it
    /// alone shows, the data it reads, and its beat (agent dogfood 7).
    #[serde(default)]
    pub slide: Option<String>,
    /// With `restore`, say what it would change, and write nothing.
    #[serde(default)]
    pub dry_run: bool,
    /// Begin a history where the bundle keeps none, and list its versions: its first holds the
    /// deck as it is, and every write after is recorded. A bundle that keeps one lists it.
    #[serde(default)]
    pub begin: bool,
    /// Listing the versions, only the last this many, each by its number in the whole history.
    #[serde(default)]
    pub last: Option<usize>,
}

/// What a bundle's history answers: its versions, one seen, two compared, or one restored.
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct History {
    /// Its versions, oldest first, when nothing else is asked, each told in a person's words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versions: Option<Vec<Told>>,
    /// With `begin`, whether the history was begun here: false where the bundle kept one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub begun: Option<bool>,
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
    let asked = [ask.at.is_some(), !ask.compare.is_empty(), ask.restore.is_some(), ask.begin];
    if asked.iter().filter(|a| **a).count() > 1 {
        return Err(OpsError::new("ask one of `at`, `compare`, `restore`, and `begin` at a time"));
    }
    if ask.begin {
        let (begun, doc) = begin(b, "history begins")?;
        return Ok(History { versions: Some(told(&doc)?), begun: Some(begun), ..History::default() });
    }
    if let Some(name) = &ask.at {
        return Ok(History { seen: Some(at(b, name, ask.scn)?), ..History::default() });
    }
    if ask.slide.is_some() && ask.restore.is_none() {
        return Err(OpsError::new("`slide` names the slide a `restore` puts back: ask it with `restore`"));
    }
    if let Some(name) = &ask.restore {
        let restored = match &ask.slide {
            Some(slide) => restore_slide(b, name, slide, ask.dry_run)?,
            None => restore(b, name, ask.dry_run)?,
        };
        return Ok(History { restored: Some(restored), ..History::default() });
    }
    let compared = match ask.compare.as_slice() {
        [] => {
            // Only the last `last`, each by its number in the whole history (FUNC-5).
            let mut versions = told(&kept(b)?)?;
            if let Some(last) = ask.last {
                versions.drain(..versions.len().saturating_sub(last));
            }
            return Ok(History { versions: Some(versions), ..History::default() });
        }
        [from] => diff(b, from, None)?,
        [from, to] => diff(b, from, Some(to))?,
        _ => return Err(OpsError::new("compare one version with the deck as it is now, or two versions")),
    };
    Ok(History { compared: Some(compared), ..History::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ridgeline() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/examples/ridgeline.deck.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn deck(v: &Value) -> Deck {
        Deck::from_json(&v.to_string()).unwrap()
    }

    /// R1's agent dogfood 6: a comparison says what changed of the spine beat by beat, and of the
    /// fonts by file, never either whole before and after.
    #[test]
    fn a_comparison_says_the_spine_by_its_beats_and_the_fonts_by_their_files() {
        let was = ridgeline();
        let mut is = was.clone();
        is["spine"]["sections"][1]["beats"][0]["notes"] = "Say the growth.".into();
        is["spine"]["sections"][1]["beats"][1]["claim"] = "Hilltop barely moved.".into();
        is["fonts"][2]["file"] = "fonts/Other-VF.ttf".into();
        let (after, before) = spine_changed(&deck(&was), &deck(&is));
        assert_eq!(
            after,
            serde_json::json!({ "beats": {
                "growth": { "notes": "Say the growth." },
                "hilltop-stalled": { "claim": "Hilltop barely moved." },
            }})
        );
        assert_eq!(
            before,
            serde_json::json!({ "beats": {
                "growth": { "notes": null },
                "hilltop-stalled": { "claim": "Hilltop, the steepest neighborhood, barely moved until June." },
            }})
        );
        let (added, gone) = fonts_changed(&deck(&was), &deck(&is));
        assert_eq!(added, serde_json::json!(["fonts/Other-VF.ttf"]));
        assert_eq!(gone, serde_json::json!(["fonts/JetBrainsMono-VF.ttf"]));

        // A beat moved to another section is the outline's, a beat taken away null.
        let mut moved = was.clone();
        let beat = moved["spine"]["sections"][1]["beats"].as_array_mut().unwrap().remove(1);
        moved["spine"]["sections"][2]["beats"].as_array_mut().unwrap().insert(0, beat);
        moved["spine"]["sections"][3]["beats"].as_array_mut().unwrap().remove(1);
        let (after, before) = spine_changed(&deck(&was), &deck(&moved));
        assert_eq!(after["beats"], serde_json::json!({ "setup": null }));
        assert_eq!(
            before["beats"]["setup"]["claim"],
            "The $1.4 million set-up cost bought a system that now runs cheaply."
        );
        assert_eq!(after["sections"][2]["beats"][0], "hilltop-stalled");
        assert_eq!(before["sections"][1]["beats"][1], "hilltop-stalled");
        assert!(after.to_string().len() < 700, "{} bytes: {after}", after.to_string().len());

        // Only a section's own field changed, no beat's: the outline says it, never `{}`.
        let mut retitled = was.clone();
        retitled["spine"]["sections"][2]["title"] = "Hills".into();
        let (after, before) = spine_changed(&deck(&was), &deck(&retitled));
        assert_eq!(after.get("beats"), None, "{after}");
        assert_eq!(after["sections"][2]["title"], "Hills");
        assert_eq!(before["sections"][2]["title"], "E-bikes");
        assert_eq!(after["sections"][2]["beats"], serde_json::json!(["ebikes", "lesson"]));
    }

    /// R1's agent dogfood 6: ten slides' notes set in one patch, a slide's hold, and a slide written
    /// again each say what changed, and on every slide, never "A slide changed — Slide 3".
    #[test]
    fn a_version_says_what_it_changed_on_every_slide() {
        let was = ridgeline();
        // Notes on ten slides: two of their own, the rest on their beats, as `set_state` writes them.
        let mut notes = was.clone();
        notes["states"][2]["notes"] = "Say the total.".into();
        notes["states"][5]["notes"] = "Say sixfold.".into();
        for (section, beat) in [(1, 0), (1, 1), (2, 0), (2, 1), (3, 0), (3, 1), (4, 0), (4, 1)] {
            notes["spine"]["sections"][section]["beats"][beat]["notes"] = "Say it plainly.".into();
        }
        let (changed, summary) = touched(&deck(&was), &deck(&notes)).unwrap();
        assert_eq!(changed.len(), 1, "{changed:?}");
        let said = &changed[0];
        assert!(said.starts_with("Presenter Notes on Slide 2, "), "{said}");
        for slide in 2..=11 {
            assert!(said.contains(&format!("Slide {slide},")) || said.ends_with(&format!("Slide {slide}")), "{said}");
        }
        assert!(!said.contains("spine"), "{said}");
        assert_eq!(summary.as_deref(), Some("Presenter Notes changed on 10 slides"));

        // A hold.
        let mut hold = was.clone();
        hold["states"][8]["hold"] = 9000.0.into();
        let (changed, summary) = touched(&deck(&was), &deck(&hold)).unwrap();
        assert_eq!(changed, ["Advance on Slide 9"]);
        assert_eq!(summary.as_deref(), Some("Advance changed on Slide 9"));

        // A slide written again in its place: taken away and added under another id.
        let mut again = was.clone();
        again["states"][9]["id"] = "recap-2".into();
        again["spine"]["sections"][4]["beats"][0]["states"] = serde_json::json!(["recap-2"]);
        let (changed, summary) = touched(&deck(&was), &deck(&again)).unwrap();
        assert_eq!(changed.first().map(String::as_str), Some("Slide 10 written again"), "{changed:?}");
        assert!(!changed.iter().any(|c| c.contains("added") || c.contains("taken away")), "{changed:?}");
        assert_eq!(summary, None, "a slide written again says what wrote it");
    }
}
