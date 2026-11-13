//! The words and figures a PDF draws on its pages, read only so a deck's notes may say a figure
//! its sources hold (W1's dogfood 17). This is no text extraction: where words stand, the order
//! a reader takes them in, and whatever a font does not say the characters of are not read, so
//! nothing should be grounded in what [`words`] gives as if it were the PDF's text. The PDF
//! itself goes to the model whole.
//!
//! Read within bounds, as an Office file is (lift-up STAB-2): the objects are found by their
//! `N G obj` in the file's bytes and in its object streams, no cross-reference table trusted;
//! each stream is inflated through a bound; a dictionary or an array nests at most [`DEPTH`]
//! deep, parsed with a stack of its own, never the program's. Each page's text operators (`Tj`,
//! `TJ`, `'`, `"`) are read in the font its resources name, form XObjects too: a string is
//! words only through its own font's ToUnicode CMap, or the standard encoding a simple font
//! names, else none, never a guess. An encrypted PDF gives none. A file past a bound is refused,
//! naming it, never a hang, a run out of memory, or a stack overflow.

use crate::SourceError;
use std::collections::BTreeMap;
use std::io::Read;
use std::rc::Rc;

/// How far a PDF is read. [`BOUNDS`] are a source's.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    /// Objects found in the file and its object streams.
    pub objects: usize,
    /// Streams decoded.
    pub streams: usize,
    /// Bytes one stream inflates to.
    pub inflated: usize,
    /// Bytes every stream inflates to, together.
    pub total: usize,
    /// Values parsed in every object and content stream, together.
    pub values: usize,
    /// Operators read in every content stream, together.
    pub operators: usize,
    /// Codes the CMaps map, together.
    pub codes: usize,
    /// Bytes of words kept.
    pub text: usize,
}

/// A source's bounds: a report of a few hundred pages reads well within each.
pub const BOUNDS: Bounds = Bounds {
    objects: 1_000_000,
    streams: 100_000,
    inflated: 64 << 20,
    total: 256 << 20,
    values: 50_000_000,
    operators: 10_000_000,
    codes: 1_000_000,
    text: 16 << 20,
};

/// How deep a dictionary or an array nests.
pub const DEPTH: usize = 64;

/// How deep a form XObject draws another.
const FORMS: usize = 8;

/// How many `/Parent`s a page's resources are looked for in.
const PARENTS: usize = 64;

/// The words and figures the PDF `bytes`, named `name`, draws on its pages, within [`BOUNDS`]:
/// what its figures are read from (the module's comment says what this is not).
pub fn words(name: &str, bytes: &[u8]) -> Result<String, SourceError> {
    words_within(name, bytes, &BOUNDS)
}

/// The words of each PDF among `sources`, together, and why each it could not read was not: what a
/// deck's notes may say the figures of (W1's dogfood 17).
pub fn held(sources: &[crate::Source]) -> (String, Vec<String>) {
    let (mut all, mut unread) = (String::new(), Vec::new());
    for source in sources {
        let Some(pdf) = &source.pdf else { continue };
        match words(&source.name, pdf) {
            Ok(w) => {
                all.push_str(&w);
                all.push('\n');
            }
            Err(why) => unread.push(why.to_string()),
        }
    }
    (all, unread)
}

/// The words of each PDF among `sources`, by its name, those it could not read left out: what a
/// deck keeps the figures of, source by source (`Provenance::figures`, W1's dogfood 17).
pub fn each(sources: &[crate::Source]) -> Vec<(String, String)> {
    (sources.iter())
        .filter_map(|source| Some((source.name.clone(), words(&source.name, source.pdf.as_deref()?).ok()?)))
        .collect()
}

/// [`words`] within `bounds`.
pub fn words_within(name: &str, bytes: &[u8], bounds: &Bounds) -> Result<String, SourceError> {
    let mut file = File::new(bytes, bounds);
    let read = file.index().and_then(|()| file.pages());
    read.map_err(|why| SourceError::Unreadable(format!("{name}: its words were not read: {why}")))?;
    Ok(file.out)
}

/// A value of a PDF object, or a token of a content stream.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Null,
    Number(f64),
    Name(Vec<u8>),
    String(Vec<u8>),
    Array(Vec<Value>),
    Dict(Dict),
    Ref(u32),
    /// A keyword: `true`, `stream`, or a content stream's operator.
    Word(Vec<u8>),
}

type Dict = Vec<(Vec<u8>, Value)>;

fn get<'d>(dict: &'d Dict, key: &[u8]) -> Option<&'d Value> {
    dict.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn name(v: Option<&Value>) -> Option<&[u8]> {
    match v {
        Some(Value::Name(n)) => Some(n),
        _ => None,
    }
}

fn number(v: Option<&Value>) -> Option<usize> {
    match v {
        Some(Value::Number(n)) if *n >= 0.0 && n.fract() == 0.0 && *n < 1e12 => Some(*n as usize),
        _ => None,
    }
}

/// Where an object is: just after its `obj` in the file, or the `index`th of an object stream.
#[derive(Debug, Clone, Copy)]
enum At {
    File(usize),
    Packed { stream: u32, index: usize },
}

/// An object read: its value, and where its stream's data is in the file, where it is one.
#[derive(Debug, Clone)]
struct Object {
    value: Value,
    data: Option<std::ops::Range<usize>>,
}

/// An object stream decoded: its data, and each of its objects' offset within it.
type Packed = (Vec<u8>, Vec<usize>);

/// What a font says its strings' characters are.
enum Font {
    /// Codes of these widths in bytes, each the text a CMap says.
    Mapped { codes: BTreeMap<Vec<u8>, String>, widths: Vec<usize> },
    /// A byte a character, through an encoding; a byte it says nothing of is no character.
    Encoded(Box<[Option<char>; 256]>),
}

struct File<'a> {
    bytes: &'a [u8],
    bounds: &'a Bounds,
    at: BTreeMap<u32, At>,
    objects: BTreeMap<u32, Option<Rc<Object>>>,
    /// Each object stream decoded: its data, and each of its objects' offset within it.
    packed: BTreeMap<u32, Rc<Packed>>,
    fonts: BTreeMap<u32, Option<Rc<Font>>>,
    /// Where each `endstream` is, in order: what a stream whose `/Length` does not end it ends at,
    /// found by a search, never by scanning the file again for each stream.
    ends: Vec<usize>,
    /// Bytes looked at by the scans of the file and its streams' edges: a measure of the work done,
    /// which no file makes more than a few times its size.
    work: usize,
    streams: usize,
    total: usize,
    values: usize,
    operators: usize,
    codes: usize,
    out: String,
}

