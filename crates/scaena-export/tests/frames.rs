//! A picture's frame (PLAN T.3a) in the SVG and the PDF: the torture deck's pictures drawn in a
//! theme that frames them, from the golden display lists the engine's `torture` test holds
//! (`tests/golden/torture/<state>+<frame>.dl.json`). The frame is rings of fill, which the painters'
//! own tests draw from the same goldens; here each writer draws them too.

use scaena_core::Deck;
use scaena_core::displaylist::{Color, DisplayList, FillRule, Op, Paint};
use scaena_engine::images::BundleImages;
use scaena_export::pdf::{Page, PdfSettings, pdf};
use scaena_export::svg::{SvgSettings, svg};
use scaena_paint::Assets;

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";
const GOLDEN: &str = "../../tests/golden/torture";

fn golden(name: &str) -> DisplayList {
    DisplayList::from_json(&std::fs::read_to_string(format!("{GOLDEN}/{name}.dl.json")).unwrap()).unwrap()
}

/// The fonts and pictures the torture deck's display lists name.
fn assets(lists: &[&DisplayList]) -> Assets {
    let mut store = Assets::new();
    for font in lists.iter().flat_map(|dl| dl.fonts.iter()) {
        store.insert_font(&font.id, std::fs::read(format!("{BUNDLE}/{}", font.id)).unwrap());
    }
    let deck = Deck::from_json(&std::fs::read_to_string(format!("{BUNDLE}/deck.json")).unwrap()).unwrap();
    let mut images = BundleImages::new();
    for path in deck.image_files() {
        let bytes = std::fs::read(format!("{BUNDLE}/{path}")).unwrap();
        let info = images.register(&path, &bytes).unwrap();
        store.insert_image(&info.id, &bytes).unwrap();
    }
    store
}

/// Each ring's color, everywhere in the list.
fn rings(ops: &[Op], out: &mut Vec<Color>) {
    for op in ops {
        match op {
            Op::Layer { ops, .. } => rings(ops, out),
            Op::Fill { rule: FillRule::EvenOdd, paint: Paint::Solid(c), .. } => out.push(*c),
            _ => {}
        }
    }
}

/// `#rrggbb`, as the SVG writes a color.
fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0[0], c.0[1], c.0[2])
}

#[test]
fn the_svg_draws_each_pictures_frame_as_rings() {
    for name in ["images+mat", "shadows+keyline"] {
        let list = golden(name);
        let mut colors = Vec::new();
        rings(&list.ops, &mut colors);
        assert!(!colors.is_empty(), "{name}: the golden frames its pictures");
        let written = svg(&list, &assets(&[&list]), &SvgSettings { scale: 1.0, title: None, lang: None }).unwrap();
        let drawn = written.matches("fill-rule=\"evenodd\"").count();
        assert_eq!(drawn, colors.len(), "{name}: one even-odd path for each band");
        for c in colors {
            assert!(written.to_lowercase().contains(&hex(c)), "{name}: a band in {}", hex(c));
        }
    }
}

#[test]
fn the_pdf_draws_the_frame() {
    let deck = Deck::from_json(&std::fs::read_to_string(format!("{BUNDLE}/deck.json")).unwrap()).unwrap();
    let (plain, framed) = (golden("images"), golden("images+mat"));
    let store = assets(&[&plain, &framed]);
    let page = |list: &DisplayList| Page { state: "images".into(), list: list.clone() };
    let settings = PdfSettings::default();
    let unframed = pdf(&deck, &[page(&plain)], &store, &settings).unwrap();
    let written = pdf(&deck, &[page(&framed)], &store, &settings).unwrap();
    assert!(written.starts_with(b"%PDF-"));
    assert_ne!(written, unframed, "the frame is drawn on the page");
}
