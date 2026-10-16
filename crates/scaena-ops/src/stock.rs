//! A price chart put on a slide, refreshed, and re-ranged (ADR-0030, decisions 22–23; PLAN O.3c):
//! a price series, which the client fetched by `scaena-objects`' plan (O.3b) or which was made by
//! hand, kept as a CSV under `data/` named for its symbol, its record the source's `fetched`, and
//! drawn as the line chart there is. Each is one write of the file and the deck by its author, one
//! change in the history with the file's bytes (ADR-0014), refused as a patch is.
//!
//! - [`stocking`] declares the source and adds the chart: a line on a time axis, lit by a highlight
//!   that names nothing, its first and last closes printed and no value axis, and, for a day, a rule
//!   at the previous close. It goes in the slide's empty place for a chart, else in the box asked,
//!   kept to its slide (PLAN 3.24). The change is quoted (ADR-0019) through one transform into the
//!   slide's empty places for a figure, a headline, and a caption, and the source joins the
//!   evidence of the beat the slide is in.
//! - [`refreshing`] writes the file and the record again, for the same range or another: every
//!   chart that reads the source follows, a day's rule at its previous close comes or goes, and
//!   each quote of the source is set again, its period's words with it. Words a person wrote in
//!   place of the quotes stay as written.

use crate::lint::Write;
use crate::patch::Patched;
use crate::{Bundle, Context as _, OpsError};
use scaena_core::fetched::{Fetched, Range};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// One close of a price series: its civil time in the exchange's zone, the close, and the close
/// its change counts from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Close {
    pub time: String,
    pub close: f64,
    pub base: f64,
}

/// A price series as fetched, or made by hand: its record, with its `stock`, and its closes in
/// time order.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Series {
    pub fetched: Fetched,
    pub rows: Vec<Close>,
}

/// Where a price chart is asked to go.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Aim {
    /// The state it goes in; the first when unset.
    #[serde(default)]
    pub state: Option<String>,
    /// The box a new chart takes where the slide has no empty place for one, `[x, y, w, h]`.
    #[serde(default)]
    pub at: Option<[f32; 4]>,
}

/// What a stock op did.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Stocked {
    /// The data source, by its name.
    pub source: String,
    /// The file the series is kept in.
    pub file: String,
    /// The chart added; none for a refresh.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart: Option<String>,
    /// What was done, in a person's words.
    pub said: String,
    pub patched: Patched,
}

/// The columns a price series is kept in.
const SCHEMA: &str = r#"{ "time": "date", "close": "number", "base": "number" }"#;

/// The words a range's change is said over: "AAPL is up 2.1% today".
fn period(range: Range) -> &'static str {
    match range {
        Range::Day => "today",
        Range::Week => "over the week",
        Range::Month => "over the month",
        Range::Quarter => "over three months",
        Range::Year => "over the year",
        Range::YearToDate => "this year",
    }
}

/// The caption's words for a range, before the date its prices run to: "Three months to Oct 15".
fn caption(range: Range) -> &'static str {
    match range {
        Range::Day => "The day to",
        Range::Week => "The week to",
        Range::Month => "The month to",
        Range::Quarter => "Three months to",
        Range::Year => "The year to",
        Range::YearToDate => "The year to",
    }
}

/// The one transform every quote of a series reads through (ADR-0030, decision 23): the last
/// close and its base, the date it runs to, and the change, its size, its way, and the period's
/// words derived from them.
fn transform(range: Range) -> Value {
    json!([
        { "aggregate": { "last": "last(close)", "base": "last(base)", "through": "last(time)" } },
        { "derive": {
            "change": "last / base - 1",
            "size": "abs(last / base - 1)",
            "way": "if(last >= base, 'up', 'down')",
            "period": format!("'{}'", period(range)),
            "caption": format!("'{}'", caption(range)),
        } }
    ])
}

/// A quote of `source`'s column `column`, read through the series' transform.
fn quote(source: &str, range: Range, column: &str, format: Option<&str>) -> Value {
    let mut q = json!({ "data": format!("@{source}"), "dataTransform": transform(range), "column": column });
    if let Some(f) = format {
        q["format"] = json!(f);
    }
    q
}