impl<'a> File<'a> {
    fn new(bytes: &'a [u8], bounds: &'a Bounds) -> Self {
        File {
            bytes,
            bounds,
            at: BTreeMap::new(),
            objects: BTreeMap::new(),
            packed: BTreeMap::new(),
            fonts: BTreeMap::new(),
            ends: Vec::new(),
            work: 0,
            streams: 0,
            total: 0,
            values: 0,
            operators: 0,
            codes: 0,
            out: String::new(),
        }
    }

    /// Every object's place: each `N G obj` in the file, a later one standing for an earlier, as an
    /// update writes them; then each object of an object stream that no `obj` of the file is. An
    /// encrypted file, its trailer or its cross-reference stream naming `/Encrypt`, is refused.
    fn index(&mut self) -> Result<(), String> {
        let b = self.bytes;
        // Each pass over the file goes on from where it found the last, never back to its start.
        let mut i = 0;
        while let Some(found) = find(&b[i..], b"endstream") {
            self.ends.push(i + found);
            i += found + 9;
        }
        self.work += b.len();
        let mut i = 0;
        while let Some(found) = find(&b[i..], b"obj") {
            let o = i + found;
            i = o + 3;
            if b.get(o + 3).is_some_and(|c| regular(*c)) || (o >= 3 && &b[o - 3..o] == b"end") {
                continue;
            }
            if let Some(id) = header(b, o, &mut self.work) {
                self.at.insert(id, At::File(o + 3));
                if self.at.len() > self.bounds.objects {
                    return Err(format!("it holds more than {} objects", self.bounds.objects));
                }
            }
        }
        self.work += b.len();
        let encrypted = |dict: &Dict| matches!(get(dict, b"Encrypt"), Some(Value::Ref(_) | Value::Dict(_)));
        let mut i = 0;
        while let Some(found) = find(&b[i..], b"trailer") {
            let at = i + found + 7;
            i = at;
            // Each trailer read counts against the values every object's do, so trailers written
            // inside one another read no more than the bound.
            if let Some(Value::Dict(trailer)) = Parser::new(b, at).value(self.bounds, &mut self.values)?
                && encrypted(&trailer)
            {
                return Err("it is encrypted".into());
            }
        }
        self.work += b.len();
        let tops: Vec<u32> = self.at.keys().copied().collect();
        for id in tops {
            let Some(object) = self.object(id)? else { continue };
            let Value::Dict(dict) = &object.value else { continue };
            if get(dict, b"Type") == Some(&Value::Name(b"XRef".to_vec())) && encrypted(dict) {
                return Err("it is encrypted".into());
            }
            if name(get(dict, b"Type")) != Some(b"ObjStm") {
                continue;
            }
            let (Some(n), Some(first)) = (number(get(dict, b"N")), number(get(dict, b"First"))) else { continue };
            let Some(data) = self.data(&object)? else { continue };
            let mut header = Parser::new(&data, 0);
            let mut offsets = Vec::new();
            for index in 0..n.min(self.bounds.objects) {
                let (Ok(Some(Value::Number(packed))), Ok(Some(Value::Number(at)))) =
                    (header.value(self.bounds, &mut self.values), header.value(self.bounds, &mut self.values))
                else {
                    break;
                };
                offsets.push(first.saturating_add(at.max(0.0) as usize));
                self.at.entry(packed as u32).or_insert(At::Packed { stream: id, index });
                if self.at.len() > self.bounds.objects {
                    return Err(format!("it holds more than {} objects", self.bounds.objects));
                }
            }
            self.packed.insert(id, Rc::new((data, offsets)));
        }
        Ok(())
    }

    /// The object `id`, read once; none where the file has none, or it does not read.
    fn object(&mut self, id: u32) -> Result<Option<Rc<Object>>, String> {
        if let Some(read) = self.objects.get(&id) {
            return Ok(read.clone());
        }
        // Held as none while it is read, so an object stream that names itself reads nothing.
        self.objects.insert(id, None);
        let read = match self.at.get(&id).copied() {
            None => None,
            Some(At::File(at)) => {
                let mut p = Parser::new(self.bytes, at);
                match p.value(self.bounds, &mut self.values)? {
                    Some(value) => {
                        let data = match &value {
                            Value::Dict(dict) => stream_data(self.bytes, p.at, dict, &self.ends, &mut self.work),
                            _ => None,
                        };
                        Some(Rc::new(Object { value, data }))
                    }
                    None => None,
                }
            }
            Some(At::Packed { stream, index }) => {
                if !self.packed.contains_key(&stream) {
                    None
                } else {
                    let packed = Rc::clone(&self.packed[&stream]);
                    let (data, offsets) = &*packed;
                    match offsets.get(index) {
                        Some(&at) if at < data.len() => Parser::new(data, at)
                            .value(self.bounds, &mut self.values)?
                            .map(|value| Rc::new(Object { value, data: None })),
                        _ => None,
                    }
                }
            }
        };
        self.objects.insert(id, read.clone());
        Ok(read)
    }

    /// `v`, its reference followed where it is one.
    fn resolve(&mut self, v: &Value) -> Result<Option<Value>, String> {
        match v {
            Value::Ref(id) => Ok(self.object(*id)?.map(|o| o.value.clone())),
            other => Ok(Some(other.clone())),
        }
    }

    /// The stream `v` names, decoded: none where it is no stream, or it is in a filter not read.
    fn stream(&mut self, v: &Value) -> Result<Option<(Dict, Vec<u8>)>, String> {
        let Value::Ref(id) = v else { return Ok(None) };
        let Some(object) = self.object(*id)? else { return Ok(None) };
        let Value::Dict(dict) = &object.value else { return Ok(None) };
        Ok(self.data(&object)?.map(|data| (dict.clone(), data)))
    }

