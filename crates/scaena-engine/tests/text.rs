//! Text set across its box (SPEC §3.4, §3.5; PLAN 1.8), with the torture deck's fonts:
//! lines aligned to the start, the center, or the end in either direction, quotation
//! marks hung only at an aligned edge, `measure` in `ch`, and `case`.

use scaena_core::Deck;
use scaena_core::model::theme::Case;
use scaena_core::model::values::TextSplit;
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

const LONG_WORDS: &str = "Typographers appreciate hyphenation in extraordinarily narrow, uncompromising measures.";

/// The text of line `k`, without soft hyphens.
fn line_text(layout: &TextLayout, k: usize) -> String {
    layout.text[layout.lines[k].text.clone()].replace('\u{AD}', "")
}

#[test]
fn hyphenation_breaks_words_at_their_patterns_and_draws_the_hyphen() {
    let width = 520.0;
    let on = |s: &mut TextSpec| {
        s.hyphenate = true;
        s.lang = Some("en-US".into());
    };
    let layout = set("specimen", LONG_WORDS, width, on);
    let hyphenated: Vec<usize> = (0..layout.lines.len()).filter(|&k| layout.lines[k].hyphen).collect();
    assert!(!hyphenated.is_empty(), "{:?}", (0..layout.lines.len()).map(|k| line_text(&layout, k)).collect::<Vec<_>>());
    let dash = ids(&set("specimen", "-", width, |_| {}))[0];
    for &k in &hyphenated {
        let line = &layout.lines[k];
        assert!(layout.text[line.text.clone()].ends_with('\u{AD}'), "line {k} broke at a soft hyphen");
        assert!(line.width <= width + 1e-3, "the hyphen fits the measure: {line:?}");
        // The hyphen is the line's last glyph, on the right of its ink, on its baseline.
        let last =
            layout.runs.iter().filter(|r| r.line == k).flat_map(|r| &r.glyphs).max_by(|a, b| a.x.total_cmp(&b.x));
        assert_eq!(last.map(|g| g.id), Some(dash), "line {k}");
        assert_eq!(last.map(|g| g.y), Some(line.baseline), "line {k}");
    }
    // Every line, hyphen included, fits the measure.
    assert!(layout.lines.iter().all(|l| l.width <= width + 1e-3));
    // A word with soft hyphens of its own breaks only there; its neighbours still follow
    // the patterns.
    let typed = set("specimen", "Uncompromising co\u{AD}operation", width, on);
    assert!(typed.text.contains("co\u{AD}operation"), "{:?}", typed.text);
    assert!(typed.text.starts_with("Un\u{AD}"), "{:?}", typed.text);
    // Off, or in a language without patterns, words never break.
    for spec in [
        |s: &mut TextSpec| s.lang = Some("en".into()),
        |s: &mut TextSpec| {
            s.hyphenate = true;
            s.lang = Some("he".into());
        },
    ] {
        let plain = set("specimen", LONG_WORDS, width, spec);
        assert!(plain.lines.iter().all(|l| !l.hyphen) && !plain.text.contains('\u{AD}'));
    }
}

#[test]
fn pretty_breaking_counts_the_hyphen_and_end_aligned_hanging_punctuation_hangs_it() {
    let width = 520.0;
    let pretty = set("specimen", LONG_WORDS, width, |s| {
        s.hyphenate = true;
        s.lang = Some("en".into());
        s.wrap = Some(Wrap::Pretty);
    });
    assert_eq!((pretty.wrap, pretty.fallback), (Wrap::Pretty, None));
    assert!(pretty.lines.iter().all(|l| l.width <= width + 1e-3), "{:?}", pretty.lines);
    // End-aligned with hanging punctuation, a drawn hyphen hangs past the edge.
    let end = set("specimen", LONG_WORDS, width, |s| {
        s.hyphenate = true;
        s.lang = Some("en".into());
        s.align = TextAlign::End;
        s.hanging_punctuation = true;
    });
    let line = end.lines.iter().find(|l| l.hyphen).expect("a hyphenated line");
    assert!(line.hang_end > 0.0 && (line.x + line.width - width).abs() < 1e-3, "{line:?}");
}

/// How many words line `k` holds, by whitespace.
fn words_on(layout: &TextLayout, k: usize) -> usize {
    layout.text[layout.lines[k].text.clone()].split_whitespace().count()
}

const RAGGED: &str = "Good typography is invisible until it goes wrong, and then it is the only thing anybody sees.";

