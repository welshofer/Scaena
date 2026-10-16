//! Syllables and morae (PLAN O.4a): the table and the rules against CMUdict, a word said two ways,
//! capitals, digits, apostrophes, hyphens, accents, kana, every form of `check`, and counting that
//! never fails on any text.

use scaena_core::verse::{self, Count, Kept};
use std::path::Path;

fn kept() -> Vec<(String, Count, u32)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/verse/syllables.tsv");
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let mut fields = l.split('\t');
            let word = fields.next().unwrap().to_string();
            let said = fields.next().unwrap();
            let (low, high) = said.split_once('-').unwrap_or((said, said));
            let rules = fields.next().unwrap().parse().unwrap();
            (word, Count { low: low.parse().unwrap(), high: high.parse().unwrap() }, rules)
        })
        .collect()
}

fn en(line: &str) -> Option<(u32, u32)> {
    verse::count(line, "en").map(|c| (c.low, c.high))
}

#[test]
fn the_table_and_the_rules_agree_with_cmudict() {
    let kept = kept();
    assert_eq!(kept.len(), 30_000);
    let table = include_str!("../src/verse/words.txt");
    let tabled: Vec<&str> = table.lines().map(|l| l.trim_end_matches(|c: char| c.is_ascii_digit())).collect();
    assert_eq!(tabled.len(), 2_000);

    // The rules here are build_syllables.py's, which are words/syllable's own expressions.
    let drifted: Vec<_> = kept
        .iter()
        .filter(|(w, _, rules)| verse::rules(w.as_bytes()) != *rules)
        .map(|(w, _, rules)| format!("{w}: {} here, {rules} there", verse::rules(w.as_bytes())))
        .collect();
    assert!(drifted.is_empty(), "the rules drift from words/syllable's: {drifted:?}");

    // Every word in the table counts as CMUdict says it.
    for word in &tabled {
        let said = kept.iter().find(|(w, _, _)| w == word).unwrap().1;
        assert_eq!(verse::count(word, "en"), Some(said), "{word}");
    }

    // The frequent words count exactly: every word up to the last the table holds.
    let last = kept.iter().rposition(|(w, _, _)| tabled.contains(&w.as_str())).unwrap();
    for (word, said, _) in &kept[..=last] {
        assert_eq!(verse::count(word, "en"), Some(*said), "{word}");
    }

    // The rarer words, which the rules alone count: at least 92% within a way CMUdict says them.
    let rarer = &kept[last + 1..];
    assert!(rarer.len() >= 5_000, "{} rarer words", rarer.len());
    let right = rarer
        .iter()
        .filter(|(w, said, _)| verse::count(w, "en").is_some_and(|c| c.low <= said.high && said.low <= c.high))
        .count();
    let share = right as f64 / rarer.len() as f64;
    assert!(share >= 0.92, "{right} of {} rarer words ({share:.4})", rarer.len());
}

#[test]
fn a_word_said_two_ways_counts_either() {
    assert_eq!(en("fire"), Some((1, 2)));
    assert_eq!(en("every"), Some((2, 3)));
    assert_eq!(en("every fire"), Some((3, 5)));
    assert_eq!(Count { low: 1, high: 2 }.distance(2), 0);
    assert_eq!(Count { low: 1, high: 2 }.distance(5), 3);
    assert_eq!(Count { low: 6, high: 7 }.distance(5), 1);
}

#[test]
fn capitals_digits_apostrophes_hyphens_and_accents() {
    // Capitals of five letters or fewer are read letter by letter; W is three.
    assert_eq!(en("NASA"), Some((4, 4)));
    assert_eq!(en("the WWW"), Some((10, 10)));
    assert_eq!(en("I"), Some((1, 1)));
    assert_eq!(en("BEAUTIFUL"), en("beautiful"));
    assert_eq!(en("Pond"), Some((1, 1)));
    // A digit leaves the line uncounted.
    assert_eq!(en("in 1999"), None);
    assert_eq!(en("4th of July"), None);
    // An apostrophe joins a word, straight or curled.
    assert_eq!(en("don't"), en("dont"));
    assert_eq!(en("don’t"), en("dont"));
    assert_eq!(en("the river's mouth"), Some((4, 4)));
    // A hyphen and punctuation part words.
    assert_eq!(en("cherry-blossom"), Some((4, 4)));
    assert_eq!(en("snow — and then, silence."), en("snow and then silence"));
    // A letter with a mark counts as its base.
    assert_eq!(en("café"), en("cafe"));
    assert_eq!(en("naïve"), en("naive"));
    assert_eq!(en("Œuvre"), en("oeuvre"));
    // A letter of another script leaves the line uncounted.
    assert_eq!(en("the old pond 古池"), None);
    assert_eq!(en("αβγ"), None);
    assert_eq!(en(""), Some((0, 0)));
}