    /// A stream's data decoded through its filters, within the bounds; none where it is no
    /// stream, it names a filter not read (an image's, LZW, run lengths), or it does not inflate.
    fn data(&mut self, object: &Object) -> Result<Option<Vec<u8>>, String> {
        let (Value::Dict(dict), Some(range)) = (&object.value, object.data.clone()) else { return Ok(None) };
        self.streams += 1;
        if self.streams > self.bounds.streams {
            return Err(format!("it holds more than {} streams", self.bounds.streams));
        }
        let names: Vec<Vec<u8>> = match self.resolve(get(dict, b"Filter").unwrap_or(&Value::Null))? {
            Some(Value::Name(n)) => vec![n],
            Some(Value::Array(all)) => {
                all.into_iter().filter_map(|v| if let Value::Name(n) = v { Some(n) } else { None }).collect()
            }
            _ => Vec::new(),
        };
        // Its data, as the file holds it, counts toward what the streams come to together: streams
        // that all run on to one `endstream` would else each be read to it.
        self.total += range.len();
        if self.total > self.bounds.total {
            return Err(format!("its streams run past {} together", size(self.bounds.total)));
        }
        let mut data = self.bytes[range].to_vec();
        for filter in names {
            data = match filter.as_slice() {
                b"FlateDecode" | b"Fl" => {
                    let mut out = Vec::new();
                    let read = flate2::read::ZlibDecoder::new(&data[..])
                        .take(self.bounds.inflated as u64 + 1)
                        .read_to_end(&mut out);
                    if out.len() > self.bounds.inflated {
                        return Err(format!("a stream inflates past {}", size(self.bounds.inflated)));
                    }
                    self.total += out.len();
                    if self.total > self.bounds.total {
                        return Err(format!("its streams run past {} together", size(self.bounds.total)));
                    }
                    if read.is_err() && out.is_empty() {
                        return Ok(None);
                    }
                    out
                }
                b"ASCII85Decode" | b"A85" => ascii85(&data),
                b"ASCIIHexDecode" | b"AHx" => ascii_hex(&data),
                _ => return Ok(None),
            };
        }
        Ok(Some(data))
    }

    /// Each page's words, in the order the file holds the pages.
    fn pages(&mut self) -> Result<(), String> {
        let ids: Vec<u32> = self.at.keys().copied().collect();
        for id in ids {
            let Some(object) = self.object(id)? else { continue };
            let Value::Dict(page) = &object.value else { continue };
            if name(get(page, b"Type")) != Some(b"Page") {
                continue;
            }
            let resources = self.resources(page)?;
            let mut content = Vec::new();
            let contents = match get(page, b"Contents") {
                Some(Value::Array(all)) => all.clone(),
                Some(Value::Ref(r)) => match self.object(*r)? {
                    Some(o) if matches!(o.value, Value::Array(_)) => match &o.value {
                        Value::Array(all) => all.clone(),
                        _ => Vec::new(),
                    },
                    _ => vec![Value::Ref(*r)],
                },
                _ => Vec::new(),
            };
            for part in contents {
                if let Some((_, data)) = self.stream(&part)? {
                    content.extend_from_slice(&data);
                    content.push(b'\n');
                }
            }
            self.draw(&content, &resources, 0)?;
            self.out.push('\n');
        }
        Ok(())
    }

    /// A page's resources: its own, else its nearest `/Parent`'s.
    fn resources(&mut self, page: &Dict) -> Result<Dict, String> {
        let mut node = page.clone();
        for _ in 0..PARENTS {
            if let Some(r) = get(&node, b"Resources").cloned()
                && let Some(Value::Dict(resources)) = self.resolve(&r)?
            {
                return Ok(resources);
            }
            match get(&node, b"Parent").cloned() {
                Some(parent) => match self.resolve(&parent)? {
                    Some(Value::Dict(up)) => node = up,
                    _ => break,
                },
                None => break,
            }
        }
        Ok(Vec::new())
    }

    /// The font `key` names in `resources`: none where it says nothing of its characters.
    fn font(&mut self, resources: &Dict, key: &[u8]) -> Result<Option<Rc<Font>>, String> {
        let Some(fonts) = get(resources, b"Font").cloned() else { return Ok(None) };
        let Some(Value::Dict(fonts)) = self.resolve(&fonts)? else { return Ok(None) };
        let Some(entry) = get(&fonts, key).cloned() else { return Ok(None) };
        let id = match entry {
            Value::Ref(id) => Some(id),
            _ => None,
        };
        if let Some(id) = id
            && let Some(read) = self.fonts.get(&id)
        {
            return Ok(read.clone());
        }
        let Some(Value::Dict(font)) = self.resolve(&entry)? else { return Ok(None) };
        let read = self.read_font(&font)?.map(Rc::new);
        if let Some(id) = id {
            self.fonts.insert(id, read.clone());
        }
        Ok(read)
    }

    fn read_font(&mut self, font: &Dict) -> Result<Option<Font>, String> {
        // Its ToUnicode CMap, where it has one, says its characters whatever its kind.
        if let Some(to) = get(font, b"ToUnicode").cloned()
            && let Some((_, data)) = self.stream(&to)?
        {
            let most = self.bounds.codes.saturating_sub(self.codes);
            let codes =
                cmap(&data, most).ok_or_else(|| format!("its CMaps map more than {} codes", self.bounds.codes))?;
            self.codes += codes.len();
            if !codes.is_empty() {
                let mut widths: Vec<usize> = codes.keys().map(Vec::len).collect();
                widths.sort_unstable();
                widths.dedup();
                return Ok(Some(Font::Mapped { codes, widths }));
            }
        }
        // A composite font says nothing of its characters without one.
        if name(get(font, b"Subtype")) == Some(b"Type0") {
            return Ok(None);
        }
        let encoding = match get(font, b"Encoding").cloned() {
            Some(e) => self.resolve(&e)?,
            None => None,
        };
        let standard = |n: &[u8]| matches!(n, b"WinAnsiEncoding" | b"MacRomanEncoding" | b"StandardEncoding");
        let mut table: Box<[Option<char>; 256]> = Box::new([None; 256]);
        let base = match &encoding {
            Some(Value::Name(n)) if standard(n) => true,
            Some(Value::Dict(e)) => name(get(e, b"BaseEncoding")).is_some_and(standard) || standard14(font),
            None => standard14(font),
            _ => false,
        };
        if base {
            for (b, slot) in table.iter_mut().enumerate().take(0x7f).skip(0x20) {
                *slot = Some(b as u8 as char);
            }
        }
        if let Some(Value::Dict(e)) = &encoding
            && let Some(Value::Array(differences)) = get(e, b"Differences")
        {
            let mut code = 0usize;
            for d in differences {
                match d {
                    Value::Number(n) => code = *n as usize,
                    Value::Name(glyph) => {
                        if code < 256 {
                            table[code] = glyph_char(glyph);
                        }
                        code += 1;
                    }
                    _ => {}
                }
            }
        }
        Ok(table.iter().any(Option::is_some).then_some(Font::Encoded(table)))
    }

