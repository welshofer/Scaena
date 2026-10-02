//! `.scn` lines to the document as `deck.json` holds it (SPEC §4.2). Keys keep the order
//! the source writes them in, so a decompiled deck compiles back to itself exactly.

use super::lex::{Line, Tok, Token};
use super::{DslError, SourceMap};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// The node types a node line can name.
pub const TYPES: [&str; 10] = ["text", "shape", "image", "chart", "table", "shader", "stack", "grid", "frame", "group"];
/// The types whose name takes a kind: `chart:bar`, `shader:mesh`.
pub const KINDED: [&str; 2] = ["chart", "shader"];
/// Words that start a line in a state's body other than a node line.
pub const STATE_WORDS: [&str; 4] = ["notes", "choreo", "sequence", "parallel"];
/// Header keys that belong to the deck; any other header key belongs to its `meta`.
pub const DECK_KEYS: [&str; 7] = ["scaena", "theme", "canvas", "formats", "spine", "meta", "_comment"];

pub fn parse(src: &str, lines: &[Line]) -> Result<(Value, SourceMap), DslError> {
    let mut p = Parser { src, lines, i: 0, ..Parser::default() };
    p.prescan();
    p.document()?;
    p.finish()
}

#[derive(Default)]
struct Parser<'a> {
    src: &'a str,
    lines: &'a [Line],
    i: usize,
    deck: Map<String, Value>,
    meta: Option<Map<String, Value>>,
    fonts: Vec<Value>,
    data: Map<String, Value>,
    nodes: Map<String, Value>,
    /// Every node's type: from its declaration (found before parsing), or from the state
    /// line that first names it with one.
    types: HashMap<String, String>,
    sections: Option<Vec<Value>>,
    states: Vec<Value>,
    overrides: Map<String, Value>,
    header: bool,
    /// Where each part of the deck came from: a JSON pointer to a byte offset and length.
    spans: HashMap<String, (usize, usize)>,
}

/// One line's tokens, read in order.
struct Toks<'a> {
    src: &'a str,
    toks: &'a [Token],
    i: usize,
    /// Where the line ends, for errors about what is missing.
    end: usize,
}

impl<'a> Toks<'a> {
    fn new(src: &'a str, line: &'a Line) -> Toks<'a> {
        let end = line.tokens.last().map_or(line.start + line.indent, |t| t.end);
        Toks { src, toks: &line.tokens, i: 0, end }
    }

    fn peek(&self) -> Option<&'a Tok> {
        self.toks.get(self.i).map(|t| &t.tok)
    }

    fn peek_at(&self, n: usize) -> Option<&'a Tok> {
        self.toks.get(self.i + n).map(|t| &t.tok)
    }

    fn next(&mut self) -> Option<&'a Token> {
        let t = self.toks.get(self.i);
        self.i += 1;
        t
    }

    fn done(&self) -> bool {
        self.i >= self.toks.len()
    }

    /// Where the token before the next one ends.
    fn last_end(&self) -> usize {
        self.i.checked_sub(1).and_then(|i| self.toks.get(i)).map_or(self.end, |t| t.end)
    }

    /// An error at the next token, or at the line's end.
    fn err(&self, message: &str) -> DslError {
        match self.toks.get(self.i) {
            Some(t) => DslError::span(self.src, t.start, t.end - t.start, message),
            None => DslError::at(self.src, self.end, message),
        }
    }

    fn err_at(&self, t: &Token, message: &str) -> DslError {
        DslError::span(self.src, t.start, t.end - t.start, message)
    }

    /// A word or a quoted string: an id, or a key.
    fn name(&mut self, what: &str) -> Result<String, DslError> {
        match self.peek() {
            Some(Tok::Ident(w)) | Some(Tok::Str(w)) => {
                let w = w.clone();
                self.i += 1;
                Ok(w)
            }
            _ => Err(self.err(&format!("expected {what}"))),
        }
    }

    /// Whether a `key:` comes next.
    fn key_next(&self) -> bool {
        matches!(self.peek(), Some(Tok::Ident(_) | Tok::Str(_))) && self.peek_at(1) == Some(&Tok::Colon)
    }

    /// Whether `name(` comes next.
    fn call_next(&self) -> bool {
        matches!(self.peek(), Some(Tok::Ident(_))) && self.peek_at(1) == Some(&Tok::LParen)
    }

    fn expect(&mut self, tok: Tok, what: &str) -> Result<(), DslError> {
        if self.peek() == Some(&tok) {
            self.i += 1;
            Ok(())
        } else {
            Err(self.err(&format!("expected {what}")))
        }
    }

    fn end(&self) -> Result<(), DslError> {
        if self.done() { Ok(()) } else { Err(self.err("nothing may follow here")) }
    }

    /// A bare string standing alone (not a key): the primary value of a line.
    fn bare_string(&mut self) -> Option<String> {
        match self.peek() {
            Some(Tok::Str(s)) if self.peek_at(1) != Some(&Tok::Colon) => {
                self.i += 1;
                Some(s.clone())
            }
            _ => None,
        }
    }
}

