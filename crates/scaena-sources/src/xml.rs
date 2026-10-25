//! Just enough XML to read Office's parts: elements with their attributes and text, compared
//! by local name, as Vzestup's parser compares them, so a part written with any prefix reads
//! alike. No DTD, no namespaces resolved; entities and character references decoded.

/// An element: its qualified name, its attributes, and what it holds, in order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
}

/// A name without its prefix: `a:t` → `t`.
fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, l)| l)
}

impl Element {
    /// Its name without its prefix.
    pub fn local(&self) -> &str {
        local(&self.name)
    }

    /// The elements it holds directly.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    /// The first element it holds directly named `name`, by local name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.local() == name)
    }

    /// Each element it holds directly named `name`, by local name.
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.elements().filter(move |e| e.local() == name)
    }

    /// The element down the path of local names `path`.
    pub fn at(&self, path: &[&str]) -> Option<&Element> {
        path.iter().try_fold(self, |e, name| e.child(name))
    }

    /// Each element under it named `name`, at any depth, in document order.
    pub fn descendants<'a>(&'a self, name: &'a str, out: &mut Vec<&'a Element>) {
        for e in self.elements() {
            if e.local() == name {
                out.push(e);
            }
            e.descendants(name, out);
        }
    }

    /// The attribute named `name`, by local name, or by its whole name (`r:id`).
    pub fn attr(&self, name: &str) -> Option<&str> {
        (self.attrs.iter())
            .find(|(k, _)| k == name)
            .or_else(|| self.attrs.iter().find(|(k, _)| local(k) == name))
            .map(|(_, v)| v.as_str())
    }

    /// The relationship it points to: its `r:id`, whatever the prefix.
    pub fn rel_id(&self) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k.contains(':') && local(k) == "id").map(|(_, v)| v.as_str())
    }

    /// The text it holds directly.
    pub fn text(&self) -> String {
        self.children
            .iter()
            .filter_map(|n| match n {
                Node::Text(t) => Some(t.as_str()),
                Node::Element(_) => None,
            })
            .collect()
    }
}

/// Why a part does not read as XML.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("not XML at byte {at}: {why}")]
pub struct XmlError {
    pub at: usize,
    pub why: &'static str,
}

/// How deep elements may nest in a part, at most. Office writes a few dozen; a part nested
/// deeper is refused, so that what walks the tree read (its readers, and dropping it) never
/// recurses past this, whatever a crafted file holds (the 20261024 plan's STAB-2).
pub const MAX_DEPTH: usize = 256;

/// The document element of `text`.
pub fn parse(text: &str) -> Result<Element, XmlError> {
    let mut p = Parser { s: text.as_bytes(), text, i: 0 };
    let mut stack: Vec<Element> = vec![Element::default()];
    while p.i < p.s.len() {
        if p.s[p.i] != b'<' {
            let start = p.i;
            while p.i < p.s.len() && p.s[p.i] != b'<' {
                p.i += 1;
            }
            let raw = &text[start..p.i];
            if stack.len() > 1 && !raw.is_empty() {
                stack.last_mut().expect("the root").children.push(Node::Text(decode(raw)));
            }
            continue;
        }
        if p.starts("<?") {
            p.skip_past("?>")?;
        } else if p.starts("<!--") {
            p.skip_past("-->")?;
        } else if p.starts("<![CDATA[") {
            let start = p.i + 9;
            p.skip_past("]]>")?;
            let data = text[start..p.i - 3].to_string();
            stack.last_mut().expect("the root").children.push(Node::Text(data));
        } else if p.starts("<!") {
            p.skip_past(">")?;
        } else if p.starts("</") {
            p.i += 2;
            let name = p.name()?;
            p.skip_space();
            p.expect(b'>')?;
            let done = stack.pop().ok_or(XmlError { at: p.i, why: "an end tag with no start" })?;
            if done.name != name {
                return Err(XmlError { at: p.i, why: "an end tag that ends another element" });
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(Node::Element(done)),
                None => return Err(XmlError { at: p.i, why: "an end tag with no start" }),
            }
        } else {
            p.i += 1;
            let name = p.name()?.to_string();
            let mut attrs = Vec::new();
            loop {
                p.skip_space();
                match p.s.get(p.i) {
                    Some(b'/') => {
                        p.i += 1;
                        p.expect(b'>')?;
                        let e = Element { name, attrs, children: Vec::new() };
                        stack.last_mut().expect("the root").children.push(Node::Element(e));
                        break;
                    }
                    Some(b'>') => {
                        p.i += 1;
                        // The stack holds the root this function reads into, then each element open.
                        if stack.len() > MAX_DEPTH {
                            return Err(XmlError { at: p.i, why: "elements nested more than 256 deep" });
                        }
                        stack.push(Element { name, attrs, children: Vec::new() });
                        break;
                    }
                    Some(_) => {
                        let key = p.name()?.to_string();
                        p.skip_space();
                        p.expect(b'=')?;
                        p.skip_space();
                        let quote = *p.s.get(p.i).ok_or(XmlError { at: p.i, why: "an attribute with no value" })?;
                        if quote != b'"' && quote != b'\'' {
                            return Err(XmlError { at: p.i, why: "an attribute's value unquoted" });
                        }
                        p.i += 1;
                        let start = p.i;
                        while p.i < p.s.len() && p.s[p.i] != quote {
                            p.i += 1;
                        }
                        let value = decode(&text[start..p.i]);
                        p.expect(quote)?;
                        attrs.push((key, value));
                    }
                    None => return Err(XmlError { at: p.i, why: "a tag that does not close" }),
                }
            }
        }
    }
    let root =
        stack.pop().filter(|_| stack.is_empty()).ok_or(XmlError { at: p.i, why: "an element that does not end" })?;
    root.children
        .into_iter()
        .find_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
        .ok_or(XmlError { at: 0, why: "no element" })
}