    /// The words a content stream shows, in `resources`; a form it draws read in its own.
    fn draw(&mut self, content: &[u8], resources: &Dict, depth: usize) -> Result<(), String> {
        let mut p = Parser::new(content, 0);
        let mut operands: Vec<Value> = Vec::new();
        let mut font: Option<Rc<Font>> = None;
        while let Some(value) = p.value(self.bounds, &mut self.values)? {
            let Value::Word(op) = value else {
                if operands.len() < 64 {
                    operands.push(value);
                }
                continue;
            };
            self.operators += 1;
            if self.operators > self.bounds.operators {
                return Err(format!("it writes more than {} operators", self.bounds.operators));
            }
            match op.as_slice() {
                // An inline image's data, which is no content: passed over to its `EI`.
                b"ID" => {
                    let rest = &content[p.at.min(content.len())..];
                    p.at += rest
                        .windows(3)
                        .position(|w| w[0].is_ascii_whitespace() && &w[1..] == b"EI")
                        .map_or(rest.len(), |e| e + 3);
                }
                b"Tf" => {
                    font = match operands.first() {
                        Some(Value::Name(key)) => self.font(resources, key)?,
                        _ => None,
                    };
                }
                b"Tj" | b"'" | b"\"" => {
                    if op != b"Tj" {
                        self.out.push('\n');
                    }
                    if let Some(Value::String(s)) = operands.last() {
                        self.show(s, font.as_deref())?;
                    }
                }
                b"TJ" => {
                    if let Some(Value::Array(items)) = operands.last() {
                        for item in items {
                            match item {
                                Value::String(s) => self.show(s, font.as_deref())?,
                                // A gap of more than a fifth of an em is a space.
                                Value::Number(n) if *n < -200.0 => self.out.push(' '),
                                _ => {}
                            }
                        }
                    }
                }
                b"ET" | b"Td" | b"TD" | b"T*" | b"Tm" => self.out.push(' '),
                b"Do" if depth < FORMS => {
                    if let Some(Value::Name(key)) = operands.first()
                        && let Some(objects) = get(resources, b"XObject").cloned()
                        && let Some(Value::Dict(objects)) = self.resolve(&objects)?
                        && let Some(form) = get(&objects, key).cloned()
                        && let Some((dict, data)) = self.stream(&form)?
                        && name(get(&dict, b"Subtype")) == Some(b"Form")
                    {
                        let own = match get(&dict, b"Resources").cloned() {
                            Some(r) => match self.resolve(&r)? {
                                Some(Value::Dict(own)) => own,
                                _ => resources.clone(),
                            },
                            None => resources.clone(),
                        };
                        self.draw(&data, &own, depth + 1)?;
                    }
                }
                _ => {}
            }
            operands.clear();
        }
        Ok(())
    }

    /// `raw`, a string shown in `font`, as words: each character its font says, none it does not.
    fn show(&mut self, raw: &[u8], font: Option<&Font>) -> Result<(), String> {
        match font {
            None => {}
            Some(Font::Encoded(table)) => self.out.extend(raw.iter().filter_map(|b| table[*b as usize])),
            Some(Font::Mapped { codes, widths }) => {
                let mut at = 0;
                'codes: while at < raw.len() {
                    for &width in widths {
                        if let Some(text) = raw.get(at..at + width).and_then(|code| codes.get(code)) {
                            self.out.push_str(text);
                            at += width;
                            continue 'codes;
                        }
                    }
                    at += widths.first().copied().unwrap_or(1);
                }
            }
        }
        if self.out.len() > self.bounds.text {
            return Err(format!("its words run past {}", size(self.bounds.text)));
        }
        Ok(())
    }
}

/// Whether a simple font with no encoding of its own is one of the fourteen standard fonts that
/// are not symbols, which take the standard encoding.
fn standard14(font: &Dict) -> bool {
    let Some(base) = name(get(font, b"BaseFont")) else { return false };
    [&b"Helvetica"[..], b"Times", b"Courier"].iter().any(|f| base.starts_with(f))
}

/// The character a glyph's name stands for, of the names a figure and its words are drawn in.
fn glyph_char(glyph: &[u8]) -> Option<char> {
    if let [one] = glyph
        && one.is_ascii_alphabetic()
    {
        return Some(*one as char);
    }
    if let Some(hex) = glyph.strip_prefix(b"uni").filter(|h| h.len() == 4) {
        return u32::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok().and_then(char::from_u32);
    }
    const NAMES: [(&[u8], char); 30] = [
        (b"zero", '0'),
        (b"one", '1'),
        (b"two", '2'),
        (b"three", '3'),
        (b"four", '4'),
        (b"five", '5'),
        (b"six", '6'),
        (b"seven", '7'),
        (b"eight", '8'),
        (b"nine", '9'),
        (b"space", ' '),
        (b"period", '.'),
        (b"comma", ','),
        (b"percent", '%'),
        (b"dollar", '$'),
        (b"hyphen", '-'),
        (b"minus", '\u{2212}'),
        (b"colon", ':'),
        (b"semicolon", ';'),
        (b"slash", '/'),
        (b"plus", '+'),
        (b"parenleft", '('),
        (b"parenright", ')'),
        (b"quoteright", '\u{2019}'),
        (b"quoteleft", '\u{2018}'),
        (b"sterling", '£'),
        (b"Euro", '€'),
        (b"yen", '¥'),
        (b"endash", '\u{2013}'),
        (b"emdash", '\u{2014}'),
    ];
    NAMES.iter().find(|(n, _)| *n == glyph).map(|(_, c)| *c)
}

/// The object number of the `N G obj` that ends just before `o`, where the bytes before it read so.
/// `work` counts the bytes it looks at.
fn header(b: &[u8], o: usize, work: &mut usize) -> Option<u32> {
    let mut i = o;
    let skip_space = |i: &mut usize| {
        let from = *i;
        while *i > 0 && b[*i - 1].is_ascii_whitespace() && from - *i < SPACE {
            *i -= 1;
        }
    };
    let digits = |i: &mut usize| {
        let end = *i;
        while *i > 0 && b[*i - 1].is_ascii_digit() && end - *i < 10 {
            *i -= 1;
        }
        (*i < end).then(|| std::str::from_utf8(&b[*i..end]).ok()?.parse::<u64>().ok()).flatten()
    };
    skip_space(&mut i);
    if digits(&mut i).is_none() {
        *work += o - i;
        return None;
    }
    skip_space(&mut i);
    let id = digits(&mut i);
    *work += o - i;
    let id = id?;
    (i == 0 || !regular(b[i - 1])).then_some(u32::try_from(id).ok()?)
}

