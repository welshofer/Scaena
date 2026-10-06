//! Single-file HTML (PLAN 2.5; SPEC §10): a deck that plays from one file, offline.
//!
//! - The page is the web player's, built by `just web` from `web/standalone.html` with its
//!   code, its styles, and the engine (the player's module alone, gzipped) inside it. A
//!   `scaena` built after it carries it ([`player`]); one built before cannot export HTML.
//! - [`html`] fills it in with a bundle: its title and language, every file the player
//!   reads (each gzipped, in base64, by its path in the bundle), the states it plays, and
//!   how each reads.
//! - How a state reads is [`scaena_core::reading::html`], from the same data a tagged PDF is built from
//!   (SPEC §3.12): each node the state shows, in paint order, as a heading, a paragraph, a
//!   figure with its alt text, or a table by rows of header and data cells. The page shows
//!   the state's reading, unseen, in a live region, so a screen reader says what each state
//!   changes, and reads the slide on request.
//!
//! The file asks nothing of the network: its page's content security policy lets nothing
//! load, and nothing in it is anywhere else.

use crate::ExportError;
use base64::Engine as _;
use flate2::Compression;
use flate2::write::GzEncoder;
use scaena_core::Deck;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;

/// The page `just web` builds for a single-file export, if it was built before this crate
/// (`crates/scaena-export/player/standalone.html`): the web player, its code, and the
/// engine's player module.
#[cfg(player)]
pub fn player() -> Option<&'static str> {
    Some(include_str!("../player/standalone.html"))
}

/// The page `just web` builds for a single-file export: none, as it was not built before this
/// crate. `export --format html` says to build it, then this crate again.
#[cfg(not(player))]
pub fn player() -> Option<&'static str> {
    None
}

/// What a single-file export holds.
#[derive(Debug, Clone, Copy)]
pub struct Standalone<'a> {
    pub deck: &'a Deck,
    /// The bundle's name: its directory's, or its deck file's.
    pub name: &'a str,
    /// Every file the player reads, by its path in the bundle: the deck, its theme, its
    /// fonts, images, and data, and their licenses.
    pub files: &'a BTreeMap<String, Vec<u8>>,
    /// The states it plays, in order, each with how it reads ([`reading`]).
    pub states: &'a [(String, String)],
}

/// Where the page takes what the export fills in.
const LANG: &str = r#" lang="__SCAENA_LANG__""#;
const TITLE: &str = "__SCAENA_TITLE__";
const DECK: &str = "<!--__SCAENA_DECK__-->";

