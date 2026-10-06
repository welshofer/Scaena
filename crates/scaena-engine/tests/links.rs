//! Links in a text (SPEC §3.5, PLAN 2.70), on the torture deck's `links` case: each run with a
//! `link` is underlined at its font's underline, a link that wraps on each of its lines, and the
//! frame's display list says where each is followed (`link` ops), which `link_at` finds.

use scaena_core::Deck;
use scaena_core::displaylist::{LinkTarget, Op};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::sample::Content;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

#[test]
fn a_link_is_underlined_and_followed_where_it_is_drawn() {
    let deck: Deck = serde_json::from_slice(&read("deck.json")).unwrap();
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let data = DataFiles::new();
    let req =
        FrameRequest { deck: &deck, theme: &theme, data: &data, state: "links", t_ms: f64::INFINITY, format: None };
    let mut engine = Engine::new(fonts);
    let scene = engine.at_rest(&req).unwrap();
    let (origin, text) = scene
        .nodes
        .iter()
        .find_map(|n| match &n.content {
            Content::Text(t) if n.id == "link-text" => Some((t.origin, &t.text)),
            _ => None,
        })
        .unwrap();
    // The web address wraps: an underline on each of its two lines; the state's, one.
    let href = LinkTarget::Href("https://example.com/scaena/method".into());
    let state = LinkTarget::State("shapes".into());
    let lines = |to: &LinkTarget| {
        let mut l: Vec<usize> =
            text.underlines.iter().filter(|u| u.target.as_ref() == Some(to)).map(|u| u.line).collect();
        l.dedup();
        l
    };
    assert_eq!(lines(&href), [0, 1], "{:?}", text.underlines);
    assert_eq!(lines(&state), [2]);
    for u in &text.underlines {
        let line = &text.lines[u.line];
        // Under the baseline, within the line, and as thick as the font says.
        assert!(u.rect[1] > line.baseline && u.rect[1] < line.top + line.height, "{u:?}");
        assert!(u.rect[3] >= 1.0 && u.rect[3] < 6.0, "{u:?}");
        // Under the words, not the space the line ends with.
        let last = text.runs.iter().filter(|r| r.line == u.line).flat_map(|r| r.glyphs.iter().zip(&r.advances));
        let ink_end = last.map(|(g, a)| g.x + a).fold(0.0, f32::max);
        assert!(u.rect[0] + u.rect[2] <= ink_end + 0.01);
    }
    let first = text.underlines.iter().find(|u| u.line == 0).unwrap();
    let words_end = text.lines[0].x + text.lines[0].width;
    assert!(
        (first.rect[0] + first.rect[2] - words_end).abs() < 0.5,
        "the space after `and` is not underlined: {first:?}"
    );
    // The frame: a fill and a link op for each underline, and a point on the words follows it.
    let dl = engine.frame(&req).unwrap().display_list;
    let links = dl.links();
    assert_eq!(links.len(), text.underlines.len());
    let u = text.underlines.iter().find(|u| u.target.as_ref() == Some(&state)).unwrap();
    let at = [origin[0] + u.area[0] + u.area[2] / 2.0, origin[1] + u.area[1] + u.area[3] / 2.0];
    assert_eq!(dl.link_at(at), Some(state));
    assert_eq!(dl.link_at([origin[0] + text.lines[2].x + text.lines[2].width - 2.0, at[1]]), None, "past the link");
    let fills = |ops: &[Op]| fn_count(ops);
    fn fn_count(ops: &[Op]) -> usize {
        ops.iter()
            .map(|op| match op {
                Op::Layer { ops, .. } => fn_count(ops),
                Op::Fill { .. } => 1,
                _ => 0,
            })
            .sum()
    }
    assert!(fills(&dl.ops) >= text.underlines.len());
    // How the state reads: each link an `a`, a state's with its name.
    let snaps = scaena_core::resolve_states(&deck).unwrap();
    let i = deck.state_index("links").unwrap();
    let read = scaena_core::reading::html(&deck, &snaps[i], &dl);
    assert!(read.contains(r##"<p data-node="link-text">Read how the deck is <a href="https://example.com/scaena/method">set and checked on the web</a>, or go back to <a href="#shapes" data-state="shapes" title="To Shapes, in this deck">the shapes case</a> and look again.</p>"##), "{read}");
}