/// How far white space is looked through, between a header's numbers, before a stream's data,
/// or after it: a writer puts a few bytes there.
const SPACE: usize = 256;

/// A stream's data, after a dictionary that ends at `at`: its `/Length` where written as a number
/// and the stream ends there, else up to the first of `ends`, every `endstream`'s place, after it.
/// `work` counts the bytes it looks at.
fn stream_data(b: &[u8], at: usize, dict: &Dict, ends: &[usize], work: &mut usize) -> Option<std::ops::Range<usize>> {
    let rest = b.get(at..)?;
    let lead = rest.iter().take(SPACE).take_while(|c| c.is_ascii_whitespace()).count();
    *work += lead + 6;
    if !rest[lead..].starts_with(b"stream") {
        return None;
    }
    let mut start = at + lead + 6;
    if b.get(start) == Some(&b'\r') {
        start += 1;
    }
    if b.get(start) == Some(&b'\n') {
        start += 1;
    }
    let by_length = number(get(dict, b"Length")).and_then(|n| start.checked_add(n)).filter(|&end| {
        *work += SPACE.min(b.len().saturating_sub(end));
        end <= b.len() && b[end..].iter().take(SPACE).skip_while(|c| c.is_ascii_whitespace()).take(9).eq(b"endstream")
    });
    let end = by_length.or_else(|| ends.get(ends.partition_point(|&e| e < start)).copied())?;
    Some(start..end)
}

/// A parser of PDF values, its nesting held on a stack of its own.
struct Parser<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Parser<'a> {
    fn new(b: &'a [u8], at: usize) -> Self {
        Parser { b, at }
    }

    /// The next value, a reference read as one (`12 0 R`); none at the end. `values` counts each
    /// value parsed against the bounds.
    fn value(&mut self, bounds: &Bounds, values: &mut usize) -> Result<Option<Value>, String> {
        // Each array or dictionary open, with what it holds so far.
        let mut open: Vec<(bool, Vec<Value>)> = Vec::new();
        loop {
            let Some(token) = self.token() else {
                // An array or a dictionary never closed ends where the data does.
                while let Some((dict, items)) = open.pop() {
                    let done = if dict { Value::Dict(pairs(items)) } else { Value::Array(items) };
                    match open.last_mut() {
                        Some((_, outer)) => outer.push(done),
                        None => return Ok(Some(done)),
                    }
                }
                return Ok(None);
            };
            *values += 1;
            if *values > bounds.values {
                return Err(format!("it holds more than {} values", bounds.values));
            }
            let done = match token {
                Token::Open(dict) => {
                    if open.len() >= DEPTH {
                        return Err(format!("it nests past {DEPTH} deep"));
                    }
                    open.push((dict, Vec::new()));
                    continue;
                }
                Token::Close(dict) => match open.pop() {
                    Some((was, items)) if was == dict => {
                        if dict {
                            Value::Dict(pairs(items))
                        } else {
                            Value::Array(items)
                        }
                    }
                    Some((was, items)) => {
                        open.push((was, items));
                        continue;
                    }
                    None => continue,
                },
                Token::Value(Value::Word(w)) if w == b"R" => {
                    // `N G R`: the two numbers before it in what is open are a reference.
                    if let Some((_, items)) = open.last_mut()
                        && let [.., Value::Number(id), Value::Number(_)] = items.as_slice()
                    {
                        let id = *id as u32;
                        items.truncate(items.len() - 2);
                        items.push(Value::Ref(id));
                    }
                    continue;
                }
                Token::Value(v) => {
                    if open.is_empty()
                        && let Value::Number(id) = v
                    {
                        // At the top, a number may be the first of `N G R`.
                        let save = self.at;
                        if let (Some(Token::Value(Value::Number(_))), Some(Token::Value(Value::Word(r)))) =
                            (self.token(), self.token())
                            && r == b"R"
                        {
                            return Ok(Some(Value::Ref(id as u32)));
                        }
                        self.at = save;
                    }
                    v
                }
            };
            match open.last_mut() {
                Some((_, items)) => items.push(done),
                None => return Ok(Some(done)),
            }
        }
    }

    fn token(&mut self) -> Option<Token> {
        let d = self.b;
        while self.at < d.len() {
            let b = d[self.at];
            match b {
                b'%' => {
                    while self.at < d.len() && d[self.at] != b'\n' && d[self.at] != b'\r' {
                        self.at += 1;
                    }
                }
                _ if b.is_ascii_whitespace() || b == 0 => self.at += 1,
                b'(' => return Some(Token::Value(Value::String(self.literal()))),
                b'<' if d.get(self.at + 1) == Some(&b'<') => {
                    self.at += 2;
                    return Some(Token::Open(true));
                }
                b'>' if d.get(self.at + 1) == Some(&b'>') => {
                    self.at += 2;
                    return Some(Token::Close(true));
                }
                b'<' => {
                    self.at += 1;
                    let start = self.at;
                    while self.at < d.len() && d[self.at] != b'>' {
                        self.at += 1;
                    }
                    let hex = ascii_hex(&d[start..self.at]);
                    self.at += 1;
                    return Some(Token::Value(Value::String(hex)));
                }
                b'[' => {
                    self.at += 1;
                    return Some(Token::Open(false));
                }
                b']' => {
                    self.at += 1;
                    return Some(Token::Close(false));
                }
                b'/' => {
                    self.at += 1;
                    let start = self.at;
                    while self.at < d.len() && regular(d[self.at]) {
                        self.at += 1;
                    }
                    return Some(Token::Value(Value::Name(unescaped(&d[start..self.at]))));
                }
                b'>' | b')' | b'{' | b'}' => self.at += 1,
                _ => {
                    let start = self.at;
                    while self.at < d.len() && regular(d[self.at]) {
                        self.at += 1;
                    }
                    let word = &d[start..self.at];
                    if word[0].is_ascii_digit() || matches!(word[0], b'-' | b'+' | b'.') {
                        if let Some(n) = std::str::from_utf8(word).ok().and_then(|w| w.parse::<f64>().ok()) {
                            return Some(Token::Value(Value::Number(n)));
                        }
                        continue;
                    }
                    return Some(Token::Value(match word {
                        b"null" => Value::Null,
                        _ => Value::Word(word.to_vec()),
                    }));
                }
            }
        }
        None
    }

    /// A literal string from its `(`, its parentheses counted, never kept on a stack.
    fn literal(&mut self) -> Vec<u8> {
        let d = self.b;
        let mut depth = 0usize;
        let mut out = Vec::new();
        self.at += 1;
        while self.at < d.len() {
            let c = d[self.at];
            self.at += 1;
            match c {
                b'\\' => {
                    let Some(&e) = d.get(self.at) else { break };
                    self.at += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match d.get(self.at) {
                                    Some(&o @ b'0'..=b'7') => {
                                        v = v * 8 + u32::from(o - b'0');
                                        self.at += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(v as u8);
                        }
                        b'\r' | b'\n' => {}
                        other => out.push(other),
                    }
                }
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' if depth == 0 => break,
                b')' => {
                    depth -= 1;
                    out.push(c);
                }
                _ => out.push(c),
            }
        }
        out
    }
}

