//! Where a caret stands in a text (ADR-0013, PLAN 2.32), on the torture deck's text cases:
//! every character as a reader counts it, on its line, between the edges of the glyphs the
//! frame draws for it. Capitals that set ß as SS, soft hyphens hyphenation inserts, Hebrew
//! and Arabic around Latin, ligatures, combining marks, emoji sequences, and runs in several
//! looks all map back to the text as written.

use icu_segmenter::GraphemeClusterSegmenter;
use scaena_core::Deck;
use scaena_core::displaylist::Op;
use scaena_engine::carets::Carets;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::sample::Scene;
use scaena_engine::text::{TextAlign, TextEngine, TextSpec};
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

/// Each text case, by its state and node.
const TEXTS: [(&str, &str); 12] = [
    ("liga", "liga-on"),
    ("combining", "combining-1"),
    ("emoji", "emoji-line"),
    ("mixed", "mixed-line"),
    ("bidi-hebrew", "bidi-he-line"),
    ("bidi-arabic", "bidi-ar-line"),
    ("case-measure", "case-upper"),
    ("case-measure", "case-title"),
    ("hyphenation", "hyph-on"),
    ("hanging", "hanging-quote"),
    ("pretty", "pretty-para"),
    ("alignment", "align-rtl"),
];

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

fn torture() -> (Deck, Theme, Engine) {
    let deck: Deck = serde_json::from_slice(&read("deck.json")).unwrap();
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let mut images = BundleImages::new();
    for path in deck.image_files() {
        images.register(&path, &read(&path)).unwrap();
    }
    (deck, theme, Engine::new(fonts).with_images(images))
}

fn at_rest(engine: &mut Engine, deck: &Deck, theme: &Theme, state: &str) -> Scene {
    let data = DataFiles::new();
    let req = FrameRequest { deck, theme, data: &data, state, t_ms: f64::INFINITY, format: None };
    engine.at_rest(&req).unwrap()
}

fn carets(state: &str, node: &str) -> Carets {
    let (deck, theme, mut engine) = torture();
    at_rest(&mut engine, &deck, &theme, state).carets(node).unwrap_or_else(|| panic!("`{node}` is no text in {state}"))
}

/// `text` in the torture theme's `specimen` role, across `width`, aligned by `align`.
fn set(text: &str, width: f32, align: TextAlign) -> Carets {
    let (deck, theme, _) = torture();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let mut spec = TextSpec::plain(theme.text_role("specimen").unwrap(), text);
    spec.align = align;
    TextEngine::new().layout(&mut fonts, &theme, &spec, width).unwrap().carets([0.0, 0.0])
}

fn width(c: &scaena_engine::carets::CaretChar) -> f32 {
    (c.trail - c.lead).abs()
}

/// The character at `offset` and its line.
fn char_at(carets: &Carets, offset: usize) -> (usize, scaena_engine::carets::CaretChar) {
    carets
        .lines
        .iter()
        .enumerate()
        .find_map(|(l, line)| line.chars.iter().find(|c| c.offset == offset).map(|c| (l, *c)))
        .unwrap_or_else(|| panic!("no character at {offset} in {:?}", carets.text))
}

/// Every character of the text as written, as a reader counts it, stands on exactly one
/// line, in the text's order; and the lines run end to end over the whole text.
#[test]
fn every_character_as_written_stands_once_in_order() {
    for (state, node) in TEXTS {
        let c = carets(state, node);
        let graphemes: Vec<usize> = GraphemeClusterSegmenter::new().segment_str(&c.text).collect();
        let offsets: Vec<usize> = c.lines.iter().flat_map(|l| l.chars.iter().map(|ch| ch.offset)).collect();
        assert_eq!(offsets, graphemes[..graphemes.len() - 1], "{state}/{node}");
        assert_eq!(c.lines.first().map(|l| l.start), Some(0), "{state}/{node}");
        assert_eq!(c.lines.last().map(|l| l.end), Some(c.text.len()), "{state}/{node}");
        for pair in c.lines.windows(2) {
            assert_eq!(pair[0].end, pair[1].start, "{state}/{node}: lines run end to end");
            assert!(pair[0].bottom <= pair[1].top + 1e-3, "{state}/{node}: lines stack");
        }
        for line in &c.lines {
            for ch in &line.chars {
                assert!(line.start <= ch.offset && ch.offset < line.end, "{state}/{node}: {ch:?} on {line:?}");
            }
        }
    }
}