#[test]
fn greedy_breaking_holds_the_last_line_to_min_last_line_words() {
    let greedy = |width: f32, min: u32| {
        set("specimen", RAGGED, width, |s| {
            s.wrap = Some(Wrap::Greedy);
            s.min_last_line_words = Some(min);
        })
    };
    // The first width at which plain greedy breaking strands one word on the last line.
    let width = (600..1600)
        .step_by(10)
        .map(|w| w as f32)
        .find(|&w| {
            let bare = greedy(w, 1);
            bare.lines.len() > 1 && words_on(&bare, bare.lines.len() - 1) == 1
        })
        .expect("a width that strands a word");
    let bare = greedy(width, 1);
    assert!(!bare.widow, "minLastLineWords 1 asks for nothing");
    let held = greedy(width, 2);
    let last = held.lines.len() - 1;
    assert_eq!(held.lines.len(), bare.lines.len());
    assert!(words_on(&held, last) >= 2 && !held.widow, "{:?}", held.lines);
    assert!(held.lines.iter().all(|l| l.width <= width + 1e-3));
    // The lines above keep greedy's breaks: only the break above the last line moved.
    for k in 0..last - 1 {
        assert_eq!(held.lines[k].text, bare.lines[k].text, "line {k}");
    }
    // Balance holds it too, at its own width.
    let balanced = set("specimen", RAGGED, width, |s| {
        s.wrap = Some(Wrap::Balance);
        s.min_last_line_words = Some(2);
    });
    assert!(words_on(&balanced, balanced.lines.len() - 1) >= 2 && !balanced.widow);
}

#[test]
fn a_last_line_that_cannot_take_more_words_is_reported_as_a_widow() {
    let text = "Typographers appreciate extraordinarily uncompromising";
    let one_word = set("specimen", "uncompromising", 10_000.0, |_| {}).width;
    for wrap in [Wrap::Greedy, Wrap::Pretty, Wrap::Balance] {
        let layout = set("specimen", text, one_word + 1.0, |s| {
            s.wrap = Some(wrap);
            s.min_last_line_words = Some(2);
        });
        let last = layout.lines.len() - 1;
        assert_eq!(words_on(&layout, last), 1, "{wrap:?}: two long words never share a line here");
        assert!(layout.widow, "{wrap:?}: W200 reads this");
    }
    // A text with fewer words than asked for has no widow to report.
    let short = set("specimen", "extraordinarily uncompromising", one_word + 1.0, |s| s.min_last_line_words = Some(3));
    assert!(short.lines.len() == 2 && !short.widow);
}

#[test]
fn words_are_what_lines_break_between_and_a_hyphenated_word_is_one() {
    let layout = set("specimen", LONG_WORDS, 520.0, |s| {
        s.hyphenate = true;
        s.lang = Some("en".into());
    });
    let words: Vec<String> = layout.words.iter().map(|w| layout.text[w.clone()].replace('\u{AD}', "")).collect();
    let expected: Vec<String> = LONG_WORDS.split_inclusive(' ').map(String::from).collect();
    assert_eq!(words, expected);
    assert!(layout.lines.iter().any(|l| l.hyphen), "some word breaks across lines");
    // A hyphenated word's tail is not a word, so no last line is only a tail. Here
    // "measures." cannot join "uncompromising", so it ends the paragraph whole.
    assert_eq!(line_text(&layout, layout.lines.len() - 1), "measures.");
    assert!(!layout.widow, "one word is all minLastLineWords 1 asks for");
    // Two words the last line cannot take: it keeps the one, and W200 hears of it.
    let held = set("specimen", LONG_WORDS, 520.0, |s| {
        s.hyphenate = true;
        s.lang = Some("en".into());
        s.min_last_line_words = Some(2);
    });
    assert_eq!(line_text(&held, held.lines.len() - 1), "measures.");
    assert!(held.widow);
}

/// Every glyph of `layout` whose cluster sets ink, as `(run, glyph)`.
fn inked(layout: &TextLayout) -> Vec<(usize, usize)> {
    let mut all = Vec::new();
    for (r, run) in layout.runs.iter().enumerate() {
        for (g, &c) in run.clusters.iter().enumerate() {
            let ch = layout.text[c..].chars().next().unwrap();
            if !ch.is_whitespace() && ch != '\u{AD}' {
                all.push((r, g));
            }
        }
    }
    all.sort_unstable();
    all
}

