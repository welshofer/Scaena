//! What an editor may insert (PLAN 2.34, ADR-0013): for each node type, the nodes the theme and
//! the bundle name, each as `add_node` adds it, unplaced, with the id it starts from and the box
//! it takes at first. The page keeps no vocabulary of its own.
//!
//! - **A text** in each of the theme's roles, two of the role's lines tall and half the canvas
//!   wide, its text the role's name.
//! - **A shape** of each kind that needs no points: a rectangle and an ellipse, filled with the
//!   theme's accent (or its first color); a line and an arrow, the theme's rule.
//! - **An image** of each PNG and JPEG in the bundle.
//! - **A chart and a table** of each of the deck's data sources (PLAN 2.41), from the columns
//!   it has, half the canvas each way; the table its first rows, as many as that holds.
//! - **A shader** of each preset, filling the canvas, under what is there.

use crate::data::{self, ColumnType, Datum, SourceFiles, Table};
use crate::document::Deck;
use crate::ids::is_valid_id;
use crate::model::theme::{Theme, Vocabulary};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashSet;

/// One thing an editor may insert.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Insert {
    /// What a menu says: the node's type, and the name it is made from.
    pub label: String,
    /// The node `add_node` adds, unplaced.
    pub node: Value,
    /// What its id starts from: a role, a kind, an image's name, a preset.
    pub id: String,
    /// The box it takes at first.
    pub start: Start,
}

/// The box an inserted node takes at first.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Start {
    /// A box about the pointer, each side a fraction of the canvas's, snapped to the theme's
    /// grid as a drop snaps; a text or an image fills instead the template's slot under the
    /// pointer, if nothing fills it.
    Box { w: f32, h: f32 },
    /// A slot it fills whole, and under the nodes there (`z`): a shader fills the canvas.
    Slot(String),
}

/// Everything `deck` may have inserted, `theme` its theme, `files` its bundle's paths, and
/// `sources` its files' bytes, for its data sources' columns.
pub fn inserts(deck: &Deck, theme: &Theme, files: &[String], sources: &dyn SourceFiles) -> Vec<Insert> {
    let [w, h] = [deck.canvas.width as f32, deck.canvas.height as f32];
    let mut out = Vec::new();
    for (role, look) in &theme.typography.roles {
        let tall = (2.0 * look.size * look.leading) as f32 / h;
        out.push(Insert {
            label: format!("Text · {role}"),
            node: json!({ "type": "text", "role": role, "text": sentence(role) }),
            id: role.clone(),
            start: Start::Box { w: 0.5, h: tall.clamp(1.0 / 12.0, 0.5) },
        });
    }
    let colors = theme.names(Vocabulary::Color);
    let fill = colors.iter().find(|c| *c == "accent").or(colors.first());
    let square = |side: f32| Start::Box { w: side, h: side * w / h };
    for kind in ["rect", "ellipse"] {
        let Some(fill) = fill else { break };
        let node = json!({ "type": "shape", "kind": kind, "fill": fill });
        out.push(Insert { label: format!("Shape · {kind}"), node, id: kind.into(), start: square(0.25) });
    }
    for kind in ["line", "arrow"] {
        let node = json!({ "type": "shape", "kind": kind });
        out.push(Insert {
            label: format!("Shape · {kind}"),
            node,
            id: kind.into(),
            start: Start::Box { w: 0.25, h: 1.0 / 12.0 },
        });
    }
    let picture = |p: &str| [".png", ".jpg", ".jpeg"].iter().any(|e| p.to_ascii_lowercase().ends_with(e));
    for path in files.iter().filter(|p| picture(p)) {
        let name = path.rsplit('/').next().unwrap_or(path);
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        let node = json!({ "type": "image", "src": path });
        // An image dropped into a bundle is named by its SHA-256 (SPEC §3.1): its first eight
        // digits name it well enough.
        let (label, id) = match stem.len() == 64 && stem.bytes().all(|b| b.is_ascii_hexdigit()) {
            true => (format!("Image · {}…", &stem[..8]), format!("image-{}", stem[..8].to_ascii_lowercase())),
            false => (format!("Image · {name}"), slug(stem, "image")),
        };
        out.push(Insert { label, node, id, start: square(1.0 / 3.0) });
    }
    for name in deck.data.keys() {
        // A source that does not load offers nothing: validation says why.
        let Ok(table) = data::load(deck, sources, name) else { continue };
        let half = Start::Box { w: 0.5, h: 0.5 };
        if let Some(node) = chart(name, &table) {
            out.push(Insert {
                label: format!("Chart · {name}"),
                node,
                id: format!("{name}-chart"),
                start: half.clone(),
            });
        }
        if let Some(node) = table_of(name, &table) {
            out.push(Insert { label: format!("Table · {name}"), node, id: format!("{name}-table"), start: half });
        }
    }
    let presets = theme.shaders.as_ref().and_then(|s| s.presets.as_ref());
    for (name, preset) in presets.into_iter().flatten() {
        let node = json!({ "type": "shader", "kind": preset.kind, "preset": name, "z": -1 });
        out.push(Insert {
            label: format!("Shader · {name}"),
            node,
            id: slug(name, "shader"),
            start: Start::Slot("canvas".into()),
        });
    }
    out
}