/// The series as its CSV: `time,close,base`, a row a close.
fn csv(rows: &[Close]) -> String {
    let mut out = String::from("time,close,base\n");
    for r in rows {
        out.push_str(&format!("{},{},{}\n", r.time, r.close, r.base));
    }
    out
}

/// The record's `stock`, or why there is none; and the rows, checked: closes in time order, each
/// time civil, each figure finite.
fn checked(series: &Series) -> Result<&scaena_core::fetched::Stock, OpsError> {
    let stock = series.fetched.stock.as_ref().ok_or_else(|| OpsError::new("the record names no price series (`stock`)"))?;
    if series.rows.is_empty() {
        return Err(OpsError::new(format!("the series of {} holds no close", stock.symbol)));
    }
    for (i, r) in series.rows.iter().enumerate() {
        if !scaena_core::fetched::civil_time(&r.time) {
            return Err(OpsError::new(format!("close {i}: `{}` is no civil time", r.time)));
        }
        if !(r.close.is_finite() && r.base.is_finite()) {
            return Err(OpsError::new(format!("close {i}: its figures are not numbers")));
        }
        if i > 0 && series.rows[i - 1].time >= r.time {
            return Err(OpsError::new(format!("close {i}: `{}` does not follow the close before it", r.time)));
        }
    }
    Ok(stock)
}

/// A name for the symbol's source and file: lower case, letters, figures, and hyphens, starting
/// with a letter.
fn named(symbol: &str) -> String {
    let mut out: String =
        symbol.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    out = out.trim_matches('-').to_string();
    if !out.starts_with(|c: char| c.is_ascii_lowercase()) {
        out = format!("s-{out}");
    }
    out.truncate(56);
    out
}

/// A chart's value format for prices in `currency`: dollars where they are dollars, else the
/// figure alone, so no `$` stands before a euro.
fn money(currency: Option<&str>) -> &'static str {
    match currency {
        None | Some("USD") => "$,.2f",
        _ => ",.2f",
    }
}

/// The chart's annotations for a series of `range` whose change counts from `base`: the highlight
/// that lights every mark, and, for a day, a rule at the previous close.
fn notes(range: Range, base: f64) -> Value {
    let mut notes = vec![json!({ "kind": "highlight", "at": {} })];
    if range == Range::Day {
        notes.push(json!({ "kind": "rule", "at": { "y": base }, "text": "Previous close" }));
    }
    json!(notes)
}

/// The first of `base`, `base-2`, … that `taken` does not hold.
fn fresh(base: &str, taken: impl Fn(&str) -> bool) -> String {
    (1..)
        .map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") })
        .find(|id| !taken(id))
        .expect("some number is free")
}