struct Parser<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
}

impl<'a> Parser<'a> {
    fn starts(&self, what: &str) -> bool {
        self.s[self.i..].starts_with(what.as_bytes())
    }

    fn skip_past(&mut self, what: &str) -> Result<(), XmlError> {
        match self.text[self.i..].find(what) {
            Some(at) => {
                self.i += at + what.len();
                Ok(())
            }
            None => Err(XmlError { at: self.i, why: "a declaration, a comment, or CDATA that does not end" }),
        }
    }

    fn skip_space(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn expect(&mut self, b: u8) -> Result<(), XmlError> {
        if self.s.get(self.i) == Some(&b) {
            self.i += 1;
            Ok(())
        } else {
            Err(XmlError { at: self.i, why: "a tag written otherwise than XML writes one" })
        }
    }

    fn name(&mut self) -> Result<&'a str, XmlError> {
        let start = self.i;
        while self.i < self.s.len() && !matches!(self.s[self.i], b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>' | b'=') {
            self.i += 1;
        }
        if self.i == start {
            return Err(XmlError { at: start, why: "a name missing" });
        }
        Ok(&self.text[start..self.i])
    }
}

/// `raw` with its entities and character references written out.
fn decode(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|e| *e <= 12) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let ch = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_part_reads_by_local_names() {
        let doc = parse(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!-- a comment -->
<p:sld xmlns:a="x" xmlns:p="y"><p:cSld><p:spTree>
  <p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
    <p:txBody><a:p><a:r><a:t xml:space="preserve">Fish &amp; chips &#8212; &#x41;</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:sld>"#,
        )
        .unwrap();
        assert_eq!(doc.local(), "sld");
        let sp = doc.at(&["cSld", "spTree", "sp"]).unwrap();
        assert_eq!(sp.at(&["nvSpPr", "nvPr", "ph"]).unwrap().attr("type"), Some("title"));
        let mut ts = Vec::new();
        sp.descendants("t", &mut ts);
        assert_eq!(ts[0].text(), "Fish & chips \u{2014} A");
        assert_eq!(ts[0].attr("space"), Some("preserve"));
        assert!(parse("<a><b></a>").is_err());
        assert_eq!(parse("<a><![CDATA[<b> & c]]></a>").unwrap().text(), "<b> & c");
    }

    /// Elements nested past [`MAX_DEPTH`] are refused, however deep, so nothing that walks a part
    /// read recurses past it; at it, the part reads (the 20261024 plan's STAB-2).
    #[test]
    fn a_part_nested_too_deep_is_refused() {
        let nested = |n: usize| format!("{}x{}", "<w:r>".repeat(n), "</w:r>".repeat(n));
        let deepest = parse(&nested(MAX_DEPTH)).unwrap();
        let mut found = Vec::new();
        deepest.descendants("r", &mut found);
        assert_eq!(found.len(), MAX_DEPTH - 1);
        for n in [MAX_DEPTH + 1, 1_000_000] {
            let refused = parse(&nested(n)).unwrap_err();
            assert_eq!(refused.why, "elements nested more than 256 deep", "{n}");
        }
    }
}