/// A chart of source `name`, read as `table`, from the columns it has: its `y` the first column
/// of numbers; its `x` the first of dates, a line over time, or else the first of text, bars;
/// and, where `x` alone repeats among the rows, a `series`, the next column of text that tells
/// them apart. None where the columns do not make one (SPEC §3.7's keys are unique, E103).
fn chart(name: &str, table: &Table) -> Option<Value> {
    let first = |kind: ColumnType| table.types.iter().position(|t| *t == kind);
    let y = first(ColumnType::Number)?;
    let (x, kind) = match (first(ColumnType::Date), first(ColumnType::String)) {
        (Some(x), _) => (x, "line"),
        (None, Some(x)) => (x, "bar"),
        (None, None) => return None,
    };
    // A datum's key: its x, joined with its series; a row whose y is null is a gap, with none.
    let apart = |columns: &[usize]| {
        let mut seen = HashSet::new();
        let rows = table.rows.iter().filter(|row| row[y] != Datum::Null);
        rows.map(|row| columns.iter().map(|&c| row[c].label()).collect::<Vec<_>>()).all(|key| seen.insert(key))
    };
    let mut node = json!({
        "type": "chart",
        "kind": kind,
        "data": format!("@{name}"),
        "x": { "field": table.columns[x] },
        "y": { "field": table.columns[y] },
    });
    if kind == "line" {
        node["x"]["type"] = json!("temporal");
    }
    if !apart(&[x]) {
        let text = |c: &usize| *c != x && table.types[*c] == ColumnType::String;
        let series = (0..table.columns.len()).filter(text).find(|&c| apart(&[x, c]))?;
        node["series"] = json!({ "field": table.columns[series] });
    }
    Some(node)
}

/// The rows a table offered keeps at first: what the box it starts in holds in any theme. A
/// table taller than its cell does not lay out (it says so, and how many rows would fit).
const ROWS: usize = 8;

/// A table of source `name`, read as `table`: every column, each row keyed by the first column,
/// or else the first that tells the rows apart (E103); its first [`ROWS`] rows where it has more.
/// None where no column tells them apart.
fn table_of(name: &str, table: &Table) -> Option<Value> {
    let apart = |c: usize| {
        let mut seen = HashSet::new();
        table.rows.iter().all(|row| seen.insert(row[c].label()))
    };
    let key = (0..table.columns.len()).find(|&c| apart(c))?;
    let mut node = json!({ "type": "table", "data": format!("@{name}") });
    if table.rows.len() > ROWS {
        node["dataTransform"] = json!([{ "limit": ROWS }]);
    }
    if key > 0 {
        node["key"] = json!(table.columns[key]);
    }
    Some(node)
}

/// The first of `base`, `base-2`, `base-3`, … that names no node of `deck`.
pub fn fresh(deck: &Deck, base: &str) -> String {
    (1..)
        .map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") })
        .find(|id| !deck.nodes.contains_key(id))
        .expect("some number is free")
}

/// `name` as an id (SPEC §3.2): lowercase, each run of other characters a `-`, starting with
/// a letter (else `prefix-` first), 64 characters at most.
pub fn slug(name: &str, prefix: &str) -> String {
    let mut out = String::new();
    for c in name.chars().map(|c| c.to_ascii_lowercase()) {
        match c {
            'a'..='z' | '0'..='9' | '_' => out.push(c),
            _ if !out.is_empty() && !out.ends_with('-') => out.push('-'),
            _ => {}
        }
    }
    let out = out.trim_end_matches('-');
    let out =
        if out.starts_with(|c: char| c.is_ascii_lowercase()) { out.to_string() } else { format!("{prefix}-{out}") };
    let out: String = out.chars().take(56).collect();
    let out = out.trim_end_matches('-').to_string();
    if is_valid_id(&out) { out } else { prefix.to_string() }
}

