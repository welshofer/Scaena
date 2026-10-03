//! How a deck reads (SPEC §3.12): what each node a state shows is to someone who hears
//! the deck rather than sees it. A tagged PDF is built from it (PLAN 1.20), and so is what
//! a single-file HTML export's screen reader reads (PLAN 2.5, [`crate::html::reading`]).

use scaena_core::document::{NodeType, Props};
use scaena_core::{Deck, Snapshot};
use serde_json::Value;
use std::collections::HashMap;

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
    Reading { kind, alt: alt.filter(|a| !a.is_empty()).map(str::to_string), lang: text("lang").map(str::to_string) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
            Reading { kind: Kind::Paragraph, alt: Some("four point two times".into()), lang: Some("en-GB".into()) }
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
}
