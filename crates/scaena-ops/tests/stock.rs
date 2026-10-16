//! A price chart made, refreshed, and re-ranged (ADR-0030, PLAN O.3c), on series made by hand: the
//! series kept as a CSV under `data/`, the chart in the slide's empty place for one, the change
//! quoted into the headline and the caption there, and each write one patch with the file.

use scaena_core::fetched::Fetched;
use scaena_ops::stock::{Aim, Close, Series, refresh, stock, stocking};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const EXAMPLES: &str = "../../docs/examples";

/// The revenue example in a bundle of its own, with what it names, and a slide in the theme's
/// Chart layout with nothing on it yet, in the first beat.
fn bundle(test: &str, lang: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("stock-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    let mut deck: Value =
        serde_json::from_slice(&std::fs::read(format!("{EXAMPLES}/revenue.deck.json")).unwrap()).unwrap();
    let mut files: Vec<String> =
        deck["fonts"].as_array().unwrap().iter().map(|f| f["file"].as_str().unwrap().into()).collect();
    files.push(deck["theme"].as_str().unwrap().into());
    for source in deck["data"].as_object().unwrap().values() {
        if let Some(path) = source["source"].as_str() {
            files.push(path.into());
        }
    }
    for f in files {
        let to = dir.join(&f);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(format!("{EXAMPLES}/{f}"), to).unwrap();
    }
    deck["meta"]["lang"] = json!(lang);
    deck["states"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "price", "mode": "absolute", "layout": "figure", "props": {} }));
    deck["spine"]["sections"][0]["beats"][0]["states"].as_array_mut().unwrap().push(json!("price"));
    std::fs::write(dir.join("deck.json"), serde_json::to_vec_pretty(&deck).unwrap()).unwrap();
    dir
}

/// A series of `symbol` made by hand: `range`'s closes, rising from `from` to `to`, counted from
/// `base`.
fn series(symbol: &str, currency: &str, range: &str, from: f64, to: f64, base: f64) -> Series {
    let times: Vec<String> = match range {
        "1d" => (0..13).map(|i| format!("2026-10-15T{:02}:{:02}", 10 + (i * 30) / 60, (i * 30) % 60)).collect(),
        _ => (1..=20).map(|d| format!("2026-09-{d:02}")).collect(),
    };
    let n = times.len() as f64 - 1.0;
    let rows = (times.iter().enumerate())
        .map(|(i, t)| Close {
            time: t.clone(),
            close: ((from + (to - from) * i as f64 / n) * 100.0).round() / 100.0,
            base,
        })
        .collect::<Vec<_>>();
    let through = rows.last().unwrap().time.clone();
    let fetched: Fetched = serde_json::from_value(json!({
        "from": "twelve-data", "at": "2026-10-15T20:15:02Z", "asked": symbol,
        "credit": [{ "text": "Data provided by Twelve Data", "link": "https://twelvedata.com" }],
        "stock": { "symbol": symbol, "range": range, "exchange": "NASDAQ", "currency": currency,
                   "zone": "America/New_York", "through": through }
    }))
    .unwrap();
    Series { fetched, rows }
}

/// The words a text node's runs say.
fn said(deck: &scaena_core::Deck, id: &str) -> String {
    deck.nodes[id].props["runs"].as_array().unwrap().iter().map(|r| r["text"].as_str().unwrap()).collect()
}

fn errors(b: &scaena_ops::Bundle) -> Vec<String> {
    let linted = scaena_ops::lint::lint(b).unwrap();
    (linted.findings.iter())
        .filter(|f| f.severity == scaena_core::Severity::Error)
        .map(|f| format!("{} {}", f.code, f.message))
        .collect()
}

#[test]
fn a_price_chart_fills_the_slides_places_and_quotes_its_change() {
    let dir = bundle("make", "en-US");
    let b = scaena_ops::open(&dir).unwrap();
    let aim = Aim { state: Some("price".into()), at: None };
    let made = stock(&b, &aim, &series("ACME", "USD", "3m", 180.0, 195.8, 182.4)).unwrap();
    assert!(made.patched.applied, "{:#?}", made.patched);
    assert_eq!((made.source.as_str(), made.file.as_str()), ("acme", "data/acme.csv"));
    assert_eq!(made.said, "Price chart of ACME added");
    let b = scaena_ops::open(&dir).unwrap();
    let csv = String::from_utf8(b.read("data/acme.csv").unwrap()).unwrap();
    assert!(csv.starts_with("time,close,base\n2026-09-01,180,182.4\n"), "{csv}");
    assert_eq!(b.deck.data["acme"].fetched().unwrap().stock.unwrap().symbol, "ACME");
    let chart = &b.deck.nodes[made.chart.as_deref().unwrap()].props;
    assert_eq!(chart["at"], json!({ "in": "main" }), "in the slide's empty place for a chart");
    assert_eq!(chart["annotations"], json!([{ "kind": "highlight", "at": {} }]), "lit, with no rule for three months");
    assert_eq!(chart["y"]["format"], "$,.2f");
    assert_eq!(said(&b.deck, "acme-claim"), "ACME is up 7.3% over three months");
    assert_eq!(b.deck.nodes["acme-claim"].props["at"], json!({ "in": "header" }));
    assert_eq!(said(&b.deck, "acme-period"), "Three months to Sep 20, 2026");
    let beat = &b.deck.spine.as_ref().unwrap().sections[0].beats[0];
    assert!(beat.evidence.contains(&"@acme".to_string()), "{:?}", beat.evidence);
    // It stays on its slide, and lints with no error.
    let snaps = scaena_core::resolve_states(&b.deck).unwrap();
    assert!(snaps.iter().filter(|s| s.nodes.contains_key("acme-chart")).all(|s| s.state_id == "price"));
    assert_eq!(errors(&b), Vec::<String>::new());
    // A blank slide with no place for one asks for a box.
    let err =
        stocking(&b, &Aim { state: Some("revenue".into()), at: None }, &series("ACME", "USD", "3m", 1.0, 2.0, 1.0))
            .unwrap_err();
    assert!(err.message.contains("no empty place for a chart"), "{err}");
}

