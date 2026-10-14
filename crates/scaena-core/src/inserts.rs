//! What an editor may insert (PLAN 2.34, ADR-0013): for each node type, the nodes the theme and
//! the bundle name, each as `add_node` adds it, unplaced, with the id it starts from and the box
//! it takes at first. The page keeps no vocabulary of its own.
//!
//! - **A text** in each of the theme's roles, two of the role's lines tall and half the canvas
//!   wide, its text the role's name.
//! - **A shape** of each kind that needs no points: a rectangle and an ellipse, filled with the
//!   theme's accent (or its first color); a line and an arrow, the theme's rule. And the shape
//!   library's polygons (PLAN 2.106, [`polygons`]): triangles, a diamond, regular polygons, a
//!   star, a chevron, block arrows, a plus, and a callout, filled as the rectangle is.
//! - **An image** of each PNG and JPEG in the bundle.
//! - **A chart and a table** of each of the deck's data sources (PLAN 2.41), from the columns
//!   it has, half the canvas each way; the table its first rows, as many as that holds.
//! - **A shader** of each preset, filling the canvas, under what is there.
//!
//! An icon (PLAN 2.104) is offered by an editor's picker of the icons that ship, not here: the
//! icons are no part of the theme or the bundle, and the editor's module carries none of them.
//! [`icon`] makes what it inserts.

use crate::data::{self, ColumnType, Datum, SourceFiles, Table};
use crate::document::Deck;
use crate::ids::is_valid_id;
use crate::model::theme::{Theme, Vocabulary};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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

/// The shape library's polygons (PLAN 2.106), as a presentation app's shape library offers them:
/// each its name, its `points` as fractions of its box (§3.3), which the canvas gives handles and a
/// change of state moves one by one, and its box's width over its height. Regular ones fill their
/// box edge to edge, and each point is written to four places.
pub fn polygons() -> Vec<(&'static str, Vec<[f64; 2]>, f64)> {
    let tidy = |v: f64| (v * 1e4).round() / 1e4;
    // `n` points round a circle from the top, each `i`-th at radius `r(i)`, fitted to their box:
    // the points and the box's width over its height.
    let round = |n: usize, r: &dyn Fn(usize) -> f64| {
        let at: Vec<[f64; 2]> = (0..n)
            .map(|i| {
                let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * i as f64 / n as f64;
                [r(i) * a.cos(), r(i) * a.sin()]
            })
            .collect();
        let (x0, x1) = at.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p[0]), hi.max(p[0])));
        let (y0, y1) = at.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
        let points = at.iter().map(|p| [tidy((p[0] - x0) / (x1 - x0)), tidy((p[1] - y0) / (y1 - y0))]).collect();
        (points, (x1 - x0) / (y1 - y0))
    };
    let (pentagon, pentagon_aspect) = round(5, &|_| 1.0);
    // A five-pointed star whose inner points stand where the lines between its outer ones cross:
    // sin 18° over sin 54° of the way out.
    let inner = (std::f64::consts::PI / 10.0).sin() / (3.0 * std::f64::consts::PI / 10.0).sin();
    let (star, star_aspect) = round(10, &|i| if i % 2 == 0 { 1.0 } else { inner });
    let s = 1.0 - std::f64::consts::FRAC_1_SQRT_2;
    let octagon = [
        [s / 2.0, 0.0],
        [1.0 - s / 2.0, 0.0],
        [1.0, s / 2.0],
        [1.0, 1.0 - s / 2.0],
        [1.0 - s / 2.0, 1.0],
        [s / 2.0, 1.0],
        [0.0, 1.0 - s / 2.0],
        [0.0, s / 2.0],
    ];
    let right = [[0.0, 0.3], [0.6, 0.3], [0.6, 0.0], [1.0, 0.5], [0.6, 1.0], [0.6, 0.7], [0.0, 0.7]];
    let flip = |p: &[[f64; 2]], f: &dyn Fn([f64; 2]) -> [f64; 2]| p.iter().map(|q| f(*q)).collect::<Vec<_>>();
    vec![
        ("triangle", vec![[0.5, 0.0], [1.0, 1.0], [0.0, 1.0]], 1.0),
        ("right-triangle", vec![[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]], 1.0),
        ("diamond", vec![[0.5, 0.0], [1.0, 0.5], [0.5, 1.0], [0.0, 0.5]], 1.0),
        ("parallelogram", vec![[0.25, 0.0], [1.0, 0.0], [0.75, 1.0], [0.0, 1.0]], 1.6),
        ("trapezoid", vec![[0.2, 0.0], [0.8, 0.0], [1.0, 1.0], [0.0, 1.0]], 1.4),
        ("pentagon", pentagon, pentagon_aspect),
        (
            "hexagon",
            vec![[0.25, 0.0], [0.75, 0.0], [1.0, 0.5], [0.75, 1.0], [0.25, 1.0], [0.0, 0.5]],
            2.0 / 3f64.sqrt(),
        ),
        ("octagon", octagon.iter().map(|p| [tidy(p[0]), tidy(p[1])]).collect(), 1.0),
        ("star", star, star_aspect),
        ("chevron", vec![[0.0, 0.0], [0.7, 0.0], [1.0, 0.5], [0.7, 1.0], [0.0, 1.0], [0.3, 0.5]], 1.6),
        ("right-arrow", right.to_vec(), 1.6),
        ("left-arrow", flip(&right, &|[x, y]| [tidy(1.0 - x), y]), 1.6),
        ("up-arrow", flip(&right, &|[x, y]| [y, tidy(1.0 - x)]), 1.0 / 1.6),
        ("down-arrow", flip(&right, &|[x, y]| [y, x]), 1.0 / 1.6),
        (
            "plus",
            vec![
                [0.35, 0.0],
                [0.65, 0.0],
                [0.65, 0.35],
                [1.0, 0.35],
                [1.0, 0.65],
                [0.65, 0.65],
                [0.65, 1.0],
                [0.35, 1.0],
                [0.35, 0.65],
                [0.0, 0.65],
                [0.0, 0.35],
                [0.35, 0.35],
            ],
            1.0,
        ),
        (
            "callout",
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 0.75], [0.42, 0.75], [0.2, 1.0], [0.26, 0.75], [0.0, 0.75]],
            1.4,
        ),
    ]
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
    // The shape library's polygons, each in the box its own shape takes: as wide as the
    // rectangle, and as tall as its aspect makes it.
    for (name, points, aspect) in polygons() {
        let Some(fill) = fill else { break };
        let node = json!({ "type": "shape", "kind": "polygon", "points": points, "fill": fill });
        let start = Start::Box { w: 0.25, h: 0.25 * w / h / aspect as f32 };
        out.push(Insert { label: format!("Shape · {name}"), node, id: name.into(), start });
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

/// An icon to insert (PLAN 2.104): its name, its path data, and the view it is drawn on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Icon {
    /// Its name in its set, in kebab case: `airplane-tilt`.
    pub name: String,
    /// SVG path data.
    pub path: String,
    /// The box of the path's own coordinates it is drawn on, [x, y, w, h].
    pub view: [f64; 4],
}

