//! How a deck reads (SPEC §3.12): what each node a state shows is to someone who hears
//! the deck rather than sees it. A tagged PDF is built from it (PLAN 1.20). So is [`html`],
//! how a state reads as HTML: a single-file export carries it (PLAN 2.5), and the web player
//! shows it in a live region for a screen reader (PLAN 2.8).

use crate::displaylist::{DisplayList, Op};
use crate::document::{NodeType, Props};
use crate::model::values::{ListItem, ListKind};
use crate::{Deck, Snapshot};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

/// What a node is to a reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A heading at its level, 1 the highest: text in the `display` or `headline` role
    /// (1), or `title` (2).
    Heading(u8),
    /// Any other text.
    Paragraph,
    /// A picture: an image, a chart, or a shape or shader that has alt text.
    Figure,
    /// A table, read cell by cell.
    Table,
    /// A group: its members read in turn, or as one figure when it has alt text.
    Group,
    /// Not read: decoration (`semantic: decoration`, alt text `""`, a shape or shader
    /// with no alt text, a container's panel; its children read on their own).
    Artifact,
}

/// How a node reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub kind: Kind,
    /// Alt text: what a picture shows, or what to say for a text instead of its words.
    pub alt: Option<String>,
    /// BCP 47, the node's `lang`, where it differs from the deck's.
    pub lang: Option<String>,
    /// A text's paragraphs as a list's items (ADR-0018), one for each paragraph; empty for a
    /// text that is no list, and for any other node.
    pub items: Vec<Option<ListItem>>,
}

/// How each node `snap` shows reads, by id.
pub fn readings(deck: &Deck, snap: &Snapshot) -> HashMap<String, Reading> {
    snap.nodes
        .iter()
        .filter_map(|(id, props)| Some((id.clone(), reading(deck.nodes.get(id)?.node_type, props))))
        .collect()
}

/// How a node of type `t` with these props reads.
pub fn reading(t: NodeType, props: &Props) -> Reading {
    let text = |k: &str| props.get(k).and_then(Value::as_str);
    let alt = text("alt");
    let decorative = text("semantic") == Some("decoration") || alt == Some("");
    let kind = match t {
        _ if decorative => Kind::Artifact,
        NodeType::Text => match text("role") {
            Some("display" | "headline") => Kind::Heading(1),
            Some("title") => Kind::Heading(2),
            _ => Kind::Paragraph,
        },
        NodeType::Image | NodeType::Chart => Kind::Figure,
        NodeType::Table => Kind::Table,
        NodeType::Shape | NodeType::Shader if alt.is_some() => Kind::Figure,
        NodeType::Shape | NodeType::Shader => Kind::Artifact,
        NodeType::Group => Kind::Group,
        NodeType::Stack | NodeType::Grid | NodeType::Frame => Kind::Artifact,
    };
    let items = match (kind, props.get("list").cloned().map(serde_json::from_value::<Vec<Option<ListItem>>>)) {
        (Kind::Heading(_) | Kind::Paragraph, Some(Ok(list))) if list.iter().any(Option::is_some) => {
            crate::lists::items(&list, crate::lists::paragraphs(&words(props)).len())
        }
        _ => Vec::new(),
    };
    let alt = alt.filter(|a| !a.is_empty()).map(str::to_string);
    Reading { kind, alt, lang: text("lang").map(str::to_string), items }
}

