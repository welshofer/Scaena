//! Text set across its box (SPEC §3.4, §3.5; PLAN 1.8), with the torture deck's fonts:
//! lines aligned to the start, the center, or the end in either direction, quotation
//! marks hung only at an aligned edge, `measure` in `ch`, and `case`.

use scaena_core::Deck;
use scaena_core::model::theme::Case;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::text::{TextAlign, TextEngine, TextLayout, TextSpec};
use scaena_engine::theme::{Theme, Wrap};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

fn theme() -> Theme {
    Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap()
}

fn fonts() -> BundleFonts {
    let deck = Deck::from_json(&String::from_utf8(read("deck.json")).unwrap()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    fonts
}

/// `text` in `role`, its spec edited, laid out across `width`.
fn set(role: &str, text: &str, width: f32, edit: impl FnOnce(&mut TextSpec)) -> TextLayout {
    let theme = theme();
    let mut spec = TextSpec::plain(theme.text_role(role).unwrap(), text);
    edit(&mut spec);
    TextEngine::new().layout(&mut fonts(), &theme, &spec, width).unwrap()
}

fn ids(layout: &TextLayout) -> Vec<u32> {
    layout.runs.iter().flat_map(|r| r.glyphs.iter().map(|g| g.id)).collect()
}

/// The leftmost and rightmost glyph origins of line `k`.
fn span(layout: &TextLayout, k: usize) -> (f32, f32) {
    let xs: Vec<f32> = layout.runs.iter().filter(|r| r.line == k).flat_map(|r| r.glyphs.iter().map(|g| g.x)).collect();
    (xs.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max))
}

const PARAGRAPH: &str =
    "“Type is a beautiful group of letters, not a group of beautiful letters.” Set it well and nobody notices.";

#[test]
fn centered_lines_sit_in_the_middle_and_hang_nothing() {
    let width = 900.0;
    let layout = set("specimen", PARAGRAPH, width, |s| s.align = TextAlign::Center);
    assert!(layout.lines.len() > 2);
    for (k, line) in layout.lines.iter().enumerate() {
        assert!((2.0 * line.x + line.width - width).abs() < 1e-3, "line {k}: {line:?}");
        assert_eq!((line.hang, line.hang_end), (0.0, 0.0), "both edges are ragged: nothing hangs");
    }
    // The opening quote is inside the first line, at its left edge.
    let first = &layout.lines[0];
    assert!((span(&layout, 0).0 - first.x).abs() < 1e-3, "{:?} vs {}", span(&layout, 0), first.x);
}

#[test]
fn end_aligned_lines_end_on_the_edge_and_hang_closing_quotes_past_it() {
    let width = 1600.0;
    let layout = set("specimen", "Nobody notices “good type.”", width, |s| s.align = TextAlign::End);
    let [line] = layout.lines.as_slice() else { panic!("one line: {:?}", layout.lines) };
    assert!((line.x + line.width - width).abs() < 1e-3, "{line:?}");
    assert!(line.hang_end > 0.0 && line.hang == 0.0, "{line:?}");
    // The closing quote starts on the edge; the opening one, mid-line, does not hang.
    assert!((span(&layout, 0).1 - width).abs() < 1e-3, "{:?}", span(&layout, 0));
    // Start-aligned, the same line hangs nothing at its ragged end.
    let start = set("specimen", "Nobody notices “good type.”", width, |_| {});
    assert_eq!((start.lines[0].hang, start.lines[0].hang_end), (0.0, 0.0));
}

#[test]
fn right_to_left_pretty_breaking_is_pretty_and_starts_at_the_right_edge() {
    let width = 700.0;
    let text = "שלום הגרסה מוכנה שלום הגרסה מוכנה שלום הגרסה מוכנה שלום הגרסה מוכנה.";
    let layout = set("bidi-he", text, width, |s| s.wrap = Some(Wrap::Pretty));
    assert!(layout.rtl);
    assert_eq!((layout.wrap, layout.fallback), (Wrap::Pretty, None), "no longer greedy for right-to-left text");
    assert!(layout.lines.len() > 1);
    for line in &layout.lines {
        assert!((line.x + line.width - width).abs() < 1e-3, "starts at the right edge: {line:?}");
    }
    // Ended at the other edge, every line ends on the left.
    let end = set("bidi-he", text, width, |s| {
        s.wrap = Some(Wrap::Pretty);
        s.align = TextAlign::End;
    });
    assert!(end.lines.iter().all(|l| l.x.abs() < 1e-3), "{:?}", end.lines);
}

#[test]
fn measure_caps_the_line_length_in_ch_and_lines_still_align_across_the_box() {
    let ch = set("specimen", "0", 4000.0, |_| {}).width;
    let width = 1728.0;
    let text = "Twelve characters a line is a narrow measure for a paragraph of this length.";
    let layout = set("specimen", text, width, |s| {
        s.measure = Some(12.0);
        s.align = TextAlign::Center;
    });
    assert!(layout.lines.len() >= 6, "{}", layout.lines.len());
    for line in &layout.lines {
        assert!(line.width <= 12.0 * ch + 1e-3, "{} > 12 ch ({ch})", line.width);
        assert!((2.0 * line.x + line.width - width).abs() < 1e-3, "centered in the box, not in the measure");
    }
}

