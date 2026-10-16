//! Decks that read right to left (PLAN T.5a, ADR-0031 call 8): `tests/fixtures/rtl/`'s deck in
//! Arabic and its twin in Hebrew, written by `scripts/build_rtl_decks.py`.
//!
//! - Golden display lists per state under `tests/golden/rtl/`, as the torture deck's are. After a
//!   reviewed change: `SCAENA_BLESS=1 cargo test -p scaena-engine --test rtl`. On a mismatch the
//!   new output is written to `actual/` beside the goldens (git-ignored) for diffing.
//! - Each deck stands mirrored against itself laid out left to right (its language written in the
//!   Latin script): every box, slot, cell, `rect`, and stack; its picture and its chart's figures
//!   are not mirrored.
//! - It reads in the same order either way.

use scaena_core::Deck;
use scaena_core::displaylist::{DisplayList, Op, quantize};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::sample::Scene;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use std::collections::BTreeMap;

const BUNDLE: &str = "../../tests/fixtures/rtl";
const GOLDEN: &str = "../../tests/golden/rtl";
const DECKS: [&str; 2] = ["arabic", "hebrew"];

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

struct Fixture {
    deck: Deck,
    theme: Theme,
    data: DataFiles,
    engine: Engine,
}

/// The deck `name`, its language `lang` where given: `ar-Latn` lays the Arabic deck out left to
/// right, its words as they are.
fn fixture(name: &str, lang: Option<&str>) -> Fixture {
    let mut deck = Deck::from_json(&String::from_utf8(read(&format!("{name}.deck.json"))).unwrap()).unwrap();
    if let Some(lang) = lang {
        deck.meta.get_or_insert_with(Default::default).lang = Some(lang.into());
    }
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    fonts.check_theme(&theme).unwrap();
    let mut images = BundleImages::new();
    for path in deck.image_files() {
        images.register(&path, &read(&path)).unwrap();
    }
    Fixture { deck, theme, data: DataFiles::new(), engine: Engine::new(fonts).with_images(images) }
}

impl Fixture {
    fn states(&self) -> Vec<String> {
        self.deck.states.iter().map(|s| s.id.clone()).collect()
    }

    fn frame(&mut self, state: &str) -> DisplayList {
        let req = FrameRequest {
            deck: &self.deck,
            theme: &self.theme,
            data: &self.data,
            state,
            t_ms: f64::INFINITY,
            format: None,
        };
        self.engine.frame(&req).unwrap_or_else(|e| panic!("{state}: {e}")).display_list
    }

    fn at_rest(&mut self, state: &str) -> Scene {
        let req = FrameRequest {
            deck: &self.deck,
            theme: &self.theme,
            data: &self.data,
            state,
            t_ms: f64::INFINITY,
            format: None,
        };
        self.engine.at_rest(&req).unwrap()
    }
}

#[test]
fn display_lists_match_goldens() {
    let bless = std::env::var_os("SCAENA_BLESS").is_some();
    let mut changed = Vec::new();
    for name in DECKS {
        let mut fx = fixture(name, None);
        for state in fx.states() {
            let mut dl = fx.frame(&state);
            quantize(&mut dl);
            let json = dl.to_golden_json().unwrap_or_else(|e| panic!("{name} {state}: {e:?}"));
            let path = format!("{GOLDEN}/{name}/{state}.dl.json");
            if bless {
                std::fs::create_dir_all(format!("{GOLDEN}/{name}")).unwrap();
                std::fs::write(&path, &json).unwrap();
            } else if std::fs::read_to_string(&path).ok().as_deref() != Some(json.as_str()) {
                std::fs::create_dir_all(format!("{GOLDEN}/actual/{name}")).unwrap();
                std::fs::write(format!("{GOLDEN}/actual/{name}/{state}.dl.json"), &json).unwrap();
                changed.push(format!("{name}/{state}"));
            }
        }
    }
    assert!(
        changed.is_empty(),
        "display lists differ from tests/golden/rtl for {changed:?}; new output is in tests/golden/rtl/actual/. \
         Review the diff, then bless with SCAENA_BLESS=1."
    );
}

/// Each deck as it reads, right to left, and as it would stand laid out left to right.
fn both(name: &str) -> (Fixture, Fixture) {
    let lang = fixture(name, None).deck.meta.unwrap().lang.unwrap();
    (fixture(name, None), fixture(name, Some(&format!("{lang}-Latn"))))
}

