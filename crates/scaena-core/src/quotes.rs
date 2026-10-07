//! Copy that quotes data (SPEC §3.5, ADR-0019, PLAN 2.72): a run's `quote` names a cell of a
//! data source, and its `text` is the figure as last set. [`value`] reads what the data gives
//! now; [`stale`] finds each figure that differs, with each claim that holds the old one, and
//! [`requote`] is the patch that sets them again, which every write through `scaena-ops` makes.

use crate::Deck;
use crate::data::{self, DataError, Datum, SourceFiles};
use crate::format::{DateFormat, Locale, NumberFormat};
use crate::model::values::{Quote, QuoteKey, QuoteRow};
use crate::patch::JsonOp;
use crate::transform;
use serde_json::Value;

/// Why a quote gives no value: a lint code (E102, E103, or E106), where in the quote, from it
/// down (`column`, `row/quarter`, `dataTransform/0/filter`), and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub code: &'static str,
    pub at: String,
    pub message: String,
}

fn problem(code: &'static str, at: impl Into<String>, message: impl Into<String>) -> Problem {
    Problem { code, at: at.into(), message: message.into() }
}

/// The figure `quote` gives in `deck` with its data `files`: its cell, written by its format.
/// `Err(None)` where the source itself does not read (a file not there, or a value that does
/// not fit its column), which validation reports at the source.
pub fn value(deck: &Deck, files: &dyn SourceFiles, quote: &Quote) -> Result<String, Option<Problem>> {
    let name = quote.data.strip_prefix('@').unwrap_or(&quote.data);
    let table = match data::load(deck, files, name) {
        Ok(table) => table,
        Err(DataError::Unknown(_)) => {
            let names: Vec<String> = deck.data.keys().map(|k| format!("`@{k}`")).collect();
            let has = if names.is_empty() { "it has none".into() } else { format!("it has {}", names.join(", ")) };
            return Err(Some(problem("E102", "data", format!("a quote reads unknown data source `@{name}`: {has}"))));
        }
        Err(_) => return Err(None),
    };
    let read = if quote.data_transform.is_some() {
        format!("`@{name}` after its `dataTransform`")
    } else {
        format!("`@{name}`")
    };
    let table = match &quote.data_transform {
        Some(steps) => transform::apply(table, steps).map_err(|e| {
            let rest: String = e.at.iter().map(|k| format!("/{k}")).collect();
            let code = if e.data { "E103" } else { "E106" };
            Some(problem(code, format!("dataTransform/{}{rest}", e.step), format!("`@{name}`: {e}")))
        })?,
        None => table,
    };
    let columns = || table.columns.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ");
    let rows = table.rows.len();
    let row = match &quote.row {
        None if rows == 1 => 0,
        None => {
            return Err(Some(problem(
                "E103",
                "row",
                format!("{read} has {rows} rows: a quote names its `row`, by its values or its index"),
            )));
        }
        Some(QuoteRow::Index(i)) => {
            let at = if *i < 0 { rows as i64 + i } else { *i };
            if at < 0 || at >= rows as i64 {
                return Err(Some(problem("E103", "row", format!("{read} has no row {i}: it has {rows}"))));
            }
            at as usize
        }
        Some(QuoteRow::Keys(keys)) => {
            let mut found: Vec<usize> = (0..rows).collect();
            for (column, key) in keys {
                let Some(c) = table.column(column) else {
                    return Err(Some(problem(
                        "E103",
                        format!("row/{column}"),
                        format!("{read} has no column `{column}`; it has {}", columns()),
                    )));
                };
                found.retain(|&r| holds(&table.rows[r][c], key));
            }
            let said = || {
                keys.iter()
                    .map(|(c, k)| {
                        format!(
                            "`{c}` {}",
                            match k {
                                QuoteKey::Text(t) => format!("`{t}`"),
                                QuoteKey::Number(n) => n.to_string(),
                                QuoteKey::Bool(b) => b.to_string(),
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" and ")
            };
            match found.as_slice() {
                [one] => *one,
                [] => return Err(Some(problem("E103", "row", format!("{read} has no row with {}", said())))),
                many => {
                    return Err(Some(problem(
                        "E103",
                        "row",
                        format!("{read} has {} rows with {}: a quote reads one", many.len(), said()),
                    )));
                }
            }
        }
    };
    let Some(c) = table.column(&quote.column) else {
        return Err(Some(problem(
            "E103",
            "column",
            format!("{read} has no column `{}`; it has {}", quote.column, columns()),
        )));
    };
    let locale = Locale::of(deck.meta.as_ref().and_then(|m| m.lang.as_deref()));
    let datum = &table.rows[row][c];
    let column = &quote.column;
    match (datum, &quote.format) {
        (Datum::Null, _) => Err(Some(problem("E103", "column", format!("`{column}` of that row of {read} is empty")))),
        (Datum::Number(x), Some(spec)) => {
            let format = NumberFormat::parse(spec).map_err(|e| Some(problem("E106", "format", e.0)))?;
            Ok(format.format(*x, locale))
        }
        (Datum::Date(t), Some(spec)) => {
            let format = DateFormat::parse(spec).map_err(|e| Some(problem("E106", "format", e.0)))?;
            Ok(format.format(*t, locale))
        }
        (_, Some(_)) => Err(Some(problem(
            "E103",
            "format",
            format!("`{column}` of {read} is not a number or a date: a `format` writes those"),
        ))),
        (Datum::Number(x), None) => Ok(crate::format::format_number(*x, locale)),
        (datum, None) => Ok(datum.label()),
    }
}

/// Whether `datum` is the value `key` finds a row by: a number equal to it, a boolean, or, for
/// text, a cell that reads as it (a date in ISO 8601, as a chart writes a category).
fn holds(datum: &Datum, key: &QuoteKey) -> bool {
    match (datum, key) {
        (Datum::Number(x), QuoteKey::Number(n)) => x == n,
        (Datum::Bool(x), QuoteKey::Bool(b)) => x == b,
        (Datum::Null, _) => false,
        (datum, QuoteKey::Text(t)) => datum.label() == *t,
        _ => false,
    }
}

/// A run that quotes: the text node it is in, the pointer to the run, and the run.
pub struct Quoted<'a> {
    pub node: &'a str,
    pub path: String,
    pub run: &'a Value,
}

/// Every run with a `quote` in `doc` (a deck as JSON): in a node's own props, each state's
/// delta, and the deck's overrides.
pub fn quoted(doc: &Value) -> Vec<Quoted<'_>> {
    let esc = |s: &str| s.replace('~', "~0").replace('/', "~1");
    let mut texts: Vec<(&str, &Value, String)> = Vec::new();
    for (id, node) in doc.get("nodes").and_then(Value::as_object).into_iter().flatten() {
        texts.push((id, node, format!("/nodes/{}", esc(id))));
    }
    for (i, state) in doc.get("states").and_then(Value::as_array).into_iter().flatten().enumerate() {
        for (id, props) in state.get("props").and_then(Value::as_object).into_iter().flatten() {
            texts.push((id, props, format!("/states/{i}/props/{}", esc(id))));
        }
    }
    for (id, over) in doc.get("overrides").and_then(Value::as_object).into_iter().flatten() {
        texts.push((id, over, format!("/overrides/{}", esc(id))));
    }
    let mut out = Vec::new();
    for (node, props, path) in texts {
        for (k, run) in props.get("runs").and_then(Value::as_array).into_iter().flatten().enumerate() {
            if run.get("quote").is_some_and(Value::is_object) {
                out.push(Quoted { node, path: format!("{path}/runs/{k}"), run });
            }
        }
    }
    out
}

/// A quoted figure the data no longer gives: the run, its text, and the figure now, with each
/// beat's claim that holds the old figure as a word and shows the text, written again.
#[derive(Debug, Clone, PartialEq)]
pub struct Stale {
    pub node: String,
    /// Pointer to the run.
    pub path: String,
    pub was: String,
    pub now: String,
    /// Each claim: its pointer, and the claim with the figure set again.
    pub claims: Vec<(String, String)>,
}

/// Each quoted figure in `deck` (`doc`, as JSON) that its data, `files`, no longer gives. A
/// quote that gives no value is validation's to report, and is passed over here.
pub fn stale(deck: &Deck, doc: &Value, files: &dyn SourceFiles) -> Vec<Stale> {
    let runs = quoted(doc);
    if runs.is_empty() {
        return Vec::new();
    }
    let snapshots = crate::resolve_states(deck).unwrap_or_default();
    let mut out: Vec<Stale> = Vec::new();
    for q in runs {
        let Some(quote) = q.run.get("quote").and_then(|v| serde_json::from_value::<Quote>(v.clone()).ok()) else {
            continue;
        };
        let Ok(now) = value(deck, files, &quote) else { continue };
        let was = q.run.get("text").and_then(Value::as_str).unwrap_or_default();
        if now == was {
            continue;
        }
        // The claims of the beats whose states show the text.
        let mut claims = Vec::new();
        for (si, section) in deck.spine.iter().flat_map(|s| s.sections.iter()).enumerate() {
            for (bi, beat) in section.beats.iter().enumerate() {
                let shows = snapshots.iter().any(|s| beat.states.contains(&s.state_id) && s.nodes.contains_key(q.node));
                if !shows {
                    continue;
                }
                if let Some(claim) = replaced(&beat.claim, was, &now) {
                    claims.push((format!("/spine/sections/{si}/beats/{bi}/claim"), claim));
                }
            }
        }
        out.push(Stale { node: q.node.to_string(), path: q.path, was: was.to_string(), now, claims });
    }
    out
}

/// The patch that sets each figure `stale` found again, and each claim with it.
pub fn requote(stale: &[Stale]) -> Vec<JsonOp> {
    let mut ops = Vec::new();
    let mut claims: Vec<(String, String)> = Vec::new();
    for s in stale {
        ops.push(JsonOp::Replace { path: format!("{}/text", s.path), value: Value::String(s.now.clone()) });
        for (path, claim) in &s.claims {
            // Two figures in one claim: the second rewrites what the first left.
            match claims.iter_mut().find(|(p, _)| p == path) {
                Some((_, so_far)) => *so_far = replaced(so_far, &s.was, &s.now).unwrap_or(so_far.clone()),
                None => claims.push((path.clone(), claim.clone())),
            }
        }
    }
    ops.extend(claims.into_iter().map(|(path, claim)| JsonOp::Replace { path, value: Value::String(claim) }));
    ops
}

/// `text` with each place `was` stands as a word in it written `now`, or `None` where it stands
/// nowhere. A word is not inside another, nor a number inside a longer one (`4.2` in `14.25`);
/// a unit may follow a number (`$4.2M`).
pub fn replaced(text: &str, was: &str, now: &str) -> Option<String> {
    if was.is_empty() {
        return None;
    }
    let word = |c: char| c.is_alphanumeric();
    let mut out = String::with_capacity(text.len());
    let mut rest = 0;
    let mut found = false;
    for (at, _) in text.match_indices(was) {
        if at < rest {
            continue;
        }
        let before = text[..at].chars().next_back();
        let mut after = text[at + was.len()..].chars();
        let next = after.next();
        let joined_before = before.is_some_and(word) && was.chars().next().is_some_and(word);
        // A figure that ends in a digit may take a unit after it (`$4.2M`, `12%`), not more digits.
        let last = was.chars().next_back();
        let joined_after = match next {
            Some(c) if c.is_numeric() => last.is_some_and(word),
            Some(c) if c.is_alphabetic() => last.is_some_and(char::is_alphabetic),
            Some('.' | ',') => after.next().is_some_and(|c| c.is_ascii_digit()),
            _ => false,
        };
        if joined_before || joined_after {
            continue;
        }
        out.push_str(&text[rest..at]);
        out.push_str(now);
        rest = at + was.len();
        found = true;
    }
    found.then(|| {
        out.push_str(&text[rest..]);
        out
    })
}

#[cfg(test)]
mod tests {
    use super::replaced;

    #[test]
    fn a_figure_is_replaced_as_a_word() {
        assert_eq!(replaced("Revenue hit $4.2M in Q3.", "$4.2M", "$5.1M").as_deref(), Some("Revenue hit $5.1M in Q3."));
        assert_eq!(replaced("Up 14.25 points", "4.2", "5"), None);
        assert_eq!(replaced("4.2 then 4.25", "4.2", "5").as_deref(), Some("5 then 4.25"));
        assert_eq!(replaced("Q3 and Q30", "Q3", "Q4").as_deref(), Some("Q4 and Q30"));
        assert_eq!(replaced("nothing here", "4.2", "5"), None);
        assert_eq!(replaced("Pro hit $19.4M.", "$19.4", "$20.0").as_deref(), Some("Pro hit $20.0M."));
    }
}
