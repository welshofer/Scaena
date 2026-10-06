//! Containers through the whole engine (SPEC §3.4, PLAN 1.7), on the torture deck's
//! `containers` case: typographic anchors inside a row stack, and a child that moves
//! from one container to another between states, which morphs like any box. And a table
//! in a stack, which takes its rows as text takes its lines (SPEC §3.3).

use scaena_core::Deck;
use scaena_core::displaylist::{DisplayList, Op, Rect};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::sample::Content;
use scaena_engine::tables::TableLayout;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

/// The torture deck with only the `containers` state, then `edit` applied.
fn deck(edit: impl FnOnce(&mut Value)) -> Deck {
    let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    let states = d["states"].as_array_mut().unwrap();
    states.retain(|s| s["id"] == "containers");
    edit(&mut d);
    serde_json::from_value(d).unwrap()
}

fn engine(deck: &Deck) -> Engine {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let mut images = BundleImages::new();
    for path in deck.image_files() {
        images.register(&path, &read(&path)).unwrap();
    }
    Engine::new(fonts).with_images(images)
}

fn theme() -> Theme {
    Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap()
}

fn frame(deck: &Deck, state: &str, t_ms: f64) -> DisplayList {
    let theme = theme();
    let data = DataFiles::new();
    let req = FrameRequest { deck, theme: &theme, data: &data, state, t_ms, format: None };
    engine(deck).frame(&req).unwrap().display_list
}

/// Each layer's node and translation, in paint order.
fn layers(dl: &DisplayList) -> Vec<(String, [f32; 2])> {
    dl.ops
        .iter()
        .filter_map(|op| match op {
            Op::Layer { node: Some(n), transform, .. } => Some((n.clone(), [transform[4], transform[5]])),
            _ => None,
        })
        .collect()
}

fn at(dl: &DisplayList, node: &str) -> [f32; 2] {
    layers(dl).into_iter().find(|(n, _)| n == node).unwrap_or_else(|| panic!("no layer for `{node}`")).1
}

/// The baseline of a text node's last line, canvas units: its glyphs sit on it.
fn last_baseline(dl: &DisplayList, node: &str) -> f32 {
    let Some(Op::Layer { transform, ops, .. }) =
        dl.ops.iter().find(|op| matches!(op, Op::Layer { node: Some(n), .. } if n == node))
    else {
        panic!("no layer for `{node}`")
    };
    let y = ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyphs { glyphs, .. } => glyphs.iter().map(|g| g.y).reduce(f32::max),
            _ => None,
        })
        .reduce(f32::max)
        .unwrap();
    transform[5] + y
}

#[test]
fn a_row_stack_puts_its_texts_on_one_baseline() {
    let dl = frame(&deck(|_| {}), "containers", f64::INFINITY);
    let (figure, label) = (last_baseline(&dl, "tally-figure"), last_baseline(&dl, "tally-label"));
    // Row 5 of the torture grid ends at 642: both baselines sit on it.
    assert!((figure - 642.0).abs() < 1e-3 && (label - 642.0).abs() < 1e-3, "{figure} {label}");
    // The cards share the row: the same width apart.
    let [a, b, c] = ["stat-a", "stat-b", "stat-c"].map(|n| at(&dl, n)[0]);
    assert!(((b - a) - (c - b)).abs() < 1e-3, "{a} {b} {c}");
}

#[test]
fn a_child_moved_to_another_container_morphs_between_them() {
    let d = deck(|d| {
        let mut moved = d["states"][0].clone();
        moved["id"] = json!("moved");
        moved["mode"] = json!("delta");
        moved["props"] = json!({ "stat-a-figure": { "at": { "parent": "stat-c", "index": 1 } } });
        moved["transition"] = json!({ "duration": 400, "ease": "linear" });
        d["states"].as_array_mut().unwrap().push(moved);
    });
    let (before, after) = (frame(&d, "containers", f64::INFINITY), frame(&d, "moved", f64::INFINITY));
    let (from, to) = (at(&before, "stat-a-figure"), at(&after, "stat-a-figure"));
    assert!(to[0] > from[0] + 1000.0, "into the third card: {from:?} → {to:?}");
    let mid = at(&frame(&d, "moved", 200.0), "stat-a-figure");
    for k in 0..2 {
        assert!((mid[k] - (from[k] + to[k]) / 2.0).abs() < 1e-2, "halfway: {from:?} {mid:?} {to:?}");
    }
    // Mid-transition, layers keep the at-rest paint order: each container under its children.
    let order: Vec<String> = layers(&frame(&d, "moved", 200.0)).into_iter().map(|(n, _)| n).collect();
    let rest: Vec<String> = layers(&after).into_iter().map(|(n, _)| n).collect();
    assert_eq!(order, rest);
}

