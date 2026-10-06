//! A text's paragraphs as a list (SPEC §3.5, ADR-0018), with the torture deck's fonts and
//! theme: each item hangs at its level's indent, its lines breaking short of it, and its marker
//! is set on its first line, its end the theme's gap short of its words. Numbers count by level.
//! A text that is no list is laid out as it was.

use scaena_core::Deck;
use scaena_core::displaylist::Op;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::sample::{Content, Scene};
use scaena_engine::text::TextLayout;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

/// The torture deck with one state, `s`, of `nodes`.
fn deck(nodes: Value) -> Deck {
    let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    let props: serde_json::Map<String, Value> =
        nodes.as_object().unwrap().keys().map(|k| (k.clone(), json!({}))).collect();
    d["nodes"] = nodes;
    d["states"] = json!([{ "id": "s", "layout": "specimen", "props": props }]);
    serde_json::from_value(d).unwrap()
}

fn at_rest(deck: &Deck) -> (Scene, scaena_core::displaylist::DisplayList) {
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let data = DataFiles::new();
    let req = FrameRequest { deck, theme: &theme, data: &data, state: "s", t_ms: f64::INFINITY, format: None };
    let mut engine = Engine::new(fonts);
    let scene = engine.at_rest(&req).unwrap();
    let list = engine.frame(&req).unwrap().display_list;
    (scene, list)
}

fn text<'a>(scene: &'a Scene, id: &str) -> &'a TextLayout {
    scene
        .nodes
        .iter()
        .find_map(|n| match &n.content {
            Content::Text(t) if n.id == id => Some(&t.text),
            _ => None,
        })
        .unwrap()
}

const WORDS: &str = "First point, long enough that it has to wrap onto a second line in its box\nSecond\nA sub-point under it\nAnother\nThird";

fn node(list: Value) -> Value {
    json!({ "t": { "type": "text", "role": "body", "text": WORDS, "list": list, "at": { "col": [1, 6], "row": [2, 8] } } })
}

#[test]
fn items_hang_at_their_indent_with_their_markers() {
    let list = json!([
        { "kind": "number" }, { "kind": "number" }, { "kind": "bullet", "level": 1 }, { "kind": "bullet", "level": 1 }, null
    ]);
    let (scene, dl) = at_rest(&deck(node(list)));
    let t = text(&scene, "t");
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let (indent, gap, _, _) = theme.typography.lists.clone().unwrap_or_default().at(0);
    let size = t.runs.iter().find(|r| r.mark.is_none()).unwrap().size;
    let (one, two) = ((indent as f32) * size, 2.0 * (indent as f32) * size);
    let start = |k: usize| t.lines.iter().filter(|l| l.paragraph == k).map(|l| l.x).collect::<Vec<_>>();
    // The first item wraps, and each of its lines starts at its indent.
    assert!(start(0).len() >= 2, "{:?}", t.lines);
    assert!(start(0).iter().all(|&x| (x - one).abs() < 0.01), "{:?}", start(0));
    assert!(start(2).iter().all(|&x| (x - two).abs() < 0.01), "{:?}", start(2));
    assert!(start(4).iter().all(|&x| x.abs() < 0.01), "a paragraph that is no item: {:?}", start(4));
    // Its lines break short of the indent.
    for l in t.lines.iter().filter(|l| l.paragraph == 0) {
        assert!(l.x + l.width <= t.measure + 0.01, "{l:?}");
    }
    // Markers: 1. 2. ◦ ◦, the third paragraph's ending the gap short of its words.
    let marks: Vec<(&str, usize)> =
        t.runs.iter().filter_map(|r| Some((r.mark.as_deref()?, t.lines[r.line].paragraph))).collect();
    assert_eq!(marks, [("1.", 0), ("2.", 1), ("\u{2013}", 2), ("\u{2013}", 3)]);
    let third = t.runs.iter().find(|r| r.mark.is_some() && t.lines[r.line].paragraph == 2).unwrap();
    let end = third.glyphs.last().unwrap().x + third.advances.last().unwrap();
    assert!((end - (two - gap as f32 * size)).abs() < 0.5, "the marker ends at {end}");
    assert_eq!(third.glyphs[0].y, t.lines[third.line].baseline);
    // The display list draws each paragraph's marker and words in layers of their own.
    let mut cells = Vec::new();
    fn walk(ops: &[Op], cells: &mut Vec<[u32; 2]>) {
        for op in ops {
            if let Op::Layer { cell, ops, .. } = op {
                cells.extend(*cell);
                walk(ops, cells);
            }
        }
    }
    walk(&dl.ops, &mut cells);
    assert_eq!(cells, [[0, 0], [0, 1], [1, 0], [1, 1], [2, 0], [2, 1], [3, 0], [3, 1], [4, 1]]);
}

#[test]
fn a_text_that_is_no_list_is_laid_out_as_it_was() {
    let (plain, plain_dl) = at_rest(&deck(node(Value::Null)));
    let (empty, empty_dl) = at_rest(&deck(node(json!([null, null]))));
    assert_eq!(text(&plain, "t"), text(&empty, "t"));
    assert_eq!(plain_dl, empty_dl);
    assert!(text(&plain, "t").runs.iter().all(|r| r.mark.is_none()));
}

#[test]
fn an_empty_item_shows_its_marker() {
    let deck = deck(
        json!({ "t": { "type": "text", "role": "body", "text": "One\n", "list": [{ "kind": "bullet" }, { "kind": "bullet" }], "at": { "col": [1, 6], "row": [2, 4] } } }),
    );
    let (scene, _) = at_rest(&deck);
    let t = text(&scene, "t");
    assert_eq!(t.runs.iter().filter(|r| r.mark.is_some()).count(), 2);
    assert_eq!(t.lines.len(), 2);
}

#[test]
fn a_marker_its_font_lacks_is_e120_by_its_own_character() {
    // ADR-0018: the torture deck's family has no ◦ (U+25E6); a theme that marks level 1 with it
    // sets a box, and lint names the marker's character, not the item's first.
    let mut theme: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    theme["type"]["lists"] = json!({ "bullets": ["\u{2022}", "\u{25E6}"] });
    let theme = Theme::from_json(&theme.to_string()).unwrap();
    let deck = deck(
        json!({ "t": { "type": "text", "role": "body", "text": "One\nTwo", "list": [{ "kind": "bullet" }, { "kind": "bullet", "level": 1 }], "at": { "col": [1, 6], "row": [2, 4] } } }),
    );
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let data = DataFiles::new();
    let mut engine = Engine::new(fonts);
    let found = scaena_engine::lint::lint(&mut engine, &deck, &theme, &data, None).unwrap();
    let e120: Vec<_> = found.iter().filter(|f| f.code == "E120").collect();
    assert!(!e120.is_empty(), "{found:#?}");
    for f in e120 {
        assert!(f.message.contains("`◦` (U+25E6)") && !f.message.contains("`T`"), "{}", f.message);
    }
}