/// [`stock`] with nothing written: the patch and the file, unless it is refused.
pub fn stocking(b: &Bundle, aim: &Aim, series: &Series) -> Result<(Stocked, Option<Write>), OpsError> {
    let deck = &b.deck;
    let stock = checked(series)?;
    let state = match &aim.state {
        Some(s) => deck.states.iter().find(|x| &x.id == s).ok_or_else(|| OpsError::new(format!("no state `{s}`")))?,
        None => deck.states.first().ok_or_else(|| OpsError::new("the deck has no state to put a chart in"))?,
    }
    .id
    .clone();
    let theme = crate::theme(b)?;
    let name = named(&stock.symbol);
    let source = fresh(&name, |id| deck.data.contains_key(id));
    let file = fresh(&format!("data/{name}"), |f| {
        b.files.read(&format!("{f}.csv")).is_ok() || deck.data.values().any(|d| d.source.as_str() == Some(&format!("{f}.csv")))
    }) + ".csv";
    let fetched = serde_json::to_value(&series.fetched).context("its record")?;
    let schema: Value = serde_json::from_str(SCHEMA).expect("the schema parses");
    let mut ops = vec![json!({ "op": "add", "path": format!("/data/{source}"),
        "value": { "source": file, "schema": schema, "fetched": fetched } })];

    // The chart, in the slide's empty place for one, else in the box asked.
    let waiting = crate::states::waiting(deck, &theme, &state, None)?;
    let at = match waiting.iter().find(|w| !w.typed && !w.picture) {
        Some(place) => json!({ "in": place.slot }),
        None => {
            let rect = aim.at.ok_or_else(|| {
                OpsError::new(format!("`{state}` has no empty place for a chart: say the box a new one takes"))
            })?;
            if !(rect[2] > 0.0 && rect[3] > 0.0) {
                return Err(OpsError::new("a chart's box needs room in it"));
            }
            json!({ "rect": rect.map(|v| v.round()) })
        }
    };
    let (first, last) = (&series.rows[0], &series.rows[series.rows.len() - 1]);
    let way = if last.close >= last.base { "up" } else { "down" };
    let locale = scaena_core::format::Locale::of(deck.meta.as_ref().and_then(|m| m.lang.as_deref()));
    let change = scaena_core::format::NumberFormat::parse(".1%")
        .map(|f| f.format((last.close / last.base - 1.0).abs(), locale))
        .unwrap_or_default();
    let chart_id = fresh(&format!("{name}-chart"), |id| deck.nodes.contains_key(id));
    let chart = json!({
        "type": "chart", "kind": "line", "data": format!("@{source}"),
        "x": { "field": "time", "type": "temporal" },
        "y": { "field": "close", "type": "quantitative", "format": money(stock.currency.as_deref()) },
        "axes": { "y": { "show": false } },
        "labels": { "show": "ends" },
        "annotations": notes(stock.range, last.base),
        "alt": format!("{}'s closes from {} to {}: {way} {change} {}.", stock.symbol, first.time, last.time, period(stock.range)),
        "semantic": "evidence",
        "at": at,
    });
    let mut added = vec![(chart_id.clone(), chart)];

    // The change quoted into the slide's empty places for it.
    let range = stock.range;
    let mut words = |role: &str, runs: Value, base: &str| {
        let Some(place) = waiting.iter().find(|w| w.typed && w.role.as_deref() == Some(role)) else { return };
        let id = fresh(base, |id| deck.nodes.contains_key(id) || added.iter().any(|(a, _)| a == id));
        let semantic = if role == "headline" { "claim" } else { "evidence" };
        added.push((id, json!({ "type": "text", "role": role, "runs": runs, "semantic": semantic, "at": { "in": place.slot } })));
    };
    let symbol = stock.symbol.as_str();
    words(
        "figure",
        json!([{ "text": "", "quote": quote(&source, range, "change", Some("+.1%")) }]),
        &format!("{name}-change"),
    );
    words(
        "headline",
        json!([
            { "text": format!("{symbol} is ") },
            { "text": "", "quote": quote(&source, range, "way", None) },
            { "text": " " },
            { "text": "", "quote": quote(&source, range, "size", Some(".1%")) },
            { "text": " " },
            { "text": "", "quote": quote(&source, range, "period", None) },
        ]),
        &format!("{name}-claim"),
    );
    words(
        "caption",
        json!([
            { "text": "", "quote": quote(&source, range, "caption", None) },
            { "text": " " },
            { "text": "", "quote": quote(&source, range, "through", Some("%b %-d, %Y")) },
        ]),
        &format!("{name}-period"),
    );
    let mut nodes = Vec::new();
    for (id, node) in &added {
        ops.push(json!({ "op": "add_node", "id": id, "node": node, "state": state }));
        nodes.push((id.clone(), serde_json::from_value(node.clone()).context("a node")?));
    }
    for leave in crate::inspect::leaving(deck, &state, &nodes) {
        ops.push(serde_json::to_value(leave).context("a patch")?);
    }
    // The series is the evidence of the beat the slide is in.
    for (s, section) in deck.spine.iter().flat_map(|sp| sp.sections.iter().enumerate()) {
        for (i, beat) in section.beats.iter().enumerate() {
            if beat.states.contains(&state) {
                let path = format!("/spine/sections/{s}/beats/{i}/evidence");
                match beat.evidence.is_empty() {
                    true => ops.push(json!({ "op": "add", "path": path, "value": [format!("@{source}")] })),
                    false => ops.push(json!({ "op": "add", "path": format!("{path}/-"), "value": format!("@{source}") })),
                }
            }
        }
    }
    let said = format!("Price chart of {symbol} added");
    let mut files = BTreeMap::new();
    files.insert(file.clone(), csv(&series.rows).into_bytes());
    let (patched, write) = crate::patch::patching_with(b, &Value::Array(ops), Some(&said), files)?;
    Ok((Stocked { source, file, chart: Some(chart_id), said, patched }, write))
}