/// A bill, as a budget deck sets one: stack `bill`, `h` high, holds a table of six fees
/// (no header; `size` if any) and a total band, `space.5` (32 cu) apart. The torture
/// deck's fonts and theme, in its own format only, its tables' rows 6 cu apart and ruled
/// between.
fn bill(h: f32, size: Option<Value>) -> (Deck, Theme) {
    let mut table = json!({
        "type": "table", "data": "@fees", "header": false,
        "columns": [{ "field": "item" }, { "field": "amount", "format": "$.4~k" }],
        "at": { "parent": "bill" }
    });
    if let Some(size) = size {
        table["size"] = size;
    }
    let nodes = json!({
        "bill": { "type": "stack", "gap": "space.5", "at": { "rect": [120, 120, 900, h] } },
        "table": table,
        "total": { "type": "stack", "fill": "accent", "padding": ["space.3", "space.5"], "at": { "parent": "bill" } },
        "total-label": { "type": "text", "role": "body", "text": "Total", "at": { "parent": "total" } }
    });
    let fees = json!([
        { "item": "Coaching staff", "amount": 412000 },
        { "item": "Travel", "amount": 186500 },
        { "item": "Facilities", "amount": 240000 },
        { "item": "Equipment", "amount": 98250 },
        { "item": "Medical", "amount": 61000 },
        { "item": "Scholarships", "amount": 242250 }
    ]);
    let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    d.as_object_mut().unwrap().remove("formats");
    d["meta"] = json!({ "lang": "en-US" });
    d["data"] = json!({ "fees": { "source": { "inline": fees }, "schema": { "item": "string", "amount": "number" } } });
    let props: serde_json::Map<String, Value> =
        nodes.as_object().unwrap().keys().map(|k| (k.clone(), json!({}))).collect();
    d["nodes"] = nodes;
    d["states"] = json!([{ "id": "bill", "layout": "specimen", "props": props }]);
    let mut t: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    t["tables"] = json!({ "rowGap": 6, "rowRule": { "stroke": "thin" } });
    (serde_json::from_value(d).unwrap(), Theme::from_json(&t.to_string()).unwrap())
}

/// The bill laid out: each node's box, and the table as set.
fn lay_out_bill(deck: &Deck, theme: &Theme) -> (impl Fn(&str) -> Rect, TableLayout) {
    let snap = &scaena_core::resolve_states(deck).unwrap()[0];
    let scene = engine(deck).scene(deck, theme, &DataFiles::new(), snap).unwrap();
    let table = scene.nodes.iter().find_map(|n| match &n.content {
        Content::Table { table, .. } => Some((**table).clone()),
        _ => None,
    });
    (move |id: &str| scene.tree[id].rect, table.expect("a table"))
}

#[test]
fn a_table_in_a_stack_takes_its_rows_and_what_follows_starts_a_gap_under_them() {
    for size in [Some(json!({ "h": "fit" })), None] {
        let (deck, theme) = bill(840.0, size.clone());
        let (rect, table) = lay_out_bill(&deck, &theme);
        // Its box ends where its last row does: `rowGap` under that row's text. The rules
        // between rows lie on the lines between them and take no room.
        let last = table.cells.iter().map(|c| c.at[0]).max().unwrap();
        let text = table.cells.iter().filter(|c| c.at[0] == last).map(|c| c.origin[1] + c.text.height);
        let rows = text.fold(0.0, f32::max) + 6.0;
        let [x, y, w, h] = rect("table");
        assert_eq!(table.row_rules.len(), 5, "{size:?}");
        assert!((h - rows).abs() < 1e-3 && table.overflow.is_none(), "{size:?}: {h} for {rows} cu of rows");
        // Across the stack, and the band one gap under the last row.
        assert_eq!([x, y, w], [120.0, 120.0, 900.0], "{size:?}");
        let total = rect("total");
        assert!(
            (total[1] - (y + h + 32.0)).abs() < 1e-3,
            "{size:?}: the band at {}, the rows end at {}",
            total[1],
            y + h
        );
        // Nothing to report: the rows fit, and nothing overlaps.
        let found = scaena_engine::lint::lint(&mut engine(&deck), &deck, &theme, &DataFiles::new(), None).unwrap();
        let errors: Vec<_> = found.iter().filter(|f| f.code.starts_with('E')).collect();
        assert!(errors.is_empty(), "{size:?}: {errors:#?}");
    }
}