enum Token {
    Value(Value),
    /// `<<` (a dictionary) or `[`.
    Open(bool),
    Close(bool),
}

fn pairs(items: Vec<Value>) -> Dict {
    let mut out = Vec::new();
    let mut items = items.into_iter();
    while let Some(key) = items.next() {
        let Value::Name(key) = key else { continue };
        out.push((key, items.next().unwrap_or(Value::Null)));
    }
    out
}

/// A name's `#xx` escapes read.
fn unescaped(name: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len());
    let mut i = 0;
    while i < name.len() {
        if name[i] == b'#'
            && let Some(v) =
                name.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok())
        {
            out.push(v);
            i += 3;
        } else {
            out.push(name[i]);
            i += 1;
        }
    }
    out
}

/// The codes the ToUnicode CMap `data` maps, each by its bytes, within `most`; none where it maps
/// more.
fn cmap(data: &[u8], most: usize) -> Option<BTreeMap<Vec<u8>, String>> {
    let mut codes = BTreeMap::new();
    let mut p = Parser::new(data, 0);
    let mut section: Option<bool> = None; // Some(true): bfchar; Some(false): bfrange.
    let mut held: Vec<Value> = Vec::new();
    let mut values = 0usize;
    let unbounded = Bounds { values: usize::MAX, ..BOUNDS };
    while let Ok(Some(value)) = p.value(&unbounded, &mut values) {
        match &value {
            Value::Word(w) if w == b"beginbfchar" => (section, held) = (Some(true), Vec::new()),
            Value::Word(w) if w == b"beginbfrange" => (section, held) = (Some(false), Vec::new()),
            Value::Word(w) if w == b"endbfchar" || w == b"endbfrange" => section = None,
            _ => {
                let Some(chars) = section else { continue };
                held.push(value);
                match (chars, held.as_slice()) {
                    (true, [Value::String(code), Value::String(to)]) => {
                        codes.insert(code.clone(), utf16(to));
                        held.clear();
                    }
                    (true, [_, _]) => held.clear(),
                    (false, [Value::String(lo), Value::String(hi), Value::String(to)]) => {
                        let (a, z) = (be_number(lo), be_number(hi));
                        if z < a || z - a > 0xffff || codes.len() + (z - a) as usize >= most {
                            return None;
                        }
                        for (i, code) in (a..=z).enumerate() {
                            codes.insert(be(code, lo.len()), utf16(&step(to, i as u32)));
                        }
                        held.clear();
                    }
                    (false, [Value::String(lo), Value::String(_), Value::Array(each)]) => {
                        let mut code = be_number(lo);
                        for to in each {
                            if let Value::String(to) = to {
                                codes.insert(be(code, lo.len()), utf16(to));
                            }
                            code = code.wrapping_add(1);
                        }
                        held.clear();
                    }
                    (false, [_, _, _]) => held.clear(),
                    _ => {}
                }
                if codes.len() > most {
                    return None;
                }
            }
        }
    }
    Some(codes)
}

fn be_number(bytes: &[u8]) -> u32 {
    bytes.iter().take(4).fold(0u32, |n, b| (n << 8) | u32::from(*b))
}

fn be(code: u32, width: usize) -> Vec<u8> {
    (0..width.clamp(1, 4)).rev().map(|i| (code >> (8 * i)) as u8).collect()
}

/// UTF-16BE `bytes` with `n` added to its last unit, as a range's codes step.
fn step(bytes: &[u8], n: u32) -> Vec<u8> {
    let mut out = bytes.to_vec();
    if out.len() >= 2 {
        let at = out.len() - 2;
        let unit = u16::from_be_bytes([out[at], out[at + 1]]).wrapping_add(n as u16);
        out[at..].copy_from_slice(&unit.to_be_bytes());
    }
    out
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
    String::from_utf16_lossy(&units)
}

/// ASCII85 `data` decoded, up to its `~>`.
fn ascii85(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 4 / 5);
    let mut group = [0u8; 5];
    let mut n = 0;
    let value = |g: &[u8; 5]| g.iter().fold(0u32, |v, &d| v.wrapping_mul(85).wrapping_add(u32::from(d)));
    for &b in data {
        match b {
            b'~' => break,
            b'z' if n == 0 => out.extend_from_slice(&[0; 4]),
            b'!'..=b'u' => {
                group[n] = b - b'!';
                n += 1;
                if n == 5 {
                    out.extend_from_slice(&value(&group).to_be_bytes());
                    n = 0;
                }
            }
            _ => {}
        }
    }
    if n > 1 {
        for slot in group.iter_mut().skip(n) {
            *slot = 84;
        }
        out.extend_from_slice(&value(&group).to_be_bytes()[..n - 1]);
    }
    out
}

/// ASCIIHex `data` decoded, up to its `>`.
fn ascii_hex(data: &[u8]) -> Vec<u8> {
    let nibbles: Vec<u8> =
        data.iter().take_while(|b| **b != b'>').filter_map(|b| (*b as char).to_digit(16).map(|n| n as u8)).collect();
    nibbles.chunks(2).map(|p| (p[0] << 4) | p.get(1).copied().unwrap_or(0)).collect()
}

