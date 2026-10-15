//! The editors say the same (PLAN 2.99): the Mac's window words what it shows through `Words` and
//! `Issues` in Swift (PLAN 3.18, 3.20), and the browser's through `scaena_core::words`. These hold
//! the Swift tables to the Rust ones, so the two cannot drift apart unseen.

use regex_lite::Regex;
use scaena_core::words;
use std::collections::BTreeSet;

const WORDS_SWIFT: &str = include_str!("../../../apps/mac/ScaenaKit/Sources/Scaena/Words.swift");
const ISSUES_SWIFT: &str = include_str!("../../../apps/mac/ScaenaKit/Sources/Scaena/IssueBadges.swift");
const CUE_SWIFT: &str = include_str!("../../../apps/mac/ScaenaKit/Sources/Scaena/CueBar.swift");
const INSPECTOR_SWIFT: &str = include_str!("../../../apps/mac/ScaenaKit/Sources/Scaena/Inspector.swift");

/// The Swift source between `from` and the first `until` after it.
fn between<'a>(source: &'a str, from: &str, until: &str) -> &'a str {
    let start = source.find(from).unwrap_or_else(|| panic!("no `{from}` in the Swift"));
    let rest = &source[start..];
    &rest[..rest.find(until).unwrap_or_else(|| panic!("no `{until}` after `{from}`"))]
}