#[test]
fn split_units_partition_the_inked_glyphs_in_reading_order() {
    let layout = set("specimen", LONG_WORDS, 520.0, |s| {
        s.hyphenate = true;
        s.lang = Some("en".into());
    });
    for split in [TextSplit::Lines, TextSplit::Words, TextSplit::Glyphs] {
        let units = layout.units(split);
        // In reading order, and every inked glyph in exactly one unit.
        assert!(units.windows(2).all(|u| u[0].text.end <= u[1].text.start), "{split:?}");
        let mut covered: Vec<(usize, usize)> = units.iter().flat_map(|u| u.glyphs.iter().copied()).collect();
        covered.sort_unstable();
        let mut unique = covered.clone();
        unique.dedup();
        assert_eq!(covered, unique, "{split:?}: a glyph in two units");
        let inked = inked(&layout);
        assert!(inked.iter().all(|g| covered.contains(g)), "{split:?}: an inked glyph in no unit");
    }
    assert_eq!(layout.units(TextSplit::Lines).len(), layout.lines.len());
    let words = layout.units(TextSplit::Words);
    assert_eq!(words.len(), LONG_WORDS.split_whitespace().count());
    // The hyphenated word is one unit, with glyphs on both lines and the drawn hyphen.
    let k = layout.lines.iter().position(|l| l.hyphen).unwrap();
    let dash = ids(&set("specimen", "-", 520.0, |_| {}))[0];
    let split_word = words.iter().find(|u| u.glyphs.iter().any(|&(r, _)| layout.runs[r].line == k + 1) && u.line == k);
    let split_word = split_word.expect("a word on two lines");
    assert!(split_word.glyphs.iter().any(|&(r, g)| layout.runs[r].glyphs[g].id == dash));
    // Glyph units: one per inked cluster, never whitespace.
    let glyphs = layout.units(TextSplit::Glyphs);
    assert!(glyphs.iter().all(|u| !layout.text[u.text.clone()].trim().is_empty()));
    let letters = LONG_WORDS.chars().filter(|c| !c.is_whitespace()).count();
    assert!(glyphs.len() <= letters && glyphs.len() + 4 >= letters, "{} units for {letters} letters", glyphs.len());
}

#[test]
fn split_units_of_right_to_left_text_follow_reading_order() {
    let text = "שלום עולם יפה";
    let layout = set("specimen", text, 2000.0, |s| s.lang = Some("he".into()));
    assert!(layout.rtl);
    let words = layout.units(TextSplit::Words);
    let read: Vec<&str> = words.iter().map(|u| layout.text[u.text.clone()].trim()).collect();
    assert_eq!(read, ["שלום", "עולם", "יפה"]);
    // The first word read is the rightmost.
    let x = |u: &scaena_engine::text::TextUnit| {
        u.glyphs.iter().map(|&(r, g)| layout.runs[r].glyphs[g].x).fold(f32::MIN, f32::max)
    };
    assert!(x(&words[0]) > x(&words[1]) && x(&words[1]) > x(&words[2]));
}

/// An editor empties a text before it types a new one: one empty line in the node's look,
/// whatever its wrap, alignment, and hanging, with no glyphs and no units to split.
#[test]
fn an_empty_text_is_one_empty_line_in_its_look() {
    let one = set("specimen", "x", 900.0, |_| {});
    for wrap in [Wrap::Greedy, Wrap::Balance, Wrap::Pretty] {
        for align in [TextAlign::Start, TextAlign::Center, TextAlign::End] {
            for width in [900.0, 0.0] {
                let layout = set("specimen", "", width, |s| {
                    (s.wrap, s.align, s.hanging_punctuation, s.optical_margins) = (Some(wrap), align, true, true);
                    s.measure = Some(20.0);
                });
                let [line] = layout.lines.as_slice() else { panic!("{wrap:?} {align:?}: {:?}", layout.lines) };
                assert_eq!((line.height, line.baseline), (one.lines[0].height, one.lines[0].baseline));
                assert_eq!((line.text.clone(), line.width, layout.width), (0..0, 0.0, 0.0));
                assert_eq!(layout.height, one.height);
                assert!(layout.text.is_empty() && layout.runs.is_empty(), "{wrap:?} {align:?}: {:?}", layout.runs);
                for split in [TextSplit::Lines, TextSplit::Words, TextSplit::Glyphs] {
                    assert!(layout.units(split).iter().all(|u| u.glyphs.is_empty()), "{split:?}");
                }
            }
        }
    }
    // A case changes nothing, and a hyphenating language has nothing to hyphenate.
    let cased = set("specimen", "", 900.0, |s| {
        (s.role.case, s.hyphenate, s.lang) = (Some(Case::Upper), true, Some("en-US".into()));
    });
    assert!(cased.runs.is_empty() && cased.lines.len() == 1);
}