/// Each glyph the frame at rest draws for a text lies within a character of the line it
/// sits on: the carets are read from the glyphs painted, where the frame puts them.
#[test]
fn every_glyph_drawn_lies_within_a_character() {
    let (deck, theme, mut engine) = torture();
    let data = DataFiles::new();
    for (state, node) in TEXTS {
        let c = at_rest(&mut engine, &deck, &theme, state).carets(node).unwrap();
        let req = FrameRequest { deck: &deck, theme: &theme, data: &data, state, t_ms: f64::INFINITY, format: None };
        let dl = engine.frame(&req).unwrap().display_list;
        let Some(Op::Layer { transform, ops, .. }) =
            dl.ops.iter().find(|op| matches!(op, Op::Layer { node: Some(n), .. } if n == node))
        else {
            panic!("{state}: no layer for `{node}`")
        };
        let mut glyphs = 0;
        for op in ops {
            // The hyphen drawn at a break is no character's.
            let Op::Glyphs { glyphs: set, text, .. } = op else { continue };
            if text == "\u{AD}" {
                continue;
            }
            for g in set {
                let [x, y] = [g.x + transform[4], g.y + transform[5]];
                let line = c.lines.iter().find(|l| l.top - 1e-3 <= y && y <= l.bottom + 1e-3);
                let line = line.unwrap_or_else(|| panic!("{state}/{node}: a glyph at {y} on no line"));
                let within = line.chars.iter().any(|ch| {
                    let (left, right) = (ch.lead.min(ch.trail), ch.lead.max(ch.trail));
                    left - 0.01 <= x && x <= right + 0.01
                });
                assert!(within, "{state}/{node}: a glyph at x {x} in no character of {line:?}");
                glyphs += 1;
            }
        }
        assert!(glyphs > 0, "{state}/{node}: nothing drawn");
    }
}

/// A caret put before a character stands at its leading edge, and a point just inside that
/// edge puts a caret there: `caret` and `at` agree on every character of every case, in
/// either direction. Where two offsets stand at one edge (right-to-left text meeting
/// left-to-right), either will do; elsewhere it is the character's own.
#[test]
fn a_caret_stands_where_a_point_puts_it() {
    for (state, node) in TEXTS {
        let c = carets(state, node);
        let bidi = c.text.chars().any(|ch| ('\u{590}'..'\u{700}').contains(&ch));
        for (l, line) in c.lines.iter().enumerate() {
            let middle = (line.top + line.bottom) / 2.0;
            for ch in line.chars.iter().filter(|ch| width(ch) > 0.5) {
                let caret = c.caret(ch.offset, Some(l)).unwrap();
                assert_eq!((caret.line, caret.x), (l, ch.lead), "{state}/{node}: {ch:?}");
                let inside = ch.lead + (ch.trail - ch.lead).signum() * 0.1;
                let (offset, on) = c.at([inside, middle]);
                assert_eq!(on, l, "{state}/{node}: {ch:?}");
                assert!((c.caret(offset, Some(on)).unwrap().x - ch.lead).abs() < 1e-3, "{state}/{node}: {ch:?}");
                if !bidi {
                    assert_eq!(offset, ch.offset, "{state}/{node}: {ch:?}");
                }
                assert_eq!(c.on_line(l, inside), offset);
            }
            // Past the far end of a line a caret stands after its last character, or before
            // the break that ends it.
            let far = if bidi { -1e4 } else { 1e4 };
            let end = c.on_line(l, far);
            assert_eq!(end, if line.broken { line.last() } else { line.end }, "{state}/{node}: line {l}");
        }
        // Above every line is the first; below them all, the last.
        let (first, last) = (&c.lines[0], c.lines.last().unwrap());
        assert_eq!(c.at([first.x, first.top - 500.0]).1, 0, "{state}/{node}");
        assert_eq!(c.at([last.x, last.bottom + 500.0]).1, c.lines.len() - 1, "{state}/{node}");
    }
}

/// Capitals set ß as SS: one character as written, as wide as the two it is set as.
#[test]
fn a_character_case_sets_longer_is_one_character() {
    let c = carets("case-measure", "case-upper");
    assert_eq!(c.text, "Upper case: Straße und Weg", "the text as written, not as set");
    let sharp = c.text.find('ß').unwrap();
    let (_, s) = char_at(&c, sharp);
    let (_, capital) = char_at(&c, c.text.find("Str").unwrap());
    assert!((width(&s) - 2.0 * width(&capital)).abs() < 1.0, "ß as SS is two capitals wide: {s:?} {capital:?}");
    let offsets: Vec<usize> = c.lines[0].chars.iter().map(|ch| ch.offset).collect();
    assert!(offsets.contains(&(sharp + 'ß'.len_utf8())) && !offsets.contains(&(sharp + 1)));
    // Title case capitalizes letters, never moves them.
    let t = carets("case-measure", "case-title");
    assert!(t.text.starts_with("title case"), "{}", t.text);
}