#[test]
fn kana_count_as_morae() {
    assert_eq!(verse::morae("とうきょう"), Some(4));
    assert_eq!(verse::morae("にっぽん"), Some(4));
    assert_eq!(verse::morae("ふるいけや"), Some(5));
    assert_eq!(verse::morae("かわずとびこむ"), Some(7));
    assert_eq!(verse::morae("みずのおと"), Some(5));
    assert_eq!(verse::morae("コーヒー"), Some(4));
    assert_eq!(verse::morae("ファイル"), Some(3));
    assert_eq!(verse::morae("ふるい　いけ、"), Some(5));
    assert_eq!(verse::morae("古池や"), None);
    assert_eq!(verse::count("古池や", "ja"), None);
    assert_eq!(verse::count("ふるいけや", "ja-JP"), Some(Count { low: 5, high: 5 }));
    let poem = verse::check("ふるいけや\nかわずとびこむ\nみずのおと", "ja");
    assert_eq!(poem.kept, Kept::Kept);
}

#[test]
fn every_form_of_check() {
    let kept = verse::check("An old silent pond\nA frog jumps into the pond\nSplash! Silence again", "en");
    assert_eq!(kept.kept, Kept::Kept, "{kept:?}");
    assert_eq!(kept.distance(), Some(0));
    assert_eq!(kept.lines.iter().map(|l| l.target).collect::<Vec<_>>(), vec![Some(5), Some(7), Some(5)]);

    // Blank lines are left out, and each line is trimmed.
    let spaced = verse::check("\n  An old silent pond  \n\nA frog jumps into the pond\nSplash! Silence again\n", "en");
    assert_eq!(spaced.kept, Kept::Kept);
    assert_eq!(spaced.lines[0].text, "An old silent pond");

    let near = verse::check("An old silent pond\nA green frog jumps into the pond\nSplash! Silence again", "en");
    assert_eq!(near.kept, Kept::Near);
    assert_eq!(near.distance(), Some(1));
    assert_eq!(near.lines[1].count, Some(Count { low: 8, high: 8 }));

    let off = verse::check("Pond\nA frog jumps into the pond and is gone\nSplash", "en");
    assert_eq!(off.kept, Kept::Off);

    let four = verse::check("one\ntwo\nthree\nfour", "en");
    assert_eq!(four.kept, Kept::Off);
    assert_eq!(four.distance(), None);
    assert_eq!(four.lines[3].target, None);

    assert_eq!(verse::check("in 1999\na frog\nsplash", "en").kept, Kept::Uncounted);
    assert_eq!(verse::check("Un vieil étang\nUne grenouille saute\nLe bruit de l'eau", "fr").kept, Kept::Uncounted);
    assert_eq!(verse::check("", "en").kept, Kept::Uncounted);
    assert_eq!(verse::check("古池や\n蛙飛び込む\n水の音", "ja").kept, Kept::Uncounted);
}

#[test]
fn counting_is_total_on_any_text() {
    // Seeded text from every script: Latin, its marks, Greek, Cyrillic, Arabic, Devanagari, Han,
    // kana, Hangul, emoji, combining marks, controls, and punctuation.
    let ranges: [(u32, u32); 14] = [
        (0x20, 0x7E),
        (0xA0, 0x24F),
        (0x300, 0x36F),
        (0x370, 0x3FF),
        (0x400, 0x4FF),
        (0x600, 0x6FF),
        (0x900, 0x97F),
        (0x2000, 0x206F),
        (0x3000, 0x30FF),
        (0x4E00, 0x4F00),
        (0xAC00, 0xAD00),
        (0xFF00, 0xFFEF),
        (0x1F300, 0x1F64F),
        (0x0, 0x1F),
    ];
    let mut seed: u64 = 0x5CAE_7A00;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as u32
    };
    for _ in 0..4_000 {
        let len = next() % 120;
        let mut text = String::new();
        for _ in 0..len {
            let (low, high) = ranges[(next() % ranges.len() as u32) as usize];
            if let Some(c) = char::from_u32(low + next() % (high - low + 1)) {
                text.push(c);
            }
            if next() % 7 == 0 {
                text.push(if next() % 2 == 0 { '\n' } else { ' ' });
            }
        }
        for lang in ["en", "ja", "fr", "", "EN-gb"] {
            let _ = verse::count(&text, lang);
            let checked = verse::check(&text, lang);
            let _ = checked.distance();
        }
        let _ = verse::rules(text.as_bytes());
    }
    // A word far longer than any.
    let long = "a".repeat(10_000) + &"bcd".repeat(5_000);
    assert!(verse::count(&long, "en").is_some());
}