/// What inserts `icon` in `deck`, `theme` its theme (PLAN 2.104): a path shape on the icon's
/// view, so icons beside each other keep one weight, filled with the theme's accent (or its
/// first color), as decoration a reader passes over, named by the icon (`alt`, `airplane-tilt` as
/// "Airplane tilt"), as an editor names an object. It starts a twelfth of the canvas wide and as
/// tall, a square.
pub fn icon(deck: &Deck, theme: &Theme, icon: &Icon) -> Insert {
    let [w, h] = [deck.canvas.width as f32, deck.canvas.height as f32];
    let colors = theme.names(Vocabulary::Color);
    let fill = colors.iter().find(|c| *c == "accent").or(colors.first());
    let mut node = icon_node(&icon.name, &icon.path, icon.view, fill.map_or("", String::as_str));
    if fill.is_none() {
        node.as_object_mut().expect("an object").remove("fill");
    }
    let side = 1.0 / 12.0;
    Insert {
        label: format!("Icon · {}", icon.name),
        node,
        id: slug(&icon.name, "icon"),
        start: Start::Box { w: side, h: side * w / h },
    }
}

/// An icon's node (PLAN 2.104, G.23): a path shape on its view, filled with `fill`, a theme color,
/// as decoration a reader passes over, named by the icon as an editor names an object
/// (`airplane-tilt` as "Airplane tilt"). What Insert, a layout's icon, and a deck made from a
/// prompt each put on a slide.
pub fn icon_node(name: &str, path: &str, view: [f64; 4], fill: &str) -> Value {
    // A whole number written whole, as the set writes it: `[0, 0, 256, 256]`.
    let whole = |v: f64| if v.fract() == 0.0 && v.abs() < 1e15 { json!(v as i64) } else { json!(v) };
    json!({
        "type": "shape",
        "kind": "path",
        "path": path,
        "view": view.map(whole),
        "alt": sentence(name),
        "semantic": "decoration",
        "fill": fill,
    })
}