#[test]
fn a_table_short_of_room_in_a_stack_takes_what_is_left_and_says_what_to_cut() {
    let (deck, theme) = bill(840.0, None);
    let band = lay_out_bill(&deck, &theme).0("total")[3];
    // In 200 cu, the band keeps its line and padding, and the table takes the rest.
    let (deck, theme) = bill(200.0, None);
    let left = 200.0 - 32.0 - band;
    let data = DataFiles::new();
    let req =
        FrameRequest { deck: &deck, theme: &theme, data: &data, state: "bill", t_ms: f64::INFINITY, format: None };
    let refused = engine(&deck).frame(&req).unwrap_err().to_string();
    let cell = format!("its cell is {left:.0} high");
    assert!(refused.contains("node `table`") && refused.contains(&cell) && refused.contains("\"limit\""), "{refused}");
    // Lint lays it out anyway and reports it, in the same cell: what the stack had left.
    let found = scaena_engine::lint::lint(&mut engine(&deck), &deck, &theme, &data, None).unwrap();
    let e100: Vec<_> = found.iter().filter(|f| f.code == "E100").collect();
    assert_eq!(e100.len(), 1, "{found:#?}");
    assert!(e100[0].node.as_deref() == Some("table") && e100[0].message.contains(&cell), "{e100:#?}");
}

/// The torture deck's `containers` state with a caption `depth` containers deep: `stats`,
/// then a column of stacks, one in the next.
fn nested(depth: usize) -> Deck {
    deck(|d| {
        let mut parent = "stats".to_string();
        for level in 2..=depth {
            let id = format!("level-{level}");
            d["nodes"][&id] = json!({ "type": "stack", "semantic": "evidence", "at": { "parent": parent } });
            d["states"][0]["props"][&id] = json!({});
            parent = id;
        }
        let caption = json!({ "type": "text", "role": "caption", "text": "deep", "semantic": "evidence", "at": { "parent": parent } });
        d["nodes"]["deepest"] = caption;
        d["states"][0]["props"]["deepest"] = json!({});
    })
}

/// The torture bundle, as `scaena validate` reads it.
struct Bundle;

impl scaena_core::validate::BundleFiles for Bundle {
    fn exists(&self, path: &str) -> bool {
        std::path::Path::new(BUNDLE).join(path).is_file()
    }

    fn read_text(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(format!("{BUNDLE}/{path}")).ok()
    }
}

/// What validation says of `deck` in the torture bundle, E106 alone.
fn e106(deck: &Deck) -> Vec<scaena_core::Finding> {
    let found = scaena_core::validate::validate_bundle(&deck.to_json().unwrap(), &Bundle).unwrap();
    found.into_iter().filter(|f| f.code == "E106").collect()
}

/// Each level of containers lays out by recursion, on the small stack a browser's worker
/// has: they nest at most `MAX_NESTING` deep, which validation says (E106) and the engine,
/// which a player hands a deck it has not validated, refuses rather than overflow.
#[test]
fn containers_nest_at_most_64_deep() {
    use scaena_core::document::MAX_NESTING;
    let deep = nested(MAX_NESTING);
    assert_eq!(e106(&deep), []);
    let dl = frame(&deep, "containers", f64::INFINITY);
    assert!(layers(&dl).iter().any(|(n, _)| n == "deepest"));

    let deeper = nested(MAX_NESTING + 1);
    let found = e106(&deeper);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].message.contains("node `deepest` is 65 containers deep"), "{}", found[0].message);
    let theme = theme();
    let data = DataFiles::new();
    let req = FrameRequest {
        deck: &deeper,
        theme: &theme,
        data: &data,
        state: "containers",
        t_ms: f64::INFINITY,
        format: None,
    };
    let Err(error) = engine(&deeper).frame(&req) else { panic!("too deep to lay out") };
    assert!(error.to_string().contains("containers nest more than 64 deep"), "{error}");
}