#[test]
fn a_refresh_sets_the_figures_again_and_keeps_a_headline_a_person_rewrote() {
    let dir = bundle("refresh", "en-US");
    let b = scaena_ops::open(&dir).unwrap();
    stock(&b, &Aim { state: Some("price".into()), at: None }, &series("ACME", "USD", "3m", 180.0, 195.8, 182.4))
        .unwrap();
    // A person rewrites the caption in words of their own.
    let b = scaena_ops::open(&dir).unwrap();
    let typed =
        json!([{ "op": "replace", "path": "/nodes/acme-period/runs", "value": [{ "text": "Since the summer" }] }]);
    assert!(scaena_ops::patch::patch(&b, &typed, false).unwrap().applied);
    let b = scaena_ops::open(&dir).unwrap();
    let again = refresh(&b, "acme", &series("ACME", "USD", "3m", 180.0, 170.0, 182.4)).unwrap();
    assert!(again.patched.applied, "{:#?}", again.patched);
    assert_eq!(again.said, "Prices of ACME fetched again");
    let b = scaena_ops::open(&dir).unwrap();
    assert_eq!(said(&b.deck, "acme-claim"), "ACME is down 6.8% over three months");
    assert_eq!(said(&b.deck, "acme-period"), "Since the summer", "a person's own words stay");
    assert!(String::from_utf8(b.read("data/acme.csv").unwrap()).unwrap().contains(",170,182.4\n"));
    // Another symbol is refused, nothing written.
    let err = refresh(&b, "acme", &series("BETA", "USD", "3m", 1.0, 2.0, 1.0)).unwrap_err();
    assert!(err.message.contains("holds ACME"), "{err}");
}

#[test]
fn three_months_to_a_day_adds_the_rule_at_the_previous_close_and_back() {
    let dir = bundle("range", "en-US");
    let b = scaena_ops::open(&dir).unwrap();
    stock(&b, &Aim { state: Some("price".into()), at: None }, &series("ACME", "USD", "3m", 180.0, 195.8, 182.4))
        .unwrap();
    let b = scaena_ops::open(&dir).unwrap();
    let day = refresh(&b, "acme", &series("ACME", "USD", "1d", 194.0, 196.1, 195.8)).unwrap();
    assert!(day.patched.applied, "{:#?}", day.patched);
    assert_eq!(day.said, "Prices of ACME fetched again over today");
    let b = scaena_ops::open(&dir).unwrap();
    assert_eq!(
        b.deck.nodes["acme-chart"].props["annotations"],
        json!([{ "kind": "highlight", "at": {} }, { "kind": "rule", "at": { "y": 195.8 }, "text": "Previous close" }])
    );
    assert_eq!(said(&b.deck, "acme-claim"), "ACME is up 0.2% today");
    assert_eq!(said(&b.deck, "acme-period"), "The day to Oct 15, 2026");
    assert_eq!(errors(&b), Vec::<String>::new());
    let back = refresh(&b, "acme", &series("ACME", "USD", "3m", 180.0, 195.8, 182.4)).unwrap();
    assert!(back.patched.applied);
    let b = scaena_ops::open(&dir).unwrap();
    assert_eq!(b.deck.nodes["acme-chart"].props["annotations"], json!([{ "kind": "highlight", "at": {} }]));
    assert_eq!(said(&b.deck, "acme-claim"), "ACME is up 7.3% over three months");
}

#[test]
fn a_stock_in_euros_prints_no_dollar_and_the_bundle_holds_no_key() {
    let dir = bundle("euros", "en-US");
    let b = scaena_ops::open(&dir).unwrap();
    let made =
        stock(&b, &Aim { state: Some("price".into()), at: None }, &series("SAP", "EUR", "1y", 150.0, 165.0, 150.0))
            .unwrap();
    assert!(made.patched.applied);
    let b = scaena_ops::open(&dir).unwrap();
    assert_eq!(b.deck.nodes["sap-chart"].props["y"]["format"], ",.2f");
    for (id, node) in b.deck.nodes.iter().filter(|(id, _)| id.starts_with("sap")) {
        assert!(!serde_json::to_string(node).unwrap().contains('$'), "no dollar sign in `{id}`");
    }
    for entry in walk(&dir) {
        let bytes = std::fs::read(&entry).unwrap();
        let text = String::from_utf8_lossy(&bytes).to_lowercase();
        assert!(!text.contains("apikey") && !text.contains("authorization"), "{}", entry.display());
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}