/// Hyphenation breaks inside words, and the soft hyphens it inserts are not written: where a
/// line breaks inside a word, a caret stands at the start of the next line, or at the end of
/// the one before when it is kept there.
#[test]
fn hyphenation_breaks_inside_words_the_text_does_not_have() {
    let c = carets("hyphenation", "hyph-on");
    assert!(!c.text.contains('\u{AD}'));
    let inside = c.lines.windows(2).enumerate().find(|(_, pair)| {
        let at = pair[0].end;
        let around = (c.text[..at].chars().next_back(), c.text[at..].chars().next());
        matches!(around, (Some(a), Some(b)) if a.is_alphabetic() && b.is_alphabetic())
    });
    let (l, pair) = inside.expect("a line breaks inside a word");
    let at = pair[0].end;
    let next = c.caret(at, None).unwrap();
    assert_eq!((next.line, next.x), (l + 1, pair[1].chars[0].lead), "a caret where a line wraps starts the next");
    let kept = c.caret(at, Some(l)).unwrap();
    assert_eq!((kept.line, kept.x), (l, pair[0].chars.last().unwrap().trail), "kept on its line, after the letter");
    assert!(next.top > kept.top);
}

/// Right-to-left text runs from the right: a caret before a Hebrew or Arabic letter stands at
/// its right edge, and before a Latin letter in it at its left. The paragraph starts at the
/// right edge.
#[test]
fn right_to_left_text_runs_from_the_right() {
    for (state, node) in [("bidi-hebrew", "bidi-he-line"), ("bidi-arabic", "bidi-ar-line")] {
        let c = carets(state, node);
        let line = &c.lines[0];
        let first = line.chars[0];
        assert!(first.lead > first.trail, "{state}: {first:?}");
        assert!(line.chars.iter().all(|ch| ch.lead <= first.lead + 1e-3), "{state}: it starts at the right");
        let (_, s) = char_at(&c, c.text.find("Scaena").unwrap());
        assert!(s.lead < s.trail, "{state}: Latin in it runs left to right: {s:?}");
        // "Scaena" is selected as one stretch, inside the line.
        let start = c.text.find("Scaena").unwrap();
        let rects = c.selection(start, start + "Scaena".len());
        let (_, a) = char_at(&c, start + 5);
        assert_eq!(rects.len(), 1, "{state}: {rects:?}");
        assert!((rects[0][0] - s.lead).abs() < 1e-3 && (rects[0][0] + rects[0][2] - a.trail).abs() < 1e-3, "{state}");
    }
}

/// What a reader counts as one character is one caret stop: a letter with its combining
/// marks, an emoji with its modifier and joiner, a flag; a ligature's letters each are one.
#[test]
fn a_caret_never_stands_inside_a_character() {
    let combining = carets("combining", "combining-1");
    let stops: Vec<usize> = combining.lines[0].chars.iter().map(|c| c.offset).collect();
    let e = combining.text.find("e\u{302}\u{301}").unwrap();
    assert!(stops.contains(&e) && !stops.contains(&(e + 1)) && !stops.contains(&(e + 3)));

    let emoji = carets("emoji", "emoji-line");
    let stops: Vec<usize> = emoji.lines[0].chars.iter().map(|c| c.offset).collect();
    let coder = emoji.text.find('👩').unwrap();
    let after = coder + "👩🏽\u{200d}💻".len();
    assert!(stops.contains(&coder) && stops.contains(&after));
    assert!(stops.iter().all(|&s| s <= coder || s >= after), "nothing inside the sequence: {stops:?}");
    let flag = emoji.text.find('🇺').unwrap();
    assert!(!stops.contains(&(flag + '🇺'.len_utf8())), "a flag is one character");

    // "office": the ffi ligature is one glyph, and its three letters share its width.
    let liga = carets("liga", "liga-on");
    let letters: Vec<_> = (1..4).map(|i| char_at(&liga, i).1).collect();
    assert!(letters.iter().all(|ch| (width(ch) - width(&letters[0])).abs() < 1e-3 && width(ch) > 0.0));
    assert!(letters.windows(2).all(|p| (p[0].trail - p[1].lead).abs() < 1e-3), "{letters:?}");
}