/// The x of glyph `k` of `layout`, in its order on the line.
fn glyph_x(layout: &TextLayout, k: usize) -> f32 {
    layout.runs.iter().flat_map(|r| r.glyphs.iter().map(|g| g.x)).nth(k).unwrap()
}

/// `spec`'s one span split into spans of `parts`' texts, each in the same look.
fn split(spec: &mut TextSpec, parts: &[&str]) {
    let one = spec.spans[0].clone();
    spec.spans = parts.iter().map(|t| scaena_engine::text::Span { text: t.to_string(), ..one.clone() }).collect();
}

#[test]
fn a_pair_tracked_on_its_first_letter_keeps_the_fonts_kerning_and_adds_to_it() {
    // AV kerns tight in Roboto Serif (the `kern` case): the V sits left of where it sits unkerned.
    let kerned = glyph_x(&set("kern", "AV", 1600.0, |_| {}), 1);
    let loose = glyph_x(
        &set("kern", "AV", 1600.0, |s| {
            s.features.insert("kern".into(), 0);
        }),
        1,
    );
    assert!(kerned < loose - 5.0, "AV kerns: {kerned} against {loose}");
    // Tracking on the A alone moves the V by the tracking, from where the font's kerning set it:
    // a change of tracking no longer shapes the A and the V apart (PLAN 1.38, ADR-0004 finding 21).
    let size = theme().text_role("kern").unwrap().size;
    for tracking in [-0.05_f32, 0.08] {
        let pair = set("kern", "AV", 1600.0, |s| {
            split(s, &["A", "V"]);
            s.spans[0].style.tracking = tracking;
        });
        let v = glyph_x(&pair, 1);
        assert!((v - (kerned + tracking * size)).abs() < 1e-3, "tracking {tracking}: V at {v}, kerned at {kerned}");
    }
    // A color of its own on the V shapes the pair whole too.
    let colored = set("kern", "AV", 1600.0, |s| {
        split(s, &["A", "V"]);
        s.spans[1].style.color = Some("accent".into());
    });
    assert!((glyph_x(&colored, 1) - kerned).abs() < 1e-3);
}

#[test]
fn a_runs_own_features_and_axes_come_after_the_nodes() {
    let kerned = glyph_x(&set("kern", "AV", 1600.0, |_| {}), 1);
    let loose = glyph_x(
        &set("kern", "AV", 1600.0, |s| {
            s.features.insert("kern".into(), 0);
        }),
        1,
    );
    // The run turns kerning off where the node leaves it on, and on where the node turns it off.
    let off = set("kern", "AV", 1600.0, |s| {
        s.spans[0].features.insert("kern".into(), 0);
    });
    assert!((glyph_x(&off, 1) - loose).abs() < 1e-3);
    let on = set("kern", "AV", 1600.0, |s| {
        s.features.insert("kern".into(), 0);
        s.spans[0].features.insert("kern".into(), 1);
    });
    assert!((glyph_x(&on, 1) - kerned).abs() < 1e-3);
    // The variable family's width axis, set on one run over the node's, widens that run alone.
    let width = |l: &TextLayout| l.lines[0].width;
    let plain = set("headline", "Wide words", 1600.0, |_| {});
    let narrow = set("headline", "Wide words", 1600.0, |s| {
        s.axes.insert("wdth".into(), 50.0);
    });
    let wide = set("headline", "Wide words", 1600.0, |s| {
        s.axes.insert("wdth".into(), 50.0);
        split(s, &["Wide", " words"]);
        s.spans[0].axes.insert("wdth".into(), 150.0);
    });
    assert!(width(&narrow) < width(&plain) - 1.0, "{} against {}", width(&narrow), width(&plain));
    assert!(width(&wide) > width(&narrow) + 1.0, "{} against {}", width(&wide), width(&narrow));
    assert!(glyph_x(&wide, 5) - glyph_x(&wide, 4) < glyph_x(&plain, 5) - glyph_x(&plain, 4), "` words` stays narrow");
}