/// A role's name as a text says it at first: `big-number` as "Big number".
fn sentence(role: &str) -> String {
    let words = role.replace(['-', '_'], " ");
    let mut chars = words.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn a_name_becomes_an_id_and_a_role_a_sentence() {
        assert_eq!(slug("Team Photo 2026", "image"), "team-photo-2026");
        assert_eq!(slug("2026 plan", "image"), "image-2026-plan");
        assert_eq!(slug("--", "image"), "image");
        assert!(is_valid_id(&slug(&"x".repeat(200), "image")));
        assert_eq!(sentence("big-number"), "Big number");
        let deck = Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        let theme: Theme = serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        let sha = format!("assets/{}.png", "7130f10a".repeat(8));
        let photo = format!("assets/{}.jpg", "0c9ad21e".repeat(8));
        let offered = inserts(
            &deck,
            &theme,
            &[sha.clone(), photo, "assets/Team Photo.png".into(), "assets/notes.txt".into()],
            &BTreeMap::new(),
        );
        let images: Vec<(&str, &str)> =
            offered.iter().filter(|i| i.node["type"] == "image").map(|i| (i.label.as_str(), i.id.as_str())).collect();
        assert_eq!(
            images,
            [
                ("Image · 7130f10a…", "image-7130f10a"),
                ("Image · 0c9ad21e…", "image-0c9ad21e"),
                ("Image · Team Photo.png", "team-photo")
            ]
        );
        assert_eq!(sentence("title"), "Title");
        // A source the bundle does not hand over offers nothing to insert.
        assert!(!offered.iter().any(|i| i.node["type"] == "chart" || i.node["type"] == "table"));
    }

    /// A data source offers a chart and a table of it (PLAN 2.41), from its columns: the
    /// revenue example's quarters repeat, one row a product, so its bars are grouped by product,
    /// and its table is keyed by the first column whose values tell its rows apart.
    #[test]
    fn a_source_offers_a_chart_and_a_table_of_its_columns() {
        let deck = Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        let theme: Theme = serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        let csv = include_bytes!("../../../docs/examples/data/q3-revenue.csv");
        let files = BTreeMap::from([("data/q3-revenue.csv".to_string(), csv.to_vec())]);
        let offered = inserts(&deck, &theme, &[], &files);
        let made = |kind: &str| offered.iter().find(|i| i.node["type"] == kind).unwrap();
        let chart = made("chart");
        assert_eq!((chart.label.as_str(), chart.id.as_str()), ("Chart · q3", "q3-chart"));
        assert_eq!(
            chart.node,
            json!({
                "type": "chart",
                "kind": "bar",
                "data": "@q3",
                "x": { "field": "quarter" },
                "y": { "field": "revenue" },
                "series": { "field": "product" },
            })
        );
        let table = made("table");
        assert_eq!((table.label.as_str(), table.id.as_str()), ("Table · q3", "q3-table"));
        assert_eq!(
            table.node,
            json!({ "type": "table", "data": "@q3", "dataTransform": [{ "limit": 8 }], "key": "revenue" }),
            "its first rows, as many as its box holds"
        );

        // Dates make a line over time; a column of text alone makes a table and no chart.
        let rows = "month,visits\n2026-01-01,10\n2026-02-01,12\n";
        let mut deck = deck;
        deck.data.insert(
            "visits".into(),
            serde_json::from_value(
                json!({ "source": "data/visits.csv", "schema": { "month": "date", "visits": "number" } }),
            )
            .unwrap(),
        );
        deck.data.insert("names".into(), serde_json::from_value(json!({ "source": "data/names.csv" })).unwrap());
        let files = BTreeMap::from([
            ("data/visits.csv".to_string(), rows.as_bytes().to_vec()),
            ("data/names.csv".to_string(), b"name\nAda\nGrace\n".to_vec()),
        ]);
        let offered = inserts(&deck, &theme, &[], &files);
        let line = offered.iter().find(|i| i.label == "Chart · visits").unwrap();
        assert_eq!(line.node["kind"], "line");
        assert_eq!(line.node["x"], json!({ "field": "month", "type": "temporal" }));
        assert!(offered.iter().any(|i| i.label == "Table · names"));
        assert!(!offered.iter().any(|i| i.label == "Chart · names"));
    }
}