/// A line break the text sets ends its line: a caret after it stands on the next, and the
/// nearest point at the end of the line puts the caret before the break. An empty last line,
/// or an empty text, holds a caret where its alignment puts it.
#[test]
fn a_line_break_ends_its_line() {
    let c = set("abc\n", 900.0, TextAlign::Start);
    assert_eq!(c.lines.len(), 2);
    assert!(c.lines[0].broken && !c.lines[1].broken);
    assert_eq!((c.lines[1].start, c.lines[1].end), (4, 4));
    let after = c.caret(4, Some(0)).unwrap();
    assert_eq!((after.line, after.x), (1, 0.0), "after the break is the next line, even asked for this one");
    let middle = (c.lines[0].top + c.lines[0].bottom) / 2.0;
    assert_eq!(c.at([5000.0, middle]), (3, 0), "past the end of the line: before the break");
    assert_eq!(c.lines[0].last(), 3);

    for (align, x) in [(TextAlign::Start, 0.0), (TextAlign::Center, 450.0), (TextAlign::End, 900.0)] {
        let empty = set("", 900.0, align);
        assert_eq!(empty.lines.len(), 1);
        let caret = empty.caret(0, None).unwrap();
        assert!((caret.x - x).abs() < 1e-3, "{align:?}: {caret:?}");
        assert_eq!(empty.at([123.0, 0.0]), (0, 0));
        assert!(empty.selection(0, 0).is_empty());
    }
}

/// A selection covers its characters, a stretch per line, and nothing for an empty one.
#[test]
fn a_selection_covers_its_characters_a_stretch_per_line() {
    let c = set("hello world again", 200.0, TextAlign::Start);
    assert_eq!(c.lines.len(), 3);
    let all = c.selection(0, c.text.len());
    assert_eq!(all.len(), 3, "{all:?}");
    for (rect, line) in all.iter().zip(&c.lines) {
        assert_eq!((rect[1], rect[3]), (line.top, line.bottom - line.top));
    }
    let world = c.selection(6, 11);
    assert_eq!(world.len(), 1);
    assert!((world[0][0] - c.caret(6, None).unwrap().x).abs() < 1e-3);
    assert!((world[0][0] + world[0][2] - c.caret(11, None).unwrap().x).abs() < 1e-3);
    assert_eq!(c.selection(8, 8), Vec::<[f32; 4]>::new());
    assert_eq!(c.selection(11, 6), world, "either way round");
}

/// A point on a text hits a character: `hit` says where a caret put there stands.
#[test]
fn a_point_on_a_text_hits_a_character() {
    let (deck, theme, mut engine) = torture();
    let scene = at_rest(&mut engine, &deck, &theme, "case-measure");
    let c = scene.carets("case-upper").unwrap();
    let (l, ch) = char_at(&c, c.text.find("und").unwrap());
    let line = &c.lines[l];
    let point = [ch.lead + 0.2 * (ch.trail - ch.lead), (line.top + line.bottom) / 2.0];
    let hit = scene.hit(point);
    let text = hit.iter().find(|h| h.node == "case-upper").expect("the text is hit");
    assert_eq!(text.offset, Some(ch.offset));
    assert!(scene.carets("nowhere").is_none());
}

#[test]
fn bold_toggles_by_the_weight_each_character_is_set_in() {
    // "Thin Regular Black Big small", each word a run in a role of its own weight.
    let c = carets("mixed", "mixed-line");
    let looks: Vec<(usize, f32, f32)> = c.looks.iter().map(|l| (l.end, l.weight, l.base)).collect();
    assert_eq!(
        looks,
        [(5, 100.0, 100.0), (13, 400.0, 400.0), (19, 900.0, 900.0), (23, 400.0, 400.0), (28, 400.0, 400.0)]
    );
    let weight = |from: usize, to: usize| c.bolding(from, to)["style/weight"].clone();
    // Not all bold: bold. "Black", set bold by its role, takes 400 to stop being bold.
    assert_eq!(weight(0, 13), serde_json::json!(700));
    assert_eq!(weight(10, 16), serde_json::json!(700));
    assert_eq!(weight(13, 18), serde_json::json!(400));
    // A weight a span sets itself over a role that is not bold is taken away.
    let (deck, theme, _) = torture();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let mut spec = TextSpec::plain(theme.text_role("specimen").unwrap(), "Bold words");
    spec.spans[0].style.weight = 700.0;
    let c = TextEngine::new().layout(&mut fonts, &theme, &spec, 1600.0).unwrap().carets([0.0, 0.0]);
    assert_eq!(c.bolding(0, 4)["style/weight"], serde_json::Value::Null);
}
