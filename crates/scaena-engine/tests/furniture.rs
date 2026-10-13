//! A slide's number and footer (ADR-0028, PLAN 2.103), on the example deck in Dusk: drawn over
//! its slides where the deck shows them, numbered by slide, left off the title slides the theme
//! leaves them off unless a slide asks for them, the same through a slide's steps, and fading in
//! as a slide that shows them comes in.

use scaena_core::Deck;
use scaena_core::displaylist::{DisplayList, Op};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::furniture::{FOOTER, NUMBER};
use scaena_engine::sample::{Content, Scene};
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use serde_json::{Value, json};

const EXAMPLES: &str = "../../docs/examples";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{EXAMPLES}/{path}")).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The example deck, edited, with its fonts and data in an engine.
struct Fx {
    deck: Deck,
    theme: Theme,
    data: DataFiles,
    engine: Engine,
}

fn fx(edit: impl FnOnce(&mut Value)) -> Fx {
    let mut v: Value = serde_json::from_slice(&read("revenue.deck.json")).unwrap();
    edit(&mut v);
    let deck: Deck = serde_json::from_value(v).unwrap();
    let theme = Theme::from_json(&String::from_utf8(read("themes/dusk.theme.json")).unwrap()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let mut data = DataFiles::new();
    for source in deck.data.values() {
        if let Value::String(path) = &source.source {
            data.insert(path.clone(), read(path));
        }
    }
    Fx { deck, theme, data, engine: Engine::new(fonts) }
}

impl Fx {
    fn at(&mut self, state: &str, t_ms: f64) -> (Scene, DisplayList) {
        let req = FrameRequest { deck: &self.deck, theme: &self.theme, data: &self.data, state, t_ms, format: None };
        let scene = self.engine.at_rest(&req).unwrap();
        (scene, self.engine.frame(&req).unwrap().display_list)
    }
}

/// The words each piece of furniture sets, by its id.
fn words(scene: &Scene) -> Vec<(String, String)> {
    (scene.furniture.iter())
        .map(|f| match &f.content {
            Content::Text(placed) => (f.id.clone(), placed.text.text.clone()),
            _ => panic!("furniture is words"),
        })
        .collect()
}

/// The layers of the frame's own ops that draw furniture, in order: (id, opacity).
fn layers(dl: &DisplayList) -> Vec<(String, f32)> {
    (dl.ops.iter())
        .filter_map(|op| match op {
            Op::Layer { node: Some(id), opacity, .. } if id.starts_with(':') => Some((id.clone(), *opacity)),
            _ => None,
        })
        .collect()
}

fn shown(edit: impl FnOnce(&mut Value)) -> Fx {
    fx(|d| {
        d["furniture"] = json!({ "number": true, "footer": "Q3 Review · Board" });
        edit(d);
    })
}

#[test]
fn a_slide_shows_its_number_and_footer_where_the_theme_places_them() {
    let mut f = shown(|_| {});
    let (scene, dl) = f.at("revenue", f64::INFINITY);
    // Set in Dusk's `label`, which sets its words in capitals.
    assert_eq!(
        words(&scene),
        [(NUMBER.to_string(), "2".to_string()), (FOOTER.to_string(), "Q3 REVIEW · BOARD".to_string())],
        "the second slide of three"
    );
    // In the margin at the foot: the number at the right of the last column, the footer from the
    // left of the first. Dusk's margin is 96 down and 120 across a 1920 × 1080 canvas.
    for item in &scene.furniture {
        let ink = item.ink().unwrap();
        assert!(ink[1] > 1080.0 - 96.0 && ink[1] + ink[3] < 1080.0, "{}: {ink:?}", item.id);
        match item.id.as_str() {
            NUMBER => assert!((ink[0] + ink[2] - 1800.0).abs() < 1.0, "the number ends at the grid's edge: {ink:?}"),
            _ => assert!((ink[0] - 120.0).abs() < 4.0, "the footer starts at the grid's edge: {ink:?}"),
        }
    }
    // Drawn over every node, last.
    let ids = layers(&dl);
    assert_eq!(ids, [(NUMBER.to_string(), 1.0), (FOOTER.to_string(), 1.0)]);
    let last = dl.ops.len();
    assert!(matches!(&dl.ops[last - 1], Op::Layer { node: Some(id), .. } if id == FOOTER));
    // A step of the slide is its slide's number.
    let (step, _) = f.at("mix", f64::INFINITY);
    assert_eq!(words(&step)[0].1, "2");
}

#[test]
fn a_title_slide_shows_none_unless_it_asks_and_a_slide_may_hide_them() {
    // The intro and the close are in `title`, which Dusk leaves them off.
    let mut f = shown(|_| {});
    assert!(f.at("intro", f64::INFINITY).0.furniture.is_empty());
    assert!(layers(&f.at("close", f64::INFINITY).1).is_empty());
    // The close asks for them; the revenue slide hides them.
    let mut f = shown(|d| {
        d["states"][3]["furniture"] = json!(true);
        d["states"][1]["furniture"] = json!(false);
    });
    assert_eq!(words(&f.at("close", f64::INFINITY).0)[0].1, "3");
    assert!(f.at("revenue", f64::INFINITY).0.furniture.is_empty());
    assert!(f.at("mix", f64::INFINITY).0.furniture.is_empty(), "its step hides them with it");
    // A deck that shows neither draws none.
    let mut none = fx(|_| {});
    assert!(none.at("revenue", f64::INFINITY).0.furniture.is_empty());
    // The footer alone, without the number.
    let mut footer = fx(|d| d["furniture"] = json!({ "footer": "Board" }));
    assert_eq!(words(&footer.at("revenue", f64::INFINITY).0), [(FOOTER.to_string(), "BOARD".to_string())]);
}

#[test]
fn the_number_fades_in_with_its_slide_and_stays_through_its_steps() {
    let mut f = shown(|_| {});
    // Into the revenue slide from the title: its furniture comes in with it, part way.
    let (_, dl) = f.at("revenue", 200.0);
    let into = layers(&dl);
    assert_eq!(into.len(), 2, "{into:?}");
    assert!(into.iter().all(|(_, o)| *o > 0.0 && *o < 1.0), "fading in: {into:?}");
    // Into its step: the same number and footer, as they were.
    let (_, dl) = f.at("mix", 200.0);
    assert_eq!(layers(&dl), [(NUMBER.to_string(), 1.0), (FOOTER.to_string(), 1.0)]);
}