/// I404 (PLAN O.4h): a verse text of three lines, in English or in kana alone, off 5-7-5; one
/// finding per text, at the first state that shows it, with its counts and no fix.
#[test]
fn i404_judges_a_haiku_in_english_or_kana_alone() {
    use scaena_core::Deck;
    use scaena_core::model::theme::Theme;
    let theme: Theme = serde_json::from_str(include_str!("../../../tests/lint/theme.json")).unwrap();
    let lint = |lang: &str, nodes: serde_json::Value| {
        let ids: Vec<String> = nodes.as_object().unwrap().keys().cloned().collect();
        let states: Vec<serde_json::Value> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| serde_json::json!({ "id": format!("s{i}"), "mode": "absolute", "layout": "full", "props": { id: {} } }))
            .chain([serde_json::json!({ "id": "again", "layout": "full" })])
            .collect();
        let deck = serde_json::json!({
            "scaena": scaena_core::FORMAT_VERSION, "meta": { "lang": lang },
            "canvas": { "width": 1920, "height": 1080 }, "theme": "theme.json",
            "nodes": nodes, "states": states,
        });
        let deck = Deck::from_json(&deck.to_string()).unwrap();
        scaena_core::lint::lint_document(&deck, Some(&theme))
            .into_iter()
            .filter(|f| f.code == "I404")
            .map(|f| (f.node.unwrap(), f.state.unwrap(), f.message, f.measure.unwrap(), f.fix.is_none()))
            .collect::<Vec<_>>()
    };
    let verse =
        |text: &str| serde_json::json!({ "type": "text", "role": "verse", "text": text, "at": { "in": "main" } });
    let found = lint(
        "en-US",
        serde_json::json!({
            "off": verse("An old silent pond\nA green frog jumps into the pond\nSplash! Silence again"),
            "kept": verse("An old silent pond\nA frog jumps into the pond\nSplash! Silence again"),
            "four": verse("one\ntwo\nthree\nfour"),
            "kanji": verse("古池や\n蛙飛び込む\n水の音"),
            "kana": verse("ふるいけや\nかわずとびこむよ\nみずのおと"),
            "body": { "type": "text", "role": "body", "text": "Pond\nFrog\nSplash", "at": { "in": "main" } },
        }),
    );
    let said: Vec<(&str, &str, &str)> =
        found.iter().map(|(n, s, m, _, _)| (n.as_str(), s.as_str(), m.as_str())).collect();
    assert_eq!(
        said,
        [
            ("off", "s0", "This haiku's lines count 5, 8, and 5, not 5, 7, and 5."),
            ("kana", "s4", "This haiku's lines count 5, 8, and 5, not 5, 7, and 5."),
        ]
    );
    assert_eq!(found[0].3["counts"], serde_json::json!([[5, 5], [8, 8], [5, 5]]));
    assert_eq!(found[1].3["unit"], "morae");
    assert!(found.iter().all(|f| f.4), "no fix");
    // In another language, English words are not judged; kana alone still are.
    let found = lint(
        "fr",
        serde_json::json!({ "off": verse("An old silent pond\nA green frog jumps into the pond\nSplash! Silence again") }),
    );
    assert!(found.is_empty(), "{found:?}");
    // A text in a slot whose role is verse is judged as one in the role.
    let mut slotted = verse("An old silent pond\nA green frog jumps into the pond\nSplash! Silence again");
    slotted.as_object_mut().unwrap().remove("role");
    let mut theme_slot: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/lint/theme.json")).unwrap();
    theme_slot["layouts"]["full"]["slots"]["main"]["role"] = serde_json::json!("verse");
    let theme: Theme = serde_json::from_value(theme_slot).unwrap();
    let deck = serde_json::json!({
        "scaena": scaena_core::FORMAT_VERSION, "meta": { "lang": "en" },
        "canvas": { "width": 1920, "height": 1080 }, "theme": "theme.json",
        "nodes": { "p": slotted }, "states": [{ "id": "s", "layout": "full", "props": { "p": {} } }],
    });
    let deck = Deck::from_json(&deck.to_string()).unwrap();
    assert!(scaena_core::lint::lint_document(&deck, Some(&theme)).iter().any(|f| f.code == "I404"));
}
