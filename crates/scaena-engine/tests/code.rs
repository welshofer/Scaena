//! A code text, set as written (PLAN 2.131, ADR-0029), with the torture deck's fonts: its lines
//! break only where it breaks them, its tabs stand to stops, nothing hangs, and it runs left to
//! right whatever its first word is. It grows its box first, then shrinks, worked out from one
//! layout, and by itself never under 12 pt at presentation size. It is colored by its language in
//! the theme's colors, each reading on its ground, and a color alone shapes nothing apart (PLAN
//! 2.132).

use scaena_core::Deck;
use scaena_core::code::{Class, highlight, lang};
use scaena_core::model::values::Range;
use scaena_core::palette::{READS, STRONG, ratio};
use scaena_engine::code::{CodeSpec, Looks, TokenLook};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::text::{TextEngine, TextLayout, TextSpec};
use scaena_engine::theme::{TextBox, Theme};
use scaena_engine::{Engine, FrameRequest, PlacedText};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

/// The torture deck's fonts and theme with one state, `t`, showing `nodes`.
fn deck(nodes: Value) -> Deck {
    let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    let props: serde_json::Map<String, Value> =
        nodes.as_object().unwrap().keys().map(|k| (k.clone(), json!({}))).collect();
    d["nodes"] = nodes;
    d["states"] = json!([{ "id": "t", "layout": "specimen", "props": props }]);
    d["data"] = json!({});
    // As a bundle reads it: the torture deck's format, from before code stood on a card.
    Deck::from_json(&d.to_string()).unwrap()
}

fn fonts(deck: &Deck) -> BundleFonts {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    fonts
}

fn theme() -> Theme {
    Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap()
}