/// `page`, the built player ([`player`]), filled in with `standalone`: one HTML file.
pub fn html(page: &str, standalone: &Standalone) -> Result<String, ExportError> {
    for marker in [LANG, TITLE, DECK] {
        if page.matches(marker).count() != 1 {
            return Err(ExportError::Html(format!(
                "the player's page has no single `{marker}`: build it again with `just web`, then `scaena`"
            )));
        }
    }
    let meta = standalone.deck.meta.as_ref();
    let lang = meta.and_then(|m| m.lang.as_deref()).map(|l| format!(r#" lang="{}""#, attr(l))).unwrap_or_default();
    let title = meta.and_then(|m| m.title.as_deref()).unwrap_or(standalone.name);

    let mut deck = String::new();
    let states: Vec<&str> = standalone.states.iter().map(|(id, _)| id.as_str()).collect();
    let config = serde_json::json!({ "name": standalone.name, "states": states });
    // `<` written as an escape: nothing in it ends the script.
    let config = serde_json::to_string(&config).map_err(|e| ExportError::Html(e.to_string()))?.replace('<', "\\u003c");
    let _ = write!(deck, r#"<script type="application/json" id="scaena-deck">{config}</script>"#);
    for (path, bytes) in standalone.files {
        let packed = base64::engine::general_purpose::STANDARD.encode(gzip(bytes)?);
        let _ = write!(
            deck,
            "\n    <script type=\"application/octet-stream\" data-path=\"{}\">{packed}</script>",
            attr(path)
        );
    }
    for (state, reads) in standalone.states {
        let _ = write!(deck, "\n    <template data-state=\"{}\">{reads}</template>", attr(state));
    }
    // Each marker where the page has it: what fills one never meets another.
    let mut fills = [(LANG, lang), (TITLE, text(title)), (DECK, deck)]
        .map(|(marker, fill)| (page.find(marker).expect("counted above"), marker, fill));
    fills.sort_by_key(|(at, ..)| *at);
    let mut out = String::with_capacity(page.len() + fills.iter().map(|(.., fill)| fill.len()).sum::<usize>());
    let mut from = 0;
    for (at, marker, fill) in fills {
        out.push_str(&page[from..at]);
        out.push_str(&fill);
        from = at + marker.len();
    }
    out.push_str(&page[from..]);
    Ok(out)
}

/// `bytes`, gzipped: the same bytes every time, the header naming no time and no file.
fn gzip(bytes: &[u8]) -> Result<Vec<u8>, ExportError> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::best());
    gz.write_all(bytes).and_then(|()| gz.finish()).map_err(|e| ExportError::Html(e.to_string()))
}

/// `s` as HTML text.
fn text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// `s` as an HTML attribute's value, in double quotes.
fn attr(s: &str) -> String {
    text(s).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use serde_json::json;
    use std::io::Read as _;

    fn deck() -> Deck {
        Deck::from_json(
            &json!({
                "scaena": scaena_core::FORMAT_VERSION,
                "meta": { "title": "Q3 <Review>", "lang": "en-US" },
                "canvas": { "width": 1920, "height": 1080 },
                "theme": "theme.json",
                "nodes": {
                    "title": { "type": "text", "role": "headline", "text": "Revenue & growth" },
                    "sub": { "type": "text", "role": "body", "runs": [{ "text": "Up " }, { "text": "12%", "emphasis": "high" }] },
                    "fr": { "type": "text", "role": "body", "text": "Bonjour", "lang": "fr" },
                    "x": { "type": "text", "role": "numeral", "text": "4.2×", "alt": "four point two times" },
                    "rule": { "type": "shape", "shape": "line" },
                    "rev": { "type": "chart", "alt": "Revenue by quarter, \"up\"" },
                    "pair": { "type": "group" },
                    "table": { "type": "table" }
                },
                "states": [{ "id": "one", "layout": "title", "props": {
                    "title": {}, "sub": {}, "fr": {}, "x": {}, "rule": {}, "rev": {}, "pair": {}, "table": {}
                } }]
            })
            .to_string(),
        )
        .expect("a deck")
    }

    #[test]
    fn the_page_takes_the_bundle_its_states_and_their_readings() {
        let deck = deck();
        let page = r#"<html lang="__SCAENA_LANG__"><title>__SCAENA_TITLE__</title><body><!--__SCAENA_DECK__--></body>"#;
        let files = BTreeMap::from([
            ("deck.json".to_string(), b"{\"a\": \"</script>\"}".to_vec()),
            ("fonts/a \"b\".ttf".to_string(), vec![0, 1, 2, 255]),
        ]);
        let states = [("one".to_string(), "<h1 data-node=\"t\">Hi</h1>".to_string())];
        let made = html(page, &Standalone { deck: &deck, name: "q3</script>", files: &files, states: &states })
            .expect("a page");
        assert!(made.starts_with(r#"<html lang="en-US"><title>Q3 &lt;Review&gt;</title>"#), "{made}");
        assert!(
            made.contains(
                r#"<script type="application/json" id="scaena-deck">{"name":"q3\u003c/script>","states":["one"]}</script>"#
            ),
            "{made}"
        );
        assert!(made.contains(r#"<template data-state="one"><h1 data-node="t">Hi</h1></template>"#));
        // Each file, gzipped, in base64, by its path.
        for (path, bytes) in &files {
            let at = format!(r#"data-path="{}">"#, attr(path));
            let start = made.find(&at).expect("the file") + at.len();
            let packed = &made[start..start + made[start..].find('<').expect("its end")];
            let gz = base64::engine::general_purpose::STANDARD.decode(packed).expect("base64");
            let mut unpacked = Vec::new();
            GzDecoder::new(gz.as_slice()).read_to_end(&mut unpacked).expect("gzip");
            assert_eq!(&unpacked, bytes);
        }
        assert_eq!(made.matches("</script>").count(), 3, "only the page's own scripts end: {made}");
        // The same bundle, the same bytes.
        assert_eq!(
            made,
            html(page, &Standalone { deck: &deck, name: "q3</script>", files: &files, states: &states }).unwrap()
        );
        let wrong = html("<html>", &Standalone { deck: &deck, name: "q3", files: &files, states: &states });
        assert!(wrong.unwrap_err().to_string().contains("just web"));
    }
}