/// Whether `b` is a regular character of a PDF token: neither white space nor a delimiter.
fn regular(b: u8) -> bool {
    !b.is_ascii_whitespace() && !matches!(b, 0 | b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

/// `bytes` as a person reads a size: in MB from one, else in KB.
fn size(bytes: usize) -> String {
    if bytes >= 1 << 20 { format!("{} MB", bytes >> 20) } else { format!("{} KB", bytes.div_ceil(1024)) }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A PDF of `objects`, each its dictionary and its stream's data where it has one, numbered
    /// from 1; the trailer `trailer`.
    fn pdf(objects: &[(&str, Option<Vec<u8>>)], trailer: &str) -> Vec<u8> {
        let mut out = b"%PDF-1.7\n".to_vec();
        for (i, (dict, data)) in objects.iter().enumerate() {
            match data {
                Some(data) => {
                    write!(out, "{} 0 obj\n<< {dict} /Length {} >>\nstream\n", i + 1, data.len()).unwrap();
                    out.extend_from_slice(data);
                    out.extend_from_slice(b"\nendstream\nendobj\n");
                }
                None => write!(out, "{} 0 obj\n{dict}\nendobj\n", i + 1).unwrap(),
            }
        }
        write!(out, "trailer\n<< /Root 1 0 R {trailer} >>\n%%EOF\n").unwrap();
        out
    }

    fn deflated(data: &[u8]) -> Vec<u8> {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        z.write_all(data).unwrap();
        z.finish().unwrap()
    }

    /// A page drawing `content` in three fonts: `/F1`, a composite font whose ToUnicode maps
    /// glyph ids 0x10–0x19 to the digits and a few more; `/F2`, a composite font with none; and
    /// `/F3`, Helvetica in the standard encoding.
    fn page(content: &[u8]) -> Vec<u8> {
        let cmap = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
            1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
            1 beginbfrange <0010> <0019> <0030> endbfrange\n\
            2 beginbfchar <0003> <0020> <0024> <0025> endbfchar\n\
            1 beginbfrange <0040> <0041> [<0041> <0042>] endbfrange\n\
            endcmap CMapName currentdict /CMap defineresource pop end end";
        pdf(
            &[
                ("<< /Type /Catalog /Pages 2 0 R >>", None),
                (
                    "<< /Type /Pages /Kids [3 0 R] /Count 1 /Resources << /Font << /F1 5 0 R /F2 6 0 R /F3 7 0 R >> >> >>",
                    None,
                ),
                ("<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>", None),
                ("/Filter /FlateDecode", Some(deflated(content))),
                (
                    "<< /Type /Font /Subtype /Type0 /BaseFont /AAAAAA+Sans /Encoding /Identity-H /ToUnicode 8 0 R >>",
                    None,
                ),
                ("<< /Type /Font /Subtype /Type0 /BaseFont /AAAAAB+Sans /Encoding /Identity-H >>", None),
                ("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>", None),
                ("", Some(cmap.to_vec())),
            ],
            "",
        )
    }

    const SMALL: Bounds = Bounds {
        objects: 200,
        streams: 50,
        inflated: 1 << 20,
        total: 2 << 20,
        values: 100_000,
        operators: 1000,
        codes: 500,
        text: 4096,
    };

    /// Each string is read in its own font, found through the page's resources and its parent's:
    /// through its ToUnicode CMap, a range, an array of codes, and single codes alike; in the
    /// standard encoding a simple font takes; and not at all in a composite font with no CMap, which
    /// says nothing of its characters, never guessed. A `TJ` gap of more than a fifth of an em is a
    /// space.
    #[test]
    fn a_string_reads_in_its_own_font() {
        let content = b"BT /F1 12 Tf 72 700 Td <0024001200140011001100030040> Tj [<0041> -400 <0013>] TJ \
            /F2 12 Tf <00120014> Tj /F3 12 Tf (Up 7.5% in 2024) Tj ET";
        let words = words("glyphs.pdf", &page(content)).unwrap();
        assert!(words.contains("%2411 A"), "{words:?}");
        assert!(words.contains("B 3"), "{words:?}");
        assert!(words.contains("Up 7.5% in 2024"), "{words:?}");
        // F2's <00120014> would read 24 through F1's CMap: no word of it is read.
        assert_eq!(words.matches("24").count(), 2, "2411 and 2024 alone: {words:?}");
    }

    /// Three PDFs as the tools people make them with write them, each saying dogfood 17's
    /// paragraph of figures: Chrome's (a TrueType font by glyph ids, Identity-H, with its ToUnicode;
    /// PDF 1.4 with a table), pdflatex's (Computer Modern, Type 1 in its built-in encoding with a
    /// ToUnicode, and the dollar in a Type 3 font; PDF 1.5, its objects in object streams and a
    /// cross-reference stream), and Chrome's again rewritten by qpdf into object streams. Each figure
    /// the paragraph writes is read from each, but the dollar signs pdflatex draws in a Type 3 font
    /// with no ToUnicode.
    #[test]
    fn three_kinds_of_pdf_read_as_their_figures() {
        let figures = [
            "25.8%", "2025", "31.4%", "2016", "7,560", "4,370", "22%", "14%", "9", "6.2", "1,500", "2.1", "30%", "2040",
        ];
        for file in ["chrome.pdf", "latex.pdf", "objstm.pdf"] {
            let bytes = std::fs::read(format!("{}/tests/pdfs/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap();
            let words = words(file, &bytes).unwrap();
            let missed: Vec<&str> = figures.iter().copied().filter(|f| !words.contains(f)).collect();
            assert!(missed.is_empty(), "{file} misses {missed:?}: {words:?}");
        }
    }

    /// Dogfood 17's report, as reportlab writes one, each stream ASCII85 over Flate, in Helvetica.
    #[test]
    fn a_report_reads_as_its_figures() {
        let bytes =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/examples/dogfood-17/report.pdf")).unwrap();
        let words = words("report.pdf", &bytes).unwrap();
        for said in ["25.8%", "31.4%", "7,560", "4,370", "$6.2 million", "1,500", "2040", "Eastside"] {
            assert!(words.contains(said), "{said} in {words:?}");
        }
        assert!(!words.contains("2041"), "and no figure it does not draw");
    }

    /// Hostile files are refused at a bound, naming it, or read as what they hold: never a hang, a
    /// run out of memory, or a stack overflow.
    #[test]
    fn a_hostile_pdf_is_refused_at_a_bound() {
        let refused = |bytes: &[u8]| words_within("hostile.pdf", bytes, &SMALL).unwrap_err().to_string();
        // A deflate bomb: 64 MB of zeros in 64 KB, refused at a stream's bound before it is held.
        let bomb = page(&vec![b' '; 64 << 20]);
        assert!(bomb.len() < 128 << 10);
        assert_eq!(refused(&bomb), "hostile.pdf: its words were not read: a stream inflates past 1 MB");
        // Streams each within the bound, past it together.
        let parts: Vec<(&str, Option<Vec<u8>>)> =
            std::iter::once(("<< /Type /Page /Contents [2 0 R 3 0 R 4 0 R] >>", None))
                .chain((0..3).map(|_| ("/Filter /Fl", Some(deflated(&vec![b' '; 900 << 10])))))
                .collect();
        assert!(refused(&pdf(&parts, "")).ends_with("its streams run past 2 MB together"));
        // More streams than are read.
        let refs: String = (2..=61).map(|i| format!("{i} 0 R ")).collect();
        let head = format!("<< /Type /Page /Contents [{refs}] >>");
        let many: Vec<(&str, Option<Vec<u8>>)> =
            std::iter::once((head.as_str(), None)).chain((0..60).map(|_| ("", Some(b"q Q".to_vec())))).collect();
        assert!(refused(&pdf(&many, "")).ends_with("more than 50 streams"));
        // An operator flood.
        assert!(
            refused(&page(format!("BT {} ET", "q Q ".repeat(1000)).as_bytes())).ends_with("more than 1000 operators")
        );
        // A CMap that maps every code a range can.
        let wide = pdf(
            &[
                ("<< /Type /Page /Resources << /Font << /F1 2 0 R >> >> /Contents 3 0 R >>", None),
                ("<< /Type /Font /Subtype /Type0 /ToUnicode 4 0 R >>", None),
                ("", Some(b"BT /F1 1 Tf <0001> Tj ET".to_vec())),
                ("", Some(b"begincmap 1 beginbfrange <0000> <FFFF> <0041> endbfrange endcmap".to_vec())),
            ],
            "",
        );
        assert!(refused(&wide).ends_with("more than 500 codes"));
        // Words past what is kept.
        let long = page(format!("BT /F3 1 Tf ({}) Tj ET", "9".repeat(5000)).as_bytes());
        assert_eq!(refused(&long), "hostile.pdf: its words were not read: its words run past 4 KB");
        // An object nested a million deep, refused at its depth; strings opened a million deep in a
        // content stream, read once over with no stack kept.
        let deep = [b"1 0 obj\n".to_vec(), vec![b'['; 1 << 20], b"\nendobj\n".to_vec()].concat();
        assert!(refused(&deep).ends_with(&format!("it nests past {DEPTH} deep")));
        let wide_open = [b"BT /F3 1 Tf ".to_vec(), vec![b'('; 1 << 20], b" Tj ET".to_vec()].concat();
        assert!(
            words_within(
                "deep.pdf",
                &page(&wide_open),
                &Bounds { values: 1 << 22, inflated: 4 << 20, total: 8 << 20, text: 4 << 20, ..SMALL }
            )
            .is_ok()
        );
        // More objects than are read, and more values.
        let objects: String = (1..=300).map(|i| format!("{i} 0 obj null endobj\n")).collect();
        assert!(refused(objects.as_bytes()).ends_with("more than 200 objects"));
        let values = format!("1 0 obj [{}] endobj", "0 ".repeat(200_000));
        assert!(refused(values.as_bytes()).ends_with("more than 100000 values"));
        // Encrypted, by its trailer: no words.
        let locked = pdf(&[("<< /Type /Page >>", None)], "/Encrypt << /Filter /Standard >>");
        assert!(refused(&locked).ends_with("it is encrypted"));
    }

    /// Streams whose `/Length` lies, as many as a few MB holds, all a page's contents: each is found
    /// to end at the next `endstream` by a search of where every one is, never by scanning the file
    /// again, so the work done is a few times the file's size; with none, each is no stream. And
    /// streams that all run on to one `endstream` at the file's end are refused once the data they
    /// come to passes the bound, never each read to its end.
    #[test]
    fn streams_whose_length_lies_are_read_in_one_pass() {
        let lying = |end: &[u8]| {
            let refs: String = (2..=20_001).map(|k| format!("{k} 0 R ")).collect();
            let mut f = format!("%PDF-1.7\n1 0 obj << /Type /Page /Contents [{refs}] >> endobj\n").into_bytes();
            for k in 2..=20_001 {
                f.extend_from_slice(format!("{k} 0 obj << /Length 999999999 >> stream\n").as_bytes());
                f.extend_from_slice(&[b'A'; 400]);
                f.extend_from_slice(b"\nendobj\n");
            }
            f.extend_from_slice(end);
            f
        };
        let none = lying(b"%%EOF\n");
        assert!(none.len() > 8 << 20);
        let mut file = File::new(&none, &BOUNDS);
        file.index().and_then(|()| file.pages()).unwrap();
        assert_eq!(file.out.trim(), "");
        assert!(file.work <= 4 * none.len(), "{} bytes looked at for a file of {}", file.work, none.len());
        let one = lying(b"endstream\n%%EOF\n");
        let e = words("lying.pdf", &one).unwrap_err().to_string();
        assert_eq!(e, "lying.pdf: its words were not read: its streams run past 256 MB together");
    }

    /// Random bytes, and each real PDF cut short anywhere, read as what they hold or are refused:
    /// never a panic.
    #[test]
    fn noise_and_truncation_never_panic() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for len in [0, 1, 7, 64, 512, 4096, 65_536] {
            for _ in 0..8 {
                let noise: Vec<u8> = (0..len).map(|_| next() as u8).collect();
                let _ = words_within("noise.pdf", &noise, &SMALL);
                // Noise dressed as a PDF's parts.
                let parts = [&b"1 0 obj << /Type /Page /Contents 2 0 R >> endobj 2 0 obj << /Length 9 /Filter /FlateDecode >> stream\n"[..], &noise, b"\nendstream endobj trailer << >>"].concat();
                let _ = words_within("noise.pdf", &parts, &SMALL);
            }
        }
        for file in ["chrome.pdf", "latex.pdf", "objstm.pdf"] {
            let bytes = std::fs::read(format!("{}/tests/pdfs/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap();
            for cut in (0..bytes.len()).step_by(97) {
                let _ = words(file, &bytes[..cut]);
                let mut torn = bytes.clone();
                torn[cut] ^= 0xff;
                let _ = words(file, &torn);
            }
        }
    }
}