fn placed(deck: &Deck, node: &str) -> PlacedText {
    let (theme, data) = (theme(), DataFiles::new());
    let req = FrameRequest { deck, theme: &theme, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
    Engine::new(fonts(deck)).text_layout(&req, node).unwrap()
}

/// A code text in the torture theme's `body` role (40 cu, leading 1.35), placed `at`, with no
/// card: the code's own fit and look ([`framed`] gives it a card and numbers).
fn code(text: &str, at: Value) -> Value {
    json!({ "type": "text", "role": "body", "code": { "language": "rust", "card": false }, "text": text, "at": at })
}

/// A code text as [`code`] makes it, its `code` with `framing` too: a card where it says none.
fn framed(text: &str, at: Value, framing: Value) -> Value {
    let mut node = code(text, at);
    node["code"].as_object_mut().unwrap().remove("card");
    for (k, v) in framing.as_object().unwrap() {
        node["code"][k] = v.clone();
    }
    node
}

/// `text` as code in `body`, laid out across `width` at `scale`: what the engine lays out for
/// [`code`]'s node.
fn set(text: &str, width: f32, scale: f32) -> TextLayout {
    let (theme, deck) = (theme(), deck(json!({})));
    let mut spec = TextSpec::plain(theme.text_role("body").unwrap(), text);
    spec.code = Some(CodeSpec {
        language: "rust".into(),
        tab: 4,
        least: 24.0 / 40.0,
        looks: None,
        card: None,
        numbers: None,
        focus: None,
        dimmed: None,
        band: None,
    });
    TextEngine::new().layout(&mut fonts(&deck), &theme, &spec.scaled(scale), width).unwrap()
}

/// The x of the glyph that sets the character `written` bytes into `layout`'s text as written.
fn glyph_x(layout: &TextLayout, written: usize) -> f32 {
    let laid = layout.offsets.iter().find(|(w, _)| *w == written).unwrap().1;
    let at = layout.runs.iter().flat_map(|r| r.glyphs.iter().zip(&r.clusters)).find(|(_, c)| **c == laid);
    at.unwrap().0.x
}

/// What lint finds of `code` in `deck`, laid out, as (code, node).
fn lint(deck: &Deck) -> Vec<(String, String, String)> {
    let (theme, data) = (theme(), DataFiles::new());
    let found = scaena_engine::lint::lint(&mut Engine::new(fonts(deck)), deck, &theme, &data, None).unwrap();
    found.into_iter().map(|f| (f.code, f.node.unwrap_or_default(), f.message)).collect()
}

const SNIPPET: &str = "// مرحبا\nfn main() {\n\tlet greeting = \"hello\";\n\"quoted\" opens a line\n}";

#[test]
fn a_code_text_is_set_as_written() {
    // In three columns, far narrower than its widest line.
    let d = deck(json!({ "n": code(SNIPPET, json!({ "col": [1, 3], "row": [2, 8] })) }));
    let p = placed(&d, "n");
    let t = &p.text;
    // Lines break only where the text breaks them, though its widest runs past its box.
    assert_eq!(t.lines.len(), 5, "{:?}", t.lines);
    assert!(p.natural.unwrap()[0] > p.cell[2], "{:?} in {:?}", p.natural, p.cell);
    // An Arabic comment on the first line leaves the block left to right: every line starts at
    // its left. Prose that opens so runs right to left.
    assert!(!t.rtl);
    assert!(t.lines.iter().all(|l| l.x == 0.0), "{:?}", t.lines);
    let prose = |text: &str| {
        let d = deck(
            json!({ "n": { "type": "text", "role": "body", "text": text, "at": { "col": [1, 12], "row": [2, 8] } } }),
        );
        placed(&d, "n").text
    };
    assert!(prose(SNIPPET).rtl, "prose takes its first strong character's direction");
    // A quote that opens a line hangs in prose and not in code: it sits on the block's edge.
    let quoted = SNIPPET.find("\"quoted").unwrap();
    assert_eq!(t.lines[3].hang, 0.0);
    assert_eq!(glyph_x(t, quoted), 0.0);
    let hung = prose(SNIPPET.split_once('\n').unwrap().1);
    assert!(hung.lines.iter().any(|l| l.hang > 0.0), "{:?}", hung.lines);
    // A tab stands to its stop: `let` where four spaces would put it, and a caret steps over the
    // tab whole, from the line's start to the `l`.
    let tab = SNIPPET.find('\t').unwrap();
    let spaced = set(&SNIPPET.replace('\t', "    "), 2000.0, 1.0);
    let tabbed = set(SNIPPET, 2000.0, 1.0);
    assert_eq!(glyph_x(&tabbed, tab + 1), glyph_x(&spaced, tab + 4));
    let carets = tabbed.carets([0.0, 0.0]);
    let line = &carets.lines[2];
    assert_eq!((line.chars[0].offset, line.chars[1].offset), (tab, tab + 1));
    assert_eq!((line.chars[0].lead, line.chars[0].trail), (0.0, glyph_x(&tabbed, tab + 1)));
    assert_eq!(line.chars[1].lead, line.chars[0].trail);
    // The mark that runs it left to right is drawn by no glyph and said by none, and takes no
    // room: in a role that tracks its letters out, the first line starts where the others do.
    assert!(tabbed.runs.iter().all(|r| r.clusters.iter().all(|&c| c >= '\u{200E}'.len_utf8())));
    assert_eq!(tabbed.written, SNIPPET);
    for role in ["loose", "tight"] {
        let mut node = code("x = 1;\ny = 2;", json!({ "col": [1, 12], "row": [2, 8] }));
        node["role"] = json!(role);
        let t = placed(&deck(json!({ "n": node })), "n").text;
        assert_eq!(glyph_x(&t, 0), glyph_x(&t, "x = 1;\n".len()), "{role}");
    }
    // A CRLF is one line break, as a code editor reads it, and a caret steps over it whole.
    let crlf = SNIPPET.replace('\n', "\r\n");
    let t = set(&crlf, 2000.0, 1.0);
    assert_eq!(t.lines.len(), 5, "{:?}", t.lines);
    assert_eq!(t.written, crlf);
    let carets = t.carets([0.0, 0.0]);
    let first = &carets.lines[0];
    let end = crlf.find("\r\n").unwrap();
    assert_eq!((first.end, first.chars.last().map(|c| c.offset)), (end + 2, Some(end)));
    assert_eq!(carets.lines[1].start, end + 2);
}

#[test]
fn a_code_texts_shrink_is_worked_out_within_1_64_of_bisection() {
    let text =
        (1..=12).map(|i| format!("    let value_{i:02} = compute(input, {i}, options.clone());")).collect::<Vec<_>>();
    let text = text.join("\n");
    let d = deck(json!({ "n": code(&text, json!({ "col": [1, 6], "row": [2, 5] })) }));
    let p = placed(&d, "n");
    assert!(p.scale < 1.0 && !p.overflow, "{} {}", p.scale, p.overflow);
    // The largest scale that fits, by bisection over the same layout: 12 steps, as `fit:
    // shrink` sets prose.
    let [w, h] = [p.cell[2], p.cell[3]];
    let fits = |scale: f32| {
        let laid = set(&text, w, scale);
        let (top, bottom) = laid.trimmed(TextBox::Line);
        bottom - top <= h + 1.0 / 64.0 && laid.lines.iter().all(|l| l.width <= w + 1.0 / 64.0)
    };
    let (mut lo, mut hi) = (24.0 / 40.0, 1.0_f32);
    for _ in 0..12 {
        let mid = 0.5 * (lo + hi);
        if fits(mid) { lo = mid } else { hi = mid }
    }
    assert!((p.scale - lo).abs() <= 1.0 / 64.0, "worked out {}, bisected {lo}", p.scale);
    assert!(fits(p.scale));
}

#[test]
fn code_shrinks_no_further_than_12_pt() {
    let long = (1..=30).map(|i| format!("step_{i}();")).collect::<Vec<_>>().join("\n");
    // Body is 40 cu: by itself it stops at 24, 12 pt at presentation size, and overflows: W203.
    let d = deck(json!({ "n": code(&long, json!({ "col": [1, 6], "row": 2 })) }));
    let p = placed(&d, "n");
    assert_eq!(p.scale, 0.6);
    assert!(p.overflow);
    assert!(p.text.runs.iter().all(|r| (r.size - 24.0).abs() < 1e-3));
    let found = lint(&d);
    let w203 = found.iter().find(|(c, n, _)| c == "W203" && n == "n").expect("W203");
    assert!(w203.2.contains("code `n` shrank to 24 cu"), "{}", w203.2);
    assert!(!found.iter().any(|(c, n, _)| (c == "E100" || c == "W205") && n == "n"), "{found:?}");
    // A `minSize` of its own lets it go smaller, which W205 says.
    let mut small = code(&long, json!({ "col": [1, 6], "row": 2 }));
    small["minSize"] = json!(12);
    let d = deck(json!({ "n": small }));
    let p = placed(&d, "n");
    assert_eq!(p.scale, 0.3);
    let found = lint(&d);
    let w205 = found.iter().find(|(c, n, _)| c == "W205" && n == "n").expect("W205");
    assert!(w205.2.contains("drawn at 12 cu") && w205.2.contains("30 lines"), "{}", w205.2);
    assert!(w205.2.contains("a `minSize` lets it shrink that far"), "{}", w205.2);
    // Set under 12 pt, it is drawn so by its size, not by shrinking, and W205 says which.
    let mut set_small = code("fn main() {}", json!({ "col": [1, 6], "row": [2, 4] }));
    set_small["style"] = json!({ "size": 20 });
    let found = lint(&deck(json!({ "n": set_small })));
    let w205 = found.iter().find(|(c, n, _)| c == "W205" && n == "n").expect("W205");
    assert!(w205.2.contains("it is set at 20 cu") && w205.2.contains("its one line is"), "{}", w205.2);
    // A line far too long is quoted short.
    let mut wide = code(&"step();".repeat(300), json!({ "col": [1, 12], "row": [2, 8] }));
    wide["minSize"] = json!(4);
    let found = lint(&deck(json!({ "n": wide })));
    let w205 = found.iter().find(|(c, n, _)| c == "W205" && n == "n").expect("W205");
    assert!(w205.2.len() < 300 && w205.2.ends_with("…\""), "{}", w205.2);
    // On a canvas twice as large, 12 pt is 48 cu: past body's own 40, which it keeps.
    let mut big = deck(json!({ "n": code(&long, json!({ "col": [1, 6], "row": 2 })) }));
    (big.canvas.width, big.canvas.height) = (3840.0, 2160.0);
    assert_eq!(placed(&big, "n").scale, 1.0);
    // Its role's `minSize`, where it sets none of its own, before 12 pt.
    let mut t: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    t["type"]["roles"]["body"]["minSize"] = json!(30);
    let roled = Theme::from_json(&t.to_string()).unwrap();
    let d = deck(json!({ "n": code(&long, json!({ "col": [1, 6], "row": 2 })) }));
    let data = DataFiles::new();
    let req = FrameRequest { deck: &d, theme: &roled, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
    assert_eq!(Engine::new(fonts(&d)).text_layout(&req, "n").unwrap().scale, 0.75);
}

#[test]
fn a_code_text_grows_its_box_before_it_shrinks() {
    let lines = (1..=8).map(|i| format!("println!(\"{i}\");")).collect::<Vec<_>>().join("\n");
    let (theme, data) = (theme(), DataFiles::new());
    let grown = |d: &Deck| {
        let req = FrameRequest { deck: d, theme: &theme, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
        Engine::new(fonts(d)).grown(&req, "n").unwrap()
    };
    // By cells: its rows grow down to the first that holds its eight lines at its own size.
    let d = deck(json!({ "n": code(&lines, json!({ "col": [1, 6], "row": 2 })) }));
    let p = placed(&d, "n");
    let natural = p.natural.expect("code keeps its size at its own")[1];
    assert!(p.scale < 1.0 && natural > p.cell[3], "it shrinks where it stands");
    let target = grown(&d).expect("it grows");
    assert!(target.cell[3] >= natural - 0.5, "{:?} holds {natural}", target.cell);
    let spot = &target.spots[0].1;
    assert_eq!(spot.row, Some(Range::Span([2, 5])));
    let mut d2 = deck(json!({ "n": code(&lines, json!({ "col": [1, 6], "row": [2, 5] })) }));
    assert_eq!(placed(&d2, "n").scale, 1.0, "grown, it fits at its own size");
    assert!(grown(&d2).is_none(), "nothing to grow");
    // By `rect`: its height grows to hold it, no further than the grid's margin.
    d2 = deck(json!({ "n": code(&lines, json!({ "rect": [96, 200, 852, 100] })) }));
    let target = grown(&d2).expect("it grows");
    assert_eq!(target.cell, [96.0, 200.0, 852.0, natural.ceil()]);
    let low = deck(json!({ "n": code(&lines, json!({ "rect": [96, 700, 852, 100] })) }));
    let target = grown(&low).expect("it grows to the margin");
    assert_eq!(target.cell[3], 984.0 - 700.0);
    // And then shrinks what is left.
    let mut placed_low = low.clone();
    placed_low.nodes["n"].props.insert("at".into(), json!({ "rect": [96, 700, 852, 284] }));
    let p = placed(&placed_low, "n");
    assert!(p.scale < 1.0 && !p.overflow, "{} {}", p.scale, p.overflow);
    // Lines wider than its box shrink it whatever its height: it grows to hold them at the size
    // its width lets it, and no further.
    let wide = (1..=8).map(|i| format!("println!(\"{i}: {}\");", "x".repeat(40))).collect::<Vec<_>>().join("\n");
    let d = deck(json!({ "n": code(&wide, json!({ "rect": [96, 200, 852, 100] })) }));
    let p = placed(&d, "n");
    let [wide_at_own, natural] = p.natural.unwrap();
    let across = p.cell[2] / wide_at_own;
    assert!(across < 0.9, "{across}");
    let target = grown(&d).expect("it grows");
    assert_eq!(target.cell[3], (natural * across).ceil());
    let mut d = d;
    d.nodes["n"].props.insert("at".into(), json!({ "rect": target.cell }));
    let p = placed(&d, "n");
    assert!(!p.overflow && (p.scale - across).abs() <= 1.0 / 64.0, "{} {across}", p.scale);
}

#[test]
fn a_code_text_in_a_container_shrinks_to_the_room_it_is_given() {
    let line = "fn main() { println!(\"a line far too long for half a row of this stack\"); }";
    let text = format!("{line}\nlet x = 1;");
    let in_row = |axis: &str| {
        let mut n = code(&text, json!({ "parent": "row" }));
        n["role"] = json!("code");
        deck(json!({
            "row": { "type": "stack", "axis": axis, "gap": 24, "at": { "col": [1, 12], "row": [2, 8] } },
            "n": n,
            "m": { "type": "text", "role": "body", "text": "Beside the code, a sentence.", "at": { "parent": "row" } },
        }))
    };
    // In a row, it asks for no more than its own size and no less than its least, so it shrinks
    // to its share and leaves its neighbor on the slide, where its whole line took the row.
    let d = in_row("x");
    let (n, m) = (placed(&d, "n"), placed(&d, "m"));
    assert!(n.scale < 1.0 && n.scale >= 24.0 / 40.0 && !n.overflow, "{} {}", n.scale, n.overflow);
    assert!(m.cell[0] >= n.cell[0] + n.cell[2] && m.cell[0] + m.cell[2] <= 1824.0 + 0.5, "{:?} {:?}", n.cell, m.cell);
    // In a column, it stands as tall as the width it is given sets it, with no room under it.
    let d = in_row("y");
    let n = placed(&d, "n");
    let (top, bottom) = n.text.trimmed(TextBox::Line);
    assert!(n.scale < 1.0 && !n.overflow, "{} {}", n.scale, n.overflow);
    assert!((n.cell[3] - (bottom - top)).abs() <= 0.5, "{:?} holds {}", n.cell, bottom - top);
}

/// How each kind of token is drawn in `theme`'s `code` role on its surface: what the engine draws
/// a code text on the slide in.
fn looks_on_surface(theme: &Theme) -> Looks {
    let [r, g, b, _] = theme.color("surface").unwrap().0;
    theme.code_looks([r, g, b], &theme.text_role("code").unwrap()).unwrap()
}

/// The color the glyph at laid-out byte `laid` of `layout`'s text is drawn in.
fn color_of(layout: &TextLayout, laid: usize) -> [u8; 4] {
    layout.runs.iter().find(|r| r.clusters.contains(&laid)).map(|r| r.color.0).unwrap()
}

/// Where the character `written` bytes into `layout`'s text as written starts as laid out.
fn laid(layout: &TextLayout, written: usize) -> usize {
    layout.offsets[layout.offsets.partition_point(|o| o.0 < written)].1
}

const RUST: &str = "fn main() {\n\tlet greeting = \"hello\"; // says hi\n\treturn Vec::new(42);\n}";

#[test]
fn a_code_text_is_colored_by_its_language_in_the_themes_colors() {
    let theme = theme();
    let looks = looks_on_surface(&theme);
    let mut node = code(RUST, json!({ "col": [1, 12], "row": [2, 8] }));
    node["role"] = json!("code");
    let d = deck(json!({ "n": node }));
    let t = placed(&d, "n").text;
    // Each glyph is drawn in its token's look: the keywords in the theme's `signal`, held to read
    // at 7:1 on the paper, every other kind derived, plain words in the text's own color.
    let tokens = highlight(RUST, lang("rust"));
    let mut start = 0;
    let mut kinds = Vec::new();
    for token in &tokens {
        for (w, _) in RUST[start..token.end].char_indices() {
            let at = laid(&t, start + w);
            if t.runs.iter().any(|r| r.clusters.contains(&at)) {
                assert_eq!(color_of(&t, at), looks[token.class as usize].color, "{:?} at {}", token.class, start + w);
            }
        }
        kinds.push(token.class);
        start = token.end;
    }
    for kind in [Class::Keyword, Class::Function, Class::String, Class::Comment, Class::Number, Class::Type] {
        assert!(kinds.contains(&kind), "{kind:?} in {kinds:?}");
    }
    let paper = [0xFB, 0xFA, 0xF6];
    let keyword = looks[Class::Keyword as usize];
    assert_eq!(keyword.weight, 700.0, "the theme's weight for keywords");
    let [r, g, b, _] = theme.color("signal").unwrap().0;
    let [kr, kg, kb] = scaena_core::palette::held_strong([r, g, b], paper);
    assert_eq!(keyword.color, [kr, kg, kb, 255], "the signal, held to read");
    assert!(ratio([kr, kg, kb], paper) >= STRONG, "{keyword:?}");
    assert_eq!(looks[Class::Plain as usize].color, theme.color("ink").unwrap().0);
    // The spans are as written: colored, the text keeps its characters and its lines, and, in a
    // monospaced face, each glyph stands where it stood uncolored.
    let lay = |looks: Option<Box<Looks>>| {
        let mut spec = TextSpec::plain(theme.text_role("code").unwrap(), RUST);
        spec.code = Some(CodeSpec {
            language: "rust".into(),
            tab: 4,
            least: 1.0,
            looks,
            card: None,
            numbers: None,
            focus: None,
            dimmed: None,
            band: None,
        });
        TextEngine::new().layout(&mut fonts(&d), &theme, &spec, 2000.0).unwrap()
    };
    let (plain, colored) = (lay(None), lay(Some(Box::new(looks))));
    assert_eq!((&colored.text, &colored.written, &colored.offsets), (&plain.text, &plain.written, &plain.offsets));
    assert_eq!(colored.lines.len(), plain.lines.len());
    let xs = |t: &TextLayout| -> Vec<(usize, i32)> {
        let mut xs: Vec<(usize, i32)> = (t.runs.iter())
            .flat_map(|r| r.clusters.iter().zip(&r.glyphs).map(|(c, g)| (*c, (g.x * 64.0).round() as i32)))
            .collect();
        xs.sort();
        xs
    };
    assert_eq!(xs(&colored), xs(&plain));
    assert!(colored.runs.len() > plain.runs.len(), "split by token");
}

#[test]
fn every_code_color_reads_4_5_to_1_on_its_ground() {
    let dusk =
        || -> Value { serde_json::from_str(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap() };
    let daybreak = || -> Value {
        serde_json::from_str(include_str!("../../../docs/examples/authorability/themes/daybreak.theme.json")).unwrap()
    };
    let mut named = dusk();
    // Named colors that do not read where they are drawn: the rule's, a literal, and one that reads
    // on a light ground alone.
    named["code"] = json!({ "syntax": {
        "comment": { "color": "line" }, "string": { "color": "#2E8B57" }, "keyword": { "color": "ink" },
    } });
    let mut light_named = daybreak();
    light_named["code"] = json!({ "syntax": { "keyword": { "color": "line" }, "number": { "color": "#FFFF00" } } });
    let grounds: [(&str, [u8; 3]); 6] = [
        // The card 2.136 draws on Dusk, and on Daybreak.
        ("a card on a dark theme", [0x29, 0x29, 0x2E]),
        ("a card on a light theme", [0xDF, 0xDD, 0xD5]),
        // A node's dark fill on a light theme, and a light one on a dark theme.
        ("a dark fill", [0x16, 0x14, 0x0F]),
        ("a light fill", [0xF6, 0xF4, 0xEF]),
        // Where 7:1 cannot be had, and where 4.5:1 can be had only in black.
        ("a mid gray", [0x80, 0x80, 0x80]),
        ("a blue", [0x2F, 0x6F, 0xD0]),
    ];
    for (name, theme) in
        [("dusk", dusk()), ("daybreak", daybreak()), ("dusk named", named), ("daybreak named", light_named)]
    {
        let theme = Theme::from_json(&theme.to_string()).unwrap();
        let [r, g, b, _] = theme.color("surface").unwrap().0;
        for (on, ground) in grounds.into_iter().chain([("the surface", [r, g, b])]) {
            let looks = theme.code_looks(ground, &theme.text_role("code").unwrap()).unwrap();
            let strong = ratio([0; 3], ground).max(ratio([255; 3], ground)) >= STRONG;
            let best = ratio([0; 3], ground).max(ratio([255; 3], ground));
            for (i, look) in looks.iter().enumerate() {
                let [r, g, b, a] = look.color;
                assert_eq!(a, 255, "opaque");
                let keyword = i == Class::Keyword as usize || i == Class::Tag as usize;
                let wanted = if keyword && strong { STRONG } else { READS.min(best) };
                assert!(ratio([r, g, b], ground) >= wanted - 1e-9, "{name} on {on}: {i} {look:?} at {wanted}");
            }
        }
    }
}

#[test]
fn color_alone_keeps_shaping_whole() {
    // Roboto Serif kerns `T.`, a type and the punctuation after it in code: drawn in two colors,
    // the pair is shaped as one, so it kerns as it does uncolored.
    let theme = theme();
    let d = deck(json!({}));
    let text = "T.V,To.Wa";
    let body = theme.text_role("body").unwrap();
    let lay = |looks: Option<Looks>| {
        let mut spec = TextSpec::plain(body.clone(), text);
        spec.code = Some(CodeSpec {
            language: "rust".into(),
            tab: 4,
            least: 1.0,
            looks: looks.map(Box::new),
            card: None,
            numbers: None,
            focus: None,
            dimmed: None,
            band: None,
        });
        TextEngine::new().layout(&mut fonts(&d), &theme, &spec, 4000.0).unwrap()
    };
    let width = |s: &str| {
        TextEngine::new().layout(&mut fonts(&d), &theme, &TextSpec::plain(body.clone(), s), 4000.0).unwrap().width
    };
    assert!(width("T.") < width("T") + width(".") - 0.5, "the pair kerns");
    // Every kind its own color, at the text's weight and upright.
    let mut looks = [TokenLook { color: [0, 0, 0, 255], weight: body.weight, italic: false }; 17];
    for (i, look) in looks.iter_mut().enumerate() {
        look.color = [i as u8 * 13, 40, 200 - i as u8 * 9, 255];
    }
    let (plain, colored) = (lay(None), lay(Some(looks)));
    let glyphs = |t: &TextLayout| -> Vec<(u32, i32, usize)> {
        t.runs
            .iter()
            .flat_map(|r| r.glyphs.iter().zip(&r.clusters).map(|(g, c)| (g.id, (g.x * 64.0).round() as i32, *c)))
            .collect()
    };
    assert_eq!(glyphs(&colored), glyphs(&plain));
    let colors: std::collections::BTreeSet<[u8; 4]> = colored.runs.iter().map(|r| r.color.0).collect();
    assert!(colors.len() >= 2, "drawn in more than one color: {colors:?}");
    let tokens = highlight(text, lang("rust"));
    assert!(tokens.len() >= 2 && tokens[0].class != tokens[1].class, "{tokens:?}");
}

#[test]
fn code_stands_on_a_card_that_hugs_it_and_its_numbered_lines() {
    // PLAN 2.136: a card that hugs the code and its gutter, 32 cu in where the theme says
    // nothing, and the line numbers beside each line, from `start`.
    let lines = "fn main() {\n    run();\n}";
    let at = json!({ "rect": [100, 100, 1200, 600] });
    let d = deck(json!({ "n": framed(lines, at.clone(), json!({ "numbers": true, "start": 41 })) }));
    let p = placed(&d, "n");
    let frame = p.frame.as_ref().expect("a card and numbers");
    let card = frame.card.as_ref().expect("a card").rect;
    let (beside, gutter) = frame.gutter.as_ref().expect("numbers");
    let said: Vec<&str> = gutter.lines.iter().map(|l| gutter.text[l.text.clone()].trim()).collect();
    assert_eq!(said, ["41", "42", "43"]);
    for (number, line) in gutter.lines.iter().zip(&p.text.lines) {
        assert!((beside[1] + number.baseline - (p.origin[1] + line.baseline)).abs() < 1e-3, "on its line's baseline");
    }
    assert!(beside[0] + gutter.width < p.origin[0], "the numbers stand left of the code");
    // No caret, unit, word, or reader takes them: they are not the text's.
    assert!(!p.text.text.contains("41"));
    // The card hugs the gutter and the code, 32 cu in, at the box's top left.
    assert_eq!([card[0], card[1]], [100.0, 100.0]);
    assert!((beside[0] - card[0] - 32.0).abs() < 1e-3);
    assert!((card[0] + card[2] - (p.origin[0] + p.text.width) - 32.0).abs() < 1e-3);
    assert!(card[2] < 1200.0, "it hugs, never fills its box");
    // `align` places it in its box.
    let mut centered = framed(lines, at, json!({}));
    centered["align"] = json!({ "x": "center", "y": "center" });
    let p = placed(&deck(json!({ "n": centered })), "n");
    let card = p.frame.as_ref().and_then(|f| f.card.as_ref()).expect("a card").rect;
    assert!((card[0] + card[2] / 2.0 - 700.0).abs() < 1e-3 && (card[1] + card[3] / 2.0 - 400.0).abs() < 1e-3);
    assert!(p.frame.as_ref().unwrap().gutter.is_none(), "numbers are off where it says none");
}

#[test]
fn a_line_number_near_its_most_counts_on() {
    let d = deck(
        json!({ "n": framed("a\nb\nc", json!({ "rect": [100, 100, 1200, 600] }), json!({ "numbers": true, "start": 999_999 })) }),
    );
    let p = placed(&d, "n");
    let (_, gutter) = p.frame.as_ref().unwrap().gutter.as_ref().unwrap();
    let said: Vec<&str> = gutter.lines.iter().map(|l| gutter.text[l.text.clone()].trim()).collect();
    assert_eq!(said, ["999999", "1000000", "1000001"]);
}

#[test]
fn a_theme_with_no_card_gets_one_that_shows_and_code_reads_on_it() {
    // The torture theme names no card: `surface-2` stepped from the surface to 1.3:1.
    let theme = theme();
    let card = theme.code_card().unwrap().fill;
    let [r, g, b, _] = theme.color("surface").unwrap().0;
    let on = ratio([card[0], card[1], card[2]], [r, g, b]);
    assert!((1.3..1.32).contains(&on), "{on}");
    // Every color its code is drawn in reads on the card, not just on the surface.
    let d = deck(
        json!({ "n": framed("let x = 1; // one", json!({ "rect": [100, 100, 1200, 600] }), json!({ "numbers": true })) }),
    );
    let p = placed(&d, "n");
    let (_, gutter) = p.frame.as_ref().unwrap().gutter.as_ref().unwrap();
    for run in p.text.runs.iter().chain(&gutter.runs) {
        let [r, g, b, _] = run.color.0;
        assert!(ratio([r, g, b], [card[0], card[1], card[2]]) >= READS, "{:?} on {card:?}", run.color);
    }
}

#[test]
fn a_deck_saved_before_code_stood_on_a_card_draws_as_it_did() {
    let mut d: Value = serde_json::from_str(
        &deck(json!({ "n": framed("let x = 1;", json!({ "rect": [100, 100, 1200, 600] }), json!({})) }))
            .to_json()
            .unwrap(),
    )
    .unwrap();
    d["scaena"] = json!("0.38");
    let old = Deck::from_json(&d.to_string()).unwrap();
    assert_eq!(old.nodes["n"].props["code"]["card"], json!(false));
    assert!(placed(&old, "n").frame.is_none());
}

#[test]
fn code_on_a_card_collides_by_its_card() {
    // PLAN 2.136: what E101 judges code on a card by is its card, which its words stand inside.
    let at = json!({ "rect": [100, 100, 1200, 600] });
    let carded = framed("let x = 1;", at.clone(), json!({}));
    let card = placed(&deck(json!({ "n": carded.clone() })), "n").frame.unwrap().card.unwrap().rect;
    // A label over the card's right edge, clear of its words by most of the inset.
    let label = json!({ "type": "text", "role": "body", "text": "Note", "at": { "rect": [card[0] + card[2] - 12.0, card[1] + 8.0, 300, 80] } });
    let collides =
        |n: Value| lint(&deck(json!({ "n": n, "label": label.clone() }))).iter().any(|(c, _, _)| c == "E101");
    assert!(collides(carded), "the label stands on the card");
    assert!(!collides(code("let x = 1;", at)), "with no card, it is clear of the words");
}

/// `node` of `deck` laid out in `theme` at rest in its one state.
fn placed_in(deck: &Deck, theme: &Theme, node: &str) -> PlacedText {
    let data = DataFiles::new();
    let req = FrameRequest { deck, theme, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
    Engine::new(fonts(deck)).text_layout(&req, node).unwrap()
}

/// The torture theme with `focus` as its `code.focus` (PLAN 2.140).
fn focusing(focus: Value) -> Theme {
    let mut t: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    t["code"]["focus"] = focus;
    Theme::from_json(&t.to_string()).unwrap()
}

/// The color each glyph of `layout` is drawn in, by where its cluster starts as laid out, with
/// the line it stands on and its x.
fn glyph_colors(layout: &TextLayout) -> Vec<(usize, usize, i32, [u8; 4])> {
    let mut out: Vec<(usize, usize, i32, [u8; 4])> = (layout.runs.iter())
        .flat_map(|r| {
            r.clusters.iter().zip(&r.glyphs).map(move |(c, g)| (*c, r.line, (g.x * 64.0).round() as i32, r.color.0))
        })
        .collect();
    out.sort();
    out
}

#[test]
fn lines_out_of_focus_are_set_back_toward_their_ground_never_under_3_to_1() {
    // PLAN 2.140: line 2 in focus; the rest, and their numbers, set back toward the card, fainter
    // than they were and never under 3:1, each glyph where it stood. A theme that sets back with
    // no band, so the line in focus keeps its colors.
    let plain = focusing(json!({ "dim": 0.55 }));
    let at = json!({ "rect": [100, 100, 1600, 700] });
    let whole = placed_in(&deck(json!({ "n": framed(RUST, at.clone(), json!({ "numbers": true })) })), &plain, "n");
    let walked =
        placed_in(&deck(json!({ "n": framed(RUST, at, json!({ "numbers": true, "focus": [2] })) })), &plain, "n");
    assert_eq!(walked.focus, Some(vec![(1, 1)]));
    assert!(walked.frame.as_ref().is_some_and(|f| f.card.is_some()), "a card");
    let [r, g, b, _] = plain.code_card().unwrap().fill;
    let ground = [r, g, b];
    let (was, now) = (glyph_colors(&whole.text), glyph_colors(&walked.text));
    assert_eq!(was.len(), now.len());
    let mut set_back = 0;
    for ((c, line, x, before), (c2, line2, x2, after)) in was.iter().zip(&now) {
        assert_eq!((c, line, x), (c2, line2, x2), "a change of focus shapes nothing apart");
        let (b, a) = ([before[0], before[1], before[2]], [after[0], after[1], after[2]]);
        if *line == 1 {
            assert_eq!(before, after, "the line in focus is drawn as it was");
            continue;
        }
        if ratio(b, ground) <= 3.0 {
            continue;
        }
        assert!(ratio(a, ground) >= 3.0, "{after:?} on {ground:?}");
        assert!(ratio(a, ground) < ratio(b, ground), "set back: {before:?} to {after:?}");
        set_back += 1;
    }
    assert!(set_back > 20, "{set_back}");
    // Its numbers with their lines.
    let numbers = |p: &PlacedText| -> Vec<[u8; 4]> {
        let (_, gutter) = p.frame.as_ref().unwrap().gutter.as_ref().unwrap();
        let mut by_line: Vec<(usize, [u8; 4])> = gutter.runs.iter().map(|r| (r.line, r.color.0)).collect();
        by_line.dedup();
        by_line.into_iter().map(|(_, c)| c).collect()
    };
    let (a, b) = (numbers(&whole), numbers(&walked));
    assert_eq!(a.len(), 4);
    assert_eq!(a[1], b[1], "the number of the line in focus as it was");
    for k in [0, 2, 3] {
        assert_ne!(a[k], b[k], "line {}'s number set back", k + 1);
        assert!(ratio([b[k][0], b[k][1], b[k][2]], ground) >= 3.0);
    }
}

#[test]
fn a_band_stands_behind_the_lines_in_focus_and_they_read_on_it() {
    // PLAN 2.140: a theme that bands the lines in focus draws a band behind each run of them,
    // over the card, as tall as its lines, and the code in focus reads 4.5:1 on it.
    let theme = focusing(json!({ "dim": 0.55, "band": "surface" }));
    let d =
        deck(json!({ "n": framed(RUST, json!({ "rect": [100, 100, 1600, 700] }), json!({ "focus": [1, [3, 4]] })) }));
    let p = placed_in(&d, &theme, "n");
    let frame = p.frame.as_ref().expect("a frame");
    assert_eq!(frame.bands.len(), 2, "one band for each run of lines in focus");
    let [r, g, b, _] = theme.color("surface").unwrap().0;
    let lines = &p.text.lines;
    for (band, (first, last)) in frame.bands.iter().zip([(0, 0), (2, 3)]) {
        let top = p.origin[1] + lines[first].baseline - lines[first].ascent;
        let bottom = p.origin[1] + lines[last].baseline + lines[last].descent;
        assert!((band.rect[1] - top).abs() < 1e-3 && (band.rect[1] + band.rect[3] - bottom).abs() < 1e-3);
        assert!(band.rect[0] < p.origin[0] && band.rect[0] + band.rect[2] > p.origin[0] + p.text.width);
    }
    for run in p.text.runs.iter().filter(|r| r.line != 1) {
        let [x, y, z, _] = run.color.0;
        assert!(ratio([x, y, z], [r, g, b]) >= READS, "{:?} on the band", run.color);
    }
    // No band where nothing is in focus: every line is.
    let d = deck(json!({ "n": framed(RUST, json!({ "rect": [100, 100, 1600, 700] }), json!({})) }));
    assert!(placed_in(&d, &theme, "n").frame.unwrap().bands.is_empty());
}