/// Whether a shape of these props, each by its key, is an icon (PLAN 2.104, G.23), as
/// [`icon_node`] makes one: a path on its view, filled, and unstroked. A pen's drawing (PLAN 2.114)
/// is a path on its view too, stroked and unfilled, and no icon: what the icon picker would
/// replace, and what an empty place for an icon takes.
pub fn is_icon<'a>(prop: impl Fn(&str) -> Option<&'a Value>) -> bool {
    let set = |key: &str| prop(key).is_some_and(|v| !v.is_null());
    prop("kind").and_then(Value::as_str) == Some("path") && set("view") && set("fill") && !set("stroke")
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

    /// An image's mask (PLAN 2.113) is an oval or a polygon of the shape library, by its name: each
    /// the format names is one, and each of the library is one the format names.
    #[test]
    fn every_mask_is_an_oval_or_a_shape_of_the_library() {
        let schema = serde_json::to_value(schemars::schema_for!(crate::model::nodes::ImageMask)).unwrap();
        let mut named: Vec<String> = schema.to_string().split('"').map(String::from).collect();
        let masks: Vec<String> = polygons().into_iter().map(|(name, _, _)| name.to_string()).collect();
        for mask in masks.iter().chain([&"ellipse".to_string()]) {
            let typed: crate::model::nodes::ImageMask = serde_json::from_value(json!(mask)).unwrap();
            assert_eq!(serde_json::to_value(typed).unwrap(), json!(mask));
        }
        named.retain(|w| masks.contains(w) || w == "ellipse");
        named.dedup();
        assert_eq!(named.len(), masks.len() + 1, "{named:?}");
    }

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

    /// The shape library (PLAN 2.106): after the rectangle and the oval, each polygon, filled as
    /// they are, in a box its aspect makes, its points inside the box and touching each of its
    /// sides, as many as its shape has; and each said in a person's words.
    #[test]
    fn the_shape_librarys_polygons_are_offered_filled_and_named() {
        let deck = Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        let theme: Theme = serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
        let offered = inserts(&deck, &theme, &[], &BTreeMap::new());
        let shapes: Vec<&Insert> = offered.iter().filter(|i| i.node["type"] == "shape").collect();
        let names: Vec<&str> = shapes.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            names,
            [
                "rect",
                "ellipse",
                "triangle",
                "right-triangle",
                "diamond",
                "parallelogram",
                "trapezoid",
                "pentagon",
                "hexagon",
                "octagon",
                "star",
                "chevron",
                "right-arrow",
                "left-arrow",
                "up-arrow",
                "down-arrow",
                "plus",
                "callout",
                "line",
                "arrow"
            ]
        );
        let corners = [("triangle", 3), ("diamond", 4), ("pentagon", 5), ("hexagon", 6), ("octagon", 8), ("star", 10)];
        for insert in shapes.iter().filter(|i| i.node["kind"] == "polygon") {
            let id = insert.id.as_str();
            assert_eq!(insert.label, format!("Shape · {id}"));
            assert_eq!(insert.node["fill"], "accent", "{id}");
            let points: Vec<[f64; 2]> = serde_json::from_value(insert.node["points"].clone()).unwrap();
            assert!(points.len() >= 3, "{id}");
            if let Some((_, n)) = corners.iter().find(|(name, _)| *name == id) {
                assert_eq!(points.len(), *n, "{id}");
            }
            for axis in 0..2 {
                let (lo, hi) =
                    points.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p[axis]), hi.max(p[axis])));
                assert!(lo == 0.0 && hi == 1.0, "{id} fills its box: {lo}..{hi}");
            }
            assert!(crate::words::SHAPES.iter().any(|(name, _)| *name == id), "{id} is said in a person's words");
        }
        // A chevron's box is wider than tall; an up arrow's taller than wide; a star's as its points.
        let start = |id: &str| match &shapes.iter().find(|i| i.id == id).unwrap().start {
            Start::Box { w, h } => w * 1920.0 / (h * 1080.0),
            Start::Slot(_) => unreachable!(),
        };
        assert!((start("chevron") - 1.6).abs() < 1e-3 && (start("up-arrow") - 0.625).abs() < 1e-3);
        assert!((start("star") - 1.0515).abs() < 1e-3, "{}", start("star"));
        assert_eq!(crate::words::shape("right-arrow"), "Right Arrow");
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