/// Each `"key": "value"` in a Swift dictionary literal.
fn dictionary(source: &str) -> BTreeSet<(String, String)> {
    let pair = Regex::new(r#""([^"]+)": "([^"]+)""#).unwrap();
    pair.captures_iter(source).map(|c| (c[1].to_string(), c[2].to_string())).collect()
}

fn table(rust: &[(&str, &str)]) -> BTreeSet<(String, String)> {
    rust.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn the_mac_labels_each_property_as_the_browser_does() {
    let swift = dictionary(between(WORDS_SWIFT, "private static let labels", "]\n"));
    assert_eq!(swift, table(words::LABELS));
    // A chart's channels as its kind reads them: a donut's Slices and Values, and those of bars
    // that run across, Down the Side and Across (PLAN 1.29), which the inspector asks for where the
    // chart's bars run across.
    let channels = |kind: &str| -> BTreeSet<(String, String)> {
        let rust: Vec<(&str, &str)> =
            words::CHANNELS.iter().filter(|(k, ..)| *k == kind).map(|(_, prop, said)| (*prop, *said)).collect();
        table(&rust)
    };
    assert_eq!(dictionary(between(WORDS_SWIFT, "private static let donut", "]\n")), channels("donut"));
    assert_eq!(dictionary(between(WORDS_SWIFT, "private static let across", "]\n")), channels("across"));
    let row = between(INSPECTOR_SWIFT, "@ViewBuilder private func row(", "\n        Group {");
    assert!(row.contains("Words.label(field.prop, chart: chartKind, horizontal: barsAcross)"), "{row}");
}

/// The Mac's inspector heads a node as the browser's does (PLAN 2.99): by its kind as the engine
/// says it, a Star, a Card, a Drawing (`noun`), not by its type alone.
#[test]
fn the_mac_heads_a_node_by_its_kind_as_the_browser_does() {
    let title = between(INSPECTOR_SWIFT, "private func title(", "case .adjust:");
    assert!(title.contains("node ? (noun ?? Words.kind(type)) : \"Slide\""), "{title}");
    assert!(INSPECTOR_SWIFT.contains("title(part, type: type, noun: choices.noun, node: isNode)"));
}

#[test]
fn the_mac_says_each_word_as_the_browser_does() {
    let source = between(WORDS_SWIFT, "static func word(", "default:");
    let by_prop = Regex::new(r#"case \("([^"]+)", "([^"]+)"\): return "([^"]+)""#).unwrap();
    let swift: BTreeSet<_> =
        by_prop.captures_iter(source).map(|c| (c[1].to_string(), c[2].to_string(), c[3].to_string())).collect();
    let rust: BTreeSet<_> =
        words::WORDS.iter().map(|(p, w, s)| (p.to_string(), w.to_string(), s.to_string())).collect();
    assert_eq!(swift, rust);
    let any = Regex::new(r#"case \(_, "([^"]+)"\): return "([^"]+)""#).unwrap();
    let swift: BTreeSet<_> = any.captures_iter(source).map(|c| (c[1].to_string(), c[2].to_string())).collect();
    assert_eq!(swift, table(words::ANY_WORDS));
}

#[test]
fn the_mac_names_colors_easings_presets_shapes_and_kinds_as_the_browser_does() {
    assert_eq!(dictionary(between(WORDS_SWIFT, "private static let colors", "]\n")), table(words::COLORS));
    assert_eq!(dictionary(between(WORDS_SWIFT, "private static let easings", "]\n")), table(words::EASINGS));
    assert_eq!(dictionary(between(WORDS_SWIFT, "private static let presets", "]\n")), table(words::PRESETS));
    // Build Out's names, each build as it leaves (PLAN 2.116).
    assert_eq!(dictionary(between(WORDS_SWIFT, "private static let exits", "]\n")), table(words::EXITS));
    assert_eq!(dictionary(between(WORDS_SWIFT, "private static let shapes", "]\n")), table(words::SHAPES));
    let kinds = Regex::new(r#"case "([a-z]+)": "([^"]+)""#).unwrap();
    let source = between(WORDS_SWIFT, "static func kind(", "default:");
    for c in kinds.captures_iter(source) {
        let t: scaena_core::document::NodeType = serde_json::from_value(serde_json::json!(&c[1])).unwrap();
        assert_eq!(words::kind(Some(t)), &c[2], "{}", &c[1]);
    }
    assert_eq!(words::kind(None), "Object");
    let motions = dictionary(between(WORDS_SWIFT, "static func motion(", "}\n"));
    for (kind, said) in motions.iter().filter(|(k, _)| ["enter", "exit", "emphasis", "anim"].contains(&k.as_str())) {
        assert_eq!(words::motion(kind, None), *said);
    }
    for (split, by) in motions
        .iter()
        .filter(|(k, _)| ["lines", "words", "glyphs", "paragraphs", "children", "marks"].contains(&k.as_str()))
    {
        assert_eq!(words::motion("enter", Some(split)), format!("Enters {by}"));
    }
}

/// The Mac's Add Motion names each build as the browser's menu does (PLAN 2.112, 3.18): by its name,
/// `Move In from Left`, and as its object leaves by Build Out's, `Move Out to Left` (PLAN 2.116),
/// under its object's words, never by the theme's key or the node's id.
#[test]
fn the_mac_names_each_build_it_offers_as_the_browser_does() {
    let adding = between(CUE_SWIFT, "private var adding: some View", ".menuStyle(");
    assert!(adding.contains("Button(Words.build(preset, as: offer.motion))"), "{adding}");
    assert!(!adding.contains("Button(preset)") && !adding.contains("offer.label"), "{adding}");
    let add = between(CUE_SWIFT, "private func add(_ preset: String", "\n    }\n");
    assert!(add.contains("Words.build(preset, as: offer.motion)") && !add.contains("phrase"), "{add}");
    // A build is said by Build Out's name where its object leaves, else by its own.
    let build = between(WORDS_SWIFT, "static func build(", "\n    }\n");
    assert!(build.contains(r#"if motion == "exit", let said = exits[preset] { return said }"#), "{build}");
    assert!(build.contains(r#"return name(preset, of: "motion-preset", theme: nil)"#), "{build}");
}

/// A build the inspector keeps but no longer offers for its kind reads as its name, as the browser
/// says it (PLAN 2.116): Pulse, not Custom, which is for a value written out.
#[test]
fn the_mac_says_a_build_it_keeps_by_its_name() {
    let pick = between(INSPECTOR_SWIFT, "private struct PickRow", "\n}\n");
    assert!(pick.contains("Text(Self.kept(value, said: said)).tag(current)"), "{pick}");
    assert!(!pick.contains(r#"Text("Custom").tag(current)"#), "{pick}");
    // Build Out's pick says each build as its object leaves (PLAN 2.116), as the browser's does:
    // with one object selected, and with several (PLAN 3.13).
    let several = between(INSPECTOR_SWIFT, "private func several(", "\n    }\n");
    for picks in [INSPECTOR_SWIFT, several] {
        let named = between(picks, "case .name(let of, let names, _):", "case .word(");
        assert!(
            named.contains("? Words.build(name, as: field.prop) : Words.name(name, of: of, theme: look)"),
            "{named}"
        );
        assert!(named.contains("said: say, choose: choose"), "{named}");
    }
}

/// A data source is named as a person names it (PLAN 3.18), not by the id the deck writes it with:
/// by its file's name in words, else by its own. The Mac says it so through `Words.source`, with the
/// deck's sources the inspector reads, as the session words the browser's.
#[test]
fn the_mac_names_a_data_source_as_the_browser_does() {
    assert_eq!(words::source("funding", Some("data/trails-funding.csv")), "Trails Funding");
    assert_eq!(words::source("segments", None), "Segments");
    assert_eq!(words::word("data", "@funding"), "Funding");
    let source = between(WORDS_SWIFT, "static func source(", "\n    }\n");
    for step in [
        r#"file.split(separator: "/", omittingEmptySubsequences: false).last"#,
        r#"leaf.lastIndex(of: ".")"#,
        "if !said.isEmpty { return said }",
        "return phrase(name)",
    ] {
        assert!(source.contains(step), "{step}: {source}");
    }
    let word = between(WORDS_SWIFT, "static func word(", "\n    }\n");
    assert!(word.contains("return source(name, file: sources.first(where: { $0.name == name })?.file)"), "{word}");
    let row = between(INSPECTOR_SWIFT, "@ViewBuilder private func row(", "\n    }\n");
    assert!(row.contains("Words.word(word, of: field.prop, type: choices?.type, sources: sources)"), "{row}");
    assert!(INSPECTOR_SWIFT.contains("sources = (try? session.dataSources()) ?? []"));
}

/// A choice left unset where that leaves none is None, as a presentation app calls it, not Default
/// (PLAN 3.18): a chart's source, each channel it reads a column by, a picture's mask, and a shadow.
/// A key is not one: unset, it reads the chart's `x`.
#[test]
fn the_mac_calls_what_leaves_none_none() {
    let quoted = Regex::new(r#""([^"]+)""#).unwrap();
    let nothing: BTreeSet<String> = quoted
        .captures_iter(between(INSPECTOR_SWIFT, "private static let nothing", "]\n"))
        .map(|c| c[1].to_string())
        .collect();
    let pick = between(INSPECTOR_SWIFT, "private struct PickRow", "\n}\n");
    assert!(pick.contains(r#"Text(shows?.title ?? (nothing ? "None" : "Default")).tag("")"#), "{pick}");
    let theme =
        scaena_core::model::Theme::from_json(include_str!("../../../docs/examples/themes/dusk.theme.json")).unwrap();
    let csv = include_bytes!("../../../docs/examples/data/q3-revenue.csv").to_vec();
    let files = std::collections::BTreeMap::from([("data/q3-revenue.csv".to_string(), csv)]);
    let mut channels = BTreeSet::new();
    for kind in ["bar", "range"] {
        let mut doc: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        doc["nodes"]["rev"]["kind"] = serde_json::json!(kind);
        let deck: scaena_core::Deck = serde_json::from_value(doc).unwrap();
        let chart = scaena_core::choices::choices(&deck, &theme, "revenue", "rev", &files).unwrap();
        let reads = chart.fields.iter().map(|f| f.prop.clone());
        channels.extend(reads.filter(|p| p.ends_with("/field") || p.starts_with("interval/")));
    }
    assert!(channels.len() >= 8, "{channels:?}");
    for prop in channels.iter().map(String::as_str).chain(["data", "mask", "shadow"]) {
        assert!(nothing.contains(prop), "{prop} left unset is called Default: {nothing:?}");
    }
    assert!(!nothing.contains("key"), "a key left unset reads the chart's x");
}

#[test]
fn the_mac_says_each_issue_and_its_fix_as_the_browser_does() {
    let said = Regex::new(r#"case ((?:"[EWI]\d+"(?:, )?)+): "([^"]+)""#).unwrap();
    let code = Regex::new(r#""([EWI]\d+)""#).unwrap();
    let pairs = |source: &str| -> BTreeSet<(String, String)> {
        said.captures_iter(source)
            .flat_map(|c| {
                let sentence = c[2].to_string();
                code.captures_iter(&c[1]).map(move |k| (k[1].to_string(), sentence.clone())).collect::<Vec<_>>()
            })
            .collect()
    };
    let swift = pairs(between(ISSUES_SWIFT, "static func said(", "default:"));
    assert_eq!(swift, table(words::MARKED));
    for (code, fix) in pairs(between(ISSUES_SWIFT, "static func fixing(", "default:")) {
        assert_eq!(words::fixing(&code), fix, "{code}");
    }
    assert_eq!(words::fixing("E101"), "Fix");
}

/// Every code lint has, in SPEC §7.5's catalog, is said in a sentence, and none twice.
#[test]
fn every_lint_code_is_said_in_a_sentence() {
    let spec = include_str!("../../../docs/SPEC.md");
    let catalog = between(spec, "### 7.5 Lint catalog", "\n### 7.6");
    let row = Regex::new(r"(?m)^\| ([EWI]\d{3}) \|").unwrap();
    let codes: Vec<String> = row.captures_iter(catalog).map(|c| c[1].to_string()).collect();
    assert!(codes.len() > 40, "{codes:?}");
    for code in &codes {
        let said = words::issue(code).unwrap_or_else(|| panic!("{code} is said in no sentence"));
        assert!(said.ends_with('.') && !said.contains('`'), "{code}: {said}");
    }
    let all: Vec<&str> = words::MARKED.iter().chain(words::LISTED).map(|(c, _)| *c).collect();
    let set: BTreeSet<&str> = all.iter().copied().collect();
    assert_eq!(all.len(), set.len(), "a code said twice");
    assert_eq!(set.len(), codes.len(), "a code said that the catalog has not: {set:?}");
}

/// A change the history keeps reads as what was done and who did it (PLAN 2.99), in the words
/// every editor's Versions lists it in: no operation's name, and no author's id.
#[test]
fn a_change_of_the_history_is_said_as_a_person_says_it() {
    assert_eq!(words::change("patch: add_node, place, place"), "An object added, an object moved");
    assert_eq!(words::change("patch: a_new_op"), "An edit");
    assert_eq!(words::change(""), "A change");
    assert_eq!(words::change("history begins"), "The history begins");
    assert_eq!(words::change("data_edit sales: 2 cells"), "The data “sales” changed");
    assert_eq!(words::change("theme --apply themes/daybreak.theme.json"), "The theme Daybreak taken");
    assert_eq!(words::change("theme_edit: colors from assets/photo.jpg"), "The theme given the colors of photo.jpg");
    assert_eq!(words::change("undo patch: replace_text"), "Undone: words typed");
    assert_eq!(words::change("deck.json changed outside Scaena"), "Changed outside Scaena");
    assert_eq!(words::author("user"), "you");
    assert_eq!(words::author("fs"), "outside Scaena");
    assert_eq!(words::author("agent:claude-sonnet"), "the assistant (claude-sonnet)");
    assert_eq!(words::author("agent:"), "the assistant");
    assert_eq!(words::author("jay"), "jay");
}