/// Comment lines, then a trailing comment, as one `_comment`.
fn joined(pending: &mut Vec<String>, trailing: &Option<String>) -> Option<String> {
    let mut all: Vec<String> = std::mem::take(pending);
    all.extend(trailing.iter().cloned());
    if all.is_empty() { None } else { Some(all.join("\n")) }
}

fn esc(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// A time as its field holds it: milliseconds, or seconds where the field is in seconds.
fn time(numeral: &str, written_in_seconds: bool, field_in_seconds: bool) -> Option<Value> {
    let scaled = |x: f64| {
        if x.fract() == 0.0 && x.abs() < 9.0e15 {
            Some(Value::from(x as i64))
        } else {
            serde_json::Number::from_f64(x).map(Value::Number)
        }
    };
    let n: f64 = numeral.parse().ok()?;
    match (written_in_seconds, field_in_seconds) {
        (true, false) => scaled(n * 1000.0),
        (false, true) => scaled(n / 1000.0),
        _ => serde_json::from_str(numeral).ok(),
    }
}

/// Whether the field at `ptr` holds seconds: a beat's `duration`. Every other time in a
/// deck is in milliseconds.
pub fn in_seconds(ptr: &str) -> bool {
    matches!(ptr.split('/').collect::<Vec<_>>().as_slice(), ["", "spine", "sections", _, "beats", _, "duration"])
}

impl<'a> Parser<'a> {
    /// Records that the part of the deck at `ptr` came from `start..end` of the source.
    fn mark(&mut self, ptr: &str, start: usize, end: usize) {
        self.spans.insert(ptr.to_string(), (start, end.saturating_sub(start)));
    }

    fn mark_token(&mut self, ptr: &str, token: &Token) {
        self.mark(ptr, token.start, token.end);
    }

    /// Moves what was recorded at `from`, and under it, to `to`: a part the deck holds
    /// somewhere other than where the source wrote it.
    fn move_marks(&mut self, from: &str, to: &str) {
        let moved: Vec<String> =
            self.spans.keys().filter(|k| k.as_str() == from || k.starts_with(&format!("{from}/"))).cloned().collect();
        for key in moved {
            let span = self.spans.remove(&key).expect("listed");
            self.spans.insert(format!("{to}{}", &key[from.len()..]), span);
        }
    }

    /// The type of every declared node, so a state may name a node declared after it.
    fn prescan(&mut self) {
        for line in self.lines.iter().filter(|l| l.indent == 0) {
            if let [Token { tok: Tok::Ident(kw), .. }, id, Token { tok: Tok::Ident(ty), .. }, ..] =
                line.tokens.as_slice()
                && kw == "node"
                && TYPES.contains(&ty.as_str())
                && let Tok::Ident(id) | Tok::Str(id) = &id.tok
            {
                self.types.entry(id.clone()).or_insert_with(|| ty.clone());
            }
        }
    }

    fn line(&self) -> &'a Line {
        &self.lines[self.i]
    }

    fn deeper(&self, indent: usize) -> bool {
        self.i < self.lines.len() && self.lines[self.i].indent > indent
    }

    fn document(&mut self) -> Result<(), DslError> {
        let mut pending = Vec::new();
        while self.i < self.lines.len() {
            let line = self.line();
            if line.indent != 0 {
                return Err(DslError::at(
                    self.src,
                    line.start,
                    "this line is indented, but nothing above it takes a block",
                ));
            }
            if line.tokens.is_empty() {
                pending.extend(line.comment.clone());
                self.i += 1;
                continue;
            }
            let comment = joined(&mut pending, &line.comment);
            let mut t = Toks::new(self.src, line);
            let word = match t.peek() {
                Some(Tok::Ident(w)) => w.clone(),
                _ => return Err(t.err("expected a declaration: deck, font, data, node, override, section, or state")),
            };
            t.next();
            match word.as_str() {
                "deck" => self.header(&mut t, comment)?,
                "font" => self.font(&mut t)?,
                "data" => self.data(&mut t)?,
                "node" => self.node(&mut t, comment)?,
                "override" => self.override_(&mut t, comment)?,
                "section" => self.section(&mut t)?,
                "state" => self.state(&mut t, comment)?,
                other => {
                    return Err(t.err_at(
                        &line.tokens[0],
                        &format!("`{other}` does not start a declaration: deck, font, data, node, override, section, or state"),
                    ));
                }
            }
        }
        Ok(())
    }

    // --- values ------------------------------------------------------------------

    /// A value, recorded at `ptr`.
    fn value(&mut self, t: &mut Toks, ptr: &str) -> Result<Value, DslError> {
        let Some(token) = t.next() else { return Err(t.err("expected a value")) };
        let v = match &token.tok {
            Tok::Str(s) | Tok::Percent(s) | Tok::Ref(s) | Tok::Ratio(s) => Value::String(s.clone()),
            Tok::Num(n) => serde_json::from_str(n).map_err(|_| t.err_at(token, "not a number"))?,
            Tok::Time(n, seconds) => {
                time(n, *seconds, in_seconds(ptr)).ok_or_else(|| t.err_at(token, "not a number"))?
            }
            Tok::Ident(w) => match w.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                "null" => Value::Null,
                _ => Value::String(w.clone()),
            },
            Tok::LBracket => {
                let mut items = Vec::new();
                if t.peek() == Some(&Tok::RBracket) {
                    t.next();
                } else {
                    loop {
                        let item = self.value(t, &format!("{ptr}/{}", items.len()))?;
                        items.push(item);
                        match t.next().map(|x| &x.tok) {
                            Some(Tok::Comma) => {}
                            Some(Tok::RBracket) => break,
                            _ => return Err(t.err("expected `,` or `]`")),
                        }
                    }
                }
                Value::Array(items)
            }
            Tok::LBrace => {
                let mut map = Map::new();
                if t.peek() == Some(&Tok::RBrace) {
                    t.next();
                } else {
                    loop {
                        let at = t.i;
                        let key = t.name("a key")?;
                        t.expect(Tok::Colon, "`:` after the key")?;
                        if map.contains_key(&key) {
                            return Err(t.err_at(&t.toks[at], &format!("`{key}` is written twice")));
                        }
                        let at_key = format!("{ptr}/{}", esc(&key));
                        let v = self.value(t, &at_key)?;
                        self.mark(&at_key, t.toks[at].start, t.last_end());
                        map.insert(key, v);
                        match t.next().map(|x| &x.tok) {
                            Some(Tok::Comma) => {}
                            Some(Tok::RBrace) => break,
                            _ => return Err(t.err("expected `,` or `}`")),
                        }
                    }
                }
                Value::Object(map)
            }
            Tok::Dim(..) => return Err(t.err_at(token, "a size like `1920x1080` is for `canvas` only")),
            Tok::Range(..) => {
                return Err(t.err_at(token, "a range like `1-7` is for `at:` calls only, as in `col(1-7)`"));
            }
            _ => return Err(t.err_at(token, "expected a value")),
        };
        self.mark(ptr, token.start, t.last_end());
        Ok(v)
    }

    /// `name(arg, …) …`: an `at` value as calls. One argument is the value, a range
    /// `a-b` is `[a, b]`, and several make a list.
    fn calls(&mut self, t: &mut Toks, ptr: &str) -> Result<Value, DslError> {
        let mut at = Map::new();
        while t.call_next() {
            let start = t.i;
            let name = t.name("a call")?;
            t.expect(Tok::LParen, "`(`")?;
            let mut args = Vec::new();
            if t.peek() == Some(&Tok::RParen) {
                return Err(t.err(&format!("`{name}()` needs a value")));
            }
            let call = format!("{ptr}/{}", esc(&name));
            loop {
                let arg = match t.peek() {
                    Some(Tok::Range(a, b)) => {
                        t.next();
                        json!([serde_json::from_str::<Value>(a).unwrap(), serde_json::from_str::<Value>(b).unwrap()])
                    }
                    _ => self.value(t, &format!("{call}/{}", args.len()))?,
                };
                args.push(arg);
                match t.next().map(|x| &x.tok) {
                    Some(Tok::Comma) => {}
                    Some(Tok::RParen) => break,
                    _ => return Err(t.err("expected `,` or `)`")),
                }
            }
            if at.contains_key(&name) {
                return Err(t
                    .err_at(&t.toks[start], &format!("`{name}` is written twice"))
                    .pointer(format!("{ptr}/{}", esc(&name))));
            }
            if args.len() == 1 {
                self.move_marks(&format!("{call}/0"), &call);
            }
            self.mark(&call, t.toks[start].start, t.last_end());
            let v = if args.len() == 1 { args.pop().expect("one") } else { Value::Array(args) };
            at.insert(name, v);
        }
        Ok(Value::Object(at))
    }

    /// `key:value …` to the end of the line, into `obj`.
    fn props(&mut self, t: &mut Toks, obj: &mut Map<String, Value>, ptr: &str) -> Result<(), DslError> {
        while !t.done() {
            self.prop(t, obj, ptr)?;
        }
        Ok(())
    }

    /// One `key:value`, into `obj`.
    fn prop(&mut self, t: &mut Toks, obj: &mut Map<String, Value>, ptr: &str) -> Result<(), DslError> {
        let at = t.i;
        if !t.key_next() {
            return Err(t.err("expected `key:value`"));
        }
        let key = t.name("a key")?;
        t.next(); // the colon
        if obj.contains_key(&key) {
            return Err(t
                .err_at(&t.toks[at], &format!("`{key}` is written twice"))
                .pointer(format!("{ptr}/{}", esc(&key))));
        }
        let at_key = format!("{ptr}/{}", esc(&key));
        let v = match (key.as_str(), t.peek()) {
            ("at", _) if t.call_next() => self.calls(t, &at_key)?,
            ("canvas", Some(Tok::Dim(w, h))) => {
                t.next();
                json!({ "width": serde_json::from_str::<Value>(w).unwrap(), "height": serde_json::from_str::<Value>(h).unwrap() })
            }
            _ => self.value(t, &at_key)?,
        };
        self.mark(&at_key, t.toks[at].start, t.last_end());
        obj.insert(key, v);
        Ok(())
    }

    /// A node line's props, or a continuation line's: `key:value`s, and the bare string
    /// that is a text node's `text` or an image's `src`, wherever it stands among them.
    fn node_line(&mut self, t: &mut Toks, obj: &mut Map<String, Value>, ty: &str, ptr: &str) -> Result<(), DslError> {
        while !t.done() {
            let at = t.i;
            let Some(primary) = t.bare_string() else {
                self.prop(t, obj, ptr)?;
                continue;
            };
            let key = match ty {
                "text" => "text",
                "image" => "src",
                "" => {
                    return Err(t.err_at(
                        &t.toks[at],
                        "a bare string sets a text node's text or an image's src, and this node has no type: declare it (`node id text …`) or give its type here (`id text …`)",
                    ));
                }
                _ => {
                    return Err(t.err_at(
                        &t.toks[at],
                        &format!("a bare string sets a text node's text or an image's src, and this is a {ty} node"),
                    ));
                }
            };
            if obj.contains_key(key) {
                return Err(t
                    .err_at(&t.toks[at], &format!("`{key}` is written twice"))
                    .pointer(format!("{ptr}/{key}")));
            }
            self.mark_token(&format!("{ptr}/{key}"), &t.toks[at]);
            obj.insert(key.into(), Value::String(primary));
        }
        Ok(())
    }

    /// The lines deeper than `indent` that follow, each `key:value …`, into `obj`.
    fn more_props(&mut self, indent: usize, obj: &mut Map<String, Value>, ptr: &str) -> Result<(), DslError> {
        while self.deeper(indent) {
            let line = self.line();
            self.i += 1;
            let mut t = Toks::new(self.src, line);
            self.props(&mut t, obj, ptr)?;
        }
        Ok(())
    }

    // --- declarations ---------------------------------------------------------------

    /// `deck "Title" key:value …` and its block of `key:value` lines.
    fn header(&mut self, t: &mut Toks, comment: Option<String>) -> Result<(), DslError> {
        let indent = self.line().indent;
        if std::mem::replace(&mut self.header, true) {
            return Err(t.err_at(&self.line().tokens[0], "a file has one `deck` header"));
        }
        let mut keys = Map::new();
        if let Some(c) = comment {
            keys.insert("_comment".into(), Value::String(c));
        }
        self.mark_token("", &t.toks[0]);
        let title_at = t.i;
        let title = t.bare_string();
        if title.is_some() {
            self.mark_token("/meta/title", &t.toks[title_at]);
        }
        self.props(t, &mut keys, "")?;
        self.i += 1;
        self.more_props(indent, &mut keys, "")?;
        let mut meta = self.meta.take();
        if let Some(title) = title {
            meta.get_or_insert_with(Map::new).insert("title".into(), Value::String(title));
        }
        for (key, value) in keys {
            if key == "meta" {
                let Value::Object(more) = value else {
                    return Err(
                        DslError::at(self.src, self.lines[self.i - 1].start, "`meta` takes a map").pointer("/meta")
                    );
                };
                meta.get_or_insert_with(Map::new).extend(more);
            } else if DECK_KEYS.contains(&key.as_str()) {
                self.deck.insert(key, value);
            } else {
                self.move_marks(&format!("/{}", esc(&key)), &format!("/meta/{}", esc(&key)));
                meta.get_or_insert_with(Map::new).insert(key, value);
            }
        }
        self.meta = meta;
        Ok(())
    }

    /// `font "Family" "fonts/file.ttf" key:value …`.
    fn font(&mut self, t: &mut Toks) -> Result<(), DslError> {
        let indent = self.line().indent;
        let ptr = format!("/fonts/{}", self.fonts.len());
        let mut font = Map::new();
        self.mark_token(&ptr, &t.toks[0]);
        let family = match (t.peek(), t.peek_at(1)) {
            (Some(Tok::Str(f) | Tok::Ident(f)), next) if next != Some(&Tok::Colon) => {
                t.i += 1;
                Some(f.clone())
            }
            _ => None,
        };
        if let Some(family) = family {
            self.mark_token(&format!("{ptr}/family"), &t.toks[t.i - 1]);
            font.insert("family".into(), Value::String(family));
            if let Some(file) = t.bare_string() {
                self.mark_token(&format!("{ptr}/file"), &t.toks[t.i - 1]);
                font.insert("file".into(), Value::String(file));
            }
        }
        self.props(t, &mut font, &ptr)?;
        self.i += 1;
        self.more_props(indent, &mut font, &ptr)?;
        self.fonts.push(Value::Object(font));
        Ok(())
    }

    /// `data id "data/file.csv" key:value …`, or `data id inline:[…]`.
    fn data(&mut self, t: &mut Toks) -> Result<(), DslError> {
        let indent = self.line().indent;
        let start = t.i;
        let id = t.name("the data source's id")?;
        let ptr = format!("/data/{}", esc(&id));
        self.mark_token(&ptr, &t.toks[start]);
        let mut source = Map::new();
        if let Some(path) = t.bare_string() {
            self.mark_token(&format!("{ptr}/source"), &t.toks[t.i - 1]);
            source.insert("source".into(), Value::String(path));
        }
        self.props(t, &mut source, &ptr)?;
        self.i += 1;
        self.more_props(indent, &mut source, &ptr)?;
        if let Some(rows) = source.shift_remove("inline") {
            if source.contains_key("source") {
                return Err(t.err_at(&t.toks[start], "a data source is a file or `inline:`, not both").pointer(ptr));
            }
            self.move_marks(&format!("{ptr}/inline"), &format!("{ptr}/source/inline"));
            if let Some(&(offset, len)) = self.spans.get(&format!("{ptr}/source/inline")) {
                self.mark(&format!("{ptr}/source"), offset, offset + len);
            }
            source.insert("source".into(), json!({ "inline": rows }));
        }
        if self.data.contains_key(&id) {
            return Err(t.err_at(&t.toks[start], &format!("data `{id}` is declared twice")).pointer(ptr));
        }
        self.data.insert(id, Value::Object(source));
        Ok(())
    }

    /// `node id type ["text"] key:value …`: a node and its defaults.
    fn node(&mut self, t: &mut Toks, comment: Option<String>) -> Result<(), DslError> {
        let indent = self.line().indent;
        let start = t.i;
        let id = t.name("the node's id")?;
        let ptr = format!("/nodes/{}", esc(&id));
        if self.nodes.contains_key(&id) {
            return Err(t.err_at(&t.toks[start], &format!("node `{id}` is declared twice")).pointer(ptr));
        }
        self.mark_token(&ptr, &t.toks[start]);
        let spec = t.i;
        let Some((ty, kind)) = self.type_spec(t) else {
            return Err(t.err(
                "a node needs a type: text, shape, image, chart:KIND, shader:KIND, stack, grid, frame, or group",
            ));
        };
        self.mark_type(t, spec, &ptr);
        self.types.insert(id.clone(), ty.clone());
        let props = self.node_props(t, indent, comment, kind, &ty, &ptr)?;
        let mut node = Map::new();
        node.insert("type".into(), Value::String(ty));
        node.extend(props);
        self.nodes.insert(id, Value::Object(node));
        Ok(())
    }

    /// Records the type spec read from token `spec` on: its type, and its kind.
    fn mark_type(&mut self, t: &Toks, spec: usize, ptr: &str) {
        self.mark_token(&format!("{ptr}/type"), &t.toks[spec]);
        if t.i - spec == 3 {
            self.mark_token(&format!("{ptr}/kind"), &t.toks[spec + 2]);
        }
    }

    /// A type at the head of a node line: a type word, or `chart:KIND` / `shader:KIND`.
    fn type_spec(&self, t: &mut Toks) -> Option<(String, Option<Value>)> {
        let Some(Tok::Ident(word)) = t.peek() else { return None };
        if !TYPES.contains(&word.as_str()) {
            return None;
        }
        match (t.peek_at(1), t.peek_at(2)) {
            (Some(Tok::Colon), Some(Tok::Ident(kind))) if KINDED.contains(&word.as_str()) => {
                t.i += 3;
                Some((word.clone(), Some(Value::String(kind.clone()))))
            }
            (Some(Tok::Colon), _) => None, // a `text:` prop
            _ => {
                t.i += 1;
                Some((word.clone(), None))
            }
        }
    }

    /// The rest of a node line, and its continuation lines: `_comment`, a kind, then props in
    /// the order the lines write them.
    fn node_props(
        &mut self,
        t: &mut Toks,
        indent: usize,
        comment: Option<String>,
        kind: Option<Value>,
        ty: &str,
        ptr: &str,
    ) -> Result<Map<String, Value>, DslError> {
        let mut props = Map::new();
        if let Some(c) = comment {
            props.insert("_comment".into(), Value::String(c));
        }
        if let Some(kind) = kind {
            props.insert("kind".into(), kind);
        }
        self.node_line(t, &mut props, ty, ptr)?;
        self.i += 1;
        while self.deeper(indent) {
            let line = self.line();
            self.i += 1;
            let mut t = Toks::new(self.src, line);
            self.node_line(&mut t, &mut props, ty, ptr)?;
        }
        Ok(props)
    }

    /// `override id key:value …`.
    fn override_(&mut self, t: &mut Toks, comment: Option<String>) -> Result<(), DslError> {
        let indent = self.line().indent;
        let start = t.i;
        let id = t.name("the node's id")?;
        let ptr = format!("/overrides/{}", esc(&id));
        self.mark_token(&ptr, &t.toks[start]);
        let mut props = Map::new();
        if let Some(c) = comment {
            props.insert("_comment".into(), Value::String(c));
        }
        self.props(t, &mut props, &ptr)?;
        self.i += 1;
        self.more_props(indent, &mut props, &ptr)?;
        if self.overrides.contains_key(&id) {
            return Err(t.err_at(&t.toks[start], &format!("overrides for `{id}` are declared twice")).pointer(ptr));
        }
        self.overrides.insert(id, Value::Object(props));
        Ok(())
    }

    /// `section id ["Title"]` and its beats.
    fn section(&mut self, t: &mut Toks) -> Result<(), DslError> {
        let indent = self.line().indent;
        let si = self.sections.as_ref().map_or(0, Vec::len);
        let ptr = format!("/spine/sections/{si}");
        let mut section = Map::new();
        section.insert("id".into(), Value::String(t.name("the section's id")?));
        self.mark_token(&ptr, &t.toks[t.i - 1]);
        self.mark_token(&format!("{ptr}/id"), &t.toks[t.i - 1]);
        if let Some(title) = t.bare_string() {
            self.mark_token(&format!("{ptr}/title"), &t.toks[t.i - 1]);
            section.insert("title".into(), Value::String(title));
        }
        self.props(t, &mut section, &ptr)?;
        self.i += 1;
        let mut beats = Vec::new();
        while self.deeper(indent) {
            let line = self.line();
            if line.tokens.is_empty() {
                self.i += 1;
                continue;
            }
            let mut t = Toks::new(self.src, line);
            match t.peek() {
                Some(Tok::Ident(w)) if w == "beat" => {
                    t.next();
                    beats.push(self.beat(&mut t, &format!("{ptr}/beats/{}", beats.len()))?);
                }
                _ if t.key_next() => {
                    self.props(&mut t, &mut section, &ptr)?;
                    self.i += 1;
                }
                _ => return Err(t.err("a section holds `beat` lines")),
            }
        }
        section.insert("beats".into(), Value::Array(beats));
        self.sections.get_or_insert_with(Vec::new).push(Value::Object(section));
        Ok(())
    }

    /// `beat id "Claim" key:value …`, and its `notes` and `key:value` lines.
    fn beat(&mut self, t: &mut Toks, ptr: &str) -> Result<Value, DslError> {
        let indent = self.line().indent;
        let mut beat = Map::new();
        beat.insert("id".into(), Value::String(t.name("the beat's id")?));
        self.mark_token(ptr, &t.toks[t.i - 1]);
        self.mark_token(&format!("{ptr}/id"), &t.toks[t.i - 1]);
        if let Some(claim) = t.bare_string() {
            self.mark_token(&format!("{ptr}/claim"), &t.toks[t.i - 1]);
            beat.insert("claim".into(), Value::String(claim));
        }
        self.props(t, &mut beat, ptr)?;
        self.i += 1;
        while self.deeper(indent) {
            let line = self.line();
            self.i += 1;
            if line.tokens.is_empty() {
                continue;
            }
            let mut t = Toks::new(self.src, line);
            if let Some(Tok::Ident(w)) = t.peek()
                && w == "media"
                && t.peek_at(1) != Some(&Tok::Colon)
            {
                t.next();
                let at = format!("{ptr}/media");
                if beat.contains_key("media") {
                    return Err(t.err_at(&t.toks[0], "`media` is written twice").pointer(at));
                }
                self.mark(&at, t.toks[0].start, t.end);
                let mut media = Map::new();
                self.props(&mut t, &mut media, &at)?;
                self.more_props(line.indent, &mut media, &at)?;
                beat.insert("media".into(), Value::Object(media));
                continue;
            }
            if let (Some(Tok::Ident(w)), Some(Tok::Str(notes))) = (t.peek(), t.peek_at(1))
                && w == "notes"
            {
                t.i += 2;
                t.end()?;
                self.mark(&format!("{ptr}/notes"), t.toks[0].start, t.toks[1].end);
                if beat.insert("notes".into(), Value::String(notes.clone())).is_some() {
                    return Err(t.err_at(&t.toks[0], "`notes` is written twice").pointer(format!("{ptr}/notes")));
                }
                continue;
            }
            self.props(&mut t, &mut beat, ptr)?;
        }
        Ok(Value::Object(beat))
    }

    /// `state id key:value …` and its body.
    fn state(&mut self, t: &mut Toks, comment: Option<String>) -> Result<(), DslError> {
        let indent = self.line().indent;
        let ptr = format!("/states/{}", self.states.len());
        let mut state = Map::new();
        state.insert("id".into(), Value::String(t.name("the state's id")?));
        self.mark_token(&ptr, &t.toks[t.i - 1]);
        self.mark_token(&format!("{ptr}/id"), &t.toks[t.i - 1]);
        if let Some(c) = comment {
            state.insert("_comment".into(), Value::String(c));
        }
        self.props(t, &mut state, &ptr)?;
        self.i += 1;
        let (mut deltas, mut remove, mut choreography) = (Map::new(), Vec::new(), Vec::new());
        let mut pending = Vec::new();
        while self.deeper(indent) {
            let line = self.line();
            if line.tokens.is_empty() {
                pending.extend(line.comment.clone());
                self.i += 1;
                continue;
            }
            let comment = joined(&mut pending, &line.comment);
            let mut t = Toks::new(self.src, line);
            match (t.peek(), t.peek_at(1)) {
                (Some(Tok::Minus), _) => {
                    t.next();
                    remove.push(Value::String(t.name("the id of the node that exits")?));
                    t.end()?;
                    self.mark(&format!("{ptr}/remove/{}", remove.len() - 1), t.toks[0].start, t.last_end());
                    self.i += 1;
                }
                (Some(Tok::Ident(w)), Some(Tok::Str(notes))) if w == "notes" => {
                    t.i += 2;
                    t.end()?;
                    self.mark(&format!("{ptr}/notes"), t.toks[0].start, t.toks[1].end);
                    if state.insert("notes".into(), Value::String(notes.clone())).is_some() {
                        return Err(t.err_at(&t.toks[0], "`notes` is written twice").pointer(format!("{ptr}/notes")));
                    }
                    self.i += 1;
                }
                (Some(Tok::Ident(w)), _) if w == "choreo" => {
                    t.next();
                    let at = format!("{ptr}/choreography/{}", choreography.len());
                    choreography.push(self.choreo(&mut t, &at)?);
                }
                (Some(Tok::Ident(w)), next) if (w == "sequence" || w == "parallel") && next != Some(&Tok::Colon) => {
                    let w = w.clone();
                    t.next();
                    let at = format!("{ptr}/choreography/{}", choreography.len());
                    choreography.push(self.group(&mut t, &w, &at)?);
                }
                _ if t.key_next() => {
                    if matches!(t.peek(), Some(Tok::Ident(k)) if k == "props" || k == "remove" || k == "choreography") {
                        return Err(t.err("write a state's props, exits, and choreography as lines of its body"));
                    }
                    self.props(&mut t, &mut state, &ptr)?;
                    self.i += 1;
                }
                _ => {
                    let start = t.i;
                    let id = t.name("a node line, a `-id` exit, `choreo`, `notes`, or `key:value`")?;
                    let at = format!("{ptr}/props/{}", esc(&id));
                    if deltas.contains_key(&id) {
                        return Err(t
                            .err_at(&t.toks[start], &format!("`{id}` has two lines in this state"))
                            .pointer(at));
                    }
                    self.mark_token(&at, &t.toks[start]);
                    let spec_at = t.i;
                    let spec = self.type_spec(&mut t);
                    let line_indent = line.indent;
                    match (spec, self.types.get(&id).cloned()) {
                        (Some((new, _)), Some(old)) if new != old => {
                            return Err(t
                                .err_at(
                                    &t.toks[start],
                                    &format!("`{id}` is a {old} node; a node's type never changes (E104)"),
                                )
                                .pointer(at));
                        }
                        (Some((new, kind)), None) => {
                            // The first line that gives a node's type declares it, there: its
                            // props are the node's own, and the state shows it as it is.
                            let declared = format!("/nodes/{}", esc(&id));
                            self.mark_token(&declared, &t.toks[start]);
                            self.mark_type(&t, spec_at, &declared);
                            self.types.insert(id.clone(), new.clone());
                            let props = self.node_props(&mut t, line_indent, comment, kind, &new, &declared)?;
                            let mut node = Map::new();
                            node.insert("type".into(), Value::String(new));
                            node.extend(props);
                            self.nodes.insert(id.clone(), Value::Object(node));
                            deltas.insert(id, Value::Object(Map::new()));
                        }
                        (spec, old) => {
                            // A node's line in a state: what changes. A type it repeats is
                            // checked; a kind it names is part of the change. A line for no
                            // node keeps its props too, and `validate` says so (E102).
                            if spec.is_some() {
                                self.mark_type(&t, spec_at, &at);
                            }
                            let kind = spec.and_then(|(_, kind)| kind);
                            let ty = old.unwrap_or_default();
                            let delta = self.node_props(&mut t, line_indent, comment, kind, &ty, &at)?;
                            deltas.insert(id, Value::Object(delta));
                        }
                    }
                }
            }
        }
        if !deltas.is_empty() {
            state.insert("props".into(), Value::Object(deltas));
        }
        if !remove.is_empty() {
            state.insert("remove".into(), Value::Array(remove));
        }
        if !choreography.is_empty() {
            state.insert("choreography".into(), Value::Array(choreography));
        }
        self.states.push(Value::Object(state));
        Ok(())
    }

    /// `choreo target key:value …`, `choreo [a, b] …`, or `choreo = value`: one item.
    fn choreo(&mut self, t: &mut Toks, ptr: &str) -> Result<Value, DslError> {
        let indent = self.line().indent;
        self.mark_token(ptr, &t.toks[0]);
        if matches!(t.peek(), Some(Tok::Equals | Tok::LBrace)) {
            if t.peek() == Some(&Tok::Equals) {
                t.next();
            }
            let item = self.value(t, ptr)?;
            t.end()?;
            self.i += 1;
            return Ok(item);
        }
        let mut item = Map::new();
        if !t.done() && !t.key_next() {
            let target_at = format!("{ptr}/target");
            let target = match t.peek() {
                Some(Tok::LBracket) => self.value(t, &target_at)?,
                _ => {
                    let id = t.name("the node the item moves")?;
                    self.mark_token(&target_at, &t.toks[t.i - 1]);
                    Value::String(id)
                }
            };
            item.insert("target".into(), target);
        }
        self.props(t, &mut item, ptr)?;
        self.i += 1;
        self.more_props(indent, &mut item, ptr)?;
        Ok(Value::Object(item))
    }

    /// `sequence key:value …` or `parallel …`, and the items inside it.
    fn group(&mut self, t: &mut Toks, kind: &str, ptr: &str) -> Result<Value, DslError> {
        let indent = self.line().indent;
        let mut group = Map::new();
        self.mark_token(ptr, &t.toks[0]);
        group.insert(kind.into(), Value::Array(Vec::new()));
        self.props(t, &mut group, ptr)?;
        self.i += 1;
        let mut items = Vec::new();
        while self.deeper(indent) {
            let line = self.line();
            if line.tokens.is_empty() {
                self.i += 1;
                continue;
            }
            let mut t = Toks::new(self.src, line);
            let at = format!("{ptr}/{kind}/{}", items.len());
            match (t.peek(), t.peek_at(1)) {
                (Some(Tok::Ident(w)), _) if w == "choreo" => {
                    t.next();
                    items.push(self.choreo(&mut t, &at)?);
                }
                (Some(Tok::Ident(w)), next) if (w == "sequence" || w == "parallel") && next != Some(&Tok::Colon) => {
                    let w = w.clone();
                    t.next();
                    items.push(self.group(&mut t, &w, &at)?);
                }
                _ if t.key_next() => {
                    self.props(&mut t, &mut group, ptr)?;
                    self.i += 1;
                }
                _ => return Err(t.err("a group holds `choreo`, `sequence`, and `parallel` lines")),
            }
        }
        group.insert(kind.into(), Value::Array(items));
        Ok(Value::Object(group))
    }

    fn finish(mut self) -> Result<(Value, SourceMap), DslError> {
        let mut doc = Map::new();
        let deck_key = |deck: &mut Map<String, Value>, k: &str| deck.shift_remove(k);
        doc.insert("scaena".into(), deck_key(&mut self.deck, "scaena").unwrap_or(json!(crate::FORMAT_VERSION)));
        if let Some(meta) = self.meta {
            doc.insert("meta".into(), Value::Object(meta));
        }
        let Some(canvas) = deck_key(&mut self.deck, "canvas") else {
            return Err(DslError::at(self.src, 0, "a deck needs a canvas: `deck \"Title\" canvas:1920x1080`")
                .pointer("/canvas"));
        };
        doc.insert("canvas".into(), canvas);
        for key in ["formats", "theme"] {
            if let Some(v) = deck_key(&mut self.deck, key) {
                doc.insert(key.into(), v);
            }
        }
        if !self.fonts.is_empty() {
            doc.insert("fonts".into(), Value::Array(self.fonts));
        }
        if !self.data.is_empty() {
            doc.insert("data".into(), Value::Object(self.data));
        }
        match (self.sections, deck_key(&mut self.deck, "spine")) {
            (Some(_), Some(_)) => {
                return Err(DslError::at(self.src, 0, "a spine is `section` lines or a `spine:` header key, not both")
                    .pointer("/spine"));
            }
            (Some(sections), None) => {
                doc.insert("spine".into(), json!({ "sections": sections }));
            }
            (None, Some(spine)) => {
                doc.insert("spine".into(), spine);
            }
            (None, None) => {}
        }
        doc.insert("nodes".into(), Value::Object(self.nodes));
        doc.insert("states".into(), Value::Array(self.states));
        if !self.overrides.is_empty() {
            doc.insert("overrides".into(), Value::Object(self.overrides));
        }
        if let Some(c) = deck_key(&mut self.deck, "_comment") {
            doc.insert("_comment".into(), c);
        }
        Ok((Value::Object(doc), SourceMap(self.spans)))
    }
}
