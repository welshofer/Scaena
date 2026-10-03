//! Single-file HTML (PLAN 2.5; SPEC §10): a deck that plays from one file, offline.
//!
//! - The page is the web player's, built by `just web` from `web/standalone.html` with its
//!   code, its styles, and the engine (the player's module alone, gzipped) inside it. A
//!   `scaena` built after it carries it ([`player`]); one built before cannot export HTML.
//! - [`html`] fills it in with a bundle: its title and language, every file the player
//!   reads (each gzipped, in base64, by its path in the bundle), the states it plays, and
//!   how each reads.
//! - How a state reads is [`reading`]'s HTML, from the same data a tagged PDF is built from
//!   (SPEC §3.12): each node the state shows, in paint order, as a heading, a paragraph, a
//!   figure with its alt text, or a table by rows of header and data cells. The page shows
//!   the state's reading, unseen, in a live region, so a screen reader says what each state
//!   changes, and reads the slide on request.
//!
//! The file asks nothing of the network: its page's content security policy lets nothing
//! load, and nothing in it is anywhere else.

use crate::ExportError;
use crate::reading::{self, Kind, Reading};
use base64::Engine as _;
use flate2::Compression;
use flate2::write::GzEncoder;
use scaena_core::displaylist::{DisplayList, Op};
use scaena_core::{Deck, Snapshot};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
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

/// How the state `snap` resolves, drawn as `list` at rest, reads (SPEC §3.12), as HTML:
/// each node it shows that is read, in paint order, an element of its own that names it
/// (`data-node`). A heading (`h1`, `h2`) or paragraph says the node's text, or its alt text
/// instead; a figure (`role="img"`) is named by its alt text; a table reads by rows, its
/// header row's cells headers, with a cell for every column of every row. A group's members
/// read in turn, or as one figure when it has alt text. What no node reads (decoration, a
/// container's panel, the background) is not there.
pub fn reading(deck: &Deck, snap: &Snapshot, list: &DisplayList) -> String {
    let readings = reading::readings(deck, snap);
    let lang = deck.meta.as_ref().and_then(|m| m.lang.as_deref());
    let mut out = String::new();
    Reader { snap, readings: &readings, lang }.read(&list.ops, &mut out);
    out
}

struct Reader<'a> {
    snap: &'a Snapshot,
    readings: &'a HashMap<String, Reading>,
    /// The deck's language: a node in another says so.
    lang: Option<&'a str>,
}