#[test]
fn every_box_stands_mirrored_where_the_deck_reads_right_to_left() {
    for name in DECKS {
        let (mut rtl, mut ltr) = both(name);
        let width = rtl.deck.canvas.width as f32;
        assert!(scaena_core::direction::mirrored(&rtl.deck) && !scaena_core::direction::mirrored(&ltr.deck));
        let mut compared = 0;
        for state in rtl.states() {
            let (mirrored, laid) = (rtl.at_rest(&state), ltr.at_rest(&state));
            assert!(mirrored.mirrored && !laid.mirrored);
            let boxes = |scene: &Scene| -> BTreeMap<String, [f32; 4]> {
                scene.boxes().into_iter().map(|b| (b.node, b.rect)).collect()
            };
            let (mirrored, laid) = (boxes(&mirrored), boxes(&laid));
            assert_eq!(mirrored.keys().collect::<Vec<_>>(), laid.keys().collect::<Vec<_>>());
            for (id, [x, y, w, h]) in &laid {
                let want = [width - x - w, *y, *w, *h];
                let got = mirrored[id];
                assert!(
                    got.iter().zip(want).all(|(a, b)| (a - b).abs() < 0.01),
                    "{name} {state}: `{id}` stands at {got:?}, not {want:?}"
                );
                compared += 1;
            }
        }
        assert!(compared >= 30, "{name}: {compared} boxes");
        // The stack's words run from the right: its first on the right, its last on the left.
        let order = rtl.at_rest("free");
        let at = |id: &str| order.boxes().into_iter().find(|b| b.node == id).unwrap().rect[0];
        assert!(at("free-order-1") > at("free-order-2") && at("free-order-2") > at("free-order-3"), "{name}");
    }
}

#[test]
fn a_picture_and_a_charts_figures_are_not_mirrored() {
    for name in DECKS {
        let (mut rtl, mut ltr) = both(name);
        // The picture is drawn as it is in its box, which stands mirrored: the same part of it,
        // the right way round.
        let image = |dl: &DisplayList| {
            fn find(ops: &[Op]) -> Option<([f32; 4], [f32; 4])> {
                ops.iter().find_map(|op| match op {
                    Op::Image { src, dst, .. } => Some((*src, *dst)),
                    Op::Layer { ops, .. } => find(ops),
                    _ => None,
                })
            }
            find(&dl.ops).expect("the photo slide draws its picture")
        };
        let ((src, dst), (was_src, was_dst)) = (image(&rtl.frame("photo")), image(&ltr.frame("photo")));
        assert_eq!(src, was_src, "{name}: the same part of the picture");
        assert_eq!(dst, was_dst, "{name}: drawn the same in its box");
        assert!(src[2] > 0.0 && dst[2] > 0.0, "{name}: the right way round");
        // The chart's bars run as its rows do, January first on the left, each where it stands
        // in the chart laid out left to right.
        let marks = |scene: &Scene| scene.marks_in("free-chart").unwrap().0;
        let (bars, was) = (marks(&rtl.at_rest("free")), marks(&ltr.at_rest("free")));
        assert_eq!(bars.len(), 4);
        assert!(bars.windows(2).all(|w| w[0].rect[0] < w[1].rect[0]), "{name}: the months run left to right");
        let chart = |scene: &Scene| scene.boxes().into_iter().find(|b| b.node == "free-chart").unwrap().rect;
        let shift = chart(&rtl.at_rest("free"))[0] - chart(&ltr.at_rest("free"))[0];
        for (bar, was) in bars.iter().zip(&was) {
            assert!((bar.rect[0] - was.rect[0] - shift).abs() < 0.01, "{name}: {:?} against {:?}", bar.rect, was.rect);
        }
    }
}

#[test]
fn a_mirrored_deck_reads_in_its_own_order() {
    for name in DECKS {
        let (mut rtl, mut ltr) = both(name);
        let snapshots = scaena_core::resolve_states(&rtl.deck).unwrap();
        for snap in &snapshots {
            let state = snap.state_id.clone();
            let read = |fx: &mut Fixture| {
                let dl = fx.frame(&state);
                let snaps = scaena_core::resolve_states(&fx.deck).unwrap();
                let snap = snaps.iter().find(|s| s.state_id == state).unwrap();
                scaena_core::reading::parts(&fx.deck, snap, &dl).into_iter().map(|p| p.node).collect::<Vec<_>>()
            };
            let (mirrored, laid) = (read(&mut rtl), read(&mut ltr));
            assert_eq!(mirrored, laid, "{name} {state}");
            // As the deck writes them: the slide's header first, then what it says, in turn.
            let written: Vec<&String> = snap.nodes.keys().filter(|id| mirrored.contains(id)).collect();
            assert_eq!(mirrored.iter().collect::<Vec<_>>(), written, "{name} {state}");
        }
        // The cover reads its kicker, its title, then its subtitle.
        let dl = rtl.frame("cover");
        let parts = scaena_core::reading::parts(&rtl.deck, &snapshots[0], &dl);
        let ids: Vec<&str> = parts.iter().map(|p| p.node.as_str()).collect();
        assert_eq!(ids, ["cover-kicker", "cover-title", "cover-subtitle"], "{name}");
    }
}