/// A price chart of `series` put on a slide where `aim` says, written into the bundle.
pub fn stock(b: &Bundle, aim: &Aim, series: &Series) -> Result<Stocked, OpsError> {
    let (stocked, write) = stocking(b, aim, series)?;
    if let Some(write) = write {
        crate::lint::write(b, write)?;
    }
    Ok(stocked)
}

/// [`refresh`] with nothing written: the patch and the file, unless it is refused.
pub fn refreshing(b: &Bundle, source: &str, series: &Series) -> Result<(Stocked, Option<Write>), OpsError> {
    let deck = &b.deck;
    let stock = checked(series)?;
    let name = source.strip_prefix('@').unwrap_or(source);
    let declared = deck.data.get(name).ok_or_else(|| OpsError::new(format!("the deck has no data source `{name}`")))?;
    let had = declared.fetched().and_then(|f| f.stock).ok_or_else(|| OpsError::new(format!("`{name}` is no price series")))?;
    if had.symbol != stock.symbol {
        return Err(OpsError::new(format!("`{name}` holds {}, and this series is {}", had.symbol, stock.symbol)));
    }
    let file = declared
        .source
        .as_str()
        .ok_or_else(|| OpsError::new(format!("`{name}` keeps its rows in the deck, not in a file")))?
        .to_string();
    let fetched = serde_json::to_value(&series.fetched).context("its record")?;
    let mut ops = vec![json!({ "op": "replace", "path": format!("/data/{name}/fetched"), "value": fetched })];
    let reads = format!("@{name}");
    let last = &series.rows[series.rows.len() - 1];
    // Every chart that reads it: a day's rule at its previous close, which comes and goes with the
    // range; each quote of it: the period's words with the range.
    for (id, node) in &deck.nodes {
        let path = format!("/nodes/{}", scaena_core::document::token(id));
        let props = &node.props;
        if props.get("data").and_then(Value::as_str) == Some(reads.as_str())
            && let Some(Value::Array(notes)) = props.get("annotations")
        {
            let mut kept: Vec<Value> = notes
                .iter()
                .filter(|n| !(n["kind"] == "rule" && n["text"] == "Previous close"))
                .cloned()
                .collect();
            if stock.range == Range::Day {
                kept.push(json!({ "kind": "rule", "at": { "y": last.base }, "text": "Previous close" }));
            }
            if &kept != notes {
                ops.push(json!({ "op": "replace", "path": format!("{path}/annotations"), "value": kept }));
            }
        }
        if stock.range != had.range
            && let Some(Value::Array(runs)) = props.get("runs")
        {
            for (i, run) in runs.iter().enumerate() {
                if run.pointer("/quote/data").and_then(Value::as_str) == Some(reads.as_str()) {
                    let at = format!("{path}/runs/{i}/quote/dataTransform");
                    ops.push(json!({ "op": "replace", "path": at, "value": transform(stock.range) }));
                }
            }
        }
    }
    let said = match stock.range == had.range {
        true => format!("Prices of {} fetched again", stock.symbol),
        false => format!("Prices of {} fetched again over {}", stock.symbol, period(stock.range).trim_start_matches("over ")),
    };
    let mut files = BTreeMap::new();
    files.insert(file.clone(), csv(&series.rows).into_bytes());
    let (patched, write) = crate::patch::patching_with(b, &Value::Array(ops), Some(&said), files)?;
    Ok((Stocked { source: name.to_string(), file, chart: None, said, patched }, write))
}

/// The price series `source` refreshed, or re-ranged, from `series`, written into the bundle.
pub fn refresh(b: &Bundle, source: &str, series: &Series) -> Result<Stocked, OpsError> {
    let (stocked, write) = refreshing(b, source, series)?;
    if let Some(write) = write {
        crate::lint::write(b, write)?;
    }
    Ok(stocked)
}