impl Reader<'_> {
    fn read(&self, ops: &[Op], out: &mut String) {
        for op in ops {
            let Op::Layer { node, ops: inner, .. } = op else { continue };
            let Some(id) = node.as_deref() else {
                self.read(inner, out);
                continue;
            };
            let reading = self.readings.get(id);
            let alt = reading.and_then(|r| r.alt.as_deref());
            match reading.map_or(Kind::Group, |r| r.kind) {
                Kind::Artifact => {}
                Kind::Heading(level) => self.text(&format!("h{level}"), id, reading, out),
                Kind::Paragraph => self.text("p", id, reading, out),
                Kind::Table => self.table(id, reading, inner, out),
                Kind::Group if alt.is_none() => self.read(inner, out),
                Kind::Figure | Kind::Group => {
                    let named = alt.map(|alt| format!(r#" aria-label="{}""#, attr(alt))).unwrap_or_default();
                    let _ =
                        write!(out, r#"<div role="img" data-node="{}"{named}{}></div>"#, attr(id), self.lang(reading));
                }
            }
        }
    }

    /// A text node's element: its words as the deck writes them (its `text`, or its runs'),
    /// or its alt text instead. One with none reads nothing.
    fn text(&self, tag: &str, id: &str, reading: Option<&Reading>, out: &mut String) {
        let words = match reading.and_then(|r| r.alt.clone()) {
            Some(alt) => alt,
            None => self.snap.nodes.get(id).map(words).unwrap_or_default(),
        };
        if words.trim().is_empty() {
            return;
        }
        let _ = write!(out, r#"<{tag} data-node="{}"{}>{}</{tag}>"#, attr(id), self.lang(reading), text(&words));
    }

    /// A table by rows of cells, each with the text it draws: the first row's headers of
    /// their columns. A row with no text in a column (a null) has an empty cell there.
    fn table(&self, id: &str, reading: Option<&Reading>, ops: &[Op], out: &mut String) {
        let mut cells: BTreeMap<(u32, u32), String> = BTreeMap::new();
        for op in ops {
            if let Op::Layer { cell: Some([row, column]), ops, .. } = op {
                cells.insert((*row, *column), drawn(ops));
            }
        }
        let named = reading
            .and_then(|r| r.alt.as_deref())
            .map(|alt| format!(r#" aria-label="{}""#, attr(alt)))
            .unwrap_or_default();
        let _ = write!(out, r#"<table data-node="{}"{named}{}>"#, attr(id), self.lang(reading));
        let columns = cells.keys().map(|&(_, c)| c + 1).max().unwrap_or(0);
        if let (Some(&(first, _)), Some(&(last, _))) = (cells.keys().next(), cells.keys().next_back()) {
            for row in first..=last {
                out.push_str("<tr>");
                for column in 0..columns {
                    let said = cells.get(&(row, column)).map(|t| text(t)).unwrap_or_default();
                    if row == 0 {
                        let _ = write!(out, r#"<th scope="col">{said}</th>"#);
                    } else {
                        let _ = write!(out, "<td>{said}</td>");
                    }
                }
                out.push_str("</tr>");
            }
        }
        out.push_str("</table>");
    }

    /// ` lang="…"` for a node in another language than the deck's.
    fn lang(&self, reading: Option<&Reading>) -> String {
        match reading.and_then(|r| r.lang.as_deref()) {
            Some(lang) if Some(lang) != self.lang => format!(r#" lang="{}""#, attr(lang)),
            _ => String::new(),
        }
    }
}

/// A text node's words: its `text`, or its runs' texts in turn.
fn words(props: &scaena_core::document::Props) -> String {
    match (props.get("text"), props.get("runs")) {
        (Some(Value::String(text)), _) => text.clone(),
        (_, Some(Value::Array(runs))) => runs.iter().filter_map(|r| r.get("text")?.as_str()).collect(),
        _ => String::new(),
    }
}

/// The text `ops` draw: their glyph runs' in turn, a line break a space. A hyphen drawn at a
/// break is not said.
fn drawn(ops: &[Op]) -> String {
    fn walk(ops: &[Op], out: &mut String, line: &mut Option<f32>) {
        for op in ops {
            match op {
                Op::Layer { ops, .. } => walk(ops, out, line),
                Op::Glyphs { text, glyphs, .. } if text != "\u{AD}" => {
                    let y = glyphs.first().map(|g| g.y);
                    if line.is_some() && y.is_some() && y != *line && !out.ends_with(char::is_whitespace) {
                        out.push(' ');
                    }
                    out.push_str(text);
                    *line = glyphs.last().map(|g| g.y).or(*line);
                }
                _ => {}
            }
        }
    }
    let mut out = String::new();
    walk(ops, &mut out, &mut None);
    out.trim().to_string()
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
    use scaena_core::displaylist::{Blend, Color, FillRule, Glyph, IDENTITY, Paint, Path};
    use serde_json::json;
    use std::io::Read as _;

    fn layer(node: Option<&str>, cell: Option<[u32; 2]>, ops: Vec<Op>) -> Op {
        Op::Layer {
            node: node.map(str::to_string),
            cell,
            transform: IDENTITY,
            opacity: 1.0,
            blend: Blend::Normal,
            clip: None,
            ops,
        }
    }

    fn glyphs(text: &str, y: f32) -> Op {
        Op::Glyphs {
            font: 0,
            size: 20.0,
            coords: Vec::new(),
            paint: Paint::Solid(Color([0, 0, 0, 255])),
            text: text.to_string(),
            glyphs: vec![Glyph { id: 1, x: 0.0, y }],
            clusters: vec![0],
        }
    }

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
    fn a_state_reads_in_paint_order_as_its_nodes_do() {
        let deck = deck();
        let snaps = scaena_core::resolve_states(&deck).expect("snapshots");
        let mut list = DisplayList::new([1920.0, 1080.0]);
        list.ops = vec![
            // The background, drawn outside every node, reads nothing.
            Op::Fill {
                path: Path::rect([0.0, 0.0, 1920.0, 1080.0]),
                rule: FillRule::NonZero,
                paint: Paint::Solid(Color([0, 0, 0, 255])),
            },
            layer(Some("rule"), None, vec![]),
            layer(Some("title"), None, vec![glyphs("REVENUE & GROWTH", 100.0)]),
            layer(Some("pair"), None, vec![layer(Some("sub"), None, vec![]), layer(Some("fr"), None, vec![])]),
            layer(Some("x"), None, vec![]),
            layer(Some("rev"), None, vec![layer(None, None, vec![glyphs("Q1", 900.0)])]),
            layer(
                Some("table"),
                None,
                vec![
                    layer(None, Some([0, 0]), vec![glyphs("Region", 10.0)]),
                    layer(None, Some([0, 1]), vec![glyphs("Total", 10.0)]),
                    layer(None, Some([1, 0]), vec![glyphs("North", 40.0), glyphs("east", 70.0)]),
                    layer(None, Some([2, 0]), vec![glyphs("South", 100.0)]),
                    layer(
                        None,
                        Some([2, 1]),
                        vec![glyphs("pre", 130.0), glyphs("\u{AD}", 130.0), glyphs("sold", 160.0)],
                    ),
                ],
            ),
        ];
        let html = reading(&deck, &snaps[0], &list);
        assert_eq!(
            html,
            concat!(
                r#"<h1 data-node="title">Revenue &amp; growth</h1>"#,
                r#"<p data-node="sub">Up 12%</p>"#,
                r#"<p data-node="fr" lang="fr">Bonjour</p>"#,
                r#"<p data-node="x">four point two times</p>"#,
                r#"<div role="img" data-node="rev" aria-label="Revenue by quarter, &quot;up&quot;"></div>"#,
                r#"<table data-node="table"><tr><th scope="col">Region</th><th scope="col">Total</th></tr>"#,
                r#"<tr><td>North east</td><td></td></tr><tr><td>South</td><td>pre sold</td></tr></table>"#,
            )
        );
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