/// How the state `snap` resolves, drawn as `list` at rest, reads (SPEC §3.12), as HTML:
/// each node it shows that is read, in paint order, an element of its own that names it
/// (`data-node`). A heading (`h1`, `h2`) or paragraph says the node's text, or its alt text
/// instead; a figure (`role="img"`) is named by its alt text; a table reads by rows, its
/// header row's cells headers, with a cell for every column of every row. A group's members
/// read in turn, or as one figure when it has alt text. What no node reads (decoration, a
/// container's panel, the background) is not there.
pub fn html(deck: &Deck, snap: &Snapshot, list: &DisplayList) -> String {
    let readings = readings(deck, snap);
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
        let items = reading.filter(|r| r.alt.is_none()).map_or(&[][..], |r| &r.items[..]);
        if !items.is_empty() {
            let _ =
                write!(out, r#"<div data-node="{}"{}>{}</div>"#, attr(id), self.lang(reading), listed(&words, items));
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
/// `words`' paragraphs as HTML (ADR-0018): each item an `li` of a `ul` or an `ol`, nested by
/// level inside the item before it, and each paragraph that is no item a `p`.
fn listed(words: &str, items: &[Option<ListItem>]) -> String {
    let close = |kind: ListKind| if kind == ListKind::Bullet { "</li></ul>" } else { "</li></ol>" };
    let open = |kind: ListKind| if kind == ListKind::Bullet { "<ul><li>" } else { "<ol><li>" };
    let mut out = String::new();
    let mut open_lists: Vec<(u8, ListKind)> = Vec::new();
    for (range, item) in crate::lists::paragraphs(words).into_iter().zip(items) {
        let said = text(&words[range]);
        let Some(item) = item else {
            while let Some((_, kind)) = open_lists.pop() {
                out.push_str(close(kind));
            }
            let _ = write!(out, "<p>{said}</p>");
            continue;
        };
        let level = item.depth();
        while open_lists.last().is_some_and(|&(l, _)| l > level) {
            out.push_str(close(open_lists.pop().map(|(_, k)| k).unwrap_or(ListKind::Bullet)));
        }
        match open_lists.last() {
            Some(&(l, kind)) if l == level && kind == item.kind => out.push_str("</li><li>"),
            Some(&(l, kind)) if l == level => {
                out.push_str(close(kind));
                open_lists.pop();
                out.push_str(open(item.kind));
                open_lists.push((level, item.kind));
            }
            _ => {
                out.push_str(open(item.kind));
                open_lists.push((level, item.kind));
            }
        }
        out.push_str(&said);
    }
    while let Some((_, kind)) = open_lists.pop() {
        out.push_str(close(kind));
    }
    out
}

fn words(props: &Props) -> String {
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
    use crate::displaylist::{Blend, Color, FillRule, Glyph, IDENTITY, Paint, Path};
    use serde_json::json;

    #[test]
    fn a_list_reads_as_a_list() {
        let props = json!({ "role": "body", "text": "A\nB\nC\nD\nE",
            "list": [{ "kind": "bullet" }, { "kind": "number", "level": 1 }, { "kind": "number", "level": 1 }, { "kind": "bullet" }] });
        let r = read(NodeType::Text, props);
        assert_eq!(r.items.len(), 5);
        assert_eq!(
            listed("A\nB\nC\nD\nE", &r.items),
            "<ul><li>A<ol><li>B</li><li>C</li></ol></li><li>D</li></ul><p>E</p>"
        );
        // A text that is no list, or one whose list marks no paragraph, reads as a paragraph.
        assert!(read(NodeType::Text, json!({ "text": "A\nB", "list": [null] })).items.is_empty());
    }

    fn read(t: NodeType, props: Value) -> Reading {
        let Value::Object(map) = props else { panic!("an object") };
        reading(t, &map.into_iter().collect())
    }

    #[test]
    fn roles_make_headings_and_decoration_is_not_read() {
        assert_eq!(read(NodeType::Text, json!({ "role": "headline" })).kind, Kind::Heading(1));
        assert_eq!(read(NodeType::Text, json!({ "role": "display" })).kind, Kind::Heading(1));
        assert_eq!(read(NodeType::Text, json!({ "role": "title" })).kind, Kind::Heading(2));
        assert_eq!(read(NodeType::Text, json!({ "role": "body" })).kind, Kind::Paragraph);
        assert_eq!(read(NodeType::Text, json!({})).kind, Kind::Paragraph);
        // Navigation is read: a cover's title and an agenda are content.
        assert_eq!(read(NodeType::Text, json!({ "role": "display", "semantic": "navigation" })).kind, Kind::Heading(1));
        assert_eq!(read(NodeType::Text, json!({ "role": "headline", "semantic": "decoration" })).kind, Kind::Artifact);
        let numeral =
            read(NodeType::Text, json!({ "role": "numeral", "alt": "four point two times", "lang": "en-GB" }));
        assert_eq!(
            numeral,
            Reading {
                kind: Kind::Paragraph,
                alt: Some("four point two times".into()),
                lang: Some("en-GB".into()),
                items: Vec::new()
            }
        );
    }

    #[test]
    fn pictures_read_by_their_alt_text() {
        let chart = read(NodeType::Chart, json!({ "alt": "Revenue by quarter" }));
        assert_eq!((chart.kind, chart.alt.as_deref()), (Kind::Figure, Some("Revenue by quarter")));
        assert_eq!(read(NodeType::Image, json!({})).kind, Kind::Figure);
        assert_eq!(read(NodeType::Image, json!({ "alt": "" })).kind, Kind::Artifact);
        assert_eq!(read(NodeType::Shape, json!({})).kind, Kind::Artifact);
        assert_eq!(read(NodeType::Shape, json!({ "alt": "An arrow from plan to build" })).kind, Kind::Figure);
        assert_eq!(read(NodeType::Shader, json!({ "alt": "" })).kind, Kind::Artifact);
        assert_eq!(read(NodeType::Table, json!({})).kind, Kind::Table);
        assert_eq!(read(NodeType::Group, json!({})).kind, Kind::Group);
        assert_eq!(read(NodeType::Stack, json!({ "fill": "surface" })).kind, Kind::Artifact);
    }

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
                "scaena": crate::FORMAT_VERSION,
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
        let snaps = crate::resolve_states(&deck).expect("snapshots");
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
        let read = html(&deck, &snaps[0], &list);
        assert_eq!(
            read,
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
}