#[test]
fn case_sets_capitals_title_case_and_the_fonts_small_capitals() {
    let theme = theme();
    let cased = |role: &str, case: Case, text: &str| {
        let mut role = theme.text_role(role).unwrap();
        role.case = Some(case);
        let spec = TextSpec::plain(role, text);
        ids(&TextEngine::new().layout(&mut fonts(), &theme, &spec, 4000.0).unwrap())
    };
    let plain = |role: &str, text: &str| ids(&set(role, text, 4000.0, |_| {}));
    assert_eq!(cased("specimen", Case::Upper, "Straße und Weg"), plain("specimen", "STRASSE UND WEG"));
    assert_eq!(cased("specimen", Case::Lower, "LOUD Words"), plain("specimen", "loud words"));
    assert_eq!(
        cased("specimen", Case::Title, "the (quick) brown 3rd fox"),
        plain("specimen", "The (Quick) Brown 3rd Fox")
    );
    // EB Garamond has small capitals; Roboto Serif does not, and none are made up.
    assert_ne!(cased("garamond", Case::Smallcaps, "small capitals"), plain("garamond", "small capitals"));
    assert_eq!(cased("specimen", Case::Smallcaps, "small capitals"), plain("specimen", "small capitals"));
}

#[test]
fn hanging_punctuation_hangs_brackets_stops_commas_and_hyphens_at_an_aligned_edge() {
    let width = 1600.0;
    let punct = |s: &mut TextSpec| s.hanging_punctuation = true;
    // At a start edge, an opening bracket hangs like a quote.
    let start = set("specimen", "(Brackets open lines too)", width, punct);
    let bracket = set("specimen", "(", width, |_| {}).width;
    assert!((start.lines[0].hang - bracket).abs() < 1e-3, "{:?}", start.lines[0]);
    assert!((span(&start, 0).0 + bracket).abs() < 1e-3, "the bracket sits left of the edge");
    // At an end edge, the stop hangs, then nothing else: the letter before it is on the edge.
    let end = set("specimen", "Stops hang past the edge.", width, |s| {
        punct(s);
        s.align = TextAlign::End;
    });
    let stop = set("specimen", ".", width, |_| {}).width;
    let line = &end.lines[0];
    assert!((line.hang_end - stop).abs() < 1e-3 && (line.x + line.width - width).abs() < 1e-3, "{line:?}");
    // Without the setting, only quotation marks hang.
    let plain = set("specimen", "Stops hang past the edge.", width, |s| s.align = TextAlign::End);
    assert_eq!(plain.lines[0].hang_end, 0.0);
    // Nothing hangs at a ragged edge: start-aligned, the stop stays inside.
    assert_eq!(set("specimen", "Stops hang past the edge.", width, punct).lines[0].hang_end, 0.0);
}

#[test]
fn optical_margins_move_edge_letters_and_stops_part_way_out() {
    let width = 1600.0;
    let optical = |s: &mut TextSpec| s.optical_margins = true;
    let v = set("specimen", "V", width, |_| {}).width;
    let layout = set("specimen", "Vowels lean out", width, optical);
    // 5% of the V's advance as set, kerned against the o: a little less than V alone.
    let leans = layout.lines[0].optical;
    assert!(leans > 0.04 * v && leans <= 0.05 * v + 1e-3, "{leans} for a V of {v}");
    assert!((span(&layout, 0).0 + layout.lines[0].optical).abs() < 1e-3);
    // A letter that does not lean stays on the edge.
    assert_eq!(set("specimen", "Mountains stand", width, optical).lines[0].optical, 0.0);
    // At an end edge, a stop moves out 70% of its advance; a hung mark moves out all of it
    // instead, and then nothing protrudes as well.
    let stop = set("specimen", ".", width, |_| {}).width;
    let end = set("specimen", "Ends on a stop.", width, |s| {
        optical(s);
        s.align = TextAlign::End;
    });
    assert!((end.lines[0].optical - 0.7 * stop).abs() < 1e-3, "{:?}", end.lines[0]);
    assert!((span(&end, 0).1 + stop - (width + 0.7 * stop)).abs() < 1e-3, "{:?}", span(&end, 0));
    let both = set("specimen", "Ends on a stop.", width, |s| {
        optical(s);
        s.hanging_punctuation = true;
        s.align = TextAlign::End;
    });
    assert_eq!((both.lines[0].optical, both.lines[0].hang_end > 0.0), (0.0, true));
    // Centered text has no aligned edge.
    let centered = set("specimen", "Vowels lean out", width, |s| {
        optical(s);
        s.align = TextAlign::Center;
    });
    assert_eq!(centered.lines[0].optical, 0.0);
}
