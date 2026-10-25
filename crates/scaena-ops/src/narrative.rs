//! A deck made from a narrative (ADR-0026, PLAN G.1). The spine comes first: each run of slides
//! in one beat of the arc is a section, and each slide a beat, its title the claim, its speaker
//! notes the notes, its length Narrato's estimate, a chart's data its evidence, and the rest of
//! Narrato's fields in `media.narrato`. Then each beat's state: a slide in the theme layout its
//! Narrato layout comes to, chosen by how many items it has where a layout comes in sizes, after
//! Narrato's variety pass. Each slot is filled from the beat's fields: words in the slot's role,
//! a list where it waits for one, a picture slot the picture made for the slide (PLAN G.10) with
//! the visual concept as its description, a chart's figures a data source of its own. A slide
//! with no picture made for it is drawn in a layout that waits for none. A slot with nothing to
//! say is left out, and the card under it.

use crate::create::{Attach, Create, Created, attaching, creating};
use crate::lint::{View, Write, lint_in};
use crate::{Context, OpsError};
use base64::Engine;
use scaena_core::inserts::slug;
use scaena_core::model::nodes::TextFit;
use scaena_core::model::theme::{Example, Prompt, PromptItems, PromptShapeKind, Slot, Theme};
use scaena_core::model::values::Range;
use scaena_core::{Deck, Finding};
pub use scaena_narrato::brief::Brief;
use scaena_narrato::fit::Long;
use scaena_narrato::layouts::{self, Varied};
use scaena_narrato::look::{Change, Choice, Seen};
use scaena_narrato::pictures::{self, Frame, Look, Palette};
use scaena_narrato::{
    ChartData, Document, Generation, Icon, Narrative, Picture, Provenance, Provider, Request, Settings, Slide, Step,
};
pub use scaena_sources::Given;
use scaena_sources::{Source, Table};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The canvas a made deck draws on.
const CANVAS: [u32; 2] = [1920, 1080];

/// What a narrative made: the deck, as `deck.json` holds it, less its theme and fonts, which
/// [`creating`] sets; and the layout each slide took. In the browser it crosses from the
/// generator's module, which builds it, to the editor's, which makes it a bundle, as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Made {
    pub deck: Value,
    pub layouts: Vec<String>,
    /// The pictures the deck shows, by the path the bundle keeps each at (PLAN G.10): carried
    /// beside the JSON, as bytes.
    #[serde(skip)]
    pub assets: BTreeMap<String, Vec<u8>>,
}

/// A picture made for a slide (PLAN G.10): the path the bundle keeps it at, and what it shows,
/// for a reader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placed {
    pub src: String,
    pub alt: String,
}

/// The pictures made for a narrative's slides, by the slide's index.
pub type Pictures = BTreeMap<usize, Placed>;

/// A deck generated: the bundle made, as `deck_create` reports one; the arc it follows, and why
/// the heuristic chose it where the model's choice did not come back; each slide's layout; the
/// findings whose fixes were applied, each checked by laying its state out again; and the texts
/// the model rewrote to fit their places, or why it did not.
#[derive(Debug, Clone, Serialize)]
pub struct Generated {
    pub created: Created,
    pub arc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    pub layouts: Vec<String>,
    pub fixed: Vec<Finding>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fitted: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unfitted: Option<String>,
    /// How many pictures were made for its slides (PLAN G.10).
    #[serde(skip_serializing_if = "is_zero")]
    pub pictures: usize,
    /// Why each picture asked for and not made was not.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unpictured: Vec<String>,
    /// What the model changed on the deck it saw drawn, slide by slide (PLAN G.13).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub looked: Vec<Change>,
    /// Why the model did not look at the deck drawn, or what it asked was not kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlooked: Option<String>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// What a request a generation asks its client to make answered: its status, 0 where it never
/// came back; its status line's words, or why it did not come back; its body; and its
/// `Retry-After`, where it has one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reply {
    pub status: u16,
    pub status_text: String,
    pub body: String,
    pub after: Option<String>,
}

/// How a client makes a generation's calls: one at a time, or several at once where it can
/// (PLAN G.10), and a pause before asking a busy provider again.
pub trait Transport {
    /// Make `request`, and hand back what it answered.
    fn send(&mut self, request: &Request) -> Reply;
    /// Make each of `requests`, at once where the client can, and hand back what each answered,
    /// in their order.
    fn send_all(&mut self, requests: &[Request]) -> Vec<Reply> {
        requests.iter().map(|r| self.send(r)).collect()
    }
    /// Wait `seconds` before asking again.
    fn pause(&mut self, _seconds: f64) {}
}

/// A function that makes one request is a transport that makes them one at a time, and never
/// waits.
impl<F: FnMut(&Request) -> Reply> Transport for F {
    fn send(&mut self, request: &Request) -> Reply {
        self(request)
    }
}

/// Who makes a deck's pictures, with whose key, and as what (PLAN G.10).
#[derive(Debug, Clone)]
pub struct Painter<'a> {
    pub settings: pictures::Settings,
    pub key: &'a str,
    pub look: Look,
}

/// How many times a picture is asked for while its provider is busy: Vzestup's four, and two
/// more for a key whose rate limit counts pictures by the minute; the slides' too (REL-1).
const TRIES: usize = scaena_narrato::TRIES;
/// The longest wait between two asks, in seconds.
const LONGEST_WAIT: f64 = scaena_narrato::LONGEST_WAIT;

/// The pictures `asked` for, made by `painter` through `transport` ([`Painting`]): every one at
/// once, each asked again while its provider is busy, up to six times in all, waiting as
/// [`Painting::answer`] says between. What each made is kept at, and the reason each not made
/// was not.
pub fn painting(
    asked: &[Asked],
    painter: &Painter,
    transport: &mut dyn Transport,
) -> (Pictures, BTreeMap<String, Vec<u8>>, Vec<String>) {
    let mut p = Painting::new(asked.to_vec(), painter.settings.clone());
    loop {
        let requests = p.requests(painter.key);
        if requests.is_empty() {
            break;
        }
        let wait = p.answer(transport.send_all(&requests));
        if wait > 0.0 {
            transport.pause(wait);
        }
    }
    p.finish()
}

/// Pictures being made (PLAN G.10), the client making the calls: [`Painting::requests`] gives
/// each round's, every picture not yet made at once; [`Painting::answer`] reads their replies in
/// the same order and says how long to wait before the next round. A picture is asked again
/// while its provider is busy, up to six times in all and after a wait that doubles from two
/// seconds (or the provider's own, a minute at most), and again as a PNG for a model that makes
/// no JPEG; one refused is left out; an answer that says no picture can be made with the key
/// stops the rest. [`Painting::finish`] keeps each made at `assets/picture-NN.EXT`, NN its
/// slide's number.
#[derive(Debug, Clone)]
pub struct Painting {
    asked: Vec<Asked>,
    settings: pictures::Settings,
    made: Vec<Option<pictures::Picture>>,
    why: Vec<Option<String>>,
    plain: Vec<bool>,
    stopped: Option<String>,
    round: usize,
    /// Which pictures the last round asked for, by their place in `asked`.
    open: Vec<usize>,
}

impl Painting {
    pub fn new(asked: Vec<Asked>, settings: pictures::Settings) -> Painting {
        let n = asked.len();
        Painting {
            asked,
            settings,
            made: vec![None; n],
            why: vec![None; n],
            plain: vec![false; n],
            stopped: None,
            round: 0,
            open: Vec::new(),
        }
    }

    /// The requests this round makes, with `key`: one for each picture not yet made; none once
    /// every picture is made or given up, or the key is refused.
    pub fn requests(&mut self, key: &str) -> Vec<Request> {
        self.open = (0..self.asked.len()).filter(|i| self.made[*i].is_none() && self.why[*i].is_none()).collect();
        if self.stopped.is_some() || self.round >= TRIES {
            self.open.clear();
        }
        (self.open.iter())
            .map(|&i| pictures::request(&self.settings, key, &self.asked[i].prompt, self.asked[i].frame, self.plain[i]))
            .collect()
    }

    /// The replies to this round's requests, in their order: each picture made kept, and how
    /// many seconds to wait before the next round, 0 where it need not wait.
    pub fn answer(&mut self, replies: Vec<Reply>) -> f64 {
        let provider = self.settings.provider;
        let last = self.round + 1 == TRIES;
        let mut wait: f64 = 0.0;
        for (&i, reply) in self.open.iter().zip(replies) {
            match pictures::read(provider, reply.status, &reply.status_text, &reply.body, reply.after.as_deref()) {
                Ok(picture) => self.made[i] = Some(picture),
                Err(pictures::Unmade::Again { why, after }) => {
                    wait = wait.max(after.unwrap_or((2u32 << self.round) as f64).min(LONGEST_WAIT));
                    if last {
                        self.why[i] = Some(why);
                    }
                }
                Err(pictures::Unmade::Plain(why)) => {
                    if self.plain[i] {
                        self.why[i] = Some(why);
                    }
                    self.plain[i] = true;
                }
                Err(pictures::Unmade::Skipped(why)) => self.why[i] = Some(why),
                Err(pictures::Unmade::Stopped(why)) => self.stopped = Some(why),
            }
        }
        self.round += 1;
        if self.stopped.is_some() || self.round >= TRIES { 0.0 } else { wait }
    }

    /// How many pictures are asked for.
    pub fn asked(&self) -> usize {
        self.asked.len()
    }

    /// The pictures made, by their slides, each at the path the bundle keeps it at and with what
    /// it shows; their bytes, by those paths; and the reason each picture not made was not, by its
    /// slide where it was asked for one.
    pub fn finish(self) -> (Pictures, BTreeMap<String, Vec<u8>>, Vec<String>) {
        let mut placed = Pictures::new();
        let mut assets = BTreeMap::new();
        let mut unmade = Vec::new();
        for (i, (a, made)) in self.asked.iter().zip(self.made).enumerate() {
            match made {
                Some(picture) => {
                    let src = format!("assets/picture-{:02}.{}", a.slide + 1, picture.ext);
                    assets.insert(src.clone(), picture.bytes);
                    placed.insert(a.slide, Placed { src, alt: a.alt.clone() });
                }
                None => {
                    let why = (self.why[i].clone())
                        .or_else(|| self.stopped.clone())
                        .unwrap_or_else(|| "it was not asked for".into());
                    unmade.push(if a.alone { why } else { format!("slide {}: {why}", a.slide + 1) });
                }
            }
        }
        (placed, assets, unmade)
    }
}

/// How far a generation has come, for its client to say as it goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Progress {
    /// Arc selection is asked.
    Choosing,
    /// The arc is chosen: by the model, or by the heuristic where the model's choice did not
    /// come back, saying why.
    Chose { arc: String, fallback: Option<String> },
    /// The slides are asked, at least this many.
    Writing { slides: usize },
    /// The provider was busy, failed for now, or did not answer: the same step is asked again
    /// after `wait` seconds, `tried` times so far (narrative REL-1).
    Retrying { wait: f64, tried: usize },
    /// The slides came back, cut short or whole: `text` is the answer as the model wrote it, what
    /// `--slides` takes, for a client that keeps it so a run that fails later resumes from it
    /// (narrative REL-2).
    Wrote { slides: usize, truncated: bool, text: String },
    /// Texts that run past their place are asked to be rewritten.
    Fitting { texts: usize },
    /// The rewrites came back, these many kept.
    Fitted { texts: usize },
    /// Pictures are asked for, these many (PLAN G.10).
    Painting { pictures: usize },
    /// The pictures came back: these many made, and why each other was not.
    Painted { pictures: usize, unmade: Vec<String> },
    /// The deck is shown to the model as it is drawn, these many slides (PLAN G.13).
    Looking { slides: usize },
    /// What the model asked came back: these many slides changed.
    Looked { slides: usize },
}

/// Whether a generation shows the model the deck as it is drawn (PLAN G.13): not at all; by a
/// call, where it has a key; or with what a model answered elsewhere, as `--slides` gives the
/// slides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Looking<'a> {
    No,
    Asked,
    Answered(&'a str),
}

/// How wide each slide is drawn for the model to see, in pixels: its words legible, its
/// pictures few enough tokens.
pub const LOOK_WIDTH: u32 = 1024;

/// The most slides shown to the model in one look request (narrative PERF-5): a dozen, so a
/// long deck is shown in rounds, each small enough to upload from an iPad and under what a
/// provider takes as images in one request, and a round refused loses the others' changes no
/// more than its own.
pub const LOOK_BATCH: usize = 12;

/// The rounds a look at `slides` slides goes in: each the indices of [`LOOK_BATCH`] slides at
/// most, in order.
pub fn look_rounds(slides: usize) -> impl Iterator<Item = std::ops::Range<usize>> {
    (0..slides).step_by(LOOK_BATCH).map(move |start| start..(start + LOOK_BATCH).min(slides))
}

/// What the sources a deck is grounded in give a generation (PLAN G.3, G.4): their texts packed
/// for Narrato's prompt as Vzestup packs them, each PDF said by its name there and handed to the
/// model as a document, and what each gave.
#[derive(Debug, Clone)]
pub struct Grounding {
    pub content: Option<String>,
    pub documents: Vec<Document>,
    pub given: Vec<Given>,
}

/// `sources` packed as Vzestup packs them (`scaena_sources::ground`): the first given the room
/// first, the whole held to Narrato's million bytes, a source cut short rather than dropped,
/// each PDF in base64 as a provider takes a document. A source with no words, or PDFs past what
/// `provider` takes in one request, is refused.
pub fn grounding(sources: &[Source], provider: Provider) -> Result<Grounding, OpsError> {
    let ground = scaena_sources::ground(sources, provider.document_bytes(), provider.name())
        .map_err(|e| OpsError::new(e.to_string()))?;
    let documents = (ground.pdfs.iter())
        .map(|(name, pdf)| Document {
            name: name.clone(),
            media_type: "application/pdf".into(),
            data: base64::engine::general_purpose::STANDARD.encode(pdf),
        })
        .collect();
    Ok(Grounding { content: ground.content, documents, given: ground.given })
}

/// What each source given made of the prompt, as the deck keeps it beside its brief (narrative
/// FUNC-3): its name, kind, address where it has one, bytes, and whether it was cut short.
pub fn provenance(given: &[Given]) -> Vec<Provenance> {
    (given.iter())
        .map(|g| Provenance {
            name: g.name.clone(),
            kind: g.kind.to_string(),
            url: g.url.clone(),
            bytes: g.bytes,
            truncated: g.truncated,
        })
        .collect()
}

/// How a beat cites the source its slide names (narrative FUNC-3): by the source's address where
/// it has one, else `source:NAME`, which W423 passes as it passes an address; none where the
/// slide names a source the deck was not grounded in. A name is matched as written, then with
/// case and spaces set aside, then as the source's address.
fn cited(sources: &[Provenance], named: &str) -> Option<String> {
    let loose = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let source = (sources.iter().find(|s| s.name == named))
        .or_else(|| sources.iter().find(|s| loose(&s.name) == loose(named)))
        .or_else(|| sources.iter().find(|s| s.url.as_deref() == Some(named)))?;
    Some(source.url.clone().unwrap_or_else(|| format!("{}{}", scaena_core::document::SOURCE_CITED, source.name)))
}

/// A data source kept from a sheet, or why it was not.
#[derive(Debug, Clone, Serialize)]
pub struct Kept {
    pub sheet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// `write`, a bundle not yet written, with each sheet given kept as a data source
/// (`scaena_sources::tables`), as `data_attach` keeps a file (`data/ID.csv`, declared and
/// typed), so a chart can plot it.
pub fn keeping(mut write: Write, tables: &[Table]) -> Result<(Write, Vec<Kept>), OpsError> {
    let mut kept = Vec::new();
    for table in tables {
        let not = |why: String| Kept { sheet: table.named.clone(), id: None, source: None, rows: None, why: Some(why) };
        let Some(csv) = table.csv.clone() else {
            kept.push(not("no row under its header".into()));
            continue;
        };
        let bundle = unwritten(&write);
        let base = slug(&table.hint, "data");
        let id = (1..)
            .map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") })
            .find(|id| !bundle.deck.data.contains_key(id))
            .expect("some number is free");
        let req = Attach { id: id.clone(), file: PathBuf::from(format!("{id}.csv")), schema: None, parse: None };
        match attaching(&bundle, &req, csv) {
            Ok((attached, Some(w))) if attached.attached => {
                write.deck = w.deck;
                write.files.extend(w.files);
                let (source, rows) = (Some(attached.source), Some(attached.rows));
                kept.push(Kept { sheet: table.named.clone(), id: Some(id), source, rows, why: None });
            }
            Ok((attached, _)) => {
                kept.push(not(attached.added.first().map_or("it would not validate".into(), |f| f.message.clone())))
            }
            Err(e) => kept.push(not(e.to_string())),
        }
    }
    Ok((write, kept))
}

/// The bundle `write` makes, before it is written: its deck, and the files beside it.
fn unwritten(write: &Write) -> crate::Bundle {
    let theme_json = match &write.deck.theme {
        Some(Value::String(file)) => write.files.get(file).map(|t| String::from_utf8_lossy(t).into_owned()),
        Some(inline @ Value::Object(_)) => Some(inline.to_string()),
        _ => None,
    };
    crate::Bundle {
        root: PathBuf::from("."),
        deck_file: "deck.json".into(),
        deck: write.deck.clone(),
        theme_json,
        files: scaena_store::Files::Zip(std::sync::Arc::new(write.files.clone())),
        author: "user".into(),
    }
}

/// A deck generated from start to end (PLAN G.2): each request `generation` asks for made by
/// `transport`, the client's own, and its answer read; then, with a `painter`, the pictures its
/// slides ask for made ([`asking`], [`painting`], PLAN G.10); then the bundle [`generating`]
/// makes in the theme, each slide with a picture drawn beside it; then, with a key, each text
/// that still runs past its place rewritten by the model to the length the engine measured for
/// it ([`overflowing`]), and lint's fixes applied again. A rewrite that fails leaves the deck as
/// it was, saying why; a picture not made leaves its slide in a layout that waits for none. A
/// step whose provider is busy, fails for now, or does not answer is asked again after the wait
/// the generation says, up to [`scaena_narrato::TRIES`] times (REL-1), as the pictures are.
/// `say` hears how far it has come. Nothing is written.
#[allow(clippy::too_many_arguments)]
pub fn generate(
    generation: Generation,
    key: Option<&str>,
    documents: &[Document],
    painter: Option<&Painter>,
    looking: Looking,
    theme_file: &Path,
    theme: String,
    font: &dyn Fn(&str) -> Result<Vec<u8>, OpsError>,
    transport: &mut dyn Transport,
    say: &mut dyn FnMut(Progress),
) -> Result<(Generated, Option<Write>), OpsError> {
    let draw = |write: &Write| drawn(write, LOOK_WIDTH);
    generate_drawn(generation, key, documents, painter, looking, theme_file, theme, font, transport, say, &draw)
}

/// [`generate`], each slide drawn for the model to see by `draw` (the CPU painter's JPEGs, as
/// [`generate`] draws them; a browser draws by its own module). The look is a polish on a deck
/// the model has written and validated (narrative STAB-3): a `draw` that fails, as a deck the
/// model asked for that cannot be made again, leaves the deck as it was, saying why in
/// `unlooked`, and is never an error.
#[allow(clippy::too_many_arguments)]
pub fn generate_drawn(
    mut generation: Generation,
    key: Option<&str>,
    documents: &[Document],
    painter: Option<&Painter>,
    looking: Looking,
    theme_file: &Path,
    theme: String,
    font: &dyn Fn(&str) -> Result<Vec<u8>, OpsError>,
    transport: &mut dyn Transport,
    say: &mut dyn FnMut(Progress),
    draw: &dyn Fn(&Write) -> Result<Vec<Picture>, OpsError>,
) -> Result<(Generated, Option<Write>), OpsError> {
    while let Some(request) = key.and_then(|key| generation.request(key, documents)) {
        let step = generation.step();
        say(if step == Step::Arc { Progress::Choosing } else { Progress::Writing { slides: generation.target() } });
        let reply = transport.send(&request);
        (generation.answer(reply.status, &reply.status_text, &reply.body, reply.after.as_deref()))
            .map_err(|e| OpsError::new(e.to_string()))?;
        if let Some(wait) = generation.again() {
            say(Progress::Retrying { wait, tried: generation.tried() });
            transport.pause(wait);
            continue;
        }
        if step == Step::Arc {
            let arc = generation.arc().map(|a| a.selection.clone()).unwrap_or_default();
            say(Progress::Chose { arc, fallback: generation.fallback().map(str::to_string) });
        }
    }
    // Each icon the slides name, found in the set that ships (G.23).
    #[cfg(feature = "shipped")]
    generation.give_icons(icons_found(&generation.icon_words()));
    let narrative = (generation.narrative().cloned())
        .ok_or_else(|| OpsError::new("the narrative is not written: a key asks the model for it"))?;
    let text = generation.written_text().unwrap_or_default().to_string();
    say(Progress::Wrote { slides: narrative.slides.len(), truncated: narrative.truncated, text });
    let model: Theme = serde_json::from_str(&theme).context("the theme")?;
    let asked = painter.map(|p| asking(&narrative, &model, p.look)).unwrap_or_default();
    let (pictures, assets, unmade) = match painter {
        Some(painter) if !asked.is_empty() => {
            say(Progress::Painting { pictures: asked.len() });
            let painted = painting(&asked, painter, transport);
            say(Progress::Painted { pictures: painted.0.len(), unmade: painted.2.clone() });
            painted
        }
        _ => Default::default(),
    };
    let mut built = deck(&narrative, &model, &pictures)?;
    built.assets = assets.clone();
    let (mut made, write) = making(built, &narrative.arc, theme_file, theme.clone(), font)?;
    made.fallback = generation.fallback().map(str::to_string);
    made.pictures = pictures.len();
    made.unpictured = unmade;
    let Some(mut write) = write else { return Ok((made, None)) };
    // The deck seen as it is drawn (PLAN G.13): what the model would change, each slide's change
    // kept where lint finds the slide no worse for it.
    let seen = seeing(&narrative, &model, &pictures, &made.layouts);
    // A round that is not answered loses its own slides' changes alone, said beside the rest.
    let mut unanswered: Option<String> = None;
    let changes = match (looking, key) {
        (Looking::Answered(text), _) => Some(scaena_narrato::look::read(text, &seen).map_err(OpsError::new)),
        (Looking::Asked, Some(key)) => match draw(&write) {
            Ok(drawn) => {
                let mut changes = Vec::new();
                let mut asked = false;
                for round in look_rounds(seen.len()) {
                    let Some(request) = generation.look_request(key, &seen[round.clone()], &drawn[round.clone()])
                    else {
                        continue;
                    };
                    if !asked {
                        say(Progress::Looking { slides: seen.len() });
                        asked = true;
                    }
                    let reply = transport.send(&request);
                    match generation.look_answer(reply.status, &reply.status_text, &reply.body, &seen[round.clone()]) {
                        Ok(answered) => changes.extend(answered),
                        Err(why) => {
                            let (first, last) = (seen[round.start].slide, seen[round.end - 1].slide);
                            let why = match round.len() {
                                1 => format!("slide {first} not looked at: {why}"),
                                _ => format!("slides {first}\u{2013}{last} not looked at: {why}"),
                            };
                            unanswered = Some(unanswered.map_or(why.clone(), |was| format!("{was}; {why}")));
                        }
                    }
                }
                match (asked, changes.is_empty(), unanswered.take()) {
                    (false, _, _) => None,
                    (true, true, Some(why)) => Some(Err(OpsError::new(why))),
                    (true, _, why) => {
                        unanswered = why;
                        Some(Ok(changes))
                    }
                }
            }
            // A slide that would not draw for the model to see: the deck stands as made.
            Err(why) => Some(Err(OpsError::new(format!("the slides could not be drawn for the model: {why}")))),
        },
        _ => None,
    };
    match changes {
        Some(Ok(changes)) => {
            let shown = Shown { narrative: &narrative, theme: &model, pictures: &pictures, assets: &assets };
            match looked(shown, changes, (made.clone(), write.clone()), theme_file, &theme, font) {
                Ok(after) => (made, write) = after,
                // The deck the model asked for could not be made again: the one made stands.
                Err(why) => made.unlooked = Some(format!("the deck the model asked for could not be made: {why}")),
            }
            say(Progress::Looked { slides: made.looked.len() });
        }
        Some(Err(why)) => made.unlooked = Some(why.to_string()),
        None => {}
    }
    if let Some(why) = unanswered {
        made.unlooked = Some(made.unlooked.map_or(why.clone(), |was| format!("{was}; {why}")));
    }
    let longs = overflowing(&write.deck, &made.created.findings)?;
    let (Some(key), false) = (key, longs.is_empty()) else { return Ok((made, Some(write))) };
    let Some(request) = generation.fit_request(key, &longs) else { return Ok((made, Some(write))) };
    say(Progress::Fitting { texts: longs.len() });
    let reply = transport.send(&request);
    let rewrites = match generation.fit_answer(reply.status, &reply.status_text, &reply.body, &longs) {
        Ok(rewrites) => rewrites,
        Err(why) => {
            made.unfitted = Some(why.to_string());
            return Ok((made, Some(write)));
        }
    };
    say(Progress::Fitted { texts: rewrites.len() });
    let write = fitted(&mut made, write, rewrites)?;
    Ok((made, Some(write)))
}

/// A deck made from slides written elsewhere, with no call (`scaena generate --slides`,
/// `deck_generate`): `slides` is the JSON Narrato writes the slides in (`executiveSummary` and
/// `slides`, each with its `slideTitle`, `subtitle`, `layoutSuggestion`, `keyBullets`,
/// `speakerNotes`, `beatIndex`, `beatName`, `intensity`, `tone`, …), as an agent writes them;
/// `brief` says what the deck is about and for whom, which health reads back (PLAN G.9); `arc`
/// names one of Narrato's, else the heuristic chooses from the brief. The bundle is made at `dir`,
/// a directory not there yet or empty, in `theme`, a theme that ships or a theme file read,
/// as [`generate`] makes one: the spine, each slide in the theme's layout its layout comes to,
/// its builds, and the `media.narrato` fields health reads, lint's fixes applied. A deck in a
/// script the theme's fonts do not draw is refused here: `scaena generate` fetches the
/// script's font (PLAN G.22), which no server's tool does.
pub fn from_slides(
    dir: &Path,
    theme: crate::create::ThemeSource,
    slides: &str,
    brief: Brief,
    arc: Option<&str>,
) -> Result<Generated, OpsError> {
    crate::create::vacant(dir)?;
    let font = |file: &str| theme.font(file);
    let language = brief.language.as_deref();
    if let Some(script) = crate::scripts::needed(language, slides, &theme.text, &font)? {
        return Err(OpsError::new(format!(
            "a deck in {} needs {}, which the theme's fonts do not draw: `scaena generate --slides` fetches it (PLAN G.22)",
            script.language, script.family
        )));
    }
    writable(language, &theme.text, theme.shipped, &font)?;
    if let Some(name) = arc
        && !scaena_narrato::arcs::arcs().iter().any(|a| a.arc.eq_ignore_ascii_case(name))
    {
        let names: Vec<&str> = scaena_narrato::arcs::arcs().iter().map(|a| a.arc.as_str()).collect();
        return Err(OpsError::new(format!("`{name}` is no arc of Narrato's: {}", names.join(", "))));
    }
    // No call is made: the provider and the model name nothing.
    let settings = Settings { provider: Provider::Anthropic, model: String::new(), base: None, browser: false };
    let mut generation = Generation::new(brief, settings).map_err(|e| OpsError::new(e.to_string()))?;
    generation.choose_arc(arc);
    generation.written(slides, false).map_err(|e| OpsError::new(format!("the slides: {e}")))?;
    let mut never =
        |_: &Request| Reply { status_text: "no call is made from written slides".into(), ..Reply::default() };
    let (made, write) = generate(
        generation,
        None,
        &[],
        None,
        Looking::No,
        &theme.file,
        theme.text.clone(),
        &font,
        &mut never,
        &mut |_| {},
    )?;
    if let Some(write) = write {
        crate::create::make(dir, write)?;
    }
    Ok(made)
}

/// `write` with `rewrites`, each a text node's id and the words the model rewrote it in to fit
/// its place ([`overflowing`]), set in their places, and each fix lint then offers applied, as
/// [`generating`] applies them; what `made` says follows.
pub fn fitted(made: &mut Generated, mut write: Write, rewrites: Vec<(String, String)>) -> Result<Write, OpsError> {
    if rewrites.is_empty() {
        return Ok(write);
    }
    let mut doc = write.deck.to_value()?;
    for (id, text) in &rewrites {
        let lang = doc["meta"]["lang"].as_str().unwrap_or("en").to_string();
        doc["nodes"][id]["text"] = json!(typeset(text, &lang));
    }
    let nothing = scaena_store::Files::Zip(Default::default());
    let view = View { base: &nothing, pending: write.files.clone() };
    let deck = Deck::from_json(&doc.to_string()).context("the deck rewritten to fit")?;
    let found = lint_in(&deck, &view)?.findings;
    let settled = settle(doc, found, &view)?;
    made.fixed.extend(settled.fixed);
    made.fitted = rewrites.into_iter().map(|(id, _)| id).collect();
    made.created.errors = errors(&settled.findings);
    made.created.findings = settled.findings;
    write.deck = settled.deck;
    Ok(write)
}

/// What a deck was made from, for making it again (PLAN G.13): the narrative, the theme as a
/// model, the pictures placed by slide, and their bytes.
#[derive(Clone, Copy)]
pub struct Shown<'a> {
    pub narrative: &'a Narrative,
    pub theme: &'a Theme,
    pub pictures: &'a Pictures,
    pub assets: &'a BTreeMap<String, Vec<u8>>,
}

/// What the model is shown of the deck `narrative` made in `theme` with `pictures` (PLAN G.13):
/// each slide with the layout `layouts` says it took, the others it may take ([`others`]), and
/// its words as written.
pub fn seeing(narrative: &Narrative, theme: &Theme, pictures: &Pictures, layouts: &[String]) -> Vec<Seen> {
    let choice = |id: &str| {
        let layout = theme.layouts.get(id);
        Choice {
            id: id.to_string(),
            name: layout.and_then(|l| l.name.clone()).unwrap_or_else(|| id.to_string()),
            draws: layout.and_then(|l| l.description.clone()).unwrap_or_default(),
        }
    };
    let varied = varied(narrative);
    let count = narrative.slides.len();
    // What each slide must leave the audience with (lift-up ATTR-2): its beat's claim in the
    // outline the person settled, by the beat the deck's sections give it. A slide that names its
    // part takes none, so its title stays the part's name: a divider is numbered by its title
    // naming an agenda item, and the spine's sections are titled by the dividers.
    let claim = |i: usize, slide: &Slide| {
        let outlined = narrative.outline.as_ref().and_then(|o| {
            let beat = (slide.beat_index.filter(|b| *b < o.beats.len()))
                .unwrap_or_else(|| scaena_narrato::beats::beat_of(i, count, o.beats.len()));
            o.beats.get(beat).map(|b| b.claim.clone())
        });
        outlined.filter(|c| !c.trim().is_empty()).unwrap_or_default()
    };
    (narrative.slides.iter().zip(&varied).zip(layouts).enumerate())
        .map(|(i, ((slide, v), own))| {
            let items = items(slide, v);
            let iconed = icons_of(narrative, slide, &items).is_some();
            let options = others(v.layout.identity(), slide, &items, theme, pictures.contains_key(&i), iconed, own);
            // The cover, a divider (a Q&A's among them, by its structural layout), the agenda,
            // and the close.
            let framing = matches!(v.layout.layout, "title" | "section" | "agenda" | "closing");
            Seen {
                slide: i + 1,
                layout: choice(own),
                options: options.iter().map(|o| choice(o)).collect(),
                title: slide.slide_title.clone(),
                subtitle: slide.subtitle.clone(),
                bullets: slide.key_bullets.clone(),
                claim: if framing { String::new() } else { claim(i, slide) },
                // What it does, said as that where no claim is known.
                purpose: if framing { String::new() } else { slide.purpose.clone().unwrap_or_default() },
            }
        })
        .collect()
}

/// The layouts a slide may take in place of `own`, the one it took (PLAN G.13): for a slide of
/// items that a layout of places sets as well as a list, the theme's layouts with a place for
/// each item that set them as they are written (each under its label where each has one, each
/// as its figure where each has one, each under its icon where each is `iconed`, G.23), and a
/// list of them; none for a cover, a divider, a quote, a chart, a slide with a picture, or a
/// slide whose layout reads its items in its own way.
fn others(
    identity: &str,
    slide: &Slide,
    items: &[Item],
    theme: &Theme,
    pictured: bool,
    iconed: bool,
    own: &str,
) -> Vec<String> {
    let free = [
        "body",
        "numbered",
        "two-column",
        "before-after",
        "steps",
        "next-steps",
        "timeline",
        "milestones",
        "scorecard",
        "kpi-grid",
        "gallery",
        "image-text",
        "callout",
        "how-it-works",
        "pipeline",
    ];
    if pictured || slide.chart_data.is_some() || !free.contains(&identity) {
        return Vec::new();
    }
    let n = items.len();
    let labelled = n > 0 && items.iter().all(|i| i.label.is_some());
    let figures = n > 0 && items.iter().all(|i| i.figure_first());
    let ordered = ["steps", "next-steps", "timeline", "milestones", "how-it-works", "pipeline"].contains(&identity);
    let mut wanted: Vec<&str> = Vec::new();
    if figures {
        wanted.extend(match n {
            2 => &["two-numbers"][..],
            3 => &["numbers"],
            4 => &["scorecard"],
            6 => &["number-grid"],
            _ => &[],
        });
    }
    if labelled {
        wanted.extend(match (n, ordered) {
            (2, _) => &["comparison", "myth-fact", "two-columns"][..],
            (3, true) => &["how-it-works", "three-columns", "cards", "three-rows"],
            (3, false) => &["three-columns", "cards", "takeaways", "three-rows"],
            (4, true) => &["numbered-cards", "steps", "process", "four-points"],
            (4, false) => &["numbered-cards", "quadrants", "four-points"],
            _ => &[],
        });
        // Each under the icon its word names, where every one names one (G.23).
        if iconed && !ordered {
            wanted.extend(match n {
                3 => &["icon-columns", "icon-cards"][..],
                4 => &["icon-rows", "icon-grid"],
                5 | 6 => &["icon-grid"],
                _ => &[],
            });
        }
    }
    wanted.extend(if ordered { ["numbered", "bullets"] } else { ["bullets", "numbered"] });
    let mut out: Vec<String> = Vec::new();
    for l in wanted {
        if l != own && theme.layouts.contains_key(l) && holds(theme, l, n) && !out.iter().any(|o| o == l) {
            out.push(l.to_string());
        }
    }
    out
}

/// `narrative` with the words `changes` tighten, and the layout each asks for, by its slide's
/// index (PLAN G.13).
pub fn changed(narrative: &Narrative, changes: &[Change]) -> (Narrative, BTreeMap<usize, String>) {
    let mut out = narrative.clone();
    let mut layouts = BTreeMap::new();
    for c in changes {
        let Some(i) = c.slide.checked_sub(1).filter(|i| *i < out.slides.len()) else { continue };
        let slide = &mut out.slides[i];
        if let Some(title) = &c.title {
            slide.slide_title = title.clone();
        }
        if let Some(subtitle) = &c.subtitle {
            slide.subtitle = subtitle.clone();
        }
        if let Some(bullets) = &c.bullets {
            slide.key_bullets = bullets.clone();
        }
        if let Some(layout) = &c.layout {
            layouts.insert(i, layout.clone());
        }
    }
    (out, layouts)
}

/// The deck `shown` makes again with `changes` (PLAN G.13), in the theme whose file is
/// `theme_file` and whose JSON is `theme`, each font read by `font`; `before`, what it made
/// without them. Each slide's change is kept where lint finds the slide no worse for it
/// ([`worse`]); the rest are let go, and the deck made once more. Where lint still finds a slide
/// worse, or the deck will not validate, `before` stands, saying why.
pub fn looked(
    shown: Shown,
    changes: Vec<Change>,
    before: (Generated, Write),
    theme_file: &Path,
    theme: &str,
    font: &dyn Fn(&str) -> Result<Vec<u8>, OpsError>,
) -> Result<(Generated, Write), OpsError> {
    if changes.is_empty() {
        return Ok(before);
    }
    let make = |changes: &[Change]| -> Result<Option<(Generated, Write)>, OpsError> {
        let (narrative, layouts) = changed(shown.narrative, changes);
        let mut built = deck_as(&narrative, shown.theme, shown.pictures, &layouts)?;
        built.assets = shown.assets.clone();
        let (made, write) = making(built, &shown.narrative.arc, theme_file, theme.to_string(), font)?;
        Ok(write.map(|w| (made, w)))
    };
    let unchanged = |mut before: (Generated, Write), why: &str| {
        before.0.unlooked = Some(why.to_string());
        Ok(before)
    };
    let invalid = "the deck the model asked for would not validate";
    let Some(after) = make(&changes)? else { return unchanged(before, invalid) };
    let refused = worse((&before.0, &before.1), (&after.0, &after.1));
    if refused.is_empty() {
        return Ok(adopt(before.0, after, changes, &[]));
    }
    let kept = kept(changes, &refused);
    if kept.is_empty() {
        return unchanged(before, &unkept(refused.len()));
    }
    let Some(again) = make(&kept)? else { return unchanged(before, invalid) };
    if !worse((&before.0, &before.1), (&again.0, &again.1)).is_empty() {
        return unchanged(before, "lint found a slide the model changed worse for it");
    }
    Ok(adopt(before.0, again, kept, &refused))
}

/// What lint finds wrong with each slide of a made deck, in all its states: its errors, and its
/// warnings.
fn wrongs(made: &Generated, write: &Write) -> Vec<(usize, usize)> {
    use scaena_core::Severity;
    (write.deck.slides().into_iter())
        .map(|states| {
            let on = || {
                made.created
                    .findings
                    .iter()
                    .filter(|f| states.iter().any(|k| f.state.as_deref() == Some(write.deck.states[*k].id.as_str())))
            };
            (
                on().filter(|f| f.severity == Severity::Error).count(),
                on().filter(|f| f.severity == Severity::Warning).count(),
            )
        })
        .collect()
}

/// The slides, by index, that lint finds worse in `after` than in `before`, the same deck made
/// again with what the model asked (PLAN G.13): more errors, or more warnings (words that run
/// past their places, too many words, a collision).
pub fn worse(before: (&Generated, &Write), after: (&Generated, &Write)) -> Vec<usize> {
    let (was, now) = (wrongs(before.0, before.1), wrongs(after.0, after.1));
    (0..was.len().min(now.len())).filter(|i| now[*i].0 > was[*i].0 || now[*i].1 > was[*i].1).collect()
}

/// `changes` less those on the slides `refused` names by index.
pub fn kept(mut changes: Vec<Change>, refused: &[usize]) -> Vec<Change> {
    changes.retain(|c| !c.slide.checked_sub(1).is_some_and(|index| refused.contains(&index)));
    changes
}

/// The slides `refused` names by index, kept as made, said: "slide 14 kept as made: lint found
/// it worse for the change".
pub fn as_made(refused: &[usize]) -> String {
    let numbers: Vec<String> = refused.iter().map(|i| (i + 1).to_string()).collect();
    match numbers.as_slice() {
        [one] => format!("slide {one} kept as made: lint found it worse for the change"),
        [rest @ .., last] => {
            format!("slides {} and {last} kept as made: lint found them worse for the change", rest.join(", "))
        }
        [] => String::new(),
    }
}

/// Why changes on `n` slides were not kept.
pub fn unkept(n: usize) -> String {
    match n {
        1 => "lint found the slide the model changed worse for it".into(),
        n => format!("lint found the {n} slides the model changed worse for it"),
    }
}

/// The deck `after` made with the model's `kept` changes, standing for `before`'s: the arc's
/// fallback and the pictures carried over, and `refused` said.
pub fn adopt(before: Generated, after: (Generated, Write), kept: Vec<Change>, refused: &[usize]) -> (Generated, Write) {
    let (mut made, write) = after;
    made.fallback = before.fallback;
    made.pictures = before.pictures;
    made.unpictured = before.unpictured;
    made.looked = kept;
    if !refused.is_empty() {
        made.unlooked = Some(as_made(refused));
    }
    (made, write)
}

/// Each slide of the deck `write` makes, whole at its last state and at rest, drawn `width`
/// pixels wide by the CPU painter, as a JPEG, in their order: what the model looks at (PLAN
/// G.13).
pub fn drawn(write: &Write, width: u32) -> Result<Vec<Picture>, OpsError> {
    let mut drawing = Drawing::new(write)?;
    (write.deck.slides().into_iter())
        .map(|states| {
            let state = &write.deck.states[*states.last().expect("a slide has a state")];
            let raster = drawing.draw(&state.id, width)?;
            let jpeg = scaena_export::pdf::jpeg(&raster.rgba, raster.width, raster.height, 82)
                .ok_or_else(|| OpsError::new(format!("state `{}` would not encode as a JPEG", state.id)))?;
            Ok(Picture {
                caption: String::new(),
                media_type: "image/jpeg".into(),
                data: base64::engine::general_purpose::STANDARD.encode(jpeg),
            })
        })
        .collect()
}

/// The state `state` of the deck `write` makes, at rest, drawn `width` pixels wide by the CPU
/// painter, opaque: what a page makes a JPEG of for the model to see (PLAN G.13).
pub fn pixels(write: &Write, state: &str, width: u32) -> Result<scaena_paint::Raster, OpsError> {
    Drawing::new(write)?.draw(state, width)
}

/// A made deck's states drawn by the CPU painter, its fonts and pictures registered once.
struct Drawing<'a> {
    write: &'a Write,
    theme: scaena_engine::theme::Theme,
    engine: scaena_engine::Engine,
    assets: scaena_paint::Assets,
    data: scaena_engine::data::DataFiles,
    painter: scaena_paint::cpu::CpuPainter,
}

impl<'a> Drawing<'a> {
    fn new(write: &'a Write) -> Result<Drawing<'a>, OpsError> {
        let nothing = scaena_store::Files::Zip(Default::default());
        let view = View { base: &nothing, pending: write.files.clone() };
        let deck = &write.deck;
        let theme = view.theme(deck)?.ok_or_else(|| OpsError::new("a generated deck names its theme"))?;
        let theme = scaena_engine::theme::Theme::from_json(&theme)?;
        let mut assets = scaena_paint::Assets::new();
        let engine = crate::lint::engine_in(deck, &view, &theme, Some(&mut assets))?;
        let data = crate::lint::data_in(deck, &view)?;
        Ok(Drawing { write, theme, engine, assets, data, painter: scaena_paint::cpu::CpuPainter::default() })
    }

    /// `state` at rest, `width` pixels wide; where it leaves the canvas bare, on black.
    fn draw(&mut self, state: &str, width: u32) -> Result<scaena_paint::Raster, OpsError> {
        use scaena_paint::Painter as _;
        let deck = &self.write.deck;
        let req = scaena_engine::FrameRequest {
            deck,
            theme: &self.theme,
            data: &self.data,
            state,
            t_ms: f64::INFINITY,
            format: None,
        };
        let list = self.engine.frame(&req)?.display_list;
        let mut raster = self.painter.paint(&list, &self.assets, width as f32 / deck.canvas.width as f32)?;
        for p in raster.rgba.as_chunks_mut::<4>().0 {
            let a = u32::from(p[3]);
            for c in &mut p[..3] {
                *c = (u32::from(*c) * a / 255) as u8;
            }
            p[3] = 255;
        }
        Ok(raster)
    }
}

/// The texts of `deck` that run past their place, as `findings` (lint's, after its fixes) say:
/// each text node lint finds too long for its box at its smallest size (E100, W203) or longer
/// than its lines (W202), with the most characters that would fit, by how far it runs over.
pub fn overflowing(deck: &Deck, findings: &[Finding]) -> Result<Vec<Long>, OpsError> {
    let doc = deck.to_value()?;
    let mut out: Vec<Long> = Vec::new();
    for f in findings {
        let (Some(id), Some(m)) = (f.node.as_deref(), f.measure.as_ref()) else { continue };
        let ratio = match f.code.as_str() {
            "E100" | "W203" => match (m["box"][1].as_f64(), m["height"].as_f64()) {
                (Some(room), Some(height)) if height > 0.0 => room / height,
                _ => continue,
            },
            "W202" => match (m["maxLines"].as_f64(), m["lines"].as_f64()) {
                (Some(most), Some(lines)) if lines > 0.0 => most / lines,
                _ => continue,
            },
            _ => continue,
        };
        let node = &doc["nodes"][id];
        let (true, Some(text)) = (node["type"] == "text", node["text"].as_str()) else { continue };
        let n = text.chars().count();
        // A little under the measure, so a line that wraps otherwise still fits.
        let most = ((n as f64 * ratio.min(1.0) * 0.9).floor() as usize).max(8).min(n.saturating_sub(1));
        match out.iter_mut().find(|l| l.id == id) {
            Some(l) => l.most = l.most.min(most),
            None => out.push(Long { id: id.to_string(), text: text.to_string(), most }),
        }
    }
    Ok(out)
}

/// The texts of `doc` that grow (`fit: grow`, as [`Slots::grown`] marks them) or shrink held to
/// one size in each set, so that each set reads as one: a set is the parts a slide's layout sets
/// side by side (the words under each step, the figures of a scorecard), or the deck's section
/// titles. Each grows or shrinks as its place allows, laid out by the engine, then all of its set
/// are held to the size the one with the least room reached, by its `maxSize`, which caps a text
/// that shrinks too (deck format 0.36). Each is measured as it was before `alike` held it, by
/// what `held` keeps of it, so that a second run after lint's fixes changes only a set a fix
/// changed. Whether any size changed.
fn alike(doc: &mut Value, view: &View, held: &mut Held) -> Result<bool, OpsError> {
    // What `alike` wrote taken back, for measuring alone: each one's room, not its cap.
    let mut measured = doc.clone();
    for (id, most) in &held.most {
        let node = &mut measured["nodes"][id];
        match (most, node.as_object_mut()) {
            (Some(most), _) => node["maxSize"] = most.clone(),
            (None, Some(node)) => {
                node.remove("maxSize");
            }
            (None, None) => {}
        }
    }
    for id in &held.grew {
        measured["nodes"][id]["fit"] = json!("grow");
    }
    let deck = Deck::from_json(&measured.to_string()).context("the deck")?;
    let Some(theme) = view.theme(&deck)? else { return Ok(false) };
    let theme = scaena_engine::theme::Theme::from_json(&theme)?;
    let members = growing(&measured, &theme);
    let reached = reached(&deck, view, &theme, &members)?;
    let mut least: BTreeMap<&str, f64> = BTreeMap::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (set, _, id, _) in &members {
        let Some((size, _)) = reached.get(id) else { continue };
        let at = least.entry(set.as_str()).or_insert(f64::INFINITY);
        *at = at.min(*size);
        *counts.entry(set.as_str()).or_default() += 1;
    }
    let mut changed = false;
    for (set, _, id, own) in &members {
        let (Some(size), Some(2..)) = (least.get(set.as_str()), counts.get(set.as_str())) else { continue };
        // Figures side by side keep the air between them: a tenth under the size the widest
        // reached its place at.
        let figures = doc["nodes"][id]["role"].as_str().is_some_and(figure_role);
        let size = if figures { size * 0.9 } else { *size };
        // A quarter of a unit under, so that every one of them fits at it.
        let size = (size * 4.0).floor() / 4.0;
        // A text that shrinks, by its own fit or its slot's, and took no more than that keeps no
        // cap it does not need.
        let shrinks = |fit: &TextFit| doc["nodes"][id]["fit"] == "shrink" || *fit == TextFit::Shrink;
        if reached.get(id).is_some_and(|(r, fit)| shrinks(fit) && *r <= size + 0.25) {
            continue;
        }
        let node = &mut doc["nodes"][id];
        let before = node.get("maxSize").cloned();
        // One of a set that a fit made shrink: one that grows never goes under its role's size,
        // so it shrinks too, under the same cap.
        if node["fit"] == "grow" && size < *own {
            node["fit"] = json!("shrink");
            node["maxSize"] = json!(size);
            held.grew.insert(id.clone());
        } else if node["maxSize"].as_f64().is_none_or(|most| size < most) {
            node["maxSize"] = json!(size);
        } else {
            continue;
        }
        held.most.entry(id.clone()).or_insert(before);
        changed = true;
    }
    Ok(changed)
}

/// Each text of `doc` that grows or shrinks and is one of a set [`alike`] holds to one size:
/// its set, its state, its id, and its role's size. A text that says no fit of its own fits as its
/// slot says, so a set of titles the slot shrinks shrinks together (lift-up dogfood).
fn growing(doc: &Value, theme: &scaena_engine::theme::Theme) -> Vec<(String, String, String, f64)> {
    let mut members = Vec::new();
    for state in doc["states"].as_array().into_iter().flatten() {
        let (Some(id), Some(shown)) = (state["id"].as_str(), state["props"].as_object()) else { continue };
        let layout = state["layout"].as_str().unwrap_or_default();
        for node in shown.keys() {
            let n = &doc["nodes"][node];
            let (Some(slot), Some(role)) = (n["at"]["in"].as_str(), n["role"].as_str()) else { continue };
            let slotted = || theme.layouts.get(layout).and_then(|l| l.slots.get(slot)).and_then(|s| s.fit);
            let fits = match n["fit"].as_str() {
                Some(fit) => matches!(fit, "grow" | "shrink"),
                None => matches!(slotted(), Some(TextFit::Grow | TextFit::Shrink)),
            };
            if !fits {
                continue;
            }
            let Some(size) = theme.typography.roles.get(role).map(|r| r.size) else { continue };
            let set = match ordinal(slot) {
                Some((_, part)) => format!("{id}\u{1f}{part}\u{1f}{role}"),
                None if slot == "title" && layout.starts_with("section") => format!("\u{1f}{layout}\u{1f}{role}"),
                // A journey's cells, every lane's, one set: the map reads at one size.
                None if layout == "journey" && journey_cell(slot) => format!("{id}\u{1f}cell\u{1f}{role}"),
                None => continue,
            };
            members.push((set, id.to_string(), node.clone(), size));
        }
    }
    members
}

/// The size each of `members` (a state, a text in it, and its role's size, as [`growing`] gives
/// them) reaches in `deck`, each state laid out once. A slide the engine cannot lay out is left
/// out; lint says why.
fn reached(
    deck: &Deck,
    view: &View,
    theme: &scaena_engine::theme::Theme,
    members: &[(String, String, String, f64)],
) -> Result<BTreeMap<String, (f64, TextFit)>, OpsError> {
    let mut engine = crate::lint::engine_in(deck, view, theme, None)?;
    let data = crate::lint::data_in(deck, view)?;
    let mut reached = BTreeMap::new();
    let mut states: Vec<&str> = members.iter().map(|m| m.1.as_str()).collect();
    states.dedup();
    for state in states {
        let ids: Vec<&str> = members.iter().filter(|m| m.1 == state).map(|m| m.2.as_str()).collect();
        let req = scaena_engine::FrameRequest { deck, theme, data: &data, state, t_ms: f64::INFINITY, format: None };
        let Ok(placed) = engine.text_layouts(&req, &ids) else { continue };
        for (id, p) in ids.iter().zip(&placed) {
            let size = members.iter().find(|m| m.2 == *id).map_or(0.0, |m| m.3);
            reached.insert(id.to_string(), (f64::from(p.scale) * size, p.fit));
        }
    }
    Ok(reached)
}

/// `doc` with each text that grows or shrinks and collides with another (lint E101, by its
/// glyphs' ink: a descender over the text under it), or whose descenders come within the
/// theme's gutter of the text under it (I405: a figure's tail over its name), set a step smaller,
/// a twentieth of the size it reached, round by round until lint finds it clear. `found` is what lint finds in `doc`
/// as it is, so the first round lints nothing again. A text that grows and must go under its
/// role's size shrinks under its new `maxSize`, as [`alike`] makes one, and `held` keeps what
/// was there before, so [`alike`] measures it as it was. A step that leaves the collision as
/// large for the text's size (a caption across the whole of it) is taken back, and the text
/// left as it was; none goes under half its role's size. Each set a text stepped in is then
/// held to its least, so a row keeps one size and one baseline. Whether any size changed.
fn clear(doc: &mut Value, view: &View, found: Vec<Finding>, held: &mut Held) -> Result<bool, OpsError> {
    // A step taken: the text's collision for its size before it, and what it changed.
    struct Step {
        was: f64,
        most: Option<Value>,
        fit: Option<Value>,
    }
    let mut findings = found;
    let mut steps: BTreeMap<String, Step> = BTreeMap::new();
    let mut stuck: BTreeSet<String> = BTreeSet::new();
    // Each text stepped, as it was before its first step: its `maxSize` and its own fit.
    let mut first: BTreeMap<String, (Option<Value>, Option<Value>)> = BTreeMap::new();
    let mut theme = None;
    for round in 0..16 {
        let deck = Deck::from_json(&doc.to_string()).context("the deck")?;
        if theme.is_none() {
            let Some(json) = view.theme(&deck)? else { return Ok(false) };
            theme = Some(scaena_engine::theme::Theme::from_json(&json)?);
        }
        let theme = theme.as_ref().expect("read above");
        if round > 0 {
            findings = lint_in(&deck, view)?.findings;
        }
        findings.extend(tight(doc, &deck, view, theme)?);
        // Each text in a collision, or whose descenders crowd the text under it (I405, by no
        // overlap): the largest overlap it is in, and where. Whether it grows or shrinks is read
        // as it is laid out, by its own fit or its slot's.
        let mut colliding: BTreeMap<String, (f64, String, f64, f64)> = BTreeMap::new();
        for f in findings.iter().filter(|f| f.code == "E101" || f.code == "I405") {
            let Some(m) = f.measure.as_ref() else { continue };
            let Some(state) = m["states"][0].as_str() else { continue };
            let nodes = m["nodes"].as_array().map_or(&[][..], Vec::as_slice);
            // The text over the other, whose tail is what crowds, and the share of its size that
            // leaves the gutter.
            let (area, ids, aim) = match (m["overlap"][0].as_f64(), m["overlap"][1].as_f64()) {
                (Some(w), Some(h)) => (w * h, nodes, 1.0),
                _ => {
                    let aim = m["aim"].as_f64().zip(m["size"].as_f64()).map_or(1.0, |(aim, size)| aim / size);
                    (0.0, &nodes[..nodes.len().min(1)], aim)
                }
            };
            for id in ids.iter().filter_map(Value::as_str) {
                let n = &doc["nodes"][id];
                let Some(role) = n["role"].as_str() else { continue };
                let Some(size) = theme.typography.roles.get(role).map(|r| r.size) else { continue };
                let at = colliding.entry(id.to_string()).or_insert((0.0, state.to_string(), size, 1.0));
                at.0 = at.0.max(area);
                at.3 = at.3.min(aim);
            }
        }
        let members: Vec<(String, String, String, f64)> = (colliding.iter())
            .filter(|(id, _)| !stuck.contains(*id))
            .map(|(id, (_, state, size, _))| (String::new(), state.clone(), id.clone(), *size))
            .collect();
        let reached = reached(&deck, view, theme, &members)?;
        // A step that left the collision as large for the text's size is taken back.
        for (id, step) in std::mem::take(&mut steps) {
            let now = match (colliding.get(&id), reached.get(&id)) {
                (Some((area, ..)), Some((at, _))) => badness(*area, *at),
                _ => continue,
            };
            if !less(now, step.was) {
                let node = &mut doc["nodes"][&id];
                match step.most {
                    Some(most) => node["maxSize"] = most,
                    None => {
                        node.as_object_mut().map(|n| n.remove("maxSize"));
                    }
                }
                match step.fit {
                    Some(fit) => node["fit"] = fit,
                    None => {
                        node.as_object_mut().map(|n| n.remove("fit"));
                    }
                }
                stuck.insert(id);
            }
        }
        let mut changed = false;
        for (_, _, id, size) in &members {
            let (Some((at, laid)), Some((area, _, _, aim))) = (reached.get(id), colliding.get(id)) else { continue };
            if stuck.contains(id) || !matches!(laid, TextFit::Grow | TextFit::Shrink) {
                continue;
            }
            // A twentieth smaller, or to the size that leaves the gutter under it, on a quarter of a
            // unit, never under half its role's size.
            let next = (at * aim.min(0.95) * 4.0).floor() / 4.0;
            let node = &mut doc["nodes"][id];
            let most = node.get("maxSize").cloned();
            if next < size * 0.5 || most.as_ref().and_then(Value::as_f64).is_some_and(|m| next >= m) {
                stuck.insert(id.clone());
                continue;
            }
            let fit = node.get("fit").cloned();
            if *laid == TextFit::Grow && next < *size {
                node["fit"] = json!("shrink");
                held.grew.insert(id.clone());
            }
            node["maxSize"] = json!(next);
            held.most.entry(id.clone()).or_insert(most.clone());
            first.entry(id.clone()).or_insert((most.clone(), fit.clone()));
            steps.insert(id.clone(), Step { was: badness(*area, *at), most, fit });
            changed = true;
        }
        if !changed {
            break;
        }
    }
    // The texts left smaller than they were.
    let stepped = scaena_core::sort::set(
        first
            .iter()
            .filter(|(id, (most, _))| doc["nodes"][id.as_str()].get("maxSize") != most.as_ref())
            .map(|(id, _)| id),
    );
    if stepped.is_empty() {
        return Ok(false);
    }
    // Each set a text stepped in, held to its least.
    let Some(theme) = theme else { return Ok(true) };
    let members = growing(doc, &theme);
    let sets = scaena_core::sort::set(members.iter().filter(|m| stepped.contains(&&m.2)).map(|m| m.0.clone()));
    for set in sets {
        let ids: Vec<(&str, f64)> = members.iter().filter(|m| m.0 == set).map(|m| (m.2.as_str(), m.3)).collect();
        let least =
            ids.iter().filter_map(|(id, _)| doc["nodes"][*id]["maxSize"].as_f64()).fold(f64::INFINITY, f64::min);
        if !least.is_finite() {
            continue;
        }
        for (id, size) in ids {
            let node = &mut doc["nodes"][id];
            if node["maxSize"].as_f64().is_some_and(|m| m <= least) {
                continue;
            }
            held.most.entry(id.to_string()).or_insert(node.get("maxSize").cloned());
            if node["fit"] == "grow" && least < size {
                node["fit"] = json!("shrink");
                held.grew.insert(id.to_string());
            }
            node["maxSize"] = json!(least);
        }
    }
    Ok(true)
}

/// Each card's words or term's meaning that grew (in a place of a set's own, `first`, …, or
/// `meaning` and `example`) and whose ink comes within two of the theme's gutters of a text under
/// it, the room I405 asks under a figure, so the text under it reads as its own (a definition
/// ending on the next term). Each is found as I405 finds a figure's tail: `clear` steps it a
/// twentieth smaller until it stands that far clear.
fn tight(doc: &Value, deck: &Deck, view: &View, theme: &scaena_engine::theme::Theme) -> Result<Vec<Finding>, OpsError> {
    use scaena_engine::sample::Content;
    let gutter = theme.grid.gutter as f32;
    let mut engine = crate::lint::engine_in(deck, view, theme, None)?;
    let data = crate::lint::data_in(deck, view)?;
    let mut found = Vec::new();
    for state in doc["states"].as_array().into_iter().flatten() {
        let (Some(id), Some(shown)) = (state["id"].as_str(), state["props"].as_object()) else { continue };
        let grown: Vec<&str> = (shown.keys())
            .filter(|node| {
                let n = &doc["nodes"][node.as_str()];
                n["type"] == "text"
                    && n["fit"] == "grow"
                    && n["at"]["in"].as_str().is_some_and(|slot| {
                        matches!(ordinal(slot), Some((_, ""))) || matches!(slot, "meaning" | "example")
                    })
            })
            .map(String::as_str)
            .collect();
        if grown.is_empty() {
            continue;
        }
        let req =
            scaena_engine::FrameRequest { deck, theme, data: &data, state: id, t_ms: f64::INFINITY, format: None };
        let Ok(scene) = engine.at_rest(&req) else { continue };
        let texts: Vec<(&str, [f32; 4])> = (scene.nodes.iter())
            .filter_map(|n| match &n.content {
                Content::Text(_) => Some((n.id.as_str(), n.ink()?)),
                _ => None,
            })
            .collect();
        for a in &grown {
            let Some(ink) = texts.iter().find(|(t, _)| t == a).map(|(_, ink)| *ink) else { continue };
            let bottom = ink[1] + ink[3];
            let under = texts.iter().find(|(b, under)| {
                let across = (ink[0] + ink[2]).min(under[0] + under[2]) - ink[0].max(under[0]);
                let gap = under[1] - bottom;
                b != a && across > 1.0 && (0.0..2.0 * gutter).contains(&gap)
            });
            if let Some((b, _)) = under {
                let mut f = Finding::new(
                    "I405",
                    scaena_core::lint::Severity::Info,
                    format!("`{a}` comes within two gutters of `{b}`"),
                );
                f.node = Some(a.to_string());
                f.state = Some(id.to_string());
                f.measure = Some(json!({ "nodes": [a, b], "states": [id] }));
                found.push(f);
            }
        }
    }
    Ok(found)
}

/// How much a text set at size `at` overlaps, for its size: the area over its size squared.
fn badness(area: f64, at: f64) -> f64 {
    area / (at * at)
}

/// Whether a step took a text from overlapping as much as `was` for its size to `now`. One that
/// only crowds the text under it (I405) always gains: its tail shortens as it is set smaller, its
/// baseline on the grid, and it comes clear before half its size, though a step that snaps the
/// baseline lower may leave less room for a round.
fn less(now: f64, was: f64) -> bool {
    was == 0.0 || now < was * 0.999
}

/// What [`alike`] wrote over, so a run after it measures each text as it was: each one's
/// `maxSize` before `alike` first set it, and those it made shrink where they grew.
#[derive(Default)]
struct Held {
    most: BTreeMap<String, Option<Value>>,
    grew: BTreeSet<String>,
}

/// How many of `findings` are errors.
fn errors(findings: &[Finding]) -> usize {
    findings.iter().filter(|f| f.severity == scaena_core::Severity::Error).count()
}

/// A deck with lint's fixes applied: the findings fixed, the deck, and what lint finds in it then.
struct Settled {
    fixed: Vec<Finding>,
    deck: Deck,
    findings: Vec<Finding>,
}

/// `doc` with each fix `found` offers applied, each once, as `lint --fix` applies them, I405's
/// aside, which [`clear`] has taken.
fn settle(mut doc: Value, found: Vec<Finding>, view: &View) -> Result<Settled, OpsError> {
    let mut fixed = Vec::new();
    // A text crowding the one under it is [`clear`]'s, which holds its set to one size.
    for f in found.iter().filter(|f| f.fix.is_some() && f.code != "I405") {
        let mut next = doc.clone();
        if scaena_core::patch::apply(&mut next, f.fix.as_deref().unwrap_or_default()).is_ok() {
            doc = next;
            fixed.push(f.clone());
        }
    }
    let deck = Deck::from_json(&doc.to_string()).context("the fixed deck")?;
    let findings = if fixed.is_empty() { found } else { lint_in(&deck, view)?.findings };
    Ok(Settled { fixed, deck, findings })
}

/// The bundle `narrative` makes in the theme whose file is `theme_file` and whose JSON is
/// `theme`, each font it names read by `font`: [`deck`] made into a bundle as
/// [`creating`] makes one, then each fix lint offers applied, as `lint --fix` applies them, so a
/// text too long for its slot shrinks to fit. Nothing is written: the deck and every file beside
/// it, if it validates.
pub fn generating(
    narrative: &Narrative,
    theme_file: &Path,
    theme: String,
    font: &dyn Fn(&str) -> Result<Vec<u8>, OpsError>,
) -> Result<(Generated, Option<Write>), OpsError> {
    let model: Theme = serde_json::from_str(&theme).context("the theme")?;
    making(deck(narrative, &model, &Pictures::new())?, &narrative.arc, theme_file, theme, font)
}

/// The bundle `made` makes, the deck a narrative on the arc `arc` made in the theme whose file
/// is `theme_file` and whose JSON is `theme`, each font it names read by `font`: [`generating`]'s
/// second half, which the browser's editor module runs on what its generator's module built
/// (PLAN G.5). Nothing is written.
pub fn making(
    made: Made,
    arc: &str,
    theme_file: &Path,
    theme: String,
    font: &dyn Fn(&str) -> Result<Vec<u8>, OpsError>,
) -> Result<(Generated, Option<Write>), OpsError> {
    let req =
        Create { theme: PathBuf::from(theme_file), deck: Some(made.deck), assets: made.assets, ..Create::default() };
    let none = |path: &Path| -> Result<Vec<u8>, OpsError> {
        Err(OpsError::new(format!("a generated deck reads no file: {}", path.display())))
    };
    let (mut created, write) = creating(&req, theme, font, &none)?;
    let mut made = Generated {
        created: created.clone(),
        arc: arc.to_string(),
        fallback: None,
        layouts: made.layouts,
        fixed: Vec::new(),
        fitted: Vec::new(),
        unfitted: None,
        pictures: 0,
        unpictured: Vec::new(),
        looked: Vec::new(),
        unlooked: None,
    };
    let Some(mut write) = write else { return Ok((made, None)) };
    let nothing = scaena_store::Files::Zip(Default::default());
    let view = View { base: &nothing, pending: write.files.clone() };
    let mut doc = write.deck.to_value()?;
    // Characters the theme's fonts have no glyph for, as plain text writes them: CO₂ as CO2.
    let unset: Vec<Finding> = created.findings.iter().filter(|f| f.code == "E120").cloned().collect();
    if !unset.is_empty() {
        for f in &unset {
            plainer(&mut doc, f);
        }
        made.fixed.extend(unset);
        let deck = Deck::from_json(&doc.to_string()).context("the deck in plain characters")?;
        created.findings = lint_in(&deck, &view)?.findings;
    }
    // Each set of texts a slide sets side by side, and every section's title, held to one size.
    let mut held = Held::default();
    if alike(&mut doc, &view, &mut held)? {
        let deck = Deck::from_json(&doc.to_string()).context("the deck set alike")?;
        created.findings = lint_in(&deck, &view)?.findings;
    }
    // Each text that grows into the one under it, a descender over its words, set smaller.
    if clear(&mut doc, &view, created.findings.clone(), &mut held)? {
        let deck = Deck::from_json(&doc.to_string()).context("the deck set clear")?;
        created.findings = lint_in(&deck, &view)?.findings;
    }
    // Then each fix lint offers, each checked by laying its state out again with it; a fix that
    // makes a text of a set shrink, the set held to one size again.
    let mut settled = settle(doc, created.findings, &view)?;
    let mut doc = settled.deck.to_value()?;
    if alike(&mut doc, &view, &mut held)? {
        settled.deck = Deck::from_json(&doc.to_string()).context("the deck set alike again")?;
        settled.findings = lint_in(&settled.deck, &view)?.findings;
    }
    made.fixed.extend(settled.fixed);
    made.created.errors = errors(&settled.findings);
    made.created.findings = settled.findings;
    write.deck = settled.deck;
    Ok((made, Some(write)))
}

/// `doc` with the characters an E120 finding names written as plain text writes them, where the
/// node draws them: its text, a chart's titles, and the cells of the data a chart reads.
fn plainer(doc: &mut Value, finding: &Finding) {
    let Some(node) = finding.node.as_deref() else { return };
    let missing: Vec<char> = (finding.measure.as_ref())
        .and_then(|m| m["missing"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|c| c.as_str()?.chars().next())
        .collect();
    // What plain text has no other way to write is dropped only where it is a symbol: a letter
    // or a mark no font draws stays, lint's E120 saying so where it stands, so a word is never
    // quietly taken out of what the deck says (PLAN G.22).
    let swap = |text: &str| -> String {
        (text.chars())
            .map(|c| match plain(c) {
                _ if !missing.contains(&c) => c.to_string(),
                "" if !symbol(c) => c.to_string(),
                written => written.to_string(),
            })
            .collect()
    };
    let Some(n) = doc.pointer_mut(&format!("/nodes/{node}")) else { return };
    if let Some(text) = n["text"].as_str() {
        n["text"] = json!(swap(text));
    }
    for run in n["runs"].as_array_mut().into_iter().flatten() {
        if let Some(text) = run["text"].as_str() {
            run["text"] = json!(swap(text));
        }
    }
    for axis in ["x", "y"] {
        if let Some(title) = n[axis]["title"].as_str() {
            n[axis]["title"] = json!(swap(title));
        }
    }
    let Some(source) = n["data"].as_str().and_then(|d| d.strip_prefix('@')).map(str::to_string) else { return };
    let rows = doc.pointer_mut(&format!("/data/{source}/source/inline")).and_then(Value::as_array_mut);
    for cell in rows.into_iter().flatten().filter_map(Value::as_object_mut).flat_map(|r| r.values_mut()) {
        if let Some(text) = cell.as_str() {
            *cell = json!(swap(text));
        }
    }
}

/// Whether `c` is a symbol, an emoji, or for private use: what a deck can lose where no font
/// draws it without losing a word.
fn symbol(c: char) -> bool {
    matches!(u32::from(c), 0x2000..=0x2BFF | 0xE000..=0xF8FF | 0xFE00..=0xFE0F | 0x1F000..=0x1FBFF)
}

/// A character as plain text writes it: a subscript or a superscript digit as the digit, a
/// fraction with a slash, a mark in letters, punctuation as ASCII has it; nothing for the rest,
/// an emoji or a symbol no font of the theme draws.
fn plain(c: char) -> &'static str {
    const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
    match c {
        '\u{2080}'..='\u{2089}' => DIGITS[c as usize - 0x2080],
        '\u{2070}' => "0",
        '\u{b9}' => "1",
        '\u{b2}' => "2",
        '\u{b3}' => "3",
        '\u{2074}'..='\u{2079}' => DIGITS[c as usize - 0x2070],
        '\u{208a}' | '\u{207a}' => "+",
        '\u{208b}' | '\u{207b}' | '\u{2212}' | '\u{2013}' | '\u{2014}' | '\u{2010}' | '\u{2011}' => "-",
        '\u{bd}' => "1/2",
        '\u{bc}' => "1/4",
        '\u{be}' => "3/4",
        '\u{2153}' => "1/3",
        '\u{2154}' => "2/3",
        '\u{2122}' => "TM",
        '\u{ae}' => "(R)",
        '\u{a9}' => "(C)",
        '\u{2026}' => "...",
        '\u{2018}' | '\u{2019}' | '\u{2032}' => "'",
        '\u{201c}' | '\u{201d}' | '\u{2033}' => "\"",
        '\u{2022}' | '\u{b7}' => "-",
        '\u{2192}' => "->",
        '\u{2190}' => "<-",
        '\u{2265}' => ">=",
        '\u{2264}' => "<=",
        '\u{2248}' => "~",
        '\u{d7}' => "x",
        '\u{a0}' | '\u{2009}' | '\u{202f}' => " ",
        _ => "",
    }
}

/// The letters each language Narrato writes in sets beside ASCII's (`scaena_narrato::brief::
/// LANGUAGES`): what a theme's fonts must draw for a deck in it to be read (PLAN G.21).
pub const LETTERS: [(&str, &str); 15] = [
    ("English", ""),
    ("Spanish", "áéíóúñü¿¡"),
    ("French", "àâçéèêëîïôûùüÿœæ«»"),
    ("German", "äöüßÄÖÜ"),
    ("Portuguese", "áâãàçéêíóôõú"),
    ("Italian", "àèéìòù"),
    ("Dutch", "éëïóöü"),
    ("Czech", "áčďéěíňóřšťúůýž"),
    ("Polish", "ąćęłńóśźż"),
    ("Ukrainian", "абвгґдеєжзиіїйклмнопрстуфхцчшщьюя"),
    ("Japanese", "日本語のひらがなカタカナ"),
    ("Chinese (Simplified)", "简体中文汉字的"),
    ("Korean", "한국어글"),
    ("Arabic", "العربية"),
    ("Hindi", "हिन्दी"),
];

/// Whether `theme`'s fonts draw the letters of `language`, asked before any call is paid for
/// (PLAN G.21): a deck written in letters its fonts cannot draw would be made with nothing
/// where its words stand. The family of every role but code's is asked, with its fallbacks;
/// `font` reads a font by the path the theme names it at. `shipped` says the theme ships, so
/// that a language written in a script none of its fonts draw is said to need the script's
/// font, which [`crate::scripts`] says how to take (PLAN G.22). A language Narrato does not
/// name is not asked about here: the brief says so.
pub fn writable(
    language: Option<&str>,
    theme: &str,
    shipped: bool,
    font: &dyn Fn(&str) -> Result<Vec<u8>, OpsError>,
) -> Result<(), OpsError> {
    let Some((language, letters)) =
        language.and_then(|l| LETTERS.iter().find(|(name, _)| name.eq_ignore_ascii_case(l.trim())))
    else {
        return Ok(());
    };
    let model: Theme = serde_json::from_str(theme).context("the theme")?;
    let families = &model.typography.families;
    let code = model.code_role();
    let mut keys: Vec<&str> = Vec::new();
    for (role, r) in &model.typography.roles {
        if Some(role.as_str()) != code && !keys.contains(&r.family.as_str()) {
            keys.push(&r.family);
        }
    }
    for key in keys {
        let Some(family) = families.get(key) else { continue };
        let stack = std::iter::once(key).chain(family.fallback.iter().flatten().map(String::as_str));
        let fonts = stack.filter_map(|k| families.get(k)).map(|f| font(&f.file)).collect::<Result<Vec<_>, _>>()?;
        let read: Vec<&[u8]> = fonts.iter().map(Vec::as_slice).collect();
        let missing = scaena_engine::fonts::unmapped(&read, letters);
        if missing.is_empty() {
            continue;
        }
        let some = missing.iter().take(6).map(char::to_string).collect::<Vec<_>>().join(", ");
        let script = crate::scripts::SCRIPTS.iter().find(|s| s.language == *language);
        return Err(if let (true, Some(script)) = (shipped, script) {
            OpsError::new(format!(
                "a deck in {language} cannot be made in {} without {}, the font its letters are set in: its \
                 own fonts have no glyphs for {some}",
                model.name, script.family
            ))
        } else {
            OpsError::new(format!(
                "a deck in {language} cannot be made in {}: the fonts of its `{key}` family and their fallbacks \
                 have no glyphs for {some}. Add a family that draws them to that family's `fallback`, its file \
                 beside the theme",
                model.name
            ))
        });
    }
    Ok(())
}

/// The deck `narrative` makes in `theme`, each slide that `pictures` holds a picture for drawn
/// in a layout that shows it.
pub fn deck(narrative: &Narrative, theme: &Theme, pictures: &Pictures) -> Result<Made, OpsError> {
    deck_as(narrative, theme, pictures, &BTreeMap::new())
}

/// [`deck`], each slide `layouts` names by its index drawn in the layout it gives there, where
/// the theme has it and it has a place for each of the slide's items (PLAN G.13).
pub fn deck_as(
    narrative: &Narrative,
    theme: &Theme,
    pictures: &Pictures,
    layouts: &BTreeMap<usize, String>,
) -> Result<Made, OpsError> {
    if narrative.slides.is_empty() {
        return Err(OpsError::new("a narrative with no slides makes no deck"));
    }
    // Its words set as a typesetter sets them: "Canada\u{2019}s", "2021\u{2013}2024" (G.16).
    let narrative = &typeset_all(narrative);
    let arc = narrative.narrative_arc();
    // The beats the slides are spread over: the outline's where the person settled one before
    // the slides were written (narrative FUNC-1), else the arc's.
    let names: Vec<&str> = match &narrative.outline {
        Some(outline) => (outline.beats.iter()).map(|b| b.name.split(':').next().unwrap_or(&b.name).trim()).collect(),
        None => arc.beat_names(),
    };
    let count = narrative.slides.len();
    let varied = varied(narrative);

    let lang = deck_lang(&narrative.brief);
    let rtl = lang.is_some_and(scaena_core::direction::right_to_left);
    let mut deck = Builder { mirrored: rtl, ..Builder::default() };
    let mut sections: Vec<Value> = Vec::new();
    let mut last_beat = None;
    let mut chosen = Vec::with_capacity(count);
    // Each slide's beat, items, icons, and layout, chosen first, so a divider is numbered by the
    // agenda item it opens, which a later slide may be.
    let planned: Vec<Planned> = (narrative.slides.iter().zip(&varied))
        .enumerate()
        .map(|(i, (slide, v))| {
            let beat = (slide.beat_index.filter(|b| *b < names.len()))
                .unwrap_or_else(|| scaena_narrato::beats::beat_of(i, count, names.len()));
            let items = items(slide, v);
            let icons = icons_of(narrative, slide, &items);
            let given = layouts.get(&i).filter(|l| {
                theme.layouts.contains_key(*l) && holds(theme, l, items.len()) && (icons.is_some() || !iconic(theme, l))
            });
            let layout = match given {
                Some(layout) => layout.clone(),
                None => {
                    layout_for(v.layout.identity(), slide, &items, i, theme, pictures.contains_key(&i), icons.is_some())
                }
            };
            Planned { beat, items, icons, layout }
        })
        .collect();
    let mut planned = planned;
    rebroken(&mut planned, narrative, &varied, theme, pictures, layouts);
    let numbers = divider_numbers(narrative, &planned);
    // The part of the talk a slide is in, as its foot names it: its divider's title, for the
    // slides of the beat the divider opened, and none once the talk moves to another.
    let mut running: Option<(String, usize)> = None;
    // Whether the section open is titled by a divider of its own.
    let mut divided = false;
    let mut previous: Option<(String, &Slide)> = None;
    for (i, ((slide, v), Planned { beat, items, icons, layout })) in
        narrative.slides.iter().zip(&varied).zip(planned).enumerate()
    {
        let picture = pictures.get(&i);
        let opens = matches!(layout.as_str(), "section" | "section-photo");
        if opens || running.as_ref().is_some_and(|(_, b)| *b != beat) {
            running = None;
        }
        let state = deck.state_id(&slide.slide_title, &layout);
        let slots = Slots {
            theme,
            layout: &layout,
            state: &state,
            slide,
            body: &v.body,
            items: &items,
            section: numbers.get(&i).copied(),
            running: running.as_ref().map(|(r, _)| r.as_str()),
            picture,
            icons,
            lang: lang.unwrap_or("en"),
        };
        let entrance = entrance(&layout, slide, previous.as_ref().map(|(l, s)| (l.as_str(), *s)), i + 1 == count);
        let made = deck.slide(slots, entrance)?;
        let divider = layout.starts_with("section") && !slide.slide_title.is_empty();
        if divider {
            running = Some((slide.slide_title.clone(), beat));
        }
        previous = Some((layout.clone(), slide));
        chosen.push(layout);

        // The spine: a section for each run of slides in one beat, a beat for each slide. A
        // section is titled by what its slides say (narrative ATTR-5): its divider's title, else
        // the beat's name as the model wrote it on the run's first slide, else that slide's
        // headline, else the arc's name for the beat, which reads as a template ("Problem",
        // "Solution") in the PDF's outline, the player's picker, and the Story tab. Narrato
        // echoes the arc's names, so a name that is one of them is no name of the model's. Where
        // the person settled an outline, its beat's name stands in for the model's and the
        // headline both: the slides were written to it (narrative FUNC-1).
        if last_beat != Some(beat) {
            let outlined = narrative.outline.is_some();
            let named = (slide.beat_name.as_deref().map(str::trim))
                .filter(|n| !n.is_empty() && !outlined && !names.iter().any(|arc| arc.eq_ignore_ascii_case(n)));
            let headline = Some(slide.slide_title.as_str()).filter(|t| !t.trim().is_empty() && !outlined);
            let name = match (divider, named, headline) {
                (true, _, _) => slide.slide_title.as_str(),
                (false, Some(named), _) => named,
                (false, None, Some(headline)) => headline,
                (false, None, None) => names.get(beat).copied().unwrap_or("Section"),
            };
            let id = deck.unique(&slug(name, "section"), Taken::Section);
            sections.push(json!({ "id": id, "title": name, "beats": [] }));
            divided = divider;
            last_beat = Some(beat);
        } else if divider && !divided {
            // The run's first divider, after a slide or two: the section takes its title.
            sections.last_mut().expect("opened above")["title"] = json!(slide.slide_title);
            divided = true;
        }
        let claim = if slide.slide_title.is_empty() { format!("Slide {}", i + 1) } else { slide.slide_title.clone() };
        let beat_id = deck.unique(&slug(&claim, "beat"), Taken::Beat);
        let seconds = scaena_narrato::duration::seconds(
            &slide.speaker_notes,
            &v.body,
            &slide.slide_title,
            v.layout.layout,
            &slide.visual_description,
        );
        let states: Vec<&str> = std::iter::once(state.as_str()).chain(made.steps.iter().map(String::as_str)).collect();
        let mut entry = json!({ "id": beat_id, "claim": claim, "states": states, "duration": seconds });
        if !slide.speaker_notes.is_empty() {
            entry["notes"] = json!(slide.speaker_notes);
        }
        let evidence: Vec<String> = (made.data.iter().map(|data| format!("@{data}")))
            .chain(slide.source_ref.as_deref().and_then(|named| cited(&narrative.sources, named)))
            .collect();
        if !evidence.is_empty() {
            entry["evidence"] = json!(evidence);
        }
        entry["media"] = json!({ "narrato": media(slide) });
        sections.last_mut().expect("opened above")["beats"].as_array_mut().expect("an array").push(entry);
    }

    let brief = &narrative.brief;
    let title = narrative.slides.first().map(|s| s.slide_title.clone()).filter(|t| !t.is_empty());
    let mut meta = json!({
        "title": title.unwrap_or_else(|| brief.topic.clone()),
        "narrative": {
            "generator": "narrato",
            "arc": narrative.arc,
            "reasoning": narrative.arc_reasoning,
            "brief": {
                "topic": brief.topic,
                "audience": brief.audience,
                "goal": brief.goal,
                "length": brief.length,
            },
        },
    });
    if !narrative.executive_summary.is_empty() {
        meta["description"] = json!(narrative.executive_summary);
    }
    if let Some(lang) = deck_lang(brief) {
        meta["lang"] = json!(lang);
    }
    // The rest of the brief, where it was given (narrative FUNC-4): later edits, the assistant's
    // and the writing tools', are written to the voice and outcomes the person asked for.
    for (key, value) in [
        ("modality", &brief.modality),
        ("desiredOutcomes", &brief.desired_outcomes),
        ("personaId", &brief.persona_id),
        ("voice", &brief.voice),
    ] {
        if let Some(value) = value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            meta["narrative"]["brief"][key] = json!(value);
        }
    }
    if narrative.truncated {
        meta["narrative"]["truncated"] = json!(true);
    }
    if !narrative.sources.is_empty() {
        meta["narrative"]["sources"] = json!(narrative.sources);
    }
    if let Some(outline) = &narrative.outline {
        meta["narrative"]["outline"] = json!(outline);
    }
    let mut out = json!({
        "scaena": scaena_core::FORMAT_VERSION,
        "meta": meta,
        "canvas": { "width": CANVAS[0], "height": CANVAS[1] },
        "spine": { "sections": sections },
        "nodes": Value::Object(deck.nodes),
        "states": deck.states,
    });
    if !deck.data.is_empty() {
        out["data"] = Value::Object(deck.data);
    }
    Ok(Made { deck: out, layouts: chosen, assets: BTreeMap::new() })
}

/// `planned` with no three slides in a row on one of the theme's layouts (lift-up ATTR-2):
/// Narrato breaks its runs by its own names, which several of come to one of the theme's
/// (`body`, `image-text`, and `callout` each to `bullets`), so a run is broken again once each
/// slide has its theme layout. The middle of three takes the first other layout of places that sets
/// its items as they are written ([`others`]: labelled points as columns or cards); a layout the
/// person chose for a slide stays, and so does a slide only a list sets. The slides moved, by their
/// index.
fn rebroken(
    planned: &mut [Planned],
    narrative: &Narrative,
    varied: &[Varied],
    theme: &Theme,
    pictures: &Pictures,
    layouts: &BTreeMap<usize, String>,
) -> Vec<usize> {
    let mut moved = Vec::new();
    let mut i = 1;
    while i + 1 < planned.len() {
        let run = planned[i - 1].layout == planned[i].layout && planned[i].layout == planned[i + 1].layout;
        let chosen = (i - 1..=i + 1).any(|k| layouts.contains_key(&k));
        if !run || chosen {
            i += 1;
            continue;
        }
        let p = &planned[i];
        let (slide, v) = (&narrative.slides[i], &varied[i]);
        let options = others(
            v.layout.identity(),
            slide,
            &p.items,
            theme,
            pictures.contains_key(&i),
            p.icons.is_some(),
            &p.layout,
        );
        // Another list is no other look, and numbering points that have no order says something
        // false of them: a run is broken only by a layout of places that sets them as they are.
        match options.into_iter().find(|l| !matches!(l.as_str(), "bullets" | "numbered")) {
            Some(layout) => {
                planned[i].layout = layout;
                moved.push(i);
                // The next run starts after the one broken.
                i += 2;
            }
            None => i += 1,
        }
    }
    moved
}

/// Each slide of `narrative` after Narrato's variety pass, but for its cap on dividers: Vzestup
/// caps them since each is a picture of its own there, but here a divider costs nothing and the
/// agenda names every part, so each the model asked for stays one.
fn varied(narrative: &Narrative) -> Vec<Varied> {
    let mut varied: Vec<Varied> = narrative.slides.iter().map(Varied::of).collect();
    let changes = layouts::vary(&mut varied, true);
    for c in changes.iter().filter(|c| c.reason == layouts::Reason::SectionCap) {
        varied[c.index].layout = Varied::of(&narrative.slides[c.index]).layout;
    }
    varied
}

/// A picture a deck asks for (PLAN G.10): the slide it is for, the prompt it is made from, the
/// shape of its place in the layout the slide takes with it, and what it shows, for a reader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asked {
    pub slide: usize,
    pub prompt: String,
    pub frame: Frame,
    pub alt: String,
    /// The one picture a person asks for in an editor (PLAN 2.107), for no slide of a made deck:
    /// why it was not made is said without one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub alone: bool,
}

/// One picture a person asks for in an editor (PLAN 2.107): of `subject`, in their words, made as
/// `look` in `theme`'s palette, as a made deck's are, for a place `width` × `height` on the slide.
/// What [`Painting`] takes, its one slide the first; `subject` is what it shows, for a reader.
pub fn asked_one(theme: &Theme, subject: &str, look: Look, width: f64, height: f64) -> Asked {
    let subject = subject.trim();
    let style = pictures::style(look, &palette(theme));
    let frame = Frame::of(width, height);
    let prompt = pictures::prompt(look, &style, subject, pictures::composition(""), frame);
    Asked { slide: 0, prompt, frame, alt: subject.to_string(), alone: true }
}

/// The pictures `narrative` asks for in `theme`, made as `look`: one for each slide that wants
/// one most, up to two in five slides (Vzestup's priorities and cap,
/// [`scaena_narrato::pictures::wanted`]), each from Vzestup's prompt, in the theme's palette,
/// and framed as the layout the slide takes with a picture frames it.
pub fn asking(narrative: &Narrative, theme: &Theme, look: Look) -> Vec<Asked> {
    let varied = varied(narrative);
    let identities: Vec<&str> = varied.iter().map(|v| v.layout.identity()).collect();
    let style = pictures::style(look, &palette(theme));
    (pictures::wanted(&identities).into_iter())
        .filter_map(|i| {
            let slide = &narrative.slides[i];
            let layout = layout_for(identities[i], slide, &items(slide, &varied[i]), i, theme, true, false);
            let slot =
                theme.layouts.get(&layout)?.slots.values().find(|s| matches!(s.example, Some(Example::Picture(_))))?;
            let frame = frame_of(theme, slot);
            let subject = pictures::subject(slide);
            let prompt = pictures::prompt(look, &style, &subject, pictures::composition(identities[i]), frame);
            Some(Asked { slide: i, prompt, frame, alt: subject, alone: false })
        })
        .collect()
}

/// The colors a theme's pictures are made in: its surface, the color set on it, and its accent.
fn palette(theme: &Theme) -> Palette {
    let color = |role: &str, fallback: &str| -> String {
        let token = theme.tokens.roles.get(role).map(String::as_str).unwrap_or(role);
        theme.tokens.color.get(token).map(|c| c.0.clone()).unwrap_or_else(|| fallback.to_string())
    };
    Palette {
        name: theme.name.clone(),
        ground: color("surface", "#FFFFFF"),
        ink: color("onSurface", "#111111"),
        accent: color("accent", "#3366FF"),
    }
}

/// The shape of `slot`'s place on the canvas a made deck draws on: its tracks' share of the
/// theme's grid, out to the canvas's edge where it bleeds.
fn frame_of(theme: &Theme, slot: &Slot) -> Frame {
    use scaena_core::model::theme::Margin;
    let grid = &theme.grid;
    let (top, right, bottom, left) = match &grid.margin {
        Margin::All(m) => (*m, *m, *m, *m),
        Margin::Sides(sides) => match sides.iter().map(|s| s.0).collect::<Vec<f64>>()[..] {
            [v, h] => (v, h, v, h),
            [t, h, b] => (t, h, b, h),
            [t, r, b, l] => (t, r, b, l),
            _ => (0.0, 0.0, 0.0, 0.0),
        },
    };
    let (cols, rows) = (grid.columns.max(1) as f64, grid.rows.unwrap_or(6).max(1) as f64);
    let [w, h] = [CANVAS[0] as f64, CANVAS[1] as f64];
    let span = |r: &Option<Range>, n: f64| match r {
        Some(Range::Index(i)) => (*i as f64, *i as f64),
        Some(Range::Span([a, b])) => (*a as f64, *b as f64),
        None => (1.0, n),
    };
    let (c0, c1) = span(&slot.col, cols);
    let (r0, r1) = span(&slot.row, rows);
    let col = (w - left - right - grid.gutter * (cols - 1.0)) / cols;
    let row = (h - top - bottom - grid.gutter * (rows - 1.0)) / rows;
    let bleed = slot.bleed == Some(true);
    let x0 = if bleed && c0 <= 1.0 { 0.0 } else { left + (c0 - 1.0) * (col + grid.gutter) };
    let x1 = if bleed && c1 >= cols { w } else { left + c1 * col + (c1 - 1.0) * grid.gutter };
    let y0 = if bleed && r0 <= 1.0 { 0.0 } else { top + (r0 - 1.0) * (row + grid.gutter) };
    let y1 = if bleed && r1 >= rows { h } else { top + r1 * row + (r1 - 1.0) * grid.gutter };
    Frame::of(x1 - x0, y1 - y0)
}

/// Narrato's fields of a slide that the deck keeps beside its beat.
fn media(slide: &Slide) -> Value {
    let mut m = Map::new();
    let mut put = |k: &str, v: Option<Value>| {
        if let Some(v) = v {
            m.insert(k.into(), v);
        }
    };
    put("layout", Some(json!(slide.layout_suggestion)));
    put("beatIndex", slide.beat_index.map(|v| json!(v)));
    put("beatName", slide.beat_name.as_ref().map(|v| json!(v)));
    put("purpose", slide.purpose.as_ref().map(|v| json!(v)));
    put("intent", slide.intent.as_ref().map(|v| json!(v)));
    put("intensity", slide.expected_intensity.map(|v| json!(v)));
    put("valence", slide.expected_valence.map(|v| json!(v)));
    put("visual", Some(&slide.visual_description).filter(|v| !v.is_empty()).map(|v| json!(v)));
    put("fact", slide.supporting_facts.as_ref().filter(|v| !v.is_empty()).map(|v| json!(v)));
    // The subtitle, kept where no place of the slide's layout sets it (dogfood: 15 of 32 did not).
    put("subtitle", Some(&slide.subtitle).filter(|v| !v.is_empty()).map(|v| json!(v)));
    Value::Object(m)
}

/// The BCP 47 tag of a language the brief names by its English name.
fn language_tag(language: &str) -> Option<&'static str> {
    Some(match language {
        "English" => "en",
        "Spanish" => "es",
        "French" => "fr",
        "German" => "de",
        "Portuguese" => "pt",
        "Italian" => "it",
        "Dutch" => "nl",
        "Czech" => "cs",
        "Polish" => "pl",
        "Ukrainian" => "uk",
        "Japanese" => "ja",
        "Chinese (Simplified)" => "zh-Hans",
        "Korean" => "ko",
        "Arabic" => "ar",
        "Hindi" => "hi",
        _ => return None,
    })
}

/// The language a deck made from `brief` is in: the one it names, else English, as Narrato
/// writes when none is named.
fn deck_lang(brief: &Brief) -> Option<&'static str> {
    brief.language.as_deref().map_or(Some("en"), language_tag)
}

/// The words a made deck says itself, not the model, in its language (`lang`, BCP 47): a
/// source's label, and how a chart is described to a reader. English where the language is none
/// Narrato writes in.
struct Own {
    /// "Source: " and its kin, with the punctuation the language sets after it.
    source: &'static str,
    bar: &'static str,
    line: &'static str,
    highest: &'static str,
    lowest: &'static str,
}

fn own(lang: &str) -> Own {
    let (source, bar, line, highest, lowest) = match lang {
        "es" => ("Fuente: ", "Gráfico de barras", "Gráfico de líneas", "máximo", "mínimo"),
        "fr" => ("Source\u{a0}: ", "Diagramme en barres", "Graphique linéaire", "maximum", "minimum"),
        "de" => ("Quelle: ", "Balkendiagramm", "Liniendiagramm", "Höchstwert", "Tiefstwert"),
        "pt" => ("Fonte: ", "Gráfico de barras", "Gráfico de linhas", "máximo", "mínimo"),
        "it" => ("Fonte: ", "Grafico a barre", "Grafico a linee", "massimo", "minimo"),
        "nl" => ("Bron: ", "Staafdiagram", "Lijndiagram", "hoogste", "laagste"),
        "cs" => ("Zdroj: ", "Sloupcový graf", "Spojnicový graf", "maximum", "minimum"),
        "pl" => ("Źródło: ", "Wykres słupkowy", "Wykres liniowy", "maksimum", "minimum"),
        "uk" => ("Джерело: ", "Стовпчикова діаграма", "Лінійний графік", "максимум", "мінімум"),
        "ja" => ("出典：", "棒グラフ", "折れ線グラフ", "最高", "最低"),
        "zh-Hans" => ("来源：", "柱状图", "折线图", "最高", "最低"),
        "ko" => ("출처: ", "막대 그래프", "꺾은선 그래프", "최고", "최저"),
        "ar" => ("المصدر: ", "مخطط شريطي", "مخطط خطي", "الأعلى", "الأدنى"),
        "hi" => ("स्रोत: ", "बार चार्ट", "लाइन चार्ट", "उच्चतम", "न्यूनतम"),
        _ => ("Source: ", "Bar chart", "Line chart", "highest", "lowest"),
    };
    Own { source, bar, line, highest, lowest }
}

/// One of a slide's items: a bullet, and its label and detail where it reads "label: detail".
#[derive(Debug, Clone, PartialEq)]
struct Item {
    whole: String,
    label: Option<String>,
    detail: String,
}

impl Item {
    /// Whether it leads with its figure: "$4.1M: Q3 revenue, up 12%".
    fn figure_first(&self) -> bool {
        self.label.as_deref().is_some_and(numeric)
    }

    /// Its figure: the label that is one, else the first in its detail.
    fn figure(&self) -> Option<String> {
        match &self.label {
            Some(label) if numeric(label) => Some(label.clone()),
            _ => figure(&self.detail),
        }
    }

    /// What it names: its label, or, where it leads with its figure, its detail's first clause.
    fn name(&self) -> Option<String> {
        if self.figure_first() { Some(change(&self.detail).0) } else { self.label.clone() }
    }
}

/// A slide's items: its bullets, each without the mark a model may lead it with.
fn items(slide: &Slide, v: &Varied) -> Vec<Item> {
    // A cover and a divider say their line alone; a Q&A, set as a divider, keeps its questions.
    if matches!(v.layout.identity(), "title" | "section") {
        return Vec::new();
    }
    let mut items: Vec<Item> = slide.key_bullets.iter().map(|b| item(b)).filter(|i| !i.whole.is_empty()).collect();
    // Questions and their answers are one structure: where one question runs too long to title
    // its answer, none does, and each reads "question? answer", never "question?: answer".
    let asked = |i: &Item| i.label.is_none() && (i.whole.contains("?: ") || i.whole.contains("!: "));
    if items.iter().any(asked) {
        for i in &mut items {
            if let Some(label) = i.label.take_if(|l| l.ends_with(['?', '!'])) {
                i.whole = format!("{label} {}", i.detail);
            }
            if i.label.is_none() {
                i.whole = i.whole.replacen("?: ", "? ", 1).replacen("!: ", "! ", 1);
                i.detail = i.whole.clone();
            }
        }
    }
    items
}

/// The icon each of `slide`'s `items` is set under (PLAN G.23): the one its word names, as the
/// client found it in the set that ships; none unless each item has one.
fn icons_of<'n>(narrative: &'n Narrative, slide: &Slide, items: &[Item]) -> Option<Vec<&'n Icon>> {
    if items.is_empty() || slide.icons.len() != items.len() {
        return None;
    }
    slide.icons.iter().map(|word| narrative.icons.get(&word.trim().to_lowercase())).collect()
}

/// The icons `words` name (PLAN G.23), each the first its word finds in the set that ships
/// ([`crate::icons::find`]), in its regular weight; a word that finds none left out, and so is one
/// with no letter or digit, which [`crate::icons::find`] would answer with every icon. What the
/// CLI and the Mac hand a generation ([`Generation::give_icons`]); the browser finds them by the
/// same rule (`web/src/icons.ts`).
#[cfg(feature = "shipped")]
pub fn icons_found(words: &[String]) -> BTreeMap<String, Icon> {
    scaena_core::sort::map(words.iter().filter_map(|word| {
        if !word.chars().any(char::is_alphanumeric) {
            return None;
        }
        let found = crate::icons::find(word, crate::icons::Weight::Regular, 1).icons.into_iter().next()?;
        Some((word.clone(), Icon { name: found.name, path: found.icon.path, view: found.icon.view }))
    }))
}

/// Whether `layout` sets an icon in a place of its own (PLAN G.23).
fn iconic(theme: &Theme, layout: &str) -> bool {
    theme.layouts.get(layout).is_some_and(|l| {
        l.slots.values().any(|s| matches!(&s.prompt, Some(Prompt::Shape(p)) if p.shape == PromptShapeKind::Icon))
    })
}

fn item(bullet: &str) -> Item {
    let bullet = bullet.trim();
    // The mark a model may lead an item with, but not a figure's minus: "-13%" keeps it.
    let whole = match bullet.strip_prefix(['-', '\u{2013}']) {
        Some(rest) if rest.starts_with(|c: char| c.is_ascii_digit()) => bullet,
        _ => bullet.trim_start_matches(['\u{2022}', '-', '*', '\u{2013}']),
    }
    .trim()
    .to_string();
    // A label is short, and ends at the first ": ", " — ", or " – ".
    let split = [": ", " \u{2014} ", " \u{2013} "]
        .iter()
        .filter_map(|sep| whole.find(sep).map(|at| (at, sep.len())))
        .min_by_key(|(at, _)| *at)
        .filter(|(at, _)| (1..=60).contains(&whole[..*at].chars().count()));
    match split {
        Some((at, len)) => {
            Item { label: Some(whole[..at].trim().to_string()), detail: whole[at + len..].trim().to_string(), whole }
        }
        None => Item { label: None, detail: whole.clone(), whole },
    }
}

/// `text`'s first clause and the rest: "Q3 revenue, up 12%" → ("Q3 revenue", "up 12%").
fn head(text: &str) -> (String, String) {
    let at = [", ", "; ", " \u{2014} ", " \u{2013} "]
        .iter()
        .filter_map(|sep| text.find(sep).map(|at| (at, sep.len())))
        .min_by_key(|(at, _)| *at);
    match at {
        Some((at, len)) => (text[..at].trim().to_string(), text[at + len..].trim().to_string()),
        None => (text.trim().to_string(), String::new()),
    }
}

/// `text`'s name and what is said after it, as [`head`] cuts them, but a name whole where all
/// that follows it is a date: "Q3 revenue, up 12%" → ("Q3 revenue", "up 12%"); "Hours dark during
/// Storm Nadia, 2026" names it whole, the year no change beside a figure.
fn change(text: &str) -> (String, String) {
    let (name, rest) = head(text);
    const MONTHS: &str = "january february march april may june july august september october november december";
    let dated = !rest.is_empty()
        && rest.split_whitespace().all(|w| {
            let w = w.trim_end_matches(['.', ',']).to_lowercase();
            w.bytes().all(|b| b.is_ascii_digit()) || MONTHS.split(' ').any(|m| m.starts_with(&w) && w.len() >= 3)
        })
        && rest.bytes().any(|b| b.is_ascii_digit());
    if dated { (text.trim().to_string(), String::new()) } else { (name, rest) }
}

/// `text` without the figure it opens with: "2,400 a month from inbound" → "a month from
/// inbound"; `text` as it is where it opens otherwise.
fn without_figure(text: &str) -> String {
    let text = text.trim();
    match figure(text).and_then(|f| text.strip_prefix(f.as_str())) {
        Some(rest) => rest.trim_start_matches([',', ';', ':']).trim().to_string(),
        None => text.to_string(),
    }
}

/// The first figure in `text`: a number with its currency sign, its suffix, or its percent.
fn figure(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let start = chars.iter().position(|c| c.is_ascii_digit())?;
    let mut begin = start;
    while begin > 0 && matches!(chars[begin - 1], '$' | '\u{20ac}' | '\u{a3}' | '\u{a5}' | '+' | '-' | '\u{2212}' | '~')
    {
        begin -= 1;
    }
    let mut end = start;
    while end < chars.len() && (chars[end].is_ascii_digit() || matches!(chars[end], ',' | '.')) {
        end += 1;
    }
    while end > start && matches!(chars[end - 1], ',' | '.') {
        end -= 1;
    }
    // A magnitude a language without spaces writes after its figure, and its currency after
    // that: "41.2\u{4e07}\u{30c9}\u{30eb}" is $412K, never "41.2".
    let rest: String = chars[end..].iter().take(3).collect();
    if MAGNITUDES.iter().any(|m| rest.starts_with(m)) {
        let after: String = chars[end + 1..].iter().take(2).collect();
        let currency = CURRENCIES.iter().filter(|c| after.starts_with(*c)).map(|c| c.chars().count()).max();
        return Some(chars[begin..end + 1 + currency.unwrap_or(0)].iter().collect());
    }
    let suffix = ["%", "\u{d7}", "x", "M", "B", "K", "k", "T", "bn", "tn", "m", "pp", "pts"]
        .iter()
        .filter(|s| rest.starts_with(*s))
        .max_by_key(|s| s.len())
        .map_or(0, |s| s.chars().count());
    // A suffix is the figure's only where the word stops there: "5 M" no, "5M" yes, "5Mbps" no.
    let after = chars.get(end + suffix).copied();
    let suffix = if suffix > 0 && after.is_none_or(|c| !c.is_alphanumeric()) { suffix } else { 0 };
    Some(chars[begin..end + suffix].iter().collect())
}

/// The magnitudes Japanese, Chinese, and Korean write after a figure: ten thousand, a hundred
/// million, a trillion, a thousand.
const MAGNITUDES: [&str; 8] =
    ["\u{4e07}", "\u{5104}", "\u{5146}", "\u{5343}", "\u{4ebf}", "\u{b9cc}", "\u{c5b5}", "\u{cc9c}"];

/// The currencies written after such a magnitude: dollars, yen, yuan, won.
const CURRENCIES: [&str; 6] =
    ["\u{30c9}\u{30eb}", "\u{5186}", "\u{5143}", "\u{7f8e}\u{5143}", "\u{c6d0}", "\u{b2ec}\u{b7ec}"];

/// Whether `text` is a figure and little else, what a number's place shows: "42%", "$4.1M",
/// "2024", "6 weeks", "212 kt", "1.55\u{b0}C", "4.5 mm/yr".
fn numeric(text: &str) -> bool {
    let text = text.trim();
    let Some(rest) = figure(text).and_then(|f| text.strip_prefix(f.as_str()).map(str::trim)) else { return false };
    let n = rest.chars().count();
    // Its unit, one word: letters, and a rate's slash or a degree's sign among them.
    let unit = |c: char| c.is_alphabetic() || matches!(c, '/' | '\u{b0}');
    n <= 2 || (n <= 12 && rest.chars().all(unit) && rest.chars().any(char::is_alphabetic))
}

/// What a big number means, said apart from it: the detail of a figure that leads its item
/// ("1.55\u{b0}C: above pre-industrial"), else the second item, as Scaena's slot shapes ask.
fn meaning(items: &[Item]) -> Option<String> {
    match items.first() {
        Some(first) if first.figure_first() && !first.detail.is_empty() => Some(sentence(&first.detail)),
        _ => items.get(1).map(|i| i.whole.clone()),
    }
}

/// The theme layout a slide is drawn in: what its identity comes to in Scaena's library, sized
/// to its items, else a list of its bullets, else the first the theme has. A slide with a
/// picture made for it takes the layout that shows its picture beside its words, where the
/// library has one; one with none takes a layout that waits for none.
fn layout_for(
    identity: &str,
    slide: &Slide,
    items: &[Item],
    index: usize,
    theme: &Theme,
    pictured: bool,
    iconed: bool,
) -> String {
    let n = items.len();
    let chart = slide.chart_data.is_some();
    let words = format!("{} {}", slide.slide_title, slide.subtitle).to_lowercase();
    let labelled = n > 0 && items.iter().all(|i| i.label.is_some());
    // Each item said in a line or two.
    let brief = items.iter().all(|i| i.detail.chars().count() <= 120);
    let side = |left: &'static str, right: &'static str| if index.is_multiple_of(2) { left } else { right };
    let wanted: &str = match identity {
        "title" if pictured => "title-split",
        "title" => "title-center",
        "section" if pictured => "section-photo",
        "section" => "section",
        "agenda" if pictured => "agenda-photo",
        "agenda" => "agenda",
        "closing" | "thanks" if pictured => "title-split",
        "closing" | "thanks" => "closing",
        "body" if pictured => "bullets-photo",
        "body" => "bullets",
        "numbered" => "numbered",
        "quote" => "quote",
        "statement" => "statement",
        "definition" => "definition",
        "metric" if pictured => "number-photo",
        "metric" => "big-number",
        "scorecard" | "kpi-grid" => match n {
            0..=2 => "two-numbers",
            // Three prices, each with its tier: a pricing slide.
            3 if priced(items) && theme.layouts.contains_key("pricing") => "pricing",
            3 => "numbers",
            // Four figures too wide for a quarter of the slide, or named at length: a square of
            // them, each as large as its quarter takes (G.16).
            4 if square(items) && theme.layouts.contains_key("four-numbers") => "four-numbers",
            4 => "scorecard",
            _ => "number-grid",
        },
        // Two bars of one series are two figures, each as large as the slide takes.
        "bar-chart" if two(slide) && theme.layouts.contains_key("two-figures") => "two-figures",
        "bar-chart" if chart => "bar-chart",
        "line-chart" if chart => "line-chart",
        "bar-chart" | "line-chart" => "bullets",
        "two-column" if n == 3 && labelled => "three-columns",
        "two-column" if n == 4 && labelled => "quadrants",
        // Two sides said in a line each: each a card, its words set large.
        "two-column" | "before-after" if n == 2 && labelled && brief => "myth-fact",
        "two-column" if labelled => "comparison",
        "two-column" => "two-columns",
        // A picture each side: with none, the two sides as two labelled columns.
        "before-after" => "comparison",
        "pros-cons" => "pros-cons",
        "quadrant" => "quadrants",
        "steps" if n <= 3 => "how-it-works",
        // Four steps, each in a card of its own under its number, as a deck's process is drawn.
        "steps" if n == 4 => "numbered-cards",
        "steps" => "steps",
        // Moments each told in a sentence go down the slide, each beside its date (G.16).
        "timeline" if n <= 4 && told(items) && theme.layouts.contains_key("milestones") => "milestones",
        "timeline" => "timeline",
        "journey" => "journey",
        "image" if pictured => "full",
        "image" => "statement",
        "image-text" if pictured => side("photo-left", "photo-right"),
        "image-text" => "bullets",
        // A picture for each place: with one or none, the places as columns of words.
        "gallery" => match n {
            0..=2 => "two-columns",
            3 => "three-columns",
            4 => "quadrants",
            _ => "bullets",
        },
        "framework" => {
            if ["cycle", "loop", "flywheel", "circular"].iter().any(|w| words.contains(w)) {
                "cycle"
            } else if ["venn", "overlap", "intersect"].iter().any(|w| words.contains(w)) || n == 3 {
                "venn"
            } else if ["hub", "spoke", "core", "center", "centre"].iter().any(|w| words.contains(w)) {
                "hub-spoke"
            } else {
                "pyramid"
            }
        }
        "callout" if pictured => "callout",
        "callout" => "bullets",
        "roadmap" => "roadmap",
        "matrix" => "matrix",
        "okrs" => "okrs",
        "raci" => "raci",
        "swot" => "swot",
        "thesis" => "thesis",
        "letter" => "letter",
        "memo" => "memo",
        "receipt" => "receipt",
        // "action — owner — when", each; else steps, each "label: detail".
        "next-steps" if n <= 3 && items.iter().all(|i| parts(&i.whole).len() >= 2) => "next-steps",
        "next-steps" if labelled && n == 4 => "numbered-cards",
        "next-steps" if labelled && n <= 4 => "steps",
        "next-steps" => "numbered",
        "milestones" => "milestones",
        "3horizons" => "three-horizons",
        "funnel" => "funnel",
        "scenarios" => "scenarios",
        "vs" => "head-to-head",
        "ranking" => "ranking",
        "headline" if pictured => "headline",
        "headline" => "statement",
        "manifesto" => "manifesto",
        // With no picture to show, the people stand by name, under the headline, with no band
        // left empty for their faces.
        "team" if !pictured => "team-names",
        "team" => match n {
            0..=3 => "team",
            4 => "team-four",
            _ => "team-six",
        },
        "bio" => "bio",
        "testimonials" if n <= 2 => "two-quotes",
        "testimonials" => "testimonials",
        "how-it-works" => "how-it-works",
        "pipeline" => "pipeline",
        "stack" => "stack",
        // Three questions on three cards, none left empty.
        "faq" if n == 3 && theme.layouts.contains_key("cards") => "cards",
        "faq" => "faq",
        "glossary" => "glossary",
        "checklist" => "checklist",
        "ask" => "the-ask",
        "qa" => "qa",
        "recipe-card" => "recipe",
        _ => "bullets",
    };
    // Items each under the icon its word names (G.23): three side by side, in columns and on
    // cards in turn; four down the slide; five or six in two rows of three. Steps, figures, and
    // a layout that reads its items in its own way keep theirs.
    let twin = match (wanted, n) {
        ("bullets" | "three-columns", 3) => side("icon-columns", "icon-cards"),
        ("bullets" | "quadrants", 4) => "icon-rows",
        ("bullets", 5 | 6) => "icon-grid",
        _ => wanted,
    };
    let wanted = if iconed && labelled && brief && theme.layouts.contains_key(twin) { twin } else { wanted };
    // A layout with no place for some of the items gives way to a list of them all.
    let sequential = [
        "steps",
        "timeline",
        "milestones",
        "how-it-works",
        "pipeline",
        "funnel",
        "roadmap",
        "agenda",
        "numbered",
        "ranking",
        "manifesto",
    ];
    let wanted = match holds(theme, wanted, n) {
        true => wanted,
        false if sequential.contains(&identity) => "numbered",
        false => "bullets",
    };
    // A theme with none of the library's names draws it in its first layout.
    [wanted, "bullets", "statement"]
        .into_iter()
        .find(|l| theme.layouts.contains_key(*l))
        .or_else(|| theme.layouts.keys().next().map(String::as_str))
        .unwrap_or("bullets")
        .to_string()
}

/// Whether `layout` has a place for each of `n` items: a list or a body holds any number; a
/// layout of places, one item for each ordinal; a line with its support, one; a quote, a number,
/// or a definition, the two it sets beside it. A chart takes its takeaway alone, its figures in
/// its data.
fn holds(theme: &Theme, layout: &str, n: usize) -> bool {
    let Some(l) = theme.layouts.get(layout) else { return true };
    match layout {
        // A pros-and-cons slide sorts its items onto two lists.
        "title" | "section" | "bar-chart" | "line-chart" | "pros-cons" | "two-figures" => return true,
        "statement" | "closing" | "thesis" | "headline" => return n <= 1,
        "quote" | "big-number" | "number-photo" | "definition" => return n <= 2,
        _ => {}
    }
    let lists = l.slots.iter().any(|(name, s)| {
        ordinal(name).is_none()
            && (matches!(s.prompt, Some(Prompt::Items(_))) || matches!(name.as_str(), "body" | "story" | "details"))
    });
    let places = l.slots.keys().filter_map(|name| ordinal(name).map(|(k, _)| k + 1)).max().unwrap_or(0);
    // A head-to-head's first item names the two sides; a receipt's last is its total.
    let extra = usize::from(matches!(layout, "head-to-head" | "receipt"));
    lists || places == 0 || n <= places + extra
}

/// `text` as a sentence begins: its first word capitalized where it is all lower case, as the
/// detail after a label begins ("cuts emissions 80%"); a word that has a capital of its own
/// ("iPhone") left as it is.
fn sentence(text: &str) -> String {
    let word = text.split_whitespace().next().unwrap_or_default();
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if first.is_lowercase() && word.chars().all(|c| !c.is_uppercase()) => {
            first.to_uppercase().chain(chars).collect()
        }
        _ => text.to_string(),
    }
}

/// A slide as the deck plans it before any is built: its beat, its items, the icons they stand
/// under, and its layout.
struct Planned<'n> {
    beat: usize,
    items: Vec<Item>,
    icons: Option<Vec<&'n Icon>>,
    layout: String,
}

/// The number each section divider stands by, by the slide's index (narrative wayfinding): the
/// place of the agenda item its title names, from 1, so "03" opens the agenda's third part; a
/// divider whose title names none counts on from the one before it. None on a deck with one
/// divider, or with no agenda to count by.
fn divider_numbers(narrative: &Narrative, planned: &[Planned]) -> BTreeMap<usize, usize> {
    let dividers: Vec<usize> = (planned.iter().enumerate())
        .filter(|(_, p)| matches!(p.layout.as_str(), "section" | "section-photo"))
        .map(|(i, _)| i)
        .collect();
    let Some(Planned { items: agenda, .. }) = planned.iter().find(|p| p.layout.starts_with("agenda")) else {
        return BTreeMap::new();
    };
    if dividers.len() < 2 {
        return BTreeMap::new();
    }
    let words = |s: &str| -> Vec<String> {
        (s.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() > 2)).map(str::to_lowercase).collect()
    };
    // Each part by its label's words, which weigh twice, and its whole line's.
    let parts: Vec<(Vec<String>, Vec<String>)> = (agenda.iter())
        .map(|item| (item.label.as_deref().map(words).unwrap_or_default(), words(&item.whole)))
        .collect();
    let mut out = BTreeMap::new();
    let mut last = 0;
    for i in dividers {
        let title = words(&narrative.slides[i].slide_title);
        let shared = |w: &Vec<String>| w.iter().filter(|w| title.contains(w)).count();
        let named = (parts.iter().enumerate())
            .map(|(k, (label, whole))| (k, 2 * shared(label) + shared(whole)))
            .filter(|&(k, n)| n > 0 && k >= last)
            .max_by_key(|&(k, n)| (n, std::cmp::Reverse(k)))
            .map(|(k, _)| k + 1);
        last = named.unwrap_or(last + 1);
        out.insert(i, last);
    }
    out
}

/// What a slide's slots are filled from.
/// How a generated slide enters, from the deck's structure (narrative ATTR-1), so that the arc
/// the model chose shows in play, as a presentation app's presets and a storyteller's turning
/// points mark the movements of a talk: `style`, a slide transition (PLAN 2.111) where the
/// structure asks for one, else the plain transition that morphs what both slides share; and
/// `slow`, the theme's `slow` duration in place of its `standard`. None where the slide is the
/// deck's first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Entrance {
    style: Option<&'static str>,
    slow: bool,
}

/// A valence at or under which a slide reads as grave: Narrato writes `expectedValence` from −1,
/// grave, to +1, hopeful.
const GRAVE: f64 = -0.5;

/// How the slide in `layout` enters after `previous`, the slide before it in its layout: a
/// section divider, and the first slide of the part it opens, push in, so each movement of the
/// talk is a turn of the page; the closing slide dissolves in slowly; a grave turn, a slide that
/// reads grave after one that did not, takes the slow pace; and content slides between keep the
/// plain transition, which morphs what two slides in one layout share. The same narrative gives
/// the same entrances. None for the deck's first slide.
fn entrance(layout: &str, slide: &Slide, previous: Option<(&str, &Slide)>, last: bool) -> Option<Entrance> {
    let (before, earlier) = previous?;
    let divider = |l: &str| l.starts_with("section");
    let valence = |s: &Slide| s.expected_valence.unwrap_or(0.0);
    let grave = valence(slide) <= GRAVE && valence(earlier) > GRAVE;
    Some(if last {
        Entrance { style: Some("dissolve"), slow: true }
    } else if divider(layout) || divider(before) {
        Entrance { style: Some("push"), slow: grave }
    } else {
        Entrance { style: None, slow: grave }
    })
}

struct Slots<'a> {
    theme: &'a Theme,
    layout: &'a str,
    state: &'a str,
    slide: &'a Slide,
    /// Its body after the variety pass: a bullet a cover or a closing took for its line.
    body: &'a str,
    items: &'a [Item],
    /// The number a section divider stands by: its agenda item's, from 1; none on a deck with one
    /// divider or no agenda, where a number would count nothing the audience was shown.
    section: Option<usize>,
    /// The title of the part of the talk the slide is in: its last divider's.
    running: Option<&'a str>,
    /// The picture made for the slide, if one was.
    picture: Option<&'a Placed>,
    /// The icon each item is set under, where each has one (G.23).
    icons: Option<Vec<&'a Icon>>,
    /// The deck's language (BCP 47).
    lang: &'a str,
}

/// What one slide added beyond its nodes: the data source its chart reads, and the steps its
/// list builds by (narrative ATTR-2), each a state after the slide's own.
struct SlideMade {
    data: Option<String>,
    steps: Vec<String>,
}

/// What fills a slot.
enum Fill {
    Text(String),
    List(Vec<String>, &'static str),
    /// The prompt's own words: a column's or a lane's label.
    Prompt,
    /// Such a label in the deck's language.
    Label(String),
    Shape,
    /// An item's icon, in a place for one (G.23).
    Icon(Icon),
    Picture,
    Chart,
}

const ORDINALS: [&str; 9] = ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth"];

/// A slot's ordinal and what part of the item it holds: `second-title` → (1, "title"),
/// `third` → (2, "").
fn ordinal(slot: &str) -> Option<(usize, &str)> {
    ORDINALS.iter().enumerate().find_map(|(k, o)| {
        if slot == *o {
            Some((k, ""))
        } else {
            slot.strip_prefix(o).and_then(|r| r.strip_prefix('-')).map(|part| (k, part))
        }
    })
}

/// Whether `slot` is one of a journey's cells: what a person does, thinks, or feels in a phase.
fn journey_cell(slot: &str) -> bool {
    ["does-", "thinks-", "feels-"].iter().any(|lane| slot.strip_prefix(lane).is_some_and(|o| ORDINALS.contains(&o)))
}

/// Slots whose prompt names a structure the slide keeps: a column's or a lane's label, a rank, a
/// step's numeral, an axis.
fn keeps_prompt(slot: &str) -> bool {
    slot.ends_with("-label")
        || slot.ends_with("-lane")
        || slot.ends_with("-role")
        || slot.ends_with("-rank")
        || slot.ends_with("-number")
        || slot.starts_with("month-")
        || matches!(slot, "needs-title" | "method-title" | "x-low" | "x-high" | "y-low" | "y-high")
}

/// Roles that set words to be read, not a figure, a title, or a label.
fn words_role(role: &str) -> bool {
    matches!(role, "body" | "lede" | "caption")
}

/// Whether `text` is a few short lines at most: three lines of the measure a comfortable line
/// keeps, 60 characters.
fn short(text: &str) -> bool {
    text.chars().count() <= 180 && text.lines().count() <= 3
}

/// Roles that set a figure.
fn figure_role(role: &str) -> bool {
    matches!(role, "figure" | "hero" | "value" | "numeral")
}

/// Someone else's words in quotation marks, the marks a model may have set taken off first.
fn quoted(text: &str, lang: &str) -> String {
    let marks = Marks::of(lang);
    format!("{}{}{}", marks.open, unquoted(text), marks.close)
}

/// The double quotation marks a language sets, each with the space it keeps inside: “…” in
/// English and most others, 「…」 in Japanese and Traditional Chinese, „…“ in German, and « … » in
/// French, its spaces ones a line never breaks at.
struct Marks {
    open: &'static str,
    close: &'static str,
}

impl Marks {
    fn of(lang: &str) -> Marks {
        let base = lang.split('-').next().unwrap_or(lang);
        match (base, lang) {
            ("ja", _) | (_, "zh-Hant") => Marks { open: "\u{300c}", close: "\u{300d}" },
            ("de", _) => Marks { open: "\u{201e}", close: "\u{201c}" },
            ("fr", _) => Marks { open: "\u{ab}\u{a0}", close: "\u{a0}\u{bb}" },
            _ => Marks { open: "\u{201c}", close: "\u{201d}" },
        }
    }
}

/// Who said a quote, led by a dash: "\u{2014} Head of Logistics".
fn attributed(who: &str) -> String {
    format!("\u{2014} {}", who.trim().trim_start_matches(['\u{2014}', '\u{2013}', '-']).trim())
}

fn unquoted(text: &str) -> String {
    text.trim().trim_matches(['"', '\u{201c}', '\u{201d}', '\u{2018}', '\u{2019}', '\'']).trim().to_string()
}

/// "words — who": the words, and who said them.
fn said(item: &Item) -> (String, Option<String>) {
    match item.whole.rsplit_once(" \u{2014} ").or_else(|| item.whole.rsplit_once(" \u{2013} ")) {
        Some((words, who)) => (unquoted(words), Some(who.trim().to_string())),
        None => (unquoted(&item.whole), None),
    }
}

/// An item cut into its parts at " — " or " | ".
fn parts(text: &str) -> Vec<String> {
    let sep = if text.contains(" \u{2014} ") { " \u{2014} " } else { " | " };
    text.split(sep).map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

/// What `text` sets side by side: "A | B", "A vs. B", "A, versus B", "A versus B".
fn versus(text: &str) -> Vec<String> {
    let sep = [" | ", " vs. ", " vs ", ", versus ", " versus "].into_iter().find(|sep| text.contains(sep));
    match sep {
        Some(sep) => text.split(sep).map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect(),
        None => vec![text.trim().to_string()],
    }
}

/// A head-to-head's row from "criterion: one side's answer, versus the other's": its criterion
/// and its two sides. Where one side is a bare figure and the other opens with its figure and
/// says what it counts after it, what it counts goes to the criterion, so that both sides read
/// as figures: "Extreme heat: 14% of people exposed, versus 37%" → "Extreme heat: people
/// exposed", "14%", "37%".
fn row(item: &Item) -> (Option<String>, Option<String>, Option<String>) {
    let sides = versus(&item.detail);
    let (mut left, mut right) = (sides.first().cloned(), sides.get(1).cloned());
    let mut criterion = item.label.clone();
    // A side that opens with its figure: the figure, and the words after it.
    let opens = |s: &str| {
        let f = figure(s).filter(|f| s.trim().starts_with(f.as_str()))?;
        let rest = s.trim()[f.len()..].trim().to_string();
        Some((f, rest))
    };
    if let (Some(l), Some(r)) = (left.clone(), right.clone()) {
        let moved = match (numeric(&l), numeric(&r)) {
            (false, true) => opens(&l).map(|(f, rest)| (f, rest, true)),
            (true, false) => opens(&r).map(|(f, rest)| (f, rest, false)),
            _ => None,
        };
        if let Some((f, rest, on_left)) = moved.filter(|(_, rest, _)| rest.starts_with(char::is_alphabetic)) {
            let counts = rest.strip_prefix("of ").unwrap_or(&rest).to_string();
            if on_left {
                left = Some(f);
            } else {
                right = Some(f);
            }
            criterion = Some(match criterion {
                Some(c) => format!("{c}: {counts}"),
                None => sentence(&counts),
            });
        }
    }
    (criterion, left.map(|s| sentence(&s)), right.map(|s| sentence(&s)))
}

/// The latest figure a chart plots, in its unit: its first series' last value.
fn latest(chart: &ChartData) -> Option<String> {
    let value = *chart.series.first()?.values.last()?;
    Some(in_unit(value, chart.unit.as_deref()))
}

/// `value` written with `unit`: "$44/MWh", "1.55\u{b0}C", "30%", "424 ppm", "$2T".
fn in_unit(value: f64, unit: Option<&str>) -> String {
    let n = if value.fract() == 0.0 && value.abs() >= 10_000.0 && value.abs() < 1e15 {
        // Thousands set apart: 1,250,000.
        let digits = format!("{}", value.abs() as u64);
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        if value < 0.0 { format!("-{out}") } else { out }
    } else {
        format!("{value}")
    };
    match unit.map(str::trim).filter(|u| !u.is_empty()) {
        None => n,
        Some(u) if u.starts_with(['$', '\u{20ac}', '\u{a3}', '\u{a5}']) => {
            let sign = u.chars().next().expect("not empty");
            format!("{sign}{n}{}", &u[sign.len_utf8()..])
        }
        Some(u) if u.starts_with(['%', '\u{b0}', '/', '\u{d7}']) => format!("{n}{u}"),
        // A unit written in a script set without spaces sits against its figure: "2時間", "340ドル".
        Some(u) if u.chars().next().is_some_and(unspaced) => format!("{n}{u}"),
        Some(u) => format!("{n} {u}"),
    }
}

/// Whether `c` is of a script written without spaces between its words: kana, Han, and Hangul.
fn unspaced(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff | 0xff66..=0xff9f)
}

/// A detail cut at "; " into a list's items.
fn listed(text: &str) -> Vec<String> {
    text.split("; ").map(|p| p.trim().trim_end_matches('.').to_string()).filter(|p| !p.is_empty()).collect()
}

/// What `text` says after the figure it opens with, where the figure is a clause of its own:
/// "1,200, 28% go on" → "28% go on"; none where it opens otherwise, or the figure runs on into
/// its sentence ("$40M revenue by 2028").
fn after_figure(text: &str) -> Option<String> {
    let f = figure(text)?;
    let rest = text.trim().strip_prefix(f.as_str())?;
    if rest.is_empty() {
        return Some(String::new());
    }
    [", ", "; ", " \u{2014} ", " \u{2013} "].iter().find_map(|sep| rest.strip_prefix(sep)).map(|r| r.trim().to_string())
}

impl Slots<'_> {
    fn title(&self) -> Option<String> {
        Some(self.slide.slide_title.clone()).filter(|t| !t.is_empty())
    }

    /// The subtitle, else the line the variety pass gave a cover or a closing.
    fn subtitle(&self) -> Option<String> {
        Some(self.slide.subtitle.clone())
            .filter(|t| !t.is_empty())
            .or_else(|| Some(self.body.to_string()).filter(|b| !b.is_empty() && !b.starts_with('\u{2022}')))
    }

    /// A chart's caption: its source, else the slide's supporting fact. What it shows is its
    /// value axis's title, which a caption never says again.
    fn caption(&self) -> Option<String> {
        let chart = self.slide.chart_data.as_ref()?;
        (chart.source_id.as_deref().filter(|s| !s.trim().is_empty())).map(|s| self.sourced(s)).or_else(|| self.fact())
    }

    fn fact(&self) -> Option<String> {
        self.slide.supporting_facts.clone().filter(|t| !t.is_empty())
    }

    fn item(&self, k: usize) -> Option<&Item> {
        self.items.get(k)
    }

    /// The icon the `k`th item is set under (G.23).
    fn icon(&self, k: usize) -> Option<&Icon> {
        self.icons.as_ref().and_then(|icons| icons.get(k).copied())
    }

    fn wholes(&self) -> Vec<String> {
        self.items.iter().map(|i| i.whole.clone()).collect()
    }

    /// Words a card holds: each item whole where every item is short, else what each names, with
    /// the figure its words give, so the cards of one slide read alike: "US freight spend · $1.1T".
    fn short(&self, item: &Item) -> String {
        let long = self.items.iter().any(|i| i.label.is_some() && i.whole.chars().count() > 36);
        match &item.label {
            Some(label) if long => match figure(&item.detail).filter(|f| !label.contains(f.as_str())) {
                Some(f) => format!("{label} \u{b7} {f}"),
                None => label.clone(),
            },
            _ => item.whole.clone(),
        }
    }

    /// Whether `slot` is a content slide's foot: in the grid's last two rows, on a slide that is
    /// no cover, divider, closing, or question.
    fn foot(&self, slot: &Slot) -> bool {
        let rows = self.theme.grid.rows.unwrap_or(6);
        let top = match &slot.row {
            Some(Range::Index(r)) => *r,
            Some(Range::Span([from, _])) => *from,
            None => return false,
        };
        let opening = ["title", "section", "closing", "qa", "photo-title"];
        top + 1 >= rows && !opening.iter().any(|o| self.layout.starts_with(o))
    }

    /// Whether each of the slide's items leads with its figure ("2021: …", "$4.1M: …"), so that
    /// a place for an item's number shows its figure rather than counting.
    fn numbered_by_figure(&self) -> bool {
        !self.items.is_empty() && self.items.iter().all(Item::figure_first)
    }

    /// Whether the layout has a slot of this name.
    fn has(&self, slot: &str) -> bool {
        self.template().is_some_and(|l| l.slots.contains_key(slot))
    }

    fn template(&self) -> Option<&scaena_core::model::theme::Layout> {
        self.theme.layouts.get(self.layout)
    }

    /// How large the text in the slot `name`, set in `role`, may grow to fill its place (Jay's
    /// review, 2026-10-12): a part of a set (`first`, `second-title`, …) to the theme's lede for
    /// words, its body for a caption or a name set as a label, its numeral for a scorecard's
    /// figure and for another figure but a date; a section's or a cover's title to
    /// the theme's display size, and what supports a section's to the lede; a quote to
    /// nine-tenths of the headline, or the display's size where no headline heads it. None where
    /// it keeps its role's size.
    fn grown(&self, name: &str, role: &str) -> Option<f64> {
        let size = |r: &str| self.theme.typography.roles.get(r).map(|r| r.size);
        let at_least = |r: &str| size(r).filter(|most| size(role).is_some_and(|own| *most > own));
        let section = matches!(self.layout, "section" | "section-photo" | "section-number");
        let dated = matches!(self.layout, "timeline" | "milestones");
        match (ordinal(name), role) {
            // A card's or a step's own number keeps its size.
            (Some((_, "number")), _) => None,
            (Some(_), "body") => at_least("lede"),
            // A card's statement, as large as three-quarters of the slide's headline; one of two
            // cards side by side, nine-tenths of it, so each fills its card and the slide's own
            // headline still leads (G.20).
            (Some(_), "lede") => {
                let most = if self.layout == "myth-fact" { 0.9 } else { 0.75 };
                size("headline").map(|h| h * most).filter(|most| size(role).is_some_and(|own| *most > own))
            }
            // Quotes side by side, as one alone grows: as large as their cards take, under the
            // headline's size, the set held to one (dogfood pass 4).
            (Some(_), "quote") => {
                size("headline").map(|h| h * 0.9).filter(|most| size(role).is_some_and(|own| *most > own))
            }
            (Some(_), "caption") => at_least("body"),
            // What a figure or a row is, named in a label, as large as words read from the room.
            (Some((_, "title" | "criterion")), "label") => at_least("body"),
            // A journey's phases and lanes, over and beside its cells, likewise.
            (Some((_, "phase" | "lane")), "label") if self.layout == "journey" => at_least("body"),
            (Some((_, "figure")), "figure") => at_least("numeral"),
            // A tier's price is its card's point: as large as its place takes, up to the hero's
            // size, the set's held to one (dogfood pass 4).
            (Some((_, "price")), "figure") => size("hero").or_else(|| at_least("numeral")),
            // Two figures, as large as the slide takes: up to the hero's size.
            (Some((_, "figure")), "hero") if self.layout == "two-figures" => size("hero"),
            // A scorecard's figures are its point: as large as their places take, toward the
            // numeral's size, the set held to one and kept clear of each other (dogfood pass 5).
            (Some(_), "figure") if !dated => at_least("numeral"),
            (Some(_), _) => None,
            (None, "headline") if name == "title" && (section || self.layout == "title-split") => at_least("display"),
            // One sentence set large: as large as its place takes, up to the hero's size
            // (dogfood pass 4).
            (None, "display") if name == "statement" => {
                size("hero").filter(|most| size(role).is_some_and(|own| *most > own))
            }
            (None, "body") if name == "support" && section => at_least("lede"),
            // A quote under a headline is the slide's words, not its title: as large as its place
            // takes, under the headline's size; on a slide no headline heads, it is the slide,
            // and grows toward the display size, as a lone statement does (dogfood pass 4).
            (None, "quote") if name == "quote" => {
                let most = if self.headed() { size("headline").map(|h| h * 0.9) } else { size("display") };
                most.filter(|most| size(role).is_some_and(|own| *most > own))
            }
            // A line's figure, as large as its column takes, up to a scorecard's; what it says
            // under it, as words read from the room (G.19).
            (None, "figure") if name == "callout" && self.layout == "line-chart" => at_least("numeral"),
            (None, "caption") if name == "note" && self.layout == "line-chart" => at_least("body"),
            // A journey's cells, a few words each, as large as their places take, up to the lede,
            // all of a slide's at one size (dogfood pass 5), set in the body where the theme sets
            // them so (lift-up dogfood) or in the caption where an older copy does.
            (None, "caption" | "body") if self.layout == "journey" && journey_cell(name) => at_least("lede"),
            _ => None,
        }
    }

    /// Three-quarters of the theme's headline: as large as words under a heading grow.
    fn under_headline(&self) -> Option<f64> {
        self.theme.typography.roles.get("headline").map(|r| r.size * 0.75)
    }

    /// How large a text set in `role` in a place of its own grows where no set holds it: a
    /// term's meaning and its example, under the term. None elsewhere.
    fn alone(&self, name: &str, role: &str) -> Option<f64> {
        match (self.layout, name) {
            ("definition", "meaning" | "example") if words_role(role) => self.under_headline(),
            _ => None,
        }
    }

    /// The size of the title over the card whose words are in the slot `name`, where the card
    /// has one: its `-title` place's role's.
    fn titled(&self, name: &str) -> Option<f64> {
        self.role_of(&format!("{name}-title")).and_then(|r| self.size(r))
    }

    /// The theme's size for `role`.
    fn size(&self, role: &str) -> Option<f64> {
        self.theme.typography.roles.get(role).map(|r| r.size)
    }

    /// Whether a figure leads the card whose words are in the slot `name`: its `-figure` place,
    /// or its `-title` set in a figure's role. Its words keep to a caption's size under it.
    fn figured(&self, name: &str) -> bool {
        self.has(&format!("{name}-figure")) || self.role_of(&format!("{name}-title")).is_some_and(figure_role)
    }

    /// Whether a place in the layout sets a headline.
    fn headed(&self) -> bool {
        self.template().is_some_and(|l| l.slots.values().any(|s| s.role.as_deref() == Some("headline")))
    }

    /// The role of the layout's slot of this name.
    fn role_of(&self, slot: &str) -> Option<&str> {
        self.template().and_then(|l| l.slots.get(slot)).and_then(|s| s.role.as_deref())
    }

    /// What the slot holds, if anything.
    fn fill(&self, name: &str, slot: &Slot) -> Option<Fill> {
        let list_kind = match &slot.prompt {
            Some(Prompt::Items(PromptItems::Number(_))) => Some("number"),
            Some(Prompt::Items(PromptItems::Bullet(_))) => Some("bullet"),
            _ => None,
        };
        let text = |t: Option<String>| t.map(Fill::Text);
        let picture = matches!(slot.example, Some(Example::Picture(_)));
        // A place for an item's icon holds the one its word names (G.23).
        if let Some(Prompt::Shape(shape)) = &slot.prompt
            && shape.shape == PromptShapeKind::Icon
        {
            return ordinal(name).and_then(|(k, _)| self.icon(k)).map(|icon| Fill::Icon(icon.clone()));
        }
        if let Some(Prompt::Shape(_)) = &slot.prompt {
            // A card, a rule, or an arrow: with its ordinal's item, or the slide's own.
            return match ordinal(name) {
                Some((k, _)) => self.item(k).map(|_| Fill::Shape),
                None => Some(Fill::Shape),
            };
        }
        // An item's number place shows the figure each item leads with, where every one does: a
        // year's card reads "2021", not "01".
        if let Some((k, "number")) = ordinal(name)
            && self.numbered_by_figure()
        {
            return text(self.item(k).and_then(|i| i.label.clone()));
        }
        // A 2×2's axes, where its items say where each stands on them.
        if self.layout == "matrix"
            && let Some((y, x, _)) = self.axes()
        {
            let axis = |more: bool, of: &str| Some(Fill::Text(format!("{} {of}", if more { "More" } else { "Less" })));
            match name {
                "y-high" => return axis(true, &y),
                "y-low" => return axis(false, &y),
                "x-high" => return axis(true, &x),
                "x-low" => return axis(false, &x),
                _ => {}
            }
        }
        if keeps_prompt(name) {
            // The theme's prompts are English: a deck in another language says them in its own,
            // a column's head, an axis's end, a month (dogfood 2), else keeps the theme's word.
            let said = (self.lang != "en")
                .then(|| slot.prompt.as_ref().and_then(Prompt::text).and_then(|(w, _)| label(self.lang, &w)))
                .flatten();
            let fill = || said.map_or(Fill::Prompt, |w| Fill::Label(w.to_string()));
            return match ordinal(name) {
                Some((k, _)) => self.item(k).map(|_| fill()),
                None => Some(fill()),
            };
        }
        if picture {
            // The slide's one picture, in its layout's first place for one.
            let first = self.template().and_then(|l| {
                l.slots.iter().find(|(_, s)| matches!(s.example, Some(Example::Picture(_)))).map(|(n, _)| n.as_str())
            });
            return (self.picture.is_some() && first == Some(name)).then_some(Fill::Picture);
        }
        if matches!(slot.example, Some(Example::Chart(_)) | Some(Example::Table(_))) {
            return self.slide.chart_data.as_ref().map(|_| Fill::Chart);
        }
        let layout = self.layout;
        // Layouts whose places read their items in their own ways.
        if let Some(fill) = self.special(layout, name, list_kind) {
            return fill;
        }
        if let Some((k, part)) = ordinal(name) {
            let item = self.item(k)?;
            if part.is_empty()
                && let Some(kind) = list_kind
            {
                // What the item's figure and name places leave of it, as a text place takes it.
                let role = slot.role.as_deref().unwrap_or_default();
                let rest = self.part(k, item, "", role).unwrap_or_else(|| item.detail.clone());
                let points: Vec<String> = listed(&rest).iter().map(|p| sentence(p)).collect();
                // One point is words, not a list of one.
                return Some(match <[String; 1]>::try_from(points) {
                    Ok([one]) => Fill::Text(one),
                    Err(points) => Fill::List(points, kind),
                });
            }
            let role = slot.role.as_deref().unwrap_or_default();
            return text(self.part(k, item, part, role));
        }
        text(match name {
            // A slide's foot names the part of the talk it is in, as a running head does.
            "kicker" if self.foot(slot) => self.running.map(str::to_string),
            "header" | "title" | "statement" | "claim" | "ask" | "question" | "term" | "headline" | "point"
            | "name" | "thanks" | "top" => self.title(),
            "subtitle" | "support" | "dek" | "follow" | "talk" | "lede" | "role" => {
                self.subtitle().or_else(|| self.item(0).map(|i| i.whole.clone()))
            }
            "hub" => self.subtitle().or_else(|| self.title()),
            "number" if matches!(layout, "section" | "section-photo") => self.section.map(|n| format!("{n:02}")),
            // A figure with its unit is set whole: "11 days", not "11".
            "number" => self.item(0).map(|i| match numeric(&i.whole) {
                true => i.whole.clone(),
                false => i.figure().unwrap_or_else(|| i.whole.clone()),
            }),
            // A quote in its marks, so that it never reads as the presenter's own words.
            "quote" => self.item(0).map(|i| quoted(&i.whole, self.lang)).or_else(|| self.title()),
            "who" => self.item(1).map(|i| attributed(&i.whole)).or_else(|| self.subtitle()),
            "meaning" => self.item(0).map(|i| i.whole.clone()).or_else(|| self.subtitle()),
            "example" => self.item(1).map(|i| i.whole.clone()),
            "takeaway" => self.item(0).map(|i| i.whole.clone()).or_else(|| self.subtitle()),
            "callout" => {
                self.fact().as_deref().and_then(figure).or_else(|| self.item(0).and_then(|i| figure(&i.whole)))
            }
            "under" | "detail" => self.fact().or_else(|| self.item(1).map(|i| i.whole.clone())),
            // A chart's source, in a place of its own beside its note (dogfood pass 4).
            "source" => self.caption(),
            // A note beside a place for the source: the slide's fact where the source took none of it.
            "note" if self.has("source") && self.slide.chart_data.is_some() => {
                let sourced = (self.slide.chart_data.as_ref().and_then(|c| c.source_id.as_deref()))
                    .is_some_and(|s| !s.trim().is_empty());
                if sourced { self.fact() } else { None }
            }
            "note" if layout.ends_with("chart") && self.slide.chart_data.is_some() => self.caption(),
            "note" => {
                let source = self.slide.chart_data.as_ref().and_then(|c| c.source_id.clone()).map(|s| self.sourced(&s));
                // What the headline leaves to say, where the layout has no place of its own for it
                // and the slide is no chart's, whose value axis says it: a source or a fact stays
                // first, never stacked with it.
                let placed = ["subtitle", "support", "dek", "follow", "talk", "lede"].iter().any(|n| self.has(n));
                let chart = self.slide.chart_data.is_some();
                let fallback = if layout == "full" || (!placed && !chart) { self.subtitle() } else { None };
                source.or_else(|| self.fact()).or(fallback)
            }
            "story" | "details" => (!self.items.is_empty()).then(|| self.wholes().join("\n")),
            "body" | "needs" | "method" => {
                if self.items.is_empty() {
                    self.subtitle()
                } else {
                    return Some(Fill::List(self.wholes(), list_kind.unwrap_or("bullet")));
                }
            }
            _ => None,
        })
    }

    /// What `part` of the `k`th item shows, in a slot of `role`: where the layout sets its figure
    /// apart, the figure; where it sets its name apart, the name; and in the slot of its own, what
    /// the others leave.
    fn part(&self, k: usize, item: &Item, part: &str, role: &str) -> Option<String> {
        let ord = ORDINALS[k];
        let sibling = |p: &str| if p.is_empty() { self.has(ord) } else { self.has(&format!("{ord}-{p}")) };
        let title_role = self.role_of(&format!("{ord}-title")).unwrap_or_default();
        // Where the layout shows the item's figure apart, and where its name.
        let figure_apart = sibling("figure")
            || sibling("target")
            || sibling("price")
            || (sibling("title") && figure_role(title_role))
            || (sibling("number") && self.numbered_by_figure());
        let name_apart = (sibling("title") && !figure_role(title_role)) || sibling("date") || sibling("item");
        // A date sets a timeline's figure; elsewhere a figure's place takes the figure.
        let dated = matches!(self.layout, "timeline" | "milestones");
        match part {
            "title" if figure_role(role) && dated => item.label.clone().or_else(|| figure(&item.whole)),
            "title" if figure_role(role) => item.figure().or_else(|| item.label.clone()),
            "title" if item.figure_first() && !sibling("") => Some(sentence(&item.detail)),
            "title" | "item" | "task" if item.figure_first() && figure_apart => item.name().map(|n| sentence(&n)),
            // Its figure shown nowhere else: the figure titles it, its words under it.
            "title" | "item" | "task" if item.figure_first() => item.label.clone(),
            "title" | "item" | "task" => item.name(),
            "date" => item.label.clone(),
            "figure" | "target" | "price" | "amount" => {
                item.figure().or_else(|| item.label.clone().filter(|_| part != "figure"))
            }
            "note" => said(item).1,
            "" => Some(match (&item.label, figure_apart, name_apart) {
                // "$4.1M: Q3 revenue, up 12%" with its figure and its name apart: "Up 12%".
                (Some(_), true, true) if item.figure_first() => sentence(&change(&item.detail).1),
                (Some(_), true, _) if item.figure_first() => sentence(&item.detail),
                // "Leads: 1,200, 28% go on" with its figure apart: "28% go on".
                // "Leads: 2,400 a month from inbound" with its figure apart: "A month from inbound".
                (Some(_), true, true) => {
                    sentence(&after_figure(&item.detail).unwrap_or_else(|| without_figure(&item.detail)))
                }
                (Some(_), _, true) => sentence(&item.detail),
                // Its figure apart, its name here: "Houston: 41% of volume" → "Houston";
                // "Revenue: $4.1M, up 12%" → "Revenue, up 12%".
                // A timeline's moment under its date: "Jan 2025: Permit denied" → "Permit denied".
                (Some(_), true, false) if dated => sentence(&item.detail),
                (Some(label), true, false) => match after_figure(&item.detail) {
                    Some(rest) if !rest.is_empty() => format!("{label}, {rest}"),
                    _ => label.clone(),
                },
                _ if figure_role(role) => item.figure().unwrap_or_else(|| item.whole.clone()),
                _ if matches!(role, "title" | "headline" | "display") => self.short(item),
                _ => item.whole.clone(),
            })
            .filter(|t| !t.is_empty()),
            _ => None,
        }
    }

    /// The slots of layouts that read their items in their own ways; none where the general
    /// rules serve.
    fn special(&self, layout: &str, name: &str, list_kind: Option<&'static str>) -> Option<Option<Fill>> {
        let text = |t: Option<String>| Some(t.map(Fill::Text));
        let ord = ordinal(name);
        match (layout, name, ord) {
            // What the number means, beside it; else the slide's title. Under it, its source, or
            // what a figure-led first item leaves over: the second.
            // Two figures: each bar's value in its unit, what it counts under it, and under them
            // what was counted, or the source.
            ("two-figures", _, Some((k, part))) => {
                let chart = self.slide.chart_data.as_ref()?;
                let value = chart.series.first().and_then(|s| s.values.get(k).copied());
                match part {
                    "figure" => text(value.map(|v| at_places(v, decimals(chart), chart.unit.as_deref()))),
                    "" => text(chart.categories.get(k).cloned()),
                    _ => Some(None),
                }
            }
            ("two-figures", "note", _) => text(self.subtitle().or_else(|| {
                (self.slide.chart_data.as_ref()).and_then(|c| c.source_id.clone()).map(|s| self.sourced(&s))
            })),
            ("big-number" | "number-photo", "claim", _) => text(meaning(self.items).or_else(|| self.title())),
            ("big-number" | "number-photo", "under", _) => text(self.fact().or_else(|| {
                self.item(0)
                    .filter(|i| i.figure_first() && !i.detail.is_empty())
                    .and(self.item(1))
                    .map(|i| i.whole.clone())
            })),
            // A line's latest figure, or the one its takeaway leads with; under it, the takeaway
            // and the source.
            ("line-chart", "callout", _) => text(
                (self.item(0).filter(|i| i.figure_first()).and_then(|i| i.label.clone()))
                    .or_else(|| self.slide.chart_data.as_ref().and_then(latest)),
            ),
            // The takeaway; then, where the layout has no place for it, the chart's source a line
            // of its own. A takeaway that names the headline's first and last figures again says
            // nothing the room has not just read, and gives way to what the latest figure counts
            // and when, from the chart's own data: "without power in 2026".
            ("line-chart", "note", _) => {
                let item = self.item(0);
                let said = item.map(|i| if i.figure_first() { sentence(&i.detail) } else { i.whole.clone() });
                let takeaway = match said {
                    Some(t) if self.title().is_some_and(|h| restates(&h, &t)) => {
                        // The latest figure is the callout's only where no takeaway leads with its own.
                        let latest = !item.is_some_and(Item::figure_first);
                        self.slide.chart_data.as_ref().filter(|_| latest).and_then(|c| counted(c, self.subtitle()))
                    }
                    said => said,
                };
                let source = self.caption().filter(|_| !self.has("source"));
                text(match (takeaway.filter(|t| !t.is_empty()), source) {
                    (Some(t), Some(c)) => Some(format!("{t}\n{c}")),
                    (t, c) => t.or(c),
                })
            }
            // The objective is the header; the slide's title stands over it.
            ("okrs", "header", _) => text(self.subtitle().or_else(|| self.title())),
            ("okrs", "kicker", _) => text(self.subtitle().and(self.title())),
            ("qa", "support", _) => {
                text(self.subtitle().or_else(|| (!self.items.is_empty()).then(|| self.wholes().join("\n"))))
            }
            // "Option A: Option B" or "A versus B", then "criterion: A's answer | B's answer".
            ("head-to-head", "first-title" | "second-title", _) => {
                let item = self.item(0)?;
                let sides = match &item.label {
                    Some(label) => vec![label.clone(), item.detail.clone()],
                    None => versus(&item.whole),
                };
                text(sides.get(usize::from(name == "second-title")).cloned().filter(|_| sides.len() == 2))
            }
            ("head-to-head", _, Some((k, part))) => {
                let (criterion, left, right) = row(self.item(k + 1)?);
                match part {
                    "criterion" => text(criterion),
                    "left" => text(left),
                    "right" => text(right),
                    _ => None,
                }
            }
            // "Strengths: a; b" — each a quadrant's list; a roadmap's phase likewise.
            ("swot" | "roadmap", _, Some((k, ""))) => {
                let item = self.item(k)?;
                let points = listed(&item.detail).iter().map(|p| sentence(p)).collect();
                Some(Some(Fill::List(points, list_kind.unwrap_or("bullet"))))
            }
            // "Pro: …" on one side, "Con: …" on the other.
            ("pros-cons", "first-title", _) => text(Some("Pros".into())),
            ("pros-cons", "second-title", _) => text(Some("Cons".into())),
            ("pros-cons", "first" | "second", _) => {
                let (pros, cons) = self.sides(&["pro", "for", "upside", "benefit", "gain"]);
                let side = if name == "first" { pros } else { cons };
                Some((!side.is_empty()).then(|| Fill::List(side, list_kind.unwrap_or("bullet"))))
            }
            // "action — owner — when".
            ("next-steps", _, Some((k, part))) => {
                let item = self.item(k)?;
                let p = parts(&item.whole);
                text(match part {
                    "" => p.first().cloned(),
                    "owner" => p.get(1).cloned(),
                    "when" => p.get(2).cloned(),
                    _ => None,
                })
            }
            // "words — who".
            ("testimonials" | "two-quotes", _, Some((k, part))) => {
                let item = self.item(k)?;
                let (words, who) = said(item);
                text(match part {
                    "" => Some(quoted(&words, self.lang)),
                    "note" => who.map(|w| attributed(&w)),
                    _ => None,
                })
            }
            // "Name, role: credit".
            ("team" | "team-four" | "team-six" | "team-names", _, Some((k, part)))
                if part != "art" && part != "card" =>
            {
                let item = self.item(k)?;
                let label = item.label.clone().unwrap_or_else(|| item.whole.clone());
                let (person, role) =
                    label.split_once(", ").map_or((label.clone(), None), |(n, r)| (n.to_string(), Some(r.to_string())));
                text(match part {
                    "title" => Some(person),
                    "" => {
                        let detail = item.label.as_ref().map(|_| item.detail.clone()).filter(|d| !d.is_empty());
                        match (role, detail) {
                            (Some(r), Some(d)) => Some(format!("{r} \u{b7} {}", sentence(&d))),
                            (r, d) => r.or(d),
                        }
                    }
                    _ => None,
                })
            }
            // "task: lead / team / sponsor".
            ("raci", _, Some((k, part))) if part != "role" && part != "card" => {
                let item = self.item(k)?;
                let owners: Vec<String> =
                    item.detail.split(['/', ',']).map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
                text(match part {
                    "task" => Some(item.label.clone().unwrap_or_else(|| item.whole.clone())),
                    "lead" => owners.first().cloned(),
                    "team" => owners.get(1).cloned(),
                    "sponsor" => owners.get(2).cloned(),
                    _ => None,
                })
            }
            // "item: amount", then "TOTAL: amount".
            ("receipt", "title", _) => text(self.title()),
            ("receipt", "total", _) => text(
                self.items
                    .iter()
                    .find(|i| i.label.as_deref().is_some_and(|l| l.eq_ignore_ascii_case("total")))
                    .map(|i| i.detail.clone()),
            ),
            ("receipt", _, Some((k, part))) => {
                let lines: Vec<&Item> = (self.items.iter())
                    .filter(|i| !i.label.as_deref().is_some_and(|l| l.eq_ignore_ascii_case("total")))
                    .collect();
                let item = lines.get(k)?;
                text(match part {
                    "item" => item.label.clone().or_else(|| Some(item.whole.clone())),
                    "amount" => item.label.is_some().then(|| item.detail.clone()),
                    _ => None,
                })
            }
            // "phase: does / thinks / feels".
            ("journey", _, _) if name.ends_with("-phase") => {
                let k = ORDINALS.iter().position(|o| name.starts_with(o))?;
                text(self.item(k).map(|i| i.label.clone().unwrap_or_else(|| i.whole.clone())))
            }
            ("journey", _, _) if ["does-", "thinks-", "feels-"].iter().any(|p| name.starts_with(p)) => {
                let (lane, ord) = name.split_once('-')?;
                let k = ORDINALS.iter().position(|o| *o == ord)?;
                let item = self.item(k)?;
                let p: Vec<String> = item.detail.split(" / ").map(|s| s.trim().to_string()).collect();
                let at = ["does", "thinks", "feels"].iter().position(|l| *l == lane)?;
                text(p.get(at).filter(|s| !s.is_empty()).map(|s| sentence(s)))
            }
            // "key result: target", the objective the header.
            ("okrs", _, Some((k, part))) => {
                let item = self.item(k)?;
                text(match part {
                    "" => item.label.clone().or_else(|| Some(item.whole.clone())),
                    "target" => item.label.is_some().then(|| item.detail.clone()),
                    _ => None,
                })
            }
            // "status: item".
            ("checklist", _, Some((k, part))) => {
                let item = self.item(k)?;
                let status = item.label.as_deref().filter(|l| l.split_whitespace().count() <= 2);
                text(match part {
                    "state" => status.map(str::to_string),
                    "" => Some(if status.is_some() { sentence(&item.detail) } else { item.whole.clone() }),
                    _ => None,
                })
            }
            // Each a quadrant's title, placed on the axes: where its words say, by them.
            ("matrix", _, Some((k, ""))) => match self.axes() {
                Some((_, _, at)) => text(self.item(at[k]).map(|i| i.label.clone().unwrap_or_else(|| i.whole.clone()))),
                None => text(self.item(k).map(|i| self.short(i))),
            },
            // A picture's caption under it: the state, and what it is.
            ("before-after", _, Some((k, ""))) => text(self.item(k).map(|i| i.whole.clone())),
            // What you need, then the method's steps.
            ("recipe", "needs" | "method", _) => {
                let needed = |i: &Item| {
                    i.label.as_deref().is_some_and(|l| {
                        let l = l.to_lowercase();
                        ["need", "ingredient", "you need"].iter().any(|n| l.starts_with(n))
                    })
                };
                let (needs, method): (Vec<String>, Vec<String>) = if self.items.iter().any(needed) {
                    (
                        self.items.iter().filter(|i| needed(i)).map(|i| sentence(&i.detail)).collect(),
                        self.items.iter().filter(|i| !needed(i)).map(|i| sentence(&i.detail)).collect(),
                    )
                } else {
                    let half = self.items.len().div_ceil(2);
                    (self.wholes()[..half].to_vec(), self.wholes()[half..].to_vec())
                };
                let side = if name == "needs" { needs } else { method };
                let kind = if name == "needs" { "bullet" } else { "number" };
                Some((!side.is_empty()).then(|| Fill::List(side, list_kind.unwrap_or(kind))))
            }
            // A memo's and a letter's fields: whom it is to, what it is about, and its body; its
            // date is left out, which no model knows.
            ("memo", "about", _) => text(self.title()),
            ("memo" | "letter", "to", _) => text(self.subtitle()),
            ("memo" | "letter", "date", _) => Some(None),
            ("memo" | "letter", "from" | "sign", _) => Some(Some(Fill::Prompt)),
            ("memo" | "letter", "body", _) => text((!self.items.is_empty()).then(|| self.wholes().join("\n\n"))),
            ("headline", "tag" | "byline", _) => Some(None),
            ("the-ask", "ask", _) => text(self.title()),
            _ => None,
        }
    }

    /// A 2×2's axes and which item stands in each quadrant, where each of its four items says
    /// where it stands on the same two scales: "Port software: high impact, low cost". The first
    /// scale named runs up the slide, the second across; the quadrants run top left, top right,
    /// bottom left, bottom right, as the layout sets them.
    fn axes(&self) -> Option<(String, String, [usize; 4])> {
        if self.items.len() != 4 {
            return None;
        }
        // "high impact" → ("impact", true).
        let level = |clause: &str| {
            let (word, of) = clause.trim().split_once(' ')?;
            let more = match word.to_lowercase().as_str() {
                "high" | "more" => true,
                "low" | "less" => false,
                _ => return None,
            };
            Some((of.trim().trim_end_matches('.').to_lowercase(), more))
        };
        let read = |i: &Item| -> Option<[(String, bool); 2]> {
            let (a, b) = i.detail.split_once(", ").or_else(|| i.detail.split_once(" and "))?;
            Some([level(a)?, level(b)?])
        };
        let first = read(&self.items[0])?;
        let (y, x) = (first[0].0.clone(), first[1].0.clone());
        if y == x {
            return None;
        }
        let mut at = [usize::MAX; 4];
        for (k, item) in self.items.iter().enumerate() {
            let said = read(item)?;
            let on = |scale: &str| said.iter().find(|(of, _)| of == scale).map(|(_, more)| *more);
            let quadrant = usize::from(!on(&y)?) * 2 + usize::from(on(&x)?);
            if at[quadrant] != usize::MAX {
                return None;
            }
            at[quadrant] = k;
        }
        Some((y, x, at))
    }

    /// The items for and against: those whose label is one of `pro`'s words, and the rest; else
    /// the first half and the second.
    fn sides(&self, pro: &[&str]) -> (Vec<String>, Vec<String>) {
        let is_pro = |i: &Item| i.label.as_deref().is_some_and(|l| pro.iter().any(|p| l.to_lowercase().starts_with(p)));
        let labelled = self.items.iter().all(|i| i.label.is_some());
        if labelled && self.items.iter().any(is_pro) {
            let (a, b): (Vec<&Item>, Vec<&Item>) = self.items.iter().partition(|i| is_pro(i));
            // A side's points written on one line, "a; b; c", are each a point.
            let details =
                |side: Vec<&Item>| side.iter().flat_map(|i| listed(&i.detail)).map(|p| sentence(&p)).collect();
            return (details(a), details(b));
        }
        let half = self.items.len().div_ceil(2);
        (self.wholes()[..half].to_vec(), self.wholes()[half..].to_vec())
    }

    /// "Source: `source`", in the deck's language.
    fn sourced(&self, source: &str) -> String {
        format!("{}{source}", own(self.lang).source)
    }

    /// What a text in `slot` is to the slide's story: the slide's claim where it says its title,
    /// or a big number's meaning, a quote, a letter's body; a source; a signpost.
    fn semantic(&self, slot: &str, text: &str) -> Option<&'static str> {
        let claim = match (self.layout, slot) {
            ("big-number" | "number-photo", "claim") | ("quote", "quote") | ("letter", "body") => true,
            _ => self.title().as_deref() == Some(text),
        };
        if claim {
            return Some("claim");
        }
        match slot {
            "takeaway" => Some("takeaway"),
            "under" | "note" if self.fact().as_deref() == Some(text) || text.starts_with(own(self.lang).source) => {
                Some("source")
            }
            "who" | "source" => Some("source"),
            "kicker" => Some("navigation"),
            "number" if matches!(self.layout, "section" | "section-photo") => Some("navigation"),
            _ => None,
        }
    }
}

/// Which ids a new one must not take.
#[derive(Clone, Copy)]
enum Taken {
    Node,
    State,
    Data,
    Section,
    Beat,
}

/// The deck as it is made.
#[derive(Default)]
struct Builder {
    nodes: Map<String, Value>,
    states: Vec<Value>,
    data: Map<String, Value>,
    sections: Vec<String>,
    beats: Vec<String>,
    /// The deck reads right to left (PLAN T.5a): a theme's shapes stand mirrored in their boxes.
    mirrored: bool,
}

impl Builder {
    /// The first of `base`, `base-2`, … that no id of its kind takes; taken.
    fn unique(&mut self, base: &str, kind: Taken) -> String {
        let taken = |b: &Builder, id: &str| match kind {
            Taken::Node => b.nodes.contains_key(id),
            Taken::State => b.states.iter().any(|s| s["id"] == id),
            Taken::Data => b.data.contains_key(id),
            Taken::Section => b.sections.iter().any(|s| s == id),
            Taken::Beat => b.beats.iter().any(|s| s == id),
        };
        let id = (1..)
            .map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") })
            .find(|id| !taken(self, id))
            .expect("some number is free");
        match kind {
            Taken::Section => self.sections.push(id.clone()),
            Taken::Beat => self.beats.push(id.clone()),
            _ => {}
        }
        id
    }

    /// A state's id: the title's first words, else its layout.
    fn state_id(&mut self, title: &str, layout: &str) -> String {
        let words: Vec<&str> = title.split_whitespace().take(5).collect();
        let base = if words.is_empty() { slug(layout, "slide") } else { slug(&words.join(" "), "slide") };
        self.unique(&base, Taken::State)
    }

    /// The slide `s` describes: its nodes, and its state showing them, in by `entrance` where
    /// another slide went before (narrative ATTR-1).
    fn slide(&mut self, s: Slots, entrance: Option<Entrance>) -> Result<SlideMade, OpsError> {
        let template = s
            .theme
            .layouts
            .get(s.layout)
            .ok_or_else(|| OpsError::new(format!("the theme has no layout `{}`", s.layout)))?;
        let mut props = Map::new();
        let mut made = SlideMade { data: None, steps: Vec::new() };
        // A list that builds by bullet (ATTR-2): its node, and how many paragraphs it has.
        let mut bulleted: Option<(String, usize)> = None;
        let mut claimed = false;
        // What builds once the transition ends (G.17): each item's nodes, by its place's ordinal
        // (`first-…`, `second-…`), and each chart.
        let mut built: Vec<(usize, Vec<String>)> = Vec::new();
        let mut charts: Vec<String> = Vec::new();
        for (name, slot) in read_order(&template.slots, |name, slot| name == "kicker" && s.foot(slot)) {
            let Some(fill) = s.fill(name, slot) else { continue };
            let id = self.unique(&format!("{}-{}", s.state, slug(name, "slot")), Taken::Node);
            let at = json!({ "in": name });
            let mut node = match fill {
                Fill::Text(text) => {
                    let Some(role) = &slot.role else { continue };
                    // Two sides each said on a card read as statements, in the words' own color
                    // rather than the muted lede's.
                    let role = match (s.layout, ordinal(name)) {
                        ("myth-fact", Some((_, ""))) if s.theme.typography.roles.contains_key("title") => "title",
                        // Two figures grow together, to one size, toward the hero's.
                        ("two-figures", Some((_, "figure"))) if s.theme.typography.roles.contains_key("figure") => {
                            "figure"
                        }
                        _ => role.as_str(),
                    };
                    let mut node = json!({ "type": "text", "role": role, "text": text, "at": at });
                    // Set as large as its place allows, up to its ceiling (`making` holds a set
                    // of them to one size).
                    let most = s
                        .grown(name, slot.role.as_deref().unwrap_or(role))
                        .or_else(|| short(&text).then(|| s.alone(name, role)).flatten())
                        .map(|most| {
                            // A few short lines of words alone in a large place, a card's own words
                            // or a term's meaning, grow toward three-quarters of the headline, never
                            // past it, as a short list does (dogfood pass 4).
                            let own = (matches!(ordinal(name), Some((_, ""))) && !s.figured(name))
                                || s.alone(name, role).is_some();
                            let most = match own && words_role(role) && short(&text) {
                                true => s.under_headline().map_or(most, |h| h.max(most)),
                                false => most,
                            };
                            // Under a card's title, its words grow no larger than the title is set.
                            match s.titled(name).filter(|_| words_role(role)) {
                                Some(title) => most.min(title),
                                None => most,
                            }
                        })
                        .filter(|most| s.size(role).is_none_or(|own| *most > own));
                    if let Some(most) = most {
                        node["fit"] = json!("grow");
                        node["maxSize"] = json!(most);
                        // A figure grows on its one line; a cover's or a divider's title on as
                        // few as its words want, one for a few and two for more, so that growing
                        // never sets one of its words on a line of its own.
                        if figure_role(role) {
                            node["maxLines"] = json!(1);
                        } else if name == "title" {
                            node["maxLines"] = json!(if text.split_whitespace().count() <= 3 { 1 } else { 2 });
                        } else if name == "statement" {
                            // A statement grows on three lines at most, as a headline stands, so
                            // it keeps room around it.
                            node["maxLines"] = json!(3);
                        }
                    }
                    // A side's statement stands in the middle of its card, on as many as three lines
                    // as it grows to fill the card (G.20); the running head stands on the slide's
                    // foot, as a footer does.
                    match (s.layout, ordinal(name)) {
                        // A statement with nothing under it takes the rows its support leaves, and
                        // stands in the middle of them.
                        ("statement", None) if name == "statement" && s.subtitle().is_none() && s.items.is_empty() => {
                            node["align"] = json!({ "y": "center" });
                            let support = template.slots.get("support").and_then(|s| s.row.as_ref());
                            if let (Some(col), Some(Range::Span([top, _])), Some(Range::Span([_, foot]))) =
                                (slot.col.as_ref(), slot.row.as_ref(), support)
                            {
                                node["at"] = json!({ "col": col, "row": [top, foot] });
                            }
                        }
                        ("myth-fact", Some((_, ""))) => {
                            node["align"] = json!({ "y": "center" });
                            node["maxLines"] = json!(3);
                        }
                        // Of two figures, the one the slide is not about stands back.
                        ("two-figures", Some((k, "figure")))
                            if s.slide
                                .chart_data
                                .as_ref()
                                .and_then(|c| signal(c, s.item(0).map(|i| i.whole.as_str())))
                                != Some(k) =>
                        {
                            node["style"] = json!({ "color": "onSurfaceMuted" })
                        }
                        _ if name == "kicker" && s.foot(slot) => node["align"] = json!({ "y": "end" }),
                        _ => {}
                    }
                    match s.semantic(name, &text) {
                        Some("claim") if claimed => {}
                        Some(semantic) => {
                            claimed |= semantic == "claim";
                            node["semantic"] = json!(semantic);
                        }
                        None => {}
                    }
                    node
                }
                Fill::List(entries, kind) => {
                    let Some(role) = &slot.role else { continue };
                    if entries.is_empty() {
                        continue;
                    }
                    let list = vec![json!({ "kind": kind }); entries.len()];
                    let mut node =
                        json!({ "type": "text", "role": role, "text": entries.join("\n"), "list": list, "at": at });
                    // A slide's one list, where it is short, set larger to fill its place: up to the
                    // size of the theme's lede, under its titles still, and where each of its
                    // lines is short, up to three-quarters of the headline, as a card's statement
                    // grows (dogfood pass 4). An agenda's short lines up to the size of its titles.
                    let size = |r: &str| s.theme.typography.roles.get(r).map(|r| r.size);
                    let short = entries.len() <= 5 && entries.iter().all(|e| e.chars().count() <= 60);
                    let most = match () {
                        _ if s.layout.starts_with("agenda") => size("title"),
                        _ if short => size("headline").map(|h| (h * 0.75).max(size("lede").unwrap_or(0.0))),
                        _ => size("lede"),
                    };
                    if name == "body"
                        && let Some(most) = most
                    {
                        node["fit"] = json!("grow");
                        node["maxSize"] = json!(most);
                    }
                    // A list in a place of a set, each side's or each phase's, grows as the set's
                    // words do, held to one size with the rest of its set (G.12).
                    // A short one in a body's place grows as the slide's one short list does.
                    if ordinal(name).is_some()
                        && let Some(most) = s.grown(name, role)
                    {
                        let most = match short && role == "body" {
                            true => size("headline").map_or(most, |h| (h * 0.75).max(most)),
                            false => most,
                        };
                        node["fit"] = json!("grow");
                        node["maxSize"] = json!(most);
                    }
                    // A slide whose one item is a short list in the bullets layout builds it by
                    // bullet, as a speaker paces an argument (ATTR-2): each paragraph enters by
                    // the theme's By Bullet on a click of its own.
                    if name == "body"
                        && s.layout.starts_with("bullets")
                        && (3..=6).contains(&entries.len())
                        && let Some(preset) = by_bullet_preset(s.theme)
                    {
                        node["enter"] = json!(preset);
                        bulleted = Some((id.clone(), entries.len()));
                    }
                    node
                }
                Fill::Prompt | Fill::Label(_) => {
                    let said = match &fill {
                        Fill::Label(words) => Some(words.clone()),
                        _ => slot.prompt.as_ref().and_then(Prompt::text).map(|(words, _)| words),
                    };
                    let (Some(role), Some(words)) = (&slot.role, said) else { continue };
                    let mut node =
                        json!({ "type": "text", "role": role, "text": words, "at": at, "semantic": "navigation" });
                    // A journey's lanes grow with its phases and cells; other labels kept keep their size.
                    if s.layout == "journey"
                        && let Some(most) = s.grown(name, role)
                    {
                        node["fit"] = json!("grow");
                        node["maxSize"] = json!(most);
                    }
                    node
                }
                Fill::Shape => {
                    let Some(Prompt::Shape(shape)) = &slot.prompt else { continue };
                    let mut node =
                        json!({ "type": "shape", "kind": shape.shape, "at": at, "z": -1, "semantic": "decoration" });
                    if let Some(fill) = &shape.fill {
                        node["fill"] = json!(fill);
                    }
                    if let Some(stroke) = &shape.stroke {
                        node["stroke"] = json!({ "paint": stroke, "width": "thin" });
                    }
                    if let Some(points) = &shape.points {
                        node["points"] =
                            if self.mirrored { json!(scaena_core::direction::flipped(points)) } else { json!(points) };
                    }
                    node
                }
                Fill::Icon(icon) => {
                    let Some(Prompt::Shape(shape)) = &slot.prompt else { continue };
                    let fill = shape.fill.as_deref().unwrap_or("accent");
                    let mut node = scaena_core::inserts::icon_node(&icon.name, &icon.path, icon.view, fill);
                    node["at"] = at;
                    node
                }
                Fill::Picture => {
                    let Some(p) = s.picture else { continue };
                    // Under the cards (`z` −1) and the words over it.
                    json!({ "type": "image", "src": p.src, "alt": p.alt, "at": at, "z": -2 })
                }
                Fill::Chart => {
                    let Some(chart) = &s.slide.chart_data else { continue };
                    let data = self.unique(&format!("{}-data", s.state), Taken::Data);
                    self.data.insert(data.clone(), source(chart, s.layout == "line-chart" && years(chart)));
                    made.data = Some(data.clone());
                    let takeaway = s.item(0).map(|i| i.whole.as_str());
                    let subtitle = Some(s.slide.subtitle.trim()).filter(|t| !t.is_empty());
                    chart_node(
                        chart,
                        &data,
                        s.layout,
                        [&s.slide.slide_title, takeaway.unwrap_or_default()],
                        subtitle,
                        s.lang,
                        at,
                    )
                }
            };
            // An email or a web address the words name links to it (PLAN 2.70, dogfood 2); a bare
            // domain only in words that do not head the slide, where "Booking.com" names a firm.
            let heading = matches!(node["role"].as_str(), Some("display" | "headline" | "title"));
            if let Some(runs) = node["text"].as_str().and_then(|t| linked(t, !heading)) {
                node.as_object_mut().map(|n| n.remove("text"));
                node["runs"] = json!(runs);
            }
            // A line's source stands under its takeaway in the theme's caption role, smaller, so it
            // never shrinks the takeaway with it to fit (dogfood: charts).
            if s.layout == "line-chart"
                && name == "note"
                && !s.has("source")
                && s.caption().is_some()
                && s.theme.typography.roles.contains_key("caption")
                && let Some((takeaway, source)) = node["text"].as_str().and_then(|t| t.rsplit_once('\n'))
            {
                let runs = json!([{ "text": takeaway }, { "text": format!("\n{source}"), "role": "caption" }]);
                node.as_object_mut().map(|n| n.remove("text"));
                node["runs"] = runs;
            }
            if node.get("semantic").is_none() && node["type"] == "text" && !claimed && s.title().is_none() {
                // A slide with no title says what it is in its first words.
                node["semantic"] = json!("claim");
                claimed = true;
            }
            if let Some((k, _)) = ordinal(name) {
                match built.iter_mut().find(|(i, _)| *i == k) {
                    Some((_, ids)) => ids.push(id.clone()),
                    None => built.push((k, vec![id.clone()])),
                }
            }
            if node["type"] == "chart" {
                charts.push(id.clone());
            }
            self.nodes.insert(id.clone(), node);
            props.insert(id, json!({}));
        }
        // The slide shows its list's first paragraph; each step one more (ATTR-2), as the
        // editor's By Bullet makes them (`scaena_core::patch` `by_bullet`, PLAN 2.129).
        if let Some((list, n)) = &bulleted {
            props.insert(list.clone(), json!({ "paragraphs": [1, 1] }));
            for k in 2..=*n {
                let id = self.unique(&format!("{}-{k}", s.state), Taken::State);
                made.steps.push(id);
            }
        }
        let mut state = json!({ "id": s.state, "mode": "absolute", "layout": s.layout, "props": props });
        let durations = &s.theme.motion.durations;
        let mut transition = durations.get("standard").map_or(420.0, |d| d.0);
        if let Some(e) = entrance {
            // The theme's `slow` where it names one, else its `standard`.
            let pace = if e.slow && durations.contains_key("slow") { "slow" } else { "standard" };
            transition = durations.get(pace).map_or(transition, |d| d.0);
            let mut t = json!({ "duration": pace, "ease": "standard" });
            if let Some(style) = e.style {
                t["style"] = json!(style);
            }
            if e.style == Some("push") {
                // In from the right, as a page turns; from the left where the deck reads right to left.
                t["direction"] = json!(if self.mirrored { "right" } else { "left" });
            }
            state["transition"] = t;
        }
        if let Some(choreography) = builds(s.theme, built, &charts, transition) {
            state["choreography"] = choreography;
        }
        self.states.push(state);
        if let Some((list, _)) = &bulleted {
            for (k, id) in made.steps.iter().enumerate() {
                let props = json!({ list.clone(): { "paragraphs": [1, k + 2] } });
                self.states.push(json!({ "id": id, "slide": s.state, "props": props }));
            }
        }
        Ok(made)
    }
}

/// `text` as runs where it names an email or a web address, each of them a run that links to it
/// (PLAN 2.70): "priya@manifest.dev · manifest.dev" mails the one and opens the other (dogfood 2).
/// A bare domain links only where `bare` says, and only written in lower case, as an address is:
/// "ASP.NET" and "Booking.com" name things. None where it names neither. A linked text is runs,
/// which [`overflowing`] does not read, so one that runs past its place is not rewritten to fit.
fn linked(text: &str, bare: bool) -> Option<Vec<Value>> {
    let mut runs = Vec::new();
    let mut plain = String::new();
    for piece in text.split_inclusive(char::is_whitespace) {
        let word = piece.trim_end();
        // The address within its punctuation: "(manifest.dev)," is manifest.dev.
        let start = word.find(char::is_alphanumeric).unwrap_or(word.len());
        let end = (word.char_indices().rev())
            .find(|(_, c)| c.is_alphanumeric() || *c == '/')
            .map_or(start, |(i, c)| (i + c.len_utf8()).max(start));
        let Some(href) = href(&word[start..end], bare) else {
            plain.push_str(piece);
            continue;
        };
        plain.push_str(&word[..start]);
        if !plain.is_empty() {
            runs.push(json!({ "text": std::mem::take(&mut plain) }));
        }
        runs.push(json!({ "text": &word[start..end], "link": { "href": href } }));
        plain.push_str(&piece[end..]);
    }
    if runs.is_empty() {
        return None;
    }
    if !plain.is_empty() {
        runs.push(json!({ "text": plain }));
    }
    Some(runs)
}

/// Where `word` goes, as a link the deck's schema takes: `mailto:` an email, else a web address
/// as written, or with `www.` or, where `bare` says, bare and in lower case, over `https:`. None
/// for a word that is neither, such as "Node.js" or "3.5".
fn href(word: &str, bare: bool) -> Option<String> {
    // A host whose last label is a top-level domain people write: "manifest.dev", "portal.org/playbook".
    let host = |s: &str| {
        let host = s.split('/').next().unwrap_or_default();
        let labels: Vec<&str> = host.split('.').collect();
        const TOP: [&str; 34] = [
            "com", "org", "net", "edu", "gov", "io", "ai", "co", "dev", "app", "info", "biz", "xyz", "tech", "me",
            "us", "uk", "eu", "de", "fr", "es", "it", "nl", "ca", "au", "in", "jp", "cn", "kr", "br", "ch", "se", "no",
            "pl",
        ];
        labels.len() >= 2
            && labels.iter().all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            && TOP.contains(&labels[labels.len() - 1].to_ascii_lowercase().as_str())
    };
    if let Some(rest) = word.strip_prefix("https://").or_else(|| word.strip_prefix("http://")) {
        return host(rest).then(|| word.to_string());
    }
    if let Some((user, domain)) = word.split_once('@') {
        let user = !user.is_empty() && user.chars().all(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c));
        return (user && !domain.contains('/') && host(domain)).then(|| format!("mailto:{word}"));
    }
    let lower = !word.chars().any(char::is_uppercase);
    ((word.starts_with("www.") || (bare && lower)) && host(word)).then(|| format!("https://{word}"))
}

/// A label a theme's layout prompts with in English (a column's head, an axis's end, a month),
/// in the language `lang` names (BCP 47), where it is one of Narrato's: none for another word.
fn label(lang: &str, word: &str) -> Option<&'static str> {
    const LANGS: [&str; 14] = ["es", "fr", "de", "pt", "it", "nl", "cs", "pl", "uk", "ja", "zh-Hans", "ko", "ar", "hi"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let at = LANGS.iter().position(|l| *l == lang)?;
    if let Some(m) = MONTHS.iter().position(|w| *w == word) {
        return Some(match lang {
            // The locales numbers and dates are written in, where Scaena has one.
            "es" | "fr" | "de" | "nl" => scaena_core::format::Locale::of(Some(lang)).short_months[m],
            _ => SHORT_MONTHS[at][m],
        });
    }
    LABELS.iter().find(|(w, _)| *w == word).map(|(_, said)| said[at])
}

/// The months, short, in Narrato's languages by [`label`]'s order; Spanish, French, German, and
/// Dutch are the locales', so theirs are empty here.
#[rustfmt::skip]
const SHORT_MONTHS: [[&str; 12]; 14] = [
    ["", "", "", "", "", "", "", "", "", "", "", ""],
    ["", "", "", "", "", "", "", "", "", "", "", ""],
    ["", "", "", "", "", "", "", "", "", "", "", ""],
    ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"],
    ["gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic"],
    ["", "", "", "", "", "", "", "", "", "", "", ""],
    ["led", "úno", "bře", "dub", "kvě", "čvn", "čvc", "srp", "zář", "říj", "lis", "pro"],
    ["sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru"],
    ["січ", "лют", "бер", "кві", "тра", "чер", "лип", "сер", "вер", "жов", "лис", "гру"],
    ["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"],
    ["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"],
    ["1월", "2월", "3월", "4월", "5월", "6월", "7월", "8월", "9월", "10월", "11월", "12월"],
    ["يناير", "فبراير", "مارس", "أبريل", "مايو", "يونيو", "يوليو", "أغسطس", "سبتمبر", "أكتوبر", "نوفمبر", "ديسمبر"],
    ["जन॰", "फ़र॰", "मार्च", "अप्रैल", "मई", "जून", "जुल॰", "अग॰", "सित॰", "अक्तू॰", "नव॰", "दिस॰"],
];

/// The other labels the shipped layouts prompt with, in Narrato's languages by [`label`]'s
/// order: es, fr, de, pt, it, nl, cs, pl, uk, ja, zh-Hans, ko, ar, hi.
#[rustfmt::skip]
const LABELS: &[(&str, [&str; 14])] = &[
    ("About", ["Asunto", "Objet", "Betreff", "Assunto", "Oggetto", "Onderwerp", "Věc", "Temat", "Тема", "件名", "主题", "제목", "الموضوع", "विषय"]),
    ("Action", ["Acción", "Action", "Maßnahme", "Ação", "Azione", "Actie", "Úkol", "Działanie", "Дія", "アクション", "行动", "할 일", "الإجراء", "कार्य"]),
    ("By", ["Plazo", "Échéance", "Bis", "Prazo", "Entro", "Uiterlijk", "Termín", "Termin", "Термін", "期限", "截止", "기한", "الموعد", "समय-सीमा"]),
    ("Customer", ["Cliente", "Client", "Kunde", "Cliente", "Cliente", "Klant", "Zákazník", "Klient", "Клієнт", "顧客", "客户", "고객", "العميل", "ग्राहक"]),
    ("Date", ["Fecha", "Date", "Datum", "Data", "Data", "Datum", "Datum", "Data", "Дата", "日付", "日期", "날짜", "التاريخ", "दिनांक"]),
    ("Does", ["Hace", "Fait", "Tut", "Faz", "Fa", "Doet", "Dělá", "Robi", "Робить", "行動", "行为", "행동", "يفعل", "करता है"]),
    ("Feels", ["Siente", "Ressent", "Fühlt", "Sente", "Prova", "Voelt", "Cítí", "Czuje", "Відчуває", "感情", "感受", "감정", "يشعر", "महसूस करता है"]),
    ("Finance", ["Finanzas", "Finance", "Finanzen", "Finanças", "Finanza", "Financiën", "Finance", "Finanse", "Фінанси", "財務", "财务", "재무", "المالية", "वित्त"]),
    ("From", ["De", "De", "Von", "De", "Da", "Van", "Od", "Od", "Від", "差出人", "发件人", "보낸 사람", "من", "प्रेषक"]),
    ("Lead", ["Responsable", "Responsable", "Verantwortlich", "Responsável", "Responsabile", "Verantwoordelijk", "Odpovídá", "Odpowiedzialny", "Відповідальний", "責任者", "负责", "책임", "المسؤول", "प्रमुख"]),
    ("Less effort", ["Menos esfuerzo", "Moins d'effort", "Weniger Aufwand", "Menos esforço", "Meno sforzo", "Minder moeite", "Méně úsilí", "Mniej wysiłku", "Менше зусиль", "労力 小", "投入少", "노력 적음", "جهد أقل", "कम मेहनत"]),
    ("Less value", ["Menos valor", "Moins de valeur", "Weniger Nutzen", "Menos valor", "Meno valore", "Minder waarde", "Menší hodnota", "Mniejsza wartość", "Менша цінність", "価値 低", "价值低", "가치 낮음", "قيمة أقل", "कम मूल्य"]),
    ("Method", ["Preparación", "Préparation", "Zubereitung", "Preparo", "Procedimento", "Bereiding", "Postup", "Przygotowanie", "Приготування", "作り方", "做法", "만드는 법", "الطريقة", "विधि"]),
    ("More effort", ["Más esfuerzo", "Plus d'effort", "Mehr Aufwand", "Mais esforço", "Più sforzo", "Meer moeite", "Více úsilí", "Więcej wysiłku", "Більше зусиль", "労力 大", "投入多", "노력 많음", "جهد أكبر", "ज़्यादा मेहनत"]),
    ("More value", ["Más valor", "Plus de valeur", "Mehr Nutzen", "Mais valor", "Più valore", "Meer waarde", "Větší hodnota", "Większa wartość", "Більша цінність", "価値 高", "价值高", "가치 높음", "قيمة أكبر", "ज़्यादा मूल्य"]),
    ("Next milestone", ["Próximo hito", "Prochaine étape", "Nächster Meilenstein", "Próximo marco", "Prossima tappa", "Volgende mijlpaal", "Další milník", "Następny etap", "Наступний етап", "次のマイルストーン", "下一里程碑", "다음 마일스톤", "المرحلة التالية", "अगला पड़ाव"]),
    ("Option", ["Opción", "Option", "Option", "Opção", "Opzione", "Optie", "Možnost", "Opcja", "Варіант", "選択肢", "选项", "옵션", "الخيار", "विकल्प"]),
    ("Owner", ["Responsable", "Responsable", "Verantwortlich", "Responsável", "Responsabile", "Eigenaar", "Vlastník", "Właściciel", "Власник", "担当", "负责人", "담당", "المسؤول", "ज़िम्मेदार"]),
    ("Risk", ["Riesgo", "Risque", "Risiko", "Risco", "Rischio", "Risico", "Riziko", "Ryzyko", "Ризик", "リスク", "风险", "위험", "المخاطر", "जोखिम"]),
    ("Sales", ["Ventas", "Ventes", "Vertrieb", "Vendas", "Vendite", "Verkoop", "Prodej", "Sprzedaż", "Продажі", "営業", "销售", "영업", "المبيعات", "बिक्री"]),
    ("Sponsor", ["Patrocinador", "Commanditaire", "Sponsor", "Patrocinador", "Sponsor", "Sponsor", "Sponzor", "Sponsor", "Спонсор", "スポンサー", "发起人", "후원", "الراعي", "प्रायोजक"]),
    ("Status", ["Estado", "Statut", "Status", "Status", "Stato", "Status", "Stav", "Status", "Статус", "状況", "状态", "상태", "الحالة", "स्थिति"]),
    ("Team", ["Equipo", "Équipe", "Team", "Equipe", "Team", "Team", "Tým", "Zespół", "Команда", "チーム", "团队", "팀", "الفريق", "टीम"]),
    ("Thinks", ["Piensa", "Pense", "Denkt", "Pensa", "Pensa", "Denkt", "Myslí", "Myśli", "Думає", "思考", "想法", "생각", "يفكر", "सोचता है"]),
    ("To", ["Para", "À", "An", "Para", "A", "Aan", "Komu", "Do", "Кому", "宛先", "收件人", "받는 사람", "إلى", "प्रति"]),
    ("Total", ["Total", "Total", "Summe", "Total", "Totale", "Totaal", "Celkem", "Razem", "Разом", "合計", "合计", "합계", "الإجمالي", "कुल"]),
    ("Workstream", ["Línea de trabajo", "Chantier", "Arbeitspaket", "Frente de trabalho", "Area di lavoro", "Werkstroom", "Pracovní oblast", "Obszar prac", "Напрям роботи", "作業領域", "工作流", "작업 흐름", "مسار العمل", "कार्यधारा"]),
    ("You need", ["Ingredientes", "Ingrédients", "Zutaten", "Ingredientes", "Ingredienti", "Ingrediënten", "Suroviny", "Składniki", "Інгредієнти", "材料", "食材", "재료", "المكونات", "सामग्री"]),
];

/// A layout's slots in the order its nodes are written, which is the order a reader hears them
/// at one depth (SPEC §3.12): each item's places together, its number, title, and body, where the
/// layout lists every number, then every title; and what `foot` says stands at the slide's foot,
/// a running head, after the rest (dogfood 2).
fn read_order<'t>(
    slots: impl IntoIterator<Item = (&'t String, &'t Slot)>,
    foot: impl Fn(&str, &Slot) -> bool,
) -> Vec<(&'t String, &'t Slot)> {
    let slots: Vec<(&String, &Slot)> = slots.into_iter().collect();
    let first = slots.iter().position(|(n, _)| ordinal(n).is_some()).unwrap_or(0);
    let mut order: Vec<(usize, (&String, &Slot))> = slots.into_iter().enumerate().collect();
    scaena_core::sort::by_key(&mut order, |(i, (name, slot))| match ordinal(name) {
        _ if foot(name, slot) => (usize::MAX, 0, *i),
        Some((k, _)) => (first, k, *i),
        None => (*i, 0, *i),
    });
    order.into_iter().map(|(_, s)| s).collect()
}

/// The theme's motion preset that builds a text by its paragraphs (By Bullet, PLAN 2.129), where
/// it has one: `bullets` where that is one, else the first that splits by paragraphs.
fn by_bullet_preset(theme: &Theme) -> Option<&str> {
    use scaena_core::model::values::SplitUnit;
    let splits = |name: &str| theme.motion.presets.get(name).is_some_and(|p| p.split == Some(SplitUnit::Paragraphs));
    if splits("bullets") {
        return Some("bullets");
    }
    theme.motion.presets.keys().map(String::as_str).find(|name| splits(name))
}

/// How a slide builds once its transition, `transition` ms long, ends (G.17): each of its items
/// rising after the one before it, as the theme's `rise` has it, the gap between them more than
/// half the theme's `standard` duration, so that no more than two move at once; and each chart
/// growing from its baseline, a mark at a time, as the theme's `grow` has it. Two to six items
/// build, within the theme's `maxBuild` (lint W321); one item, or more, comes in with the slide.
fn builds(theme: &Theme, mut built: Vec<(usize, Vec<String>)>, charts: &[String], transition: f64) -> Option<Value> {
    let mut items = Vec::new();
    let standard = theme.motion.durations.get("standard").map_or(420.0, |d| d.0);
    let gap = (standard * 0.6).round();
    // The transition, the gaps, and the last item's own rise.
    let span = |n: usize| transition + standard + gap * n.saturating_sub(1) as f64;
    if (2..=6).contains(&built.len()) && span(built.len()) <= theme.motion.max_build.unwrap_or(2500.0) {
        scaena_core::sort::by_key(&mut built, |(k, _)| *k);
        for (i, (_, ids)) in built.iter().enumerate() {
            let mut item = json!({ "target": ids, "enter": "rise" });
            if i > 0 {
                item["delay"] = json!(gap * i as f64);
            }
            items.push(item);
        }
    }
    items.extend(charts.iter().map(|id| json!({ "target": id, "enter": "grow" })));
    (!items.is_empty()).then(|| json!(items))
}

/// A chart's figures as a data source, written inline: a row for each category and series.
fn source(chart: &ChartData, dated: bool) -> Value {
    let several = chart.series.len() > 1;
    let mut rows = Vec::new();
    for (c, category) in chart.categories.iter().enumerate() {
        for (n, series) in chart.series.iter().enumerate() {
            let Some(value) = series.values.get(c) else { continue };
            let mut row = json!({ "category": category, "value": value });
            if several {
                row["series"] = json!(series.name.clone().unwrap_or_else(|| format!("Series {}", n + 1)));
            }
            rows.push(row);
        }
    }
    // Years a line runs along are dates, so the line runs on time (G.18).
    let mut schema = json!({ "category": if dated { "date" } else { "string" }, "value": "number" });
    if several {
        schema["series"] = json!("string");
    }
    json!({ "source": { "inline": rows }, "schema": schema })
}

/// Whether `categories` have an order of their own: a figure in one (a year, a quarter, a range,
/// an age), a month or a day, or a word of a scale.
fn ordered(categories: &[String]) -> bool {
    const ORDERED: [&str; 63] = [
        "low",
        "medium",
        "mid",
        "high",
        "small",
        "large",
        "poor",
        "fair",
        "good",
        "excellent",
        "never",
        "rarely",
        "sometimes",
        "often",
        "always",
        "before",
        "during",
        "after",
        "first",
        "last",
        "early",
        "late",
        "beginner",
        "intermediate",
        "advanced",
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
        "jan",
        "feb",
        "mar",
        "apr",
        "jun",
        "jul",
        "aug",
        "sep",
        "sept",
        "oct",
        "nov",
        "dec",
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
        "mon",
        "tue",
        "wed",
        "thu",
        "fri",
        "sat",
        "sun",
    ];
    categories.iter().any(|c| {
        let lower = c.to_lowercase();
        lower.chars().any(|ch| ch.is_ascii_digit())
            || lower.split(|ch: char| !ch.is_alphanumeric()).any(|w| ORDERED.contains(&w))
    })
}

/// Whether four items' figures want a square of them, not a row (G.16): the widest longer than
/// a quarter of the slide sets at a figure's size grown, "4.5 mm/yr", or one named at length.
/// Whether each of `items` is a price and the tier it buys, named first in its words: "$29:
/// Team, per agent per month".
fn priced(items: &[Item]) -> bool {
    items.iter().all(|i| {
        let tier = i.name().unwrap_or_default();
        i.figure_first()
            && i.label.as_deref().is_some_and(|l| l.starts_with(['$', '\u{20ac}', '\u{a3}', '\u{a5}']))
            && (1..=3).contains(&tier.split_whitespace().count())
    })
}

fn square(items: &[Item]) -> bool {
    let widest = items.iter().filter_map(Item::figure).map(|f| f.chars().count()).max().unwrap_or(0);
    let longest = items.iter().filter_map(Item::name).map(|n| n.chars().count()).max().unwrap_or(0);
    widest > 7 || longest > 32
}

/// Whether a timeline's moments are told in sentences, more words than a quarter of the slide
/// sets on a line or two (G.16).
fn told(items: &[Item]) -> bool {
    items.iter().any(|i| words(&i.detail) > 4)
}

/// How many words `text` says: those between its spaces, and in a script that sets none, a word
/// for each two of its characters.
fn words(text: &str) -> usize {
    let spaceless = |c: char| matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff);
    let set = text.chars().filter(|c| spaceless(*c)).count();
    text.split_whitespace().filter(|w| !w.chars().any(spaceless)).count() + set.div_ceil(2)
}

/// `narrative` with the words each slide shows set as a typesetter sets them ([`typeset`]).
fn typeset_all(narrative: &Narrative) -> Narrative {
    let lang = deck_lang(&narrative.brief).unwrap_or("en");
    let set = |s: &mut String| *s = typeset(s, lang);
    let mut out = narrative.clone();
    set(&mut out.executive_summary);
    for slide in &mut out.slides {
        set(&mut slide.slide_title);
        set(&mut slide.subtitle);
        slide.key_bullets.iter_mut().for_each(set);
        set(&mut slide.speaker_notes);
        slide.supporting_facts.iter_mut().for_each(set);
        slide.beat_name.iter_mut().for_each(set);
        if let Some(chart) = slide.chart_data.as_mut() {
            chart.categories.iter_mut().for_each(set);
            chart.series.iter_mut().filter_map(|s| s.name.as_mut()).for_each(set);
            chart.unit.iter_mut().for_each(set);
        }
    }
    out
}

/// `text` set as a typesetter sets it (G.16): an apostrophe and quotation marks curled, a range
/// of figures and a dash set apart by spaces each an en dash, and three dots an ellipsis. Words
/// that read as code keep their marks, and a figure's minus and a date's hyphens stay.
///
/// Double quotation marks are the deck's language's (`lang`, [`Marks`]): 「…」 in Japanese,
/// „…“ in German, « … » in French. A single mark is curled as English curls it, since a closing
/// one reads as an apostrophe. A short parenthetical, "P(E | H)", is kept on one line, its spaces
/// ones no line breaks at (U+00A0).
fn typeset(text: &str, lang: &str) -> String {
    if text.contains(['`', '{', '}', '<', '>']) {
        return text.to_string();
    }
    let marks = Marks::of(lang);
    // Outside English, a model's curled marks are the language's too.
    let english = marks.open == "\u{201c}";
    let chars: Vec<char> = text.chars().collect();
    let digit = |i: usize| chars.get(i).is_some_and(char::is_ascii_digit);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let before = i.checked_sub(1).map(|b| chars[b]);
        let after = chars.get(i + 1).copied();
        // A quotation opens at the start, after a space, or after a bracket, a dash, or a mark
        // that opens another.
        let opens = before.is_none_or(|b| {
            b.is_whitespace() || "([\u{2014}\u{2013}\u{201c}\u{2018}\u{300c}\u{201e}\u{ab}".contains(b)
        });
        match chars[i] {
            // An apostrophe in a word or after it ("Canada's", "countries'") and one for a decade's
            // first digits ("'90s") curl one way; a single quotation opening, the other.
            '\'' if opens && !(digit(i + 1) && digit(i + 2)) => out.push('\u{2018}'),
            '\'' => out.push('\u{2019}'),
            '"' | '\u{201c}' | '\u{201d}' if chars[i] == '"' || !english => {
                if opens {
                    out.push_str(marks.open);
                    // French keeps its own space inside; the one written goes.
                    while marks.open.ends_with('\u{a0}') && chars.get(i + 1).is_some_and(|c| *c == ' ') {
                        i += 1;
                    }
                } else {
                    if marks.close.starts_with('\u{a0}') {
                        out.truncate(out.trim_end_matches(' ').len());
                    }
                    out.push_str(marks.close);
                }
            }
            '.' if after == Some('.') && chars.get(i + 2) == Some(&'.') => {
                out.push('\u{2026}');
                i += 3;
                continue;
            }
            '-' if (i > 0 && digit(i - 1) && digit(i + 1) && !chained(&chars, i))
                || (before == Some(' ') && after == Some(' ')) =>
            {
                out.push('\u{2013}')
            }
            '(' => match short_parenthetical(&chars[i..]) {
                Some(len) => {
                    out.extend(chars[i..i + len].iter().map(|c| if *c == ' ' { '\u{a0}' } else { *c }));
                    i += len;
                    continue;
                }
                None => out.push('('),
            },
            c => out.push(c),
        }
        i += 1;
    }
    out
}

/// How many characters the parenthetical `chars` opens runs to, its brackets included, where
/// what it holds is a dozen characters or fewer with a space in it, "(E | H)": one a line should
/// not break inside.
fn short_parenthetical(chars: &[char]) -> Option<usize> {
    let close = chars.iter().position(|c| *c == ')')?;
    let inside = &chars[1..close];
    (inside.len() <= 12 && inside.contains(&' ') && !inside.contains(&'(')).then_some(close + 1)
}

/// Whether the hyphen at `at` joins one of several runs of digits, as a date's do: 2024-10-12.
fn chained(chars: &[char], at: usize) -> bool {
    let start = chars[..at].iter().rposition(|c| !c.is_ascii_digit()).map_or(0, |p| p + 1);
    let end = chars[at + 1..].iter().position(|c| !c.is_ascii_digit()).map_or(chars.len(), |p| at + 1 + p);
    let before = start >= 2 && chars[start - 1] == '-' && chars[start - 2].is_ascii_digit();
    let after = chars.get(end) == Some(&'-') && chars.get(end + 1).is_some_and(char::is_ascii_digit);
    before || after
}

/// Whether `chart`'s categories are years, two or more of them: a line along them runs on time,
/// each the distance from the last that its years are (G.18), not one step apart.
fn years(chart: &ChartData) -> bool {
    chart.categories.len() >= 2
        && chart
            .categories
            .iter()
            .all(|c| c.len() == 4 && c.bytes().all(|b| b.is_ascii_digit()) && c.as_str() >= "1000")
}

/// Whether `slide` charts two bars of one series.
fn two(slide: &Slide) -> bool {
    slide
        .chart_data
        .as_ref()
        .is_some_and(|c| c.categories.len() == 2 && c.series.len() == 1 && c.series[0].values.len() == 2)
}

/// The decimals of the most precise of `chart`'s values, two at most: 2.0 beside 1.1.
fn decimals(chart: &ChartData) -> usize {
    let places = |v: f64| (0..=2usize).find(|d| (v * 10f64.powi(*d as i32)).fract().abs() < 1e-9).unwrap_or(2);
    chart.series.iter().flat_map(|s| &s.values).map(|v| places(*v)).max().unwrap_or(0)
}

/// `value` with `places` decimals, written with `unit`: "$2.0T" beside "$1.1T".
fn at_places(value: f64, places: usize, unit: Option<&str>) -> String {
    if places == 0 {
        return in_unit(value, unit);
    }
    let n = format!("{value:.places$}");
    match unit.map(str::trim).filter(|u| !u.is_empty()) {
        None => n,
        Some(u) if u.starts_with(['$', '\u{20ac}', '\u{a3}', '\u{a5}']) => {
            let sign = u.chars().next().expect("not empty");
            format!("{sign}{n}{}", &u[sign.len_utf8()..])
        }
        Some(u) if u.starts_with(['%', '\u{b0}', '/', '\u{d7}']) => format!("{n}{u}"),
        // A unit written in a script set without spaces sits against its figure: "2時間", "340ドル".
        Some(u) if u.chars().next().is_some_and(unspaced) => format!("{n}{u}"),
        Some(u) => format!("{n} {u}"),
    }
}

/// Which of a single series' categories the slide is about, by index: the one its takeaway
/// names; else, where the categories are years, the latest; else the largest.
fn signal(chart: &ChartData, takeaway: Option<&str>) -> Option<usize> {
    let named: Vec<usize> = (0..chart.categories.len())
        .filter(|i| takeaway.is_some_and(|t| t.to_lowercase().contains(&chart.categories[*i].to_lowercase())))
        .collect();
    let years = chart.categories.iter().all(|c| c.len() == 4 && c.chars().all(|d| d.is_ascii_digit()));
    let values = chart.series.first().map(|s| s.values.as_slice()).unwrap_or_default();
    match named.as_slice() {
        [one] => Some(*one),
        _ if years => chart.categories.len().checked_sub(1),
        _ => (values.iter().enumerate()).max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, _)| i),
    }
}

/// The chart that plots `chart`'s source, `data`, in a layout's chart slot.
fn chart_node(
    chart: &ChartData,
    data: &str,
    layout: &str,
    said: [&str; 2],
    subtitle: Option<&str>,
    lang: &str,
    at: Value,
) -> Value {
    let [title, takeaway] = said;
    let takeaway = Some(takeaway).filter(|t| !t.is_empty());
    let kind = if layout == "line-chart" { "line" } else { "bar" };
    let several = chart.series.len() > 1;
    // Every value with the decimals of the most precise of them, two at most: 2.0 beside 1.1.
    let decimals = decimals(chart);
    let titled = |name: &str| match chart.unit.as_deref() {
        Some(unit) if !name.to_lowercase().contains(&unit.to_lowercase()) => format!("{name} ({unit})"),
        _ => name.to_string(),
    };
    let y_title = match (on_axis(subtitle), chart.series.first().and_then(|s| s.name.as_deref())) {
        (Some(subtitle), _) => Some(titled(subtitle)),
        (None, Some(name)) if !several => Some(titled(name)),
        _ => chart.unit.clone(),
    };
    let mut y = json!({ "field": "value", "type": "quantitative", "format": format!(",.{decimals}f") });
    let alt = described(chart, kind, y_title.as_deref(), lang);
    if let Some(t) = y_title {
        y["title"] = json!(t);
    }
    if kind == "bar" {
        y["domain"] = json!([0, null]);
    }
    let mut node = json!({
        "type": "chart",
        "kind": kind,
        "data": format!("@{data}"),
        "x": { "field": "category", "type": if kind == "line" && years(chart) { "temporal" } else { "ordinal" } },
        "y": y,
        "alt": alt,
        "semantic": "evidence",
        "at": at,
    });
    if several {
        node["series"] = json!({ "field": "series", "type": "nominal" });
        node["color"] = json!({ "field": "series", "type": "nominal", "scale": "categorical" });
        node["legend"] = json!("top");
    }
    // Bars of names with no order of their own run longest first, as a designer sorts them;
    // years, quarters, ranges, months, and the words of a scale keep the order they were written in.
    if kind == "bar" && !several && chart.categories.len() >= 4 && !ordered(&chart.categories) {
        node["dataTransform"] = json!([{ "sort": "-value" }]);
    }
    // Names too long to stand under their bars run down the side, the bars across.
    let longest = chart.categories.iter().map(|c| c.chars().count()).max().unwrap_or(0);
    if kind == "bar" && longest * chart.categories.len() > 64 {
        node["orient"] = json!("horizontal");
    }
    // The theme's one signal color on what the slide is about: the one category its takeaway
    // names; else, where the categories are years, the latest; else the largest bar.
    if kind == "bar"
        && !several
        && let Some(x) = signal(chart, takeaway).and_then(|i| chart.categories.get(i))
    {
        node["annotations"] = json!([{ "kind": "highlight", "at": { "x": x } }]);
    }
    // A line of one series is what its slide is about, drawn in the signal color by a highlight
    // that names nothing, every mark lit (deck format 0.33), and the year its headline names is
    // marked on it (G.19).
    if kind == "line" && !several {
        let mut notes = vec![json!({ "kind": "highlight", "at": {} })];
        notes.extend(period(chart, [Some(title), takeaway]));
        notes.extend(named_figure(chart, None, [Some(title), takeaway]));
        node["annotations"] = json!(notes);
    }
    // Lines of several series light the one the headline, else the takeaway, names: "SMB churn
    // climbed to 4.9% while mid-market churn fell" is about SMB's.
    if kind == "line"
        && several
        && let Some(named) = named_series(chart, [Some(title), takeaway])
    {
        let mut notes = vec![json!({ "kind": "highlight", "at": { "series": named } })];
        notes.extend(period(chart, [Some(title), takeaway]));
        notes.extend(named_figure(chart, Some(&named), [Some(title), takeaway]));
        node["annotations"] = json!(notes);
    }
    // Lines of several series light the one the headline, else the takeaway, names: "SMB churn
    // climbed to 4.9% while mid-market churn fell" is about SMB's.
    if kind == "line"
        && several
        && let Some(named) = named_series(chart, [Some(title), takeaway])
    {
        let mut notes = vec![json!({ "kind": "highlight", "at": { "series": named } })];
        notes.extend(period(chart, [Some(title), takeaway]));
        node["annotations"] = json!(notes);
    }
    node
}

/// A callout on the point of a line whose figure the first of `said` that names one of its values
/// names, where its ends' labels do not already say it: "Outage hours fell from 286 to 2 in three
/// years" labels the 286 its line peaks at (dogfood pass 4). `series` is the line of several lit.
fn named_figure(chart: &ChartData, series: Option<&str>, said: [Option<&str>; 2]) -> Option<Value> {
    let line = match series {
        Some(name) => chart.series.iter().find(|s| s.name.as_deref() == Some(name))?,
        None => chart.series.first()?,
    };
    let places = decimals(chart);
    let near = 0.5 * 10f64.powi(-(places as i32));
    let last = line.values.len().checked_sub(1)?;
    said.into_iter().flatten().find_map(|text| {
        figures(text).into_iter().filter(|n| !chart.categories.iter().any(|c| c.parse() == Ok(*n))).find_map(|n| {
            let at = line.values.iter().position(|v| (v - n).abs() < near).filter(|&i| i != 0 && i != last)?;
            let mut place = json!({ "x": chart.categories.get(at)? });
            if let Some(name) = series {
                place["series"] = json!(name);
            }
            Some(json!({ "kind": "callout", "at": place, "text": grouped(line.values[at], places) }))
        })
    })
}

/// What a line's latest figure counts, and when, from its own data: its axis's title less the
/// unit the figure carries and the rate the line runs at, else its series' name whole, then its
/// last category: "without power in 2026" under "2 hours" from "Hours without power per year".
fn counted(chart: &ChartData, subtitle: Option<String>) -> Option<String> {
    let when = chart.categories.last()?;
    // A series' own name is said whole; an axis's title loses what the figure beside it says.
    let (measure, titled) = match subtitle {
        Some(t) => (t, true),
        None => (chart.series.first().and_then(|s| s.name.clone()).unwrap_or_default(), false),
    };
    let mut words: Vec<&str> = measure.split(',').next().unwrap_or_default().split_whitespace().collect();
    let unit = if titled { chart.unit.as_deref().unwrap_or_default().trim() } else { "" };
    if words.first().is_some_and(|w| !unit.is_empty() && w.eq_ignore_ascii_case(unit)) {
        words.remove(0);
    }
    if words.last().is_some_and(|w| !unit.is_empty() && w.eq_ignore_ascii_case(unit)) {
        words.pop();
    }
    let rates = ["year", "quarter", "month", "week", "day", "hour"];
    if let [.., per, every] = words.as_slice()
        && matches!(per.to_lowercase().as_str(), "per" | "a" | "each" | "every")
        && rates.contains(&every.to_lowercase().as_str())
    {
        words.truncate(words.len() - 2);
    }
    let mut measure = words.join(" ");
    // Lowercased as the middle of a sentence, an acronym kept: "ARR in Q3", "outages in 2026".
    let acronym = measure
        .split_whitespace()
        .next()
        .is_some_and(|w| w.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase));
    if !acronym && let Some(first) = measure.chars().next() {
        measure = first.to_lowercase().chain(measure.chars().skip(1)).collect();
    }
    Some(if measure.is_empty() { format!("in {when}") } else { format!("{measure} in {when}") })
}

/// Whether `note` names the first and last of the figures `headline` writes, two or more: "Outage
/// hours fell to 2 in 2026, after 286 in the storm year" under "Outage hours fell from 286 to 2".
fn restates(headline: &str, note: &str) -> bool {
    let (said, again) = (figures(headline), figures(note));
    let named = |f: &f64| again.contains(f);
    said.len() >= 2 && said.first().is_some_and(named) && said.last().is_some_and(named)
}

/// The figures `text` writes, as numbers: "$1,140" is 1140, "4.9%" is 4.9.
fn figures(text: &str) -> Vec<f64> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .map(|w| w.trim_matches(['.', ',']).replace(',', ""))
        .filter(|w| !w.is_empty())
        .filter_map(|w| w.parse().ok())
        .collect()
}

/// `value` with `places` decimals and its thousands grouped, as the chart's own labels write it.
fn grouped(value: f64, places: usize) -> String {
    let written = format!("{:.places$}", value.abs());
    let (whole, part) = written.split_once('.').map_or((written.as_str(), None), |(w, p)| (w, Some(p)));
    let mut out = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if let Some(part) = part {
        out = format!("{out}.{part}");
    }
    if value < 0.0 { format!("\u{2212}{out}") } else { out }
}

/// The subtitle that titles a chart's value axis, as a designer titles a chart over its plot:
/// "Quarterly logo churn by segment, %" over SMB's and mid-market's lines, not a lone "%". One
/// too long for an axis leaves it as the series and the unit title it; it stays in the narrative.
fn on_axis(subtitle: Option<&str>) -> Option<&str> {
    subtitle.map(str::trim).filter(|s| !s.is_empty() && s.chars().count() <= AXIS)
}

/// The most characters a subtitle titles a value axis in.
const AXIS: usize = 60;

/// The series of several the first of `said` names, first in its words where it names more than
/// one, by name as a whole word, with case set aside: "US" in "US churn", never in "business".
fn named_series(chart: &ChartData, said: [Option<&str>; 2]) -> Option<String> {
    said.into_iter().flatten().find_map(|text| {
        let text = text.to_lowercase();
        (chart.series.iter())
            .filter_map(|s| s.name.as_deref().filter(|n| !n.trim().is_empty()))
            .filter_map(|n| word_at(&text, &n.to_lowercase()).map(|at| (at, n)))
            .min_by_key(|(at, _)| *at)
            .map(|(_, n)| n.to_string())
    })
}

/// Where `word` first stands in `text` as a whole word: no letter or digit just before or after it.
fn word_at(text: &str, word: &str) -> Option<usize> {
    text.match_indices(word).map(|(at, _)| at).find(|&at| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// What a chart shows, said to a reader who does not see it (dogfood 2), in the deck's language:
/// its kind and what it counts, or its series; then every value where there are a dozen or fewer,
/// else each series' highest and lowest. Never its slide's headline, which a reader has just heard
/// and which, as the figure's alt text, would hide the data.
fn described(chart: &ChartData, kind: &str, counts: Option<&str>, lang: &str) -> String {
    let words = own(lang);
    let places = decimals(chart);
    let value = |v: f64| at_places(v, places, chart.unit.as_deref());
    let several = chart.series.len() > 1;
    let names: Vec<&str> = chart.series.iter().filter_map(|s| s.name.as_deref()).collect();
    let mut head = (if kind == "line" { words.line } else { words.bar }).to_string();
    match (several, counts) {
        (true, _) if !names.is_empty() => head = format!("{head}: {}", names.join(", ")),
        (_, Some(counts)) => head = format!("{head}: {counts}"),
        _ => {}
    }
    let few = chart.series.iter().map(|s| s.values.len()).sum::<usize>() <= 12;
    let series: Vec<String> = chart
        .series
        .iter()
        .filter_map(|s| {
            let pairs: Vec<(&String, f64)> = chart.categories.iter().zip(s.values.iter().copied()).collect();
            let said = if few {
                pairs.iter().map(|(c, v)| format!("{c} {}", value(*v))).collect::<Vec<_>>().join(", ")
            } else {
                let high = pairs.iter().max_by(|a, b| a.1.total_cmp(&b.1))?;
                let low = pairs.iter().min_by(|a, b| a.1.total_cmp(&b.1))?;
                format!("{} {} {}, {} {} {}", words.highest, high.0, value(high.1), words.lowest, low.0, value(low.1))
            };
            Some(match s.name.as_deref() {
                Some(name) if several => format!("{name}: {said}"),
                _ => said,
            })
        })
        .collect();
    if series.is_empty() { head } else { format!("{head}. {}", series.join("; ")) }
}

/// The mark a line of years makes for the first year its headline, or else its takeaway, names
/// inside its span (G.19): where the words run on from it ("since 1980", "after", "from"), a
/// band from it to the last year, saying so; else a rule at it, saying the year.
fn period(chart: &ChartData, said: [Option<&str>; 2]) -> Option<Value> {
    if !years(chart) {
        return None;
    }
    let (first, last) = (chart.categories.iter().min()?.as_str(), chart.categories.iter().max()?.as_str());
    said.into_iter().flatten().find_map(|text| {
        let words: Vec<&str> = text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
        let at = words
            .iter()
            .position(|w| w.len() == 4 && w.bytes().all(|b| b.is_ascii_digit()) && first < *w && *w < last)?;
        let year = words[at];
        let before = at.checked_sub(1).map(|i| words[i].to_lowercase());
        Some(match before.as_deref() {
            Some(word @ ("since" | "after" | "from")) => {
                let word = format!("{}{}", word[..1].to_uppercase(), &word[1..]);
                json!({ "kind": "band", "at": { "x": [year, last] }, "text": format!("{word} {year}") })
            }
            _ => json!({ "kind": "rule", "at": { "x": year }, "text": year }),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scaena_narrato::{Brief, answer};
    use scaena_sources::Kind;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// The theme that ships as Dusk, and its fonts, read where the repository keeps them.
    fn dusk() -> String {
        std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap()
    }

    fn font(file: &str) -> Result<Vec<u8>, OpsError> {
        Ok(std::fs::read(root().join("docs/examples").join(file))?)
    }

    fn narrative(summary: &str, slides: Vec<Slide>) -> Narrative {
        Narrative {
            arc: "Problem\u{2013}Solution".into(),
            arc_reasoning: "A problem the room knows, then its answer.".into(),
            executive_summary: summary.into(),
            slides,
            truncated: false,
            icons: Default::default(),
            sources: Vec::new(),
            outline: None,
            brief: Brief {
                topic: "Pitch a Series A round for our carbon-negative cargo platform.".into(),
                audience: "Investors".into(),
                goal: "Persuade".into(),
                ..Brief::default()
            },
        }
    }

    /// A slide in each of the 56 layouts Narrato's catalog names, written as Scaena's prompt
    /// asks, in the catalog's order but the closing last.
    fn catalog() -> (String, Vec<Slide>) {
        let text = std::fs::read_to_string(root().join("crates/scaena-narrato/tests/answers/catalog.json")).unwrap();
        let written = answer::slides(&text).unwrap();
        assert_eq!(written.slides.len(), 56);
        (written.executive_summary, written.slides)
    }

    /// The deck's nodes in `state`, by slot: what each says, or its type.
    fn shown(deck: &Value, state: &str) -> Vec<(String, String)> {
        let state = deck["states"].as_array().unwrap().iter().find(|s| s["id"] == state).unwrap();
        let mut out = Vec::new();
        for id in state["props"].as_object().unwrap().keys() {
            let node = &deck["nodes"][id];
            let slot = node["at"]["in"].as_str().unwrap().to_string();
            let said = if node["text"].is_string() || node["runs"].is_array() {
                words(node)
            } else {
                node["type"].as_str().unwrap().to_string()
            };
            out.push((slot, said));
        }
        out
    }

    /// A text node's words, its runs' joined where it has them.
    fn words(node: &Value) -> String {
        match node["runs"].as_array() {
            Some(runs) => runs.iter().filter_map(|r| r["text"].as_str()).collect(),
            None => node["text"].as_str().unwrap_or_default().to_string(),
        }
    }

    /// A theme is asked whether its fonts draw a language before any call is paid for (PLAN
    /// G.21). The themes that ship draw each language Narrato writes in a Latin or a Cyrillic
    /// script, and name the script's font the rest are set in (PLAN G.22); a theme of the
    /// user's says what to add.
    #[test]
    fn a_theme_is_asked_whether_its_fonts_draw_the_language() {
        assert_eq!(LETTERS.map(|(language, _)| language), scaena_narrato::brief::LANGUAGES);
        let themes = ["docs/examples/themes/dusk.theme.json", "docs/examples/themes/ember.theme.json"]
            .into_iter()
            .chain(["docs/examples/authorability/themes/daybreak.theme.json", "docs/examples/themes/salon.theme.json"])
            .map(|path| std::fs::read_to_string(root().join(path)).unwrap());
        for theme in themes {
            for (language, _) in LETTERS {
                let asked = writable(Some(language), &theme, true, &font);
                match language {
                    "Japanese" | "Chinese (Simplified)" | "Korean" | "Arabic" | "Hindi" => {
                        // The script's font, which the deck takes (PLAN G.22), named.
                        let e = asked.expect_err(language);
                        let script = crate::scripts::SCRIPTS.iter().find(|s| s.language == language).unwrap();
                        assert_eq!(e.plan, None, "{e}");
                        assert!(e.message.starts_with(&format!("a deck in {language} cannot be made in ")), "{e}");
                        assert!(e.message.contains(&format!("without {},", script.family)), "{e}");
                    }
                    _ => asked.unwrap_or_else(|e| panic!("{language}: {e}")),
                }
            }
        }
        // No language, or one the brief will refuse, is not asked about.
        writable(None, &dusk(), true, &font).unwrap();
        writable(Some("Klingon"), &dusk(), true, &font).unwrap();
        // A theme of the user's, whose display family falls back on nothing: what to add.
        let mut mine: Value = serde_json::from_str(&dusk()).unwrap();
        mine["type"]["families"]["display"].as_object_mut().unwrap().remove("fallback");
        let e = writable(Some("Ukrainian"), &mine.to_string(), false, &font).unwrap_err();
        assert_eq!(e.plan, None);
        assert!(e.message.contains("the fonts of its `display` family") && e.message.contains("`fallback`"), "{e}");
        // A font the theme names that is not there stops it, saying which.
        let none = |file: &str| -> Result<Vec<u8>, OpsError> { Err(OpsError::new(format!("no {file}"))) };
        assert!(writable(Some("German"), &dusk(), true, &none).unwrap_err().message.contains("no fonts/"));
    }

    /// A deck in the shipped themes keeps what a talk writes beside its words: CO₂, a minus, an
    /// arrow, an approximation, written as they were, and none written plainly (PLAN G.21).
    #[test]
    fn a_talks_symbols_are_drawn_as_written() {
        let slide = |title: &str, bullets: &[&str]| Slide {
            slide_title: title.into(),
            layout_suggestion: "bullets".into(),
            key_bullets: bullets.iter().map(|b| b.to_string()).collect(),
            ..Slide::default()
        };
        let made = deck(
            &narrative(
                "Emissions, and what is working.",
                vec![
                    Slide { layout_suggestion: "title".into(), ..slide("Climate Change Now", &[]) },
                    slide(
                        "What Is Happening",
                        &[
                            "CO\u{2082} emissions keep rising: \u{2212}13% is the goal",
                            "Seas rise 4.5 mm/yr \u{2248} double the 1990s rate",
                            "Solutions work \u{2192} act now, \u{00bd} of it by 2030",
                        ],
                    ),
                ],
            ),
            &serde_json::from_str(&dusk()).unwrap(),
            &Pictures::default(),
        )
        .unwrap();
        let (made, write) =
            making(made, "Problem\u{2013}Solution", Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        let write = write.expect("a deck that validates");
        assert!(!made.fixed.iter().any(|f| f.code == "E120"), "{:#?}", made.fixed);
        assert!(!made.created.findings.iter().any(|f| f.code == "E120"), "{:#?}", made.created.findings);
        let deck = write.deck.to_value().unwrap();
        let said: String = deck["nodes"].as_object().unwrap().values().filter_map(|n| n["text"].as_str()).collect();
        for written in ["CO\u{2082}", "\u{2212}13%", "\u{2248}", "\u{2192}", "\u{00bd}"] {
            assert!(said.contains(written), "{written:?} in {said:?}");
        }
    }

    #[test]
    fn a_line_of_years_marks_the_year_its_headline_names() {
        let line = |years: &[&str]| ChartData {
            categories: years.iter().map(|y| y.to_string()).collect(),
            series: vec![answer::Series { name: None, values: vec![1.0; years.len()] }],
            ..Default::default()
        };
        let span = line(&["1880", "1950", "2024"]);
        let mark = |title: &str| period(&span, [Some(title), None]);
        // Words that run on from a year: a band from it to the last.
        assert_eq!(
            mark("Warming has accelerated since 1980"),
            Some(json!({ "kind": "band", "at": { "x": ["1980", "2024"] }, "text": "Since 1980" }))
        );
        assert_eq!(
            mark("After 1990, the rise"),
            Some(json!({ "kind": "band", "at": { "x": ["1990", "2024"] }, "text": "After 1990" }))
        );
        // A year named bare: a rule at it.
        assert_eq!(mark("The 1970 turn"), Some(json!({ "kind": "rule", "at": { "x": "1970" }, "text": "1970" })));
        // The takeaway's year where the headline names none.
        assert_eq!(
            period(&span, [Some("A long rise"), Some("It doubled from 1960")]).map(|m| m["kind"].clone()),
            Some(json!("band"))
        );
        // No mark for a year at either end, outside the span, or a figure that is no year.
        for title in ["Hottest in 2024", "Since 1880", "By 2050", "A rise of 1500 tonnes since 990", "Up 12%"] {
            assert_eq!(mark(title), None, "{title}");
        }
        assert_eq!(period(&line(&["Q1", "Q2", "Q3"]), [Some("Since 1980"), None]), None);
    }

    fn typeset_en(text: &str) -> String {
        typeset(text, "en")
    }

    #[test]
    fn words_are_set_as_a_typesetter_sets_them() {
        assert_eq!(typeset_en("Canada's wildfires"), "Canada\u{2019}s wildfires");
        assert_eq!(typeset_en("the '90s, the countries' pledges"), "the \u{2019}90s, the countries\u{2019} pledges");
        assert_eq!(
            typeset_en("\"A highway to 'climate hell'\""),
            "\u{201c}A highway to \u{2018}climate hell\u{2019}\u{201d}"
        );
        assert_eq!(
            typeset_en("2021-2024, 70-90% lost, 1.5-2\u{b0}C"),
            "2021\u{2013}2024, 70\u{2013}90% lost, 1.5\u{2013}2\u{b0}C"
        );
        assert_eq!(typeset_en("Climate - where we stand..."), "Climate \u{2013} where we stand\u{2026}");
        // A figure's minus, a date's hyphens, and a name's keep theirs; code keeps its quotes.
        assert_eq!(typeset_en("-13% by 2024-10-12, COVID-19"), "-13% by 2024-10-12, COVID-19");
        assert_eq!(typeset_en("`let s = \"x\";`"), "`let s = \"x\";`");
        // Set once, words stay as they are.
        let once = typeset_en("Canada's \"record\" 2021-2024...");
        assert_eq!(typeset_en(&once), once);
        // A figure's minus is not a bullet's mark.
        let i = item("-13%: Arctic summer ice per decade");
        assert_eq!((i.label.as_deref(), i.figure().as_deref()), (Some("-13%"), Some("-13%")));
        assert_eq!(item("- Measure our travel").whole, "Measure our travel");
    }

    /// A quotation is marked as the deck's language marks one, a model's curled marks too, and set
    /// once it stays (dogfood pass 6, V2).
    #[test]
    fn a_quotation_is_marked_as_its_language_marks_one() {
        let said = "She said \"we did this ourselves\".";
        assert_eq!(typeset(said, "en"), "She said \u{201c}we did this ourselves\u{201d}.");
        assert_eq!(typeset(said, "de"), "She said \u{201e}we did this ourselves\u{201c}.");
        assert_eq!(typeset(said, "fr"), "She said \u{ab}\u{a0}we did this ourselves\u{a0}\u{bb}.");
        assert_eq!(
            typeset("\u{300c}x\u{300d} \u{201c}\u{81ea}\u{5206}\u{305f}\u{3061}\u{3067}\u{201d}", "ja"),
            "\u{300c}x\u{300d} \u{300c}\u{81ea}\u{5206}\u{305f}\u{3061}\u{3067}\u{300d}"
        );
        for lang in ["en", "de", "fr", "ja", "zh-Hant"] {
            let once = typeset(said, lang);
            assert_eq!(typeset(&once, lang), once, "{lang}");
        }
        // A quote slot's quotation, its own marks taken off first.
        assert_eq!(quoted("\u{201c}Ours\u{201d}", "ja"), "\u{300c}Ours\u{300d}");
        assert_eq!(quoted("Ours", "zh-Hant"), "\u{300c}Ours\u{300d}");
        assert_eq!(quoted("Ours", "zh-Hans"), "\u{201c}Ours\u{201d}");
        assert_eq!(quoted("Ours", "de"), "\u{201e}Ours\u{201c}");
        // A line's latest figure sits against a unit in a script set without spaces (V3).
        assert_eq!(in_unit(2.0, Some("\u{6642}\u{9593}")), "2\u{6642}\u{9593}");
        assert_eq!(in_unit(2.0, Some("hours")), "2 hours");
        // A short parenthetical is kept on one line; a long one may still wrap.
        assert_eq!(typeset("P(H | E) = P(E | H)", "en"), "P(H\u{a0}|\u{a0}E) = P(E\u{a0}|\u{a0}H)");
        let long = "the posterior (what we believe after the test)";
        assert_eq!(typeset(long, "en"), long);
        assert_eq!(typeset("(2024)", "en"), "(2024)");
    }

    #[test]
    fn an_item_reads_its_label_and_its_figure() {
        let i = item("\u{2022} $4.1M: Q3 revenue, up 12%");
        assert_eq!((i.label.as_deref(), i.detail.as_str()), (Some("$4.1M"), "Q3 revenue, up 12%"));
        assert!(i.figure_first());
        assert_eq!((i.figure().as_deref(), i.name().as_deref()), (Some("$4.1M"), Some("Q3 revenue")));
        let i = item("Our wedge \u{2014} the only carbon-negative lane");
        assert_eq!(i.label.as_deref(), Some("Our wedge"));
        assert_eq!(item("No label here").label, None);
        assert_eq!(figure("Revenue grew 41% in pilots").as_deref(), Some("41%"));
        assert_eq!(figure("a $12.5M round").as_deref(), Some("$12.5M"));
        assert_eq!(figure("3\u{d7} faster").as_deref(), Some("3\u{d7}"));
        assert_eq!(figure("in 2024, then").as_deref(), Some("2024"));
        assert_eq!(figure("5Mbps").as_deref(), Some("5"));
        assert_eq!(figure("none"), None);
        assert!(numeric("$4.1M") && numeric("42%") && numeric("6 weeks") && numeric("2024"));
        assert!(!numeric("Q3 revenue") && !numeric("H1 2027") && !numeric("3 ships a week"));
        assert_eq!(head("Q3 revenue, up 12%"), ("Q3 revenue".to_string(), "up 12%".to_string()));
        assert_eq!(after_figure("1,200, 28% go on").as_deref(), Some("28% go on"));
        assert_eq!(after_figure("$40M revenue by 2028"), None);
        assert_eq!(after_figure("41%").as_deref(), Some(""));
    }

    #[test]
    fn each_layout_narrato_names_is_set_in_the_themes_own() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let (summary, slides) = catalog();
        let wanted = [
            "agenda",
            "section",
            "big-number",
            "statement",
            "quote",
            "bar-chart",
            "line-chart",
            "scorecard",
            "number-grid",
            "myth-fact",
            "myth-fact",
            "pros-cons",
            "quadrants",
            "numbered-cards",
            "milestones",
            "journey",
            "statement",
            "bullets",
            "three-columns",
            "cycle",
            "bullets",
            "roadmap",
            "matrix",
            "okrs",
            "raci",
            "swot",
            "thesis",
            "letter",
            "memo",
            "receipt",
            "next-steps",
            "milestones",
            "three-horizons",
            "funnel",
            "scenarios",
            "head-to-head",
            "ranking",
            "statement",
            "manifesto",
            "team-names",
            "bio",
            "testimonials",
            "how-it-works",
            "pipeline",
            "stack",
            "faq",
            "glossary",
            "checklist",
            "the-ask",
            "qa",
            "recipe",
            "definition",
            "numbered",
            "bullets",
        ];
        let (cover, closing) = (slides[0].clone(), slides[55].clone());
        let mut wrong = Vec::new();
        for (slide, wanted) in slides[1..55].iter().zip(wanted) {
            let made = deck(
                &narrative(&summary, vec![cover.clone(), slide.clone(), closing.clone()]),
                &theme,
                &Pictures::new(),
            )
            .unwrap();
            assert_eq!((made.layouts[0].as_str(), made.layouts[2].as_str()), ("title-center", "closing"));
            if made.layouts[1] != wanted {
                wrong.push(format!("{}: {} (wanted {wanted})", slide.layout_suggestion, made.layouts[1]));
            }
            let state = made.deck["states"][1]["id"].as_str().unwrap();
            let nodes = shown(&made.deck, state);
            assert!(nodes.len() >= 2, "{wanted}: {nodes:?}");
            assert!(nodes.iter().all(|(_, said)| !said.trim().is_empty()), "{wanted}: {nodes:?}");
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Each figure and its unit stays on the slide, set once, at the lengths a model writes
    /// "label: detail" bullets (40 to 90 characters), where the catalog's own are short.
    #[test]
    fn every_figure_and_its_unit_stays_on_the_slide_at_the_lengths_models_write() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let (summary, slides) = catalog();
        let made = |name: &str, bullets: &[&str]| {
            let mut slide = slides.iter().find(|s| s.layout_suggestion == name).unwrap().clone();
            slide.key_bullets = bullets.iter().map(|b| b.to_string()).collect();
            let made = deck(
                &narrative(&summary, vec![slides[0].clone(), slide, slides[55].clone()]),
                &theme,
                &Pictures::new(),
            )
            .unwrap();
            let state = made.deck["states"][1]["id"].as_str().unwrap().to_string();
            let mut said = shown(&made.deck, &state);
            said.sort();
            (made.layouts[1].clone(), said)
        };
        let has = |said: &[(String, String)], slot: &str, words: &str| {
            assert!(said.iter().any(|(s, w)| s == slot && w == words), "{slot} = {words:?} in {said:#?}");
        };
        let lacks = |said: &[(String, String)], slot: &str| {
            assert!(!said.iter().any(|(s, _)| s == slot), "no {slot} in {said:#?}");
        };
        // A big number keeps its unit: "11 days", not "11".
        let (_, number) = made("Big Number", &["11 days", "without power after Storm Elsa, October 2023"]);
        has(&number, "number", "11 days");
        has(&number, "claim", "without power after Storm Elsa, October 2023");
        let (_, number) = made("Big Number", &["12 hours", "median first reply time on a support ticket"]);
        has(&number, "number", "12 hours");
        // A funnel's stage keeps its figure beside its name.
        let (_, funnel) = made(
            "Funnel",
            &[
                "US freight spend: $1.1T a year moves by truck, rail, and sea",
                "Ocean and coastal: $180B of it moves by water along our coasts",
                "Gulf corridor: $24B a year between Houston, New Orleans, and Tampa",
                "Our first lanes: $2.1B in reach with the four ports we serve",
            ],
        );
        has(&funnel, "first", "US freight spend \u{b7} $1.1T");
        has(&funnel, "second", "Ocean and coastal \u{b7} $180B");
        // A step that leads with its figure keeps it as its title, its words under it once.
        let (layout, cards) = made(
            "Steps",
            &[
                "10 people: have the disease, at 1% prevalence in a thousand tested",
                "9 of them: test positive, since the test catches 90% of true cases",
                "99 people: test positive without the disease, a 10% false-positive rate",
                "108 positives: only 9 are sick, so a positive means about 8%",
            ],
        );
        assert_eq!(layout, "numbered-cards");
        has(&cards, "first-title", "10 people");
        has(&cards, "first", "Have the disease, at 1% prevalence in a thousand tested");
        has(&cards, "fourth-title", "108 positives");
        // A figure shown apart is not said again at the head of its words.
        let (_, scenarios) = made(
            "Scenarios",
            &[
                "Bear: $9.5M ARR if the Gulf corridor alone signs and fuel stays high",
                "Base: $22.0M ARR if two Atlantic lanes open on the plan's schedule",
                "Bull: $52.0M ARR if licensing deals close with two top-ten carriers",
            ],
        );
        has(&scenarios, "third-figure", "$52.0M");
        has(&scenarios, "third", "ARR if licensing deals close with two top-ten carriers");
        let (_, pipeline) = made(
            "Pipeline",
            &[
                "Leads: 2,400 a month from inbound and the freight marketplaces",
                "Pilots: 140 run this year, and 27% of them convert to a contract",
                "Contracts: 38 signed annual contracts, and 95% of them renew",
                "Revenue: $9.6M contracted over the next two years",
            ],
        );
        has(&pipeline, "first-figure", "2,400");
        has(&pipeline, "first", "A month from inbound and the freight marketplaces");
        // A comma inside a figure's name is no change beside it.
        let (_, grid) = made(
            "KPI Grid",
            &[
                "0: Hours dark during Storm Nadia, 2026",
                "4.2 min: Median outage restored, down from 11",
                "98.7%: Customers reached by text, up 6 points",
                "$3.1M: Saved in truck rolls this year",
            ],
        );
        has(&grid, "first-title", "Hours dark during Storm Nadia, 2026");
        lacks(&grid, "first");
        has(&grid, "second-title", "Median outage restored");
        has(&grid, "second", "Down from 11");
        // Three questions go on three cards, each titled by its question where all are short.
        let (layout, faq) = made(
            "FAQ",
            &[
                "Is the removal permanent?: Yes, mineralized carbon stays put for centuries",
                "What if fuel prices spike?: Lanes are priced on a fuel index",
                "Who verifies it?: An accredited auditor checks every voyage",
            ],
        );
        assert_eq!(layout, "cards");
        has(&faq, "first-title", "Is the removal permanent?");
        has(&faq, "first", "Yes, mineralized carbon stays put for centuries");
        // Where one runs too long to title its card, none is titled, and each reads as asked and
        // answered, with no colon after its question mark.
        let (layout, faq) = made(
            "FAQ",
            &[
                "If a test is 90% accurate and you test positive, what are the odds you're sick?: About 9%, because the disease is rare",
                "Why is it so low when the test is so good?: Most positives come from the many healthy people tested",
                "What should a doctor do next?: Test again, since a second positive raises the odds to about 47%",
            ],
        );
        assert_eq!(layout, "cards");
        has(
            &faq,
            "first",
            "If a test is 90% accurate and you test positive, what are the odds you\u{2019}re sick? About 9%, because the disease is rare",
        );
        has(
            &faq,
            "second",
            "Why is it so low when the test is so good? Most positives come from the many healthy people tested",
        );
        lacks(&faq, "first-title");
        lacks(&faq, "second-title");
        // A side's points, written on one line, are a list of them.
        let (_, sides) = made(
            "Pros / Cons",
            &[
                "Pro: cuts tailpipe emissions 80%; works with today's ships; needs no new ports",
                "Con: feedstock is scarce and costly; still carbon-positive overall; prices swing with harvests",
            ],
        );
        has(&sides, "first", "Cuts tailpipe emissions 80%\nWorks with today\u{2019}s ships\nNeeds no new ports");
        // A 2×2 reads its axes from its items and puts each where its position words say.
        let (_, matrix) = made(
            "2\u{d7}2 Matrix",
            &[
                "Own fleet: low impact, high cost",
                "Port software: high impact, low cost",
                "Dashboards: low impact, low cost",
                "Atlantic lanes: high impact, high cost",
            ],
        );
        has(&matrix, "y-high", "More impact");
        has(&matrix, "y-low", "Less impact");
        has(&matrix, "x-high", "More cost");
        has(&matrix, "x-low", "Less cost");
        has(&matrix, "first", "Port software");
        has(&matrix, "second", "Atlantic lanes");
        has(&matrix, "third", "Dashboards");
        has(&matrix, "fourth", "Own fleet");
        // Short moments on a timeline: each its date over the rule and its event under it.
        let (layout, timeline) = made(
            "Timeline",
            &[
                "Jan 2025: Permit denied",
                "Mar 2025: Appeal filed",
                "Jun 2025: Permit granted",
                "Sep 2025: Ground broken",
            ],
        );
        assert_eq!(layout, "timeline");
        has(&timeline, "first-title", "Jan 2025");
        has(&timeline, "first", "Permit denied");
        // Moments told in Japanese are told in sentences too.
        let (layout, _) = made(
            "Timeline",
            &[
                "2023年: 試作機が排気の92%を回収した",
                "2024年: 初の実証航海を終えた",
                "2025年: 三社と契約を結んだ",
                "2026年: 四つの港で運航を始めた",
            ],
        );
        assert_eq!(layout, "milestones");
    }

    /// A set's titles stay one size (G.12, lift-up dogfood): How It Works' step titles take the
    /// lines they need at the set's size, every step's words on one row, and one too long for its
    /// place shrinks its set with it, the slot's own fit held as a text's is.
    #[test]
    fn a_steps_titles_wrap_at_one_size_and_shrink_together() {
        let (summary, slides) = catalog();
        let made = |bullets: &[&str]| {
            let mut slide = slides.iter().find(|s| s.layout_suggestion == "How It Works").unwrap().clone();
            slide.key_bullets = bullets.iter().map(|b| b.to_string()).collect();
            let n = narrative(&summary, vec![slides[0].clone(), slide, slides[55].clone()]);
            let (made, write) = generating(&n, Path::new("dusk.theme.json"), dusk(), &font).unwrap();
            assert!(made.created.created, "{:#?}", made.created.findings);
            assert_eq!(made.layouts[1], "how-it-works");
            let deck = write.unwrap().deck.to_value().unwrap();
            let state = deck["states"][1]["id"].as_str().unwrap().to_string();
            let caps: Vec<Option<f64>> = ["first", "second", "third"]
                .iter()
                .map(|o| deck["nodes"][format!("{state}-{o}-title")]["maxSize"].as_f64())
                .collect();
            caps
        };
        // Each fits its two lines: none shrinks, each at the role's size.
        let held = made(&[
            "Start with how common it is: the base rate",
            "Multiply by how often the test catches it: sensitivity",
            "Divide by all the ways to test positive: the true and the false alike",
        ]);
        assert!(held.iter().all(Option::is_none), "none shrinks: {held:?}");
        // One too long for its two lines shrinks, and the others with it, to one size: theirs
        // capped at the size it reached, which, shrinking to fit, needs no cap of its own.
        let held = made(&[
            "Base rate: how common it is",
            "Sensitivity: how often it catches it",
            "Divide by every way that anyone at all could test positive: the true and the false alike",
        ]);
        let caps: Vec<f64> = held.iter().flatten().copied().collect();
        assert_eq!(caps.len(), 2, "the two that fit are capped: {held:?}");
        assert!(caps[0] == caps[1] && caps[0] < 48.0 && held[2].is_none(), "shrunk together: {held:?}");
    }

    /// Short words in a large place grow to fill it (dogfood pass 4): each side's list of a
    /// pros and cons, each month's of a roadmap, as one set; a slide's one short list past the
    /// lede, under its headline still. Prices each with a tier go on a pricing slide, the tier
    /// titling its card and the price its figure.
    #[test]
    fn short_lists_grow_to_fill_their_places_and_prices_name_their_tiers() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let (summary, slides) = catalog();
        let made = |name: &str, bullets: &[&str]| {
            let mut slide = slides.iter().find(|s| s.layout_suggestion == name).unwrap().clone();
            slide.key_bullets = bullets.iter().map(|b| b.to_string()).collect();
            let made = deck(
                &narrative(&summary, vec![slides[0].clone(), slide, slides[55].clone()]),
                &theme,
                &Pictures::new(),
            )
            .unwrap();
            let state = made.deck["states"][1]["id"].as_str().unwrap().to_string();
            (made.layouts[1].clone(), made.deck.clone(), state)
        };
        let node = |deck: &Value, state: &str, slot: &str| deck["nodes"][format!("{state}-{slot}")].clone();
        let size = |r: &str| theme.typography.roles[r].size;
        // Each side's list of short points grows as the words of a set do, to three-quarters of
        // the headline.
        let (layout, deck, state) = made(
            "Pros / Cons",
            &[
                "Pros: cuts SMB churn by an estimated 40%; simpler packaging; wins back price-sensitive prospects",
                "Cons: $1.2M of ARR at risk from downgrades; sales retraining in Q4; margin dilution of 0.4 points",
            ],
        );
        assert_eq!(layout, "pros-cons");
        for side in ["first", "second"] {
            let n = node(&deck, &state, side);
            assert!(n["list"].is_array(), "{n:#}");
            assert_eq!(
                (n["fit"].as_str(), n["maxSize"].as_f64()),
                (Some("grow"), Some(size("headline") * 0.75)),
                "{n:#}"
            );
        }
        let (layout, deck, state) = made(
            "Roadmap",
            &[
                "October: launch the Starter tier; call 400 at-risk SMB accounts; fix the renewal email flow",
                "November: close the two Fortune 500 pilots; hire four enterprise AEs; restart paid search",
                "December: run the partner webinar series; review churn weekly; set FY27 targets",
            ],
        );
        assert_eq!(layout, "roadmap");
        assert_eq!(node(&deck, &state, "first")["fit"], "grow");
        // A journey's cells, a few words each, grow to the lede, every lane's; its phases and
        // lanes to the body (dogfood pass 5).
        let (layout, deck, state) = made(
            "Journey",
            &[
                "Spring 2024: knock on every door / will anyone sign up? / hopeful but unsure",
                "Summer 2024: raise $2.1M from 312 households / this might actually work / proud",
                "Winter 2024: bolt the first panels to the co-op roof / we did this ourselves / elated",
            ],
        );
        assert_eq!(layout, "journey");
        for lane in ["does", "thinks", "feels"] {
            for part in ["first", "second", "third"] {
                let n = node(&deck, &state, &format!("{lane}-{part}"));
                assert_eq!((n["fit"].as_str(), n["maxSize"].as_f64()), (Some("grow"), Some(size("lede"))), "{n:#}");
            }
        }
        for label in ["first-phase", "third-lane"] {
            let n = node(&deck, &state, label);
            assert_eq!((n["fit"].as_str(), n["maxSize"].as_f64()), (Some("grow"), Some(size("body"))), "{n:#}");
        }
        // A slide's one list of short lines may grow past the lede, to three-quarters of its
        // headline; a long one keeps the lede.
        let (layout, deck, state) = made(
            "Numbered",
            &[
                "How common is the condition in people like me?",
                "How often does the test catch it?",
                "How often does it raise a false alarm?",
                "Of all the positives, what share are real?",
            ],
        );
        assert_eq!(layout, "numbered");
        assert_eq!(node(&deck, &state, "body")["maxSize"].as_f64(), Some(size("headline") * 0.75));
        let (_, deck, state) = made(
            "Numbered",
            &[
                "How common is the condition in people like me, counted over the last ten years of screening?",
                "How often does the test catch it?",
                "How often does it raise a false alarm?",
            ],
        );
        assert_eq!(node(&deck, &state, "body")["maxSize"].as_f64(), Some(size("lede")));
        // Prices each with its tier: the tier titles the card, the price is its figure.
        let (layout, deck, state) = made(
            "Scorecard",
            &[
                "$0: Starter, up to 3 agents and 500 drafts a month",
                "$29: Team, per agent per month, unlimited drafts",
                "$59: Business, adds SSO, audit log, and a 99.9% SLA",
            ],
        );
        assert_eq!(layout, "pricing");
        assert_eq!(node(&deck, &state, "first-title")["text"], "Starter");
        assert_eq!(node(&deck, &state, "first-price")["text"], "$0");
        assert_eq!(node(&deck, &state, "first")["text"], "Up to 3 agents and 500 drafts a month");
        assert!(node(&deck, &state, "first")["list"].is_null(), "one point is no list");
        assert_eq!(node(&deck, &state, "third-title")["text"], "Business");
        assert_eq!(node(&deck, &state, "third")["text"], "Adds SSO, audit log, and a 99.9% SLA");
    }

    /// A few short lines of words alone in a large place grow toward three-quarters of the
    /// headline (dogfood pass 4): a term's meaning, and a set of cards' answers, held to one size.
    #[test]
    fn a_short_text_alone_in_a_large_place_grows_under_its_heading() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let (summary, slides) = catalog();
        let made = |name: &str, bullets: &[&str]| {
            let mut slide = slides.iter().find(|s| s.layout_suggestion == name).unwrap().clone();
            slide.key_bullets = bullets.iter().map(|b| b.to_string()).collect();
            let made = deck(
                &narrative(&summary, vec![slides[0].clone(), slide, slides[55].clone()]),
                &theme,
                &Pictures::new(),
            )
            .unwrap();
            let state = made.deck["states"][1]["id"].as_str().unwrap().to_string();
            (made.layouts[1].clone(), made.deck.clone(), state)
        };
        let most = |deck: &Value, state: &str, slot: &str| deck["nodes"][format!("{state}-{slot}")]["maxSize"].as_f64();
        let under = theme.typography.roles["headline"].size * 0.75;
        let (layout, deck, state) = made(
            "Definition",
            &[
                "The probability that A is true given that B has happened, written P(A | B)",
                "P(rain | cloudy) is higher than P(rain): knowing it's cloudy changes the odds",
            ],
        );
        assert_eq!(layout, "definition");
        assert_eq!(most(&deck, &state, "meaning"), Some(under));
        assert_eq!(most(&deck, &state, "example"), Some(under));
        // A meaning of more than a few lines keeps its size.
        let long = "The probability that A is true given that B has happened, written P(A | B), which is the \
                    joint probability of A and B divided by the probability of B, defined wherever B can \
                    happen at all";
        let (_, deck, state) = made("Definition", &[long, "P(rain | cloudy) is higher than P(rain)"]);
        assert_eq!(most(&deck, &state, "meaning"), None);
        // Each card's short answer grows as far, the set held to one size by making.
        let (layout, deck, state) = made(
            "FAQ",
            &[
                "Does Relay send anything on its own?: Never by default; you choose which topics can auto-send",
                "Where does our data go?: It stays in your region and never trains shared models",
                "How long does setup take?: About 20 minutes with Zendesk, Intercom, or Front",
            ],
        );
        assert_eq!(layout, "cards");
        // Each card's answer grows no larger than its question's role is set.
        let title = theme.typography.roles[theme.layouts["cards"].slots["first-title"].role.as_deref().unwrap()].size;
        for card in ["first", "second", "third"] {
            assert_eq!(most(&deck, &state, card), Some(under.min(title)), "{card}");
        }
        // A tier's price grows as its card's figure, up to the hero's size.
        let (layout, deck, state) = made(
            "Scorecard",
            &[
                "$0: Starter, up to 3 agents and 500 drafts a month",
                "$29: Team, per agent per month, unlimited drafts",
                "$59: Business, adds SSO, audit log, and a 99.9% SLA",
            ],
        );
        assert_eq!(layout, "pricing");
        let hero = theme.typography.roles["hero"].size;
        for card in ["first-price", "second-price", "third-price"] {
            assert_eq!(most(&deck, &state, card), Some(hero), "{card}");
            assert_eq!(deck["nodes"][format!("{state}-{card}")]["maxLines"], 1, "{card}");
        }
        // A scorecard's figures grow past the headline's size, toward the numeral's.
        let (layout, deck, state) = made(
            "KPI Grid",
            &["$48.2M: ARR, up 9% on Q2", "114%: net revenue retention", "78.4%: gross margin, up 2.1 points"],
        );
        let headline = theme.typography.roles["headline"].size;
        assert!(layout.starts_with("number"), "{layout}");
        for card in ["first-title", "second-title", "third-title"] {
            let figure = most(&deck, &state, card);
            assert!(figure.is_some_and(|f| f > headline), "{card}: {figure:?}");
        }
        // Quotes side by side under a headline grow toward nine-tenths of it, held alike; a
        // quote set as large as that already keeps its own size.
        let (layout, deck, state) = made(
            "Testimonials",
            &[
                "Our queue used to be a Monday problem. Now it's empty by ten. \u{2014} Aisha Bello, Kettle & Co.",
                "Relay sounds like us on our best day. \u{2014} Tom Brandt, Parcelpoint",
            ],
        );
        assert_eq!(layout, "two-quotes");
        let quote = theme.typography.roles["quote"].size;
        let grows = (headline * 0.9 > quote).then_some(headline * 0.9);
        for card in ["first", "second"] {
            assert_eq!(most(&deck, &state, card), grows, "{card}");
        }
        // In Ember, whose quote is smaller than its headline, they grow.
        let ember: Theme = serde_json::from_str(
            &std::fs::read_to_string(root().join("docs/examples/themes/ember.theme.json")).unwrap(),
        )
        .unwrap();
        let mut slide = slides.iter().find(|s| s.layout_suggestion == "Testimonials").unwrap().clone();
        slide.key_bullets = vec![
            "Our queue used to be a Monday problem. Now it's empty by ten. \u{2014} Aisha Bello, Kettle & Co.".into(),
            "Relay sounds like us on our best day. \u{2014} Tom Brandt, Parcelpoint".into(),
        ];
        let made_in = super::deck(
            &narrative(&summary, vec![slides[0].clone(), slide, slides[55].clone()]),
            &ember,
            &Pictures::new(),
        )
        .unwrap();
        assert_eq!(made_in.layouts[1], "two-quotes");
        let state_in = made_in.deck["states"][1]["id"].as_str().unwrap();
        let cap = ember.typography.roles["headline"].size * 0.9;
        assert!(cap > ember.typography.roles["quote"].size);
        for card in ["first", "second"] {
            assert_eq!(most(&made_in.deck, state_in, card), Some(cap), "{card} in Ember");
        }
        // A statement with nothing under it grows toward the hero's size over the rows its
        // support leaves, in the middle of them.
        let mut slide = slides.iter().find(|s| s.layout_suggestion == "Statement").unwrap().clone();
        slide.subtitle = String::new();
        slide.key_bullets.clear();
        let made = super::deck(
            &narrative(&summary, vec![slides[0].clone(), slide, slides[55].clone()]),
            &theme,
            &Pictures::new(),
        )
        .unwrap();
        assert_eq!(made.layouts[1], "statement");
        let state = made.deck["states"][1]["id"].as_str().unwrap();
        let statement = &made.deck["nodes"][format!("{state}-statement")];
        let slots = &theme.layouts["statement"].slots;
        let (Some(Range::Span([top, _])), Some(Range::Span([_, foot]))) =
            (slots["statement"].row.as_ref(), slots["support"].row.as_ref())
        else {
            panic!("the statement layout's rows")
        };
        assert_eq!(statement["at"]["row"], json!([top, foot]));
        assert_eq!(statement["align"], json!({ "y": "center" }));
        assert_eq!(statement["maxSize"].as_f64(), Some(theme.typography.roles["hero"].size));
        assert_eq!(statement["maxLines"], 3, "a statement grows on three lines at most");
    }

    /// A figure keeps the magnitude a language writes after it: "41.2万ドル" is $412K.
    #[test]
    fn a_figure_keeps_its_magnitude_in_any_language() {
        assert_eq!(figure("41.2万ドルの予算超過").as_deref(), Some("41.2万ドル"));
        assert_eq!(figure("予算は3億円").as_deref(), Some("3億円"));
        assert_eq!(figure("1.2亿元的收入").as_deref(), Some("1.2亿元"));
        assert_eq!(figure("5兆円").as_deref(), Some("5兆円"));
        assert_eq!(figure("매출 3억원 달성").as_deref(), Some("3억원"));
        assert_eq!(figure("12만 명").as_deref(), Some("12만"));
        assert_eq!(figure("US spend: $1.1T a year").as_deref(), Some("$1.1T"));
        assert_eq!(figure("5Mbps").as_deref(), Some("5"));
        assert!(numeric("41.2万ドル") && numeric("11 days") && numeric("3億円"));
    }

    /// Someone else's words are set in quotation marks, the one who said them led by a dash, so
    /// that they never read as the presenter's own.
    #[test]
    fn a_quote_is_set_in_quotation_marks_and_its_source_after_a_dash() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let (summary, slides) = catalog();
        let made = |name: &str| {
            let slide = slides.iter().find(|s| s.layout_suggestion == name).unwrap().clone();
            let made = deck(
                &narrative(&summary, vec![slides[0].clone(), slide, slides[55].clone()]),
                &theme,
                &Pictures::new(),
            )
            .unwrap();
            let state = made.deck["states"][1]["id"].as_str().unwrap().to_string();
            shown(&made.deck, &state)
        };
        let has = |said: &[(String, String)], slot: &str, words: &str| {
            assert!(said.iter().any(|(s, w)| s == slot && w == words), "{slot} = {words:?} in {said:#?}");
        };
        let quote = made("Quote");
        has(
            &quote,
            "quote",
            "\u{201c}We have net-zero targets and no way to ship a single box without emitting\u{201d}",
        );
        has(&quote, "who", "\u{2014} Head of Logistics, a Fortune 500 retailer");
        let testimonials = made("Testimonials");
        has(&testimonials, "first", "\u{201c}We cut our freight emissions below zero in one quarter\u{201d}");
        has(&testimonials, "first-note", "\u{2014} VP Supply Chain, national grocer");
    }

    /// Texts a slide sets side by side are one size where they shrink to fit, as where they grow:
    /// a team's credits, a FAQ's answers, each set at the size the one with the least room took.
    #[test]
    fn a_set_of_texts_that_shrink_is_set_alike() {
        let (summary, slides) = catalog();
        let at = |layout: &str| slides.iter().find(|s| s.layout_suggestion == layout).unwrap().clone();
        let mut team = at("Team");
        team.key_bullets = [
            "Maya Chen, CEO: ran Gulf operations at a top-three carrier",
            "Ade Okafor, CTO: built direct air capture at scale",
            "Luis Ortega, COO: scaled ports across three continents, opened eleven terminals from Rotterdam to Singapore, and ran the largest retrofit program the industry has seen",
        ]
        .map(String::from)
        .to_vec();
        let mut faq = at("FAQ");
        faq.key_bullets = [
            "Is the removal permanent?: Yes",
            "What if fuel prices spike?: Lanes are priced on a fuel index",
            "Who verifies it?: An accredited auditor checks every voyage against the registry, publishes each certificate, and samples the mineralized carbon at the store ashore each quarter of the year",
            "Can others copy it?: Our capture design holds four patents",
        ]
        .map(String::from)
        .to_vec();
        let built = deck(
            &narrative(&summary, vec![slides[0].clone(), team, faq, slides[55].clone()]),
            &serde_json::from_str::<Theme>(&dusk()).unwrap(),
            &Pictures::new(),
        )
        .unwrap();
        let (made, write) =
            making(built, "Problem\u{2013}Solution", Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        assert!(made.created.created, "{:#?}", made.created.findings);
        let write = write.unwrap();
        let nothing = scaena_store::Files::Zip(Default::default());
        let view = View { base: &nothing, pending: write.files.clone() };
        let theme = scaena_engine::theme::Theme::from_json(&dusk()).unwrap();
        let mut engine = crate::lint::engine_in(&write.deck, &view, &theme, None).unwrap();
        let data = crate::lint::data_in(&write.deck, &view).unwrap();
        let deck = write.deck.to_value().unwrap();
        // The size each text of `slots` in the `n`th state is set at.
        let mut sizes = |n: usize, slots: &[&str]| -> Vec<f64> {
            let state = deck["states"][n]["id"].as_str().unwrap().to_string();
            let ids: Vec<String> = slots.iter().map(|s| format!("{state}-{s}")).collect();
            let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
            let req = scaena_engine::FrameRequest {
                deck: &write.deck,
                theme: &theme,
                data: &data,
                state: &state,
                t_ms: f64::INFINITY,
                format: None,
            };
            let placed = engine.text_layouts(&req, &ids).unwrap();
            ids.iter()
                .zip(&placed)
                .map(|(id, p)| {
                    let role = deck["nodes"][*id]["role"].as_str().unwrap();
                    (f64::from(p.scale) * theme.typography.roles[role].size * 4.0).round() / 4.0
                })
                .collect()
        };
        let alike = |sizes: Vec<f64>| assert!(sizes.windows(2).all(|w| (w[0] - w[1]).abs() < 0.3), "{sizes:?}");
        alike(sizes(1, &["first", "second", "third"]));
        alike(sizes(2, &["first", "second", "third", "fourth"]));
        alike(sizes(2, &["first-title", "second-title", "third-title", "fourth-title"]));
    }

    #[test]
    fn each_place_of_a_layout_takes_its_part_of_an_item() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let (summary, slides) = catalog();
        let made = |name: &str| {
            let slide = slides.iter().find(|s| s.layout_suggestion == name).unwrap().clone();
            let made = deck(
                &narrative(&summary, vec![slides[0].clone(), slide, slides[55].clone()]),
                &theme,
                &Pictures::new(),
            )
            .unwrap();
            let state = made.deck["states"][1]["id"].as_str().unwrap().to_string();
            let mut said: Vec<(String, String)> = shown(&made.deck, &state);
            said.sort();
            said
        };
        let has = |said: &[(String, String)], slot: &str, words: &str| {
            assert!(said.iter().any(|(s, w)| s == slot && w == words), "{slot} = {words:?} in {said:#?}");
        };
        let scorecard = made("Scorecard");
        has(&scorecard, "first-figure", "$2.4M");
        has(&scorecard, "first-title", "Offsets bought each year");
        has(&scorecard, "first", "Up 31%");
        has(&scorecard, "third-figure", "6 weeks");
        let pipeline = made("Pipeline");
        has(&pipeline, "first-figure", "1,200");
        has(&pipeline, "first-title", "Leads this year");
        has(&pipeline, "first", "12% go on");
        let ranking = made("Ranking");
        has(&ranking, "first-rank", "1");
        has(&ranking, "first-figure", "41%");
        has(&ranking, "first", "Houston to Tampa");
        // A timeline told in sentences goes down the slide, each moment beside its date (G.16).
        let timeline = made("Timeline");
        has(&timeline, "first-date", "2023");
        has(&timeline, "first", "Capture prototype proves 92% exhaust uptake");
        // One of a few words a moment stays across the slide.
        let mut short = slides.iter().find(|s| s.layout_suggestion == "Timeline").unwrap().clone();
        short.key_bullets = ["2023: Prototype", "2024: Pilot voyage", "2025: Three contracts", "2026: Four ports live"]
            .map(String::from)
            .to_vec();
        let across =
            deck(&narrative(&summary, vec![slides[0].clone(), short, slides[55].clone()]), &theme, &Pictures::new());
        assert_eq!(across.unwrap().layouts[1], "timeline");
        let vs = made("VS");
        has(&vs, "first-title", "Tidewater");
        has(&vs, "second-title", "Offsets");
        has(&vs, "first-criterion", "Verification");
        has(&vs, "first-left", "Per-container certificate");
        has(&vs, "first-right", "Self-reported");
        let okrs = made("OKRs");
        has(&okrs, "header", "Make the Gulf corridor the default for green freight");
        has(&okrs, "kicker", "Q4 OKRs");
        has(&okrs, "first", "Shippers under contract");
        has(&okrs, "first-target", "50");
        let number = made("Big Number");
        has(&number, "number", "3%");
        has(&number, "claim", "of global CO\u{2082} comes from shipping, more than all of aviation");
        has(&number, "under", "International Maritime Organization, Fourth GHG Study");
        let journey = made("Journey");
        has(&journey, "first-phase", "Discover");
        has(&journey, "feels-second", "Cautiously impressed");
        let checklist = made("Checklist");
        has(&checklist, "third-state", "In progress");
        has(&checklist, "third", "Two vessels retrofitted");
        let receipt = made("Receipt");
        has(&receipt, "fourth-item", "Verification");
        has(&receipt, "total", "$41");
        let team = made("Team");
        has(&team, "second-title", "Ade Okafor");
        has(&team, "second", "CTO \u{b7} Built direct air capture at scale");
        let before = made("Before / After");
        has(&before, "first-title", "Before");
        has(&before, "first", "Buy offsets, file reports, and hope");
        let matrix = made("2\u{d7}2 Matrix");
        has(&matrix, "third", "Dashboards");
        let steps = made("Steps");
        has(&steps, "first", "The shipper reserves a carbon-negative lane");
        // A deck's one divider stands by no number: it counts nothing the audience was shown.
        let section = made("Section Header");
        assert!(!section.iter().any(|(slot, _)| slot == "number"), "{section:?}");
        assert_eq!(sentence("iPhone sales"), "iPhone sales");
        assert_eq!(sentence("cuts emissions 80%"), "Cuts emissions 80%");
    }

    /// A talk on climate change for colleagues, 25 slides as Narrato writes them.
    fn climate() -> Narrative {
        let text = std::fs::read_to_string(root().join("crates/scaena-narrato/tests/answers/climate.json")).unwrap();
        let written = answer::slides(&text).unwrap();
        let mut n = narrative(&written.executive_summary, written.slides);
        n.arc = "Data Storytelling".into();
        n
    }

    /// The node in the `slot` of the `n`th state of `deck`.
    fn placed<'d>(deck: &'d Value, n: usize, slot: &str) -> &'d Value {
        let props = deck["states"][n]["props"].as_object().unwrap();
        let id = props.keys().find(|id| deck["nodes"][id.as_str()]["at"]["in"] == slot).unwrap();
        &deck["nodes"][id.as_str()]
    }

    /// Setting a deck's sets alike twice, as making does after lint's fixes, changes nothing the
    /// second time: each text is measured with its room, not the cap the first run gave it, so
    /// no figure shrinks another tenth and no title loses a quarter unit.
    #[test]
    fn a_deck_set_alike_twice_is_set_as_once() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        for narrative in [climate(), {
            let (summary, slides) = catalog();
            narrative(&summary, slides)
        }] {
            let built = deck(&narrative, &theme, &Pictures::new()).unwrap();
            let req = Create {
                theme: PathBuf::from("dusk.theme.json"),
                deck: Some(built.deck),
                assets: built.assets,
                ..Create::default()
            };
            let none = |path: &Path| -> Result<Vec<u8>, OpsError> { Err(OpsError::new(path.display().to_string())) };
            let (_, write) = creating(&req, dusk(), &font, &none).unwrap();
            let write = write.unwrap();
            let nothing = scaena_store::Files::Zip(Default::default());
            let view = View { base: &nothing, pending: write.files.clone() };
            let mut doc = write.deck.to_value().unwrap();
            let mut held = Held::default();
            assert!(alike(&mut doc, &view, &mut held).unwrap(), "the first run sets some set alike");
            let once = doc.clone();
            assert!(!alike(&mut doc, &view, &mut held).unwrap(), "the second run changes nothing");
            assert_eq!(doc, once);
        }
    }

    #[test]
    fn a_grown_definition_stands_two_gutters_clear_of_the_next_term() {
        // A glossary's definitions grow to fill their places, and a definition that wraps would end
        // on the next row's term, which reads as its own. The deck made sets them smaller until
        // each stands two of the theme's gutters clear of the text under it (dogfood pass 4).
        let text = serde_json::json!({
            "executiveSummary": "Bayes, plainly.",
            "slides": [
                { "slideTitle": "Bayes, plainly", "subtitle": "A lecture", "layoutSuggestion": "Title",
                  "keyBullets": [], "beatIndex": 0, "beatName": "Hook" },
                { "slideTitle": "Four words we'll use for the rest of the course", "subtitle": "",
                  "layoutSuggestion": "Glossary", "beatIndex": 1, "beatName": "Foundations",
                  "keyBullets": [
                    "Prior: what you believe before the evidence, P(H)",
                    "Likelihood: how probable the evidence is if H is true, P(E | H)",
                    "Evidence: how probable the evidence is overall, P(E)",
                    "Posterior: what you believe after the evidence, P(H | E)"
                  ] },
                { "slideTitle": "Thank you", "subtitle": "", "layoutSuggestion": "Closing", "keyBullets": [],
                  "beatIndex": 2, "beatName": "Close" },
            ]
        })
        .to_string();
        let written = answer::slides(&text).unwrap();
        let n = narrative(&written.executive_summary, written.slides);
        let file = "salon.theme.json";
        let json = std::fs::read_to_string(root().join("docs/examples/themes").join(file)).unwrap();
        let theme: Theme = serde_json::from_str(&json).unwrap();
        let built = deck(&n, &theme, &Pictures::new()).unwrap();
        assert_eq!(built.layouts[1], "glossary");
        let (_, write) = making(built, "Problem\u{2013}Solution", Path::new(file), json.clone(), &font).unwrap();
        let write = write.unwrap();
        let nothing = scaena_store::Files::Zip(Default::default());
        let view = View { base: &nothing, pending: write.files.clone() };
        let doc = write.deck.to_value().unwrap();
        let engine_theme = scaena_engine::theme::Theme::from_json(&json).unwrap();
        let tight = tight(&doc, &write.deck, &view, &engine_theme).unwrap();
        assert!(tight.is_empty(), "{tight:#?}");
    }

    #[test]
    fn a_dividers_title_grown_into_its_subtitle_is_set_smaller_and_its_set_kept_one_size() {
        // Two dividers whose titles may grow to 400: each grows to fill its place, set by its cap
        // box on its foot, and its "p"s, "y"s, and "g"s hang past it into the words under it,
        // which lint E101 finds by their ink. The deck made sets them smaller until it does not,
        // and holds both titles to one size.
        let text = serde_json::json!({
            "executiveSummary": "Typography gets judged.",
            "slides": [
                { "slideTitle": "Typography gets judged", "subtitle": "A talk", "layoutSuggestion": "Title",
                  "keyBullets": [], "beatIndex": 0, "beatName": "Hook" },
                { "slideTitle": "Pay up, quietly", "subtitle": "Spying on typography, yearly",
                  "layoutSuggestion": "Section Header", "keyBullets": [], "beatIndex": 1, "beatName": "Setup" },
                { "slideTitle": "Gripping, playful", "subtitle": "Judging quietly, yearly",
                  "layoutSuggestion": "Section Header", "keyBullets": [], "beatIndex": 2, "beatName": "Turn" },
                { "slideTitle": "Thank you", "subtitle": "", "layoutSuggestion": "Closing", "keyBullets": [],
                  "beatIndex": 3, "beatName": "Close" },
            ]
        })
        .to_string();
        let written = answer::slides(&text).unwrap();
        let n = narrative(&written.executive_summary, written.slides);
        for name in ["dusk", "ember", "salon"] {
            let file = format!("{name}.theme.json");
            let json = std::fs::read_to_string(root().join("docs/examples/themes").join(&file)).unwrap();
            let theme: Theme = serde_json::from_str(&json).unwrap();
            let mut built = deck(&n, &theme, &Pictures::new()).unwrap();
            let titles: Vec<String> = built.deck["states"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|s| s["layout"].as_str().is_some_and(|l| l.starts_with("section")))
                .map(|s| format!("{}-title", s["id"].as_str().unwrap()))
                .collect();
            assert_eq!(titles.len(), 2, "{name}");
            for id in &titles {
                built.deck["nodes"][id]["fit"] = json!("grow");
                built.deck["nodes"][id]["maxSize"] = json!(400);
            }
            let (made, write) = making(built, "Problem\u{2013}Solution", Path::new(&file), json, &font).unwrap();
            let e101: Vec<_> = made.created.findings.iter().filter(|f| f.code == "E101").collect();
            assert!(e101.is_empty(), "{name}: {e101:#?}");
            let write = write.unwrap();
            let mut deck = write.deck.to_value().unwrap();
            let sizes: Vec<f64> = titles.iter().map(|id| deck["nodes"][id]["maxSize"].as_f64().unwrap()).collect();
            assert!(sizes[0] == sizes[1] && sizes[0] < 400.0, "{name}: {sizes:?} {:#}", deck["nodes"][&titles[0]]);
            // The deck made is settled: setting it clear and alike again changes nothing.
            let nothing = scaena_store::Files::Zip(Default::default());
            let view = View { base: &nothing, pending: write.files.clone() };
            let found = lint_in(&write.deck, &view).unwrap().findings;
            let before = deck.clone();
            assert!(!clear(&mut deck, &view, found, &mut Held::default()).unwrap(), "{name}");
            assert_eq!(deck, before, "{name}");
        }
    }

    #[test]
    fn a_big_numbers_tail_is_set_clear_of_the_words_under_it_by_the_themes_gutter() {
        // A figure fills its place, its baseline on the place's foot, set by the place's own fit,
        // and a "y" or a comma's tail hangs below it toward the words under it (I405). The deck made
        // sets it smaller until lint finds the theme's gutter between them: "11 days" over its
        // claim, as a keynote wrote it, in Ember, and "40,000" let grow past the theme's cap.
        let slides = |figure: &str, claim: &str| {
            serde_json::json!({
                "executiveSummary": "A town kept its lights on.",
                "slides": [
                    { "slideTitle": "Port Alder", "subtitle": "A talk", "layoutSuggestion": "Title",
                      "keyBullets": [], "beatIndex": 0, "beatName": "Hook" },
                    { "slideTitle": "For eleven days in October, Port Alder went dark", "subtitle": "",
                      "layoutSuggestion": "Big Number", "keyBullets": [figure, claim], "beatIndex": 1,
                      "beatName": "Call to Adventure" },
                    { "slideTitle": "Thank you", "subtitle": "", "layoutSuggestion": "Closing", "keyBullets": [],
                      "beatIndex": 2, "beatName": "Close" },
                ]
            })
            .to_string()
        };
        let cases = [
            ("ember", "11 days", "without power after Storm Elsa, October 2023", None),
            ("dusk", "40,000", "orders a week wait more than ten days", Some(640)),
            ("ember", "40,000", "orders a week wait more than ten days", Some(640)),
            ("salon", "40,000", "orders a week wait more than ten days", Some(640)),
        ];
        for (name, figure, claim, grown) in cases {
            let written = answer::slides(&slides(figure, claim)).unwrap();
            let n = narrative(&written.executive_summary, written.slides);
            let file = format!("{name}.theme.json");
            let json = std::fs::read_to_string(root().join("docs/examples/themes").join(&file)).unwrap();
            let theme: Theme = serde_json::from_str(&json).unwrap();
            let mut built = deck(&n, &theme, &Pictures::new()).unwrap();
            let state = built.deck["states"][1]["id"].as_str().unwrap().to_string();
            let number = format!("{state}-number");
            assert_eq!(built.deck["nodes"][&number]["text"], figure, "{name}");
            match grown {
                Some(most) => {
                    built.deck["nodes"][&number]["fit"] = json!("grow");
                    built.deck["nodes"][&number]["maxSize"] = json!(most);
                }
                // As made: no fit of its own, its place's.
                None => assert!(built.deck["nodes"][&number].get("fit").is_none(), "{name}"),
            }
            let (made, write) = making(built, "Problem\u{2013}Solution", Path::new(&file), json, &font).unwrap();
            let crowded: Vec<_> =
                made.created.findings.iter().filter(|f| f.code == "I405" || f.code == "E101").collect();
            assert!(crowded.is_empty(), "{name} {figure}: {crowded:#?}");
            let deck = write.unwrap().deck.to_value().unwrap();
            let most = deck["nodes"][&number]["maxSize"].as_f64();
            assert!(
                most.is_some_and(|m| m < f64::from(grown.unwrap_or(400))),
                "{name} {figure}: {:#}",
                deck["nodes"][&number]
            );
            if grown.is_none() {
                assert!(
                    deck["nodes"][&number].get("fit").is_none(),
                    "its place's fit stands: {:#}",
                    deck["nodes"][&number]
                );
            }
        }
    }

    #[test]
    fn three_slides_on_one_of_the_themes_layouts_are_broken_once_they_have_it() {
        // Narrato reads "Bullet List", "Image + Text", and "Bar Chart" as three layouts and breaks
        // no run of them, but with no picture and no data each comes to the theme's bullets, three
        // in a row (lift-up ATTR-2).
        // The middle one takes another layout that sets its items; a layout the person chose
        // stays.
        let list = |title: &str, layout: &str| {
            serde_json::json!({ "slideTitle": title, "subtitle": "", "layoutSuggestion": layout,
              "keyBullets": ["Verification: weeks per container", "Offsets: no one sees the ledger",
                "Shippers: they pay twice"],
              "beatIndex": 1, "beatName": "Problem" })
        };
        let text = serde_json::json!({
            "executiveSummary": "Proof is the product.",
            "slides": [
                { "slideTitle": "Proof is the product", "subtitle": "A talk", "layoutSuggestion": "Title",
                  "keyBullets": [], "beatIndex": 0, "beatName": "Hook" },
                list("Why green freight stalls", "Bullet List"),
                list("What shippers told us", "Image + Text"),
                list("Where the money goes", "Bar Chart"),
                { "slideTitle": "Thank you", "subtitle": "", "layoutSuggestion": "Closing", "keyBullets": [],
                  "beatIndex": 2, "beatName": "Close" },
            ]
        })
        .to_string();
        let written = answer::slides(&text).unwrap();
        let n = narrative(&written.executive_summary, written.slides);
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let unbroken = |made: &Made| (made.layouts.windows(3)).all(|w| !(w[0] == w[1] && w[1] == w[2]));
        let made = deck(&n, &theme, &Pictures::new()).unwrap();
        assert!(unbroken(&made), "{:?}", made.layouts);
        assert_eq!((made.layouts[1].as_str(), made.layouts[3].as_str()), ("bullets", "bullets"), "{:?}", made.layouts);
        assert_eq!(made.layouts[2], "three-columns", "a list of labelled points as columns");
        // Points with no labels and no order have no other layout but a list: they stay bullets,
        // never numbered, which would say they have an order.
        let plain = text
            .replace("Verification: weeks per container", "Verification takes weeks")
            .replace("Offsets: no one sees the ledger", "No one sees the offsets' ledger")
            .replace("Shippers: they pay twice", "Shippers pay twice");
        let written = answer::slides(&plain).unwrap();
        let unlabelled =
            deck(&narrative(&written.executive_summary, written.slides), &theme, &Pictures::new()).unwrap();
        assert_eq!(unlabelled.layouts[1..4], ["bullets", "bullets", "bullets"], "{:?}", unlabelled.layouts);
        // The person's own layout for the middle one is kept, run or no run.
        let chosen: BTreeMap<usize, String> = [(2, "bullets".to_string())].into();
        let kept = deck_as(&n, &theme, &Pictures::new(), &chosen).unwrap();
        assert_eq!(kept.layouts[1..4], ["bullets", "bullets", "bullets"], "{:?}", kept.layouts);
    }

    #[test]
    fn a_text_that_cannot_come_clear_of_what_lies_across_it_is_left_as_it_was() {
        // A picture over the whole of a divider: its title, grown to its place, collides with it
        // however small it is set, the collision as large for its size. One step shows it, and
        // the title keeps its size.
        let text = serde_json::json!({
            "executiveSummary": "Typography gets judged.",
            "slides": [
                { "slideTitle": "Typography gets judged", "subtitle": "A talk", "layoutSuggestion": "Title",
                  "keyBullets": [], "beatIndex": 0, "beatName": "Hook" },
                { "slideTitle": "Pay up, quietly", "subtitle": "Spying on typography",
                  "layoutSuggestion": "Section Header", "keyBullets": [], "beatIndex": 1, "beatName": "Setup" },
            ]
        })
        .to_string();
        let written = answer::slides(&text).unwrap();
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let built = deck(&narrative(&written.executive_summary, written.slides), &theme, &Pictures::new()).unwrap();
        let (_, write) = making(built, "Problem\u{2013}Solution", Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        let mut write = write.unwrap();
        let mut deck = write.deck.to_value().unwrap();
        let state = deck["states"][1]["id"].as_str().unwrap().to_string();
        let title = format!("{state}-title");
        deck["nodes"][&title]["fit"] = json!("grow");
        deck["nodes"][&title]["maxSize"] = json!(200);
        deck["nodes"]["over"] =
            json!({ "type": "image", "src": "assets/over.jpg", "at": { "rect": [0, 0, 1920, 1080] } });
        deck["states"][1]["props"]["over"] = json!({});
        let jpeg = std::fs::read(root().join("tests/fixtures/jpeg/baseline-422.jpg")).unwrap();
        write.files.insert("assets/over.jpg".into(), jpeg);
        let nothing = scaena_store::Files::Zip(Default::default());
        let view = View { base: &nothing, pending: write.files.clone() };
        let found = lint_in(&Deck::from_json(&deck.to_string()).unwrap(), &view).unwrap().findings;
        assert!(found.iter().any(|f| f.code == "E101" && f.message.contains(&title)), "{found:#?}");
        let before = deck.clone();
        let mut held = Held::default();
        assert!(!clear(&mut deck, &view, found, &mut held).unwrap());
        assert_eq!(deck["nodes"][&title], before["nodes"][&title]);
        assert_eq!(deck, before);
    }

    #[test]
    fn a_set_of_texts_side_by_side_is_set_alike_and_as_large_as_its_places_allow() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        // The climate talk with a picture on its cover and on each of its dividers.
        let pictures: Pictures = [0, 2, 7, 11, 15, 21]
            .into_iter()
            .map(|i| (i, Placed { src: format!("assets/picture-{i}.jpg"), alt: "A picture".into() }))
            .collect();
        let mut built = deck(&climate(), &theme, &pictures).unwrap();
        let jpeg = std::fs::read(root().join("tests/fixtures/jpeg/baseline-422.jpg")).unwrap();
        for p in pictures.values() {
            built.assets.insert(p.src.clone(), jpeg.clone());
        }
        let (made, write) = making(built, "Data Storytelling", Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        assert!(made.created.created && made.created.errors == 0, "{:#?}", made.created.findings);
        let deck = write.unwrap().deck.to_value().unwrap();
        let size = |role: &str| theme.typography.roles[role].size;
        let node = |state: usize, slot: &str| {
            let id = deck["states"][state]["id"].as_str().unwrap();
            deck["nodes"][format!("{id}-{slot}")].clone()
        };
        let most = |n: &Value| n["maxSize"].as_f64().unwrap();
        let ordinals = ["first", "second", "third", "fourth"];
        // Four steps, each on a card under its number, their words at one size: larger than the
        // body's, the lede's at most.
        assert_eq!(made.layouts[19], "numbered-cards");
        let steps: Vec<f64> = ordinals.iter().map(|o| most(&node(19, o))).collect();
        assert!(
            steps.iter().all(|s| *s == steps[0]) && steps[0] > size("body") && steps[0] <= size("lede"),
            "{steps:?}"
        );
        // Four figures too wide for a quarter of the slide, "4.5 mm/yr" among them, in a square:
        // each on its one line, alike, held under where the widest filled its half of the slide,
        // the numeral's at most, each named under it and its note under that (G.16).
        assert_eq!(made.layouts[6], "four-numbers");
        let figures: Vec<Value> = ordinals.iter().map(|o| node(6, &format!("{o}-figure"))).collect();
        assert!(figures.iter().all(|f| f["maxLines"] == 1 && f["maxSize"] == figures[0]["maxSize"]), "{figures:#?}");
        assert!(most(&figures[0]) > size("figure") * 1.5 && most(&figures[0]) < size("numeral"), "{figures:#?}");
        let square = &theme.layouts["four-numbers"].slots;
        let rows = |slot: &str| match &square[slot].row {
            Some(Range::Span([from, to])) => (*from, *to),
            Some(Range::Index(row)) => (*row, *row),
            None => (0, 0),
        };
        assert_eq!(rows("first-figure"), rows("second-figure"));
        assert!(rows("first-figure").1 < rows("first-title").0 && rows("first-title").1 < rows("first").0);
        assert!(rows("first").1 < rows("third-figure").0);
        assert!(node(6, "first-title")["align"].is_null());
        // Each slide builds once its transition ends (G.17): its four cards rise one after another,
        // a quarter second and more apart, each card's number, title, and words together; its
        // bars grow from their baseline; and the cover comes in whole. Lint finds no more moving
        // at once than the theme allows, nor a build longer than its limit.
        let cards = deck["states"][19]["choreography"].as_array().unwrap();
        let delays: Vec<f64> = cards.iter().map(|c| c["delay"].as_f64().unwrap_or(0.0)).collect();
        assert_eq!(delays, [0.0, 252.0, 504.0, 756.0]);
        assert!(cards.iter().all(|c| c["enter"] == "rise"), "{cards:#?}");
        let first: Vec<&str> = cards[0]["target"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert!(first.len() >= 3 && first.iter().all(|t| t.contains("-first")), "{first:?}");
        let bars = deck["states"][8]["id"].as_str().unwrap();
        assert_eq!(deck["states"][8]["choreography"], json!([{ "target": format!("{bars}-main"), "enter": "grow" }]));
        assert!(deck["states"][0].get("choreography").is_none());
        assert!(!made.created.findings.iter().any(|f| f.code.starts_with("W32")), "{:#?}", made.created.findings);
        // Bars of names with no order of their own run longest first; years keep theirs.
        assert_eq!(node(8, "main")["dataTransform"], json!([{ "sort": "-value" }]));
        assert!(node(16, "main")["dataTransform"].is_null());
        // Two bars are two figures, grown together to one size, past a figure's (G.15).
        assert_eq!(made.layouts[18], "two-figures");
        assert_eq!(node(18, "first-figure")["maxSize"], node(18, "second-figure")["maxSize"]);
        assert!(most(&node(18, "first-figure")) > size("figure"), "{:#?}", node(18, "first-figure"));
        // Two sides said in a line each: a card each, the statements set large as titles.
        assert_eq!(made.layouts[20], "myth-fact");
        assert_eq!(node(20, "first")["role"], "title");
        assert_eq!(node(20, "first")["maxSize"], node(20, "second")["maxSize"]);
        assert!(most(&node(20, "first")) > size("title"));
        // Every divider's title at one size, past the headline's and the display's at most.
        let dividers = [2, 7, 11, 15, 21];
        let titles: Vec<f64> = dividers.iter().map(|i| most(&node(*i, "title"))).collect();
        assert!(dividers.iter().all(|i| made.layouts[*i] == "section-photo"), "{:?}", made.layouts);
        assert!(titles.iter().all(|t| *t == titles[0]), "{titles:?}");
        assert!(titles[0] > size("headline") && titles[0] <= size("display"), "{titles:?}");
        // A quote that no headline heads is the slide: it grows past the headline's size,
        // toward the display's.
        let quote = most(&node(10, "quote"));
        assert!(quote > size("headline") && quote <= size("display"), "{quote}");
        // The cover's title grows past the headline's size beside its picture, on two lines at
        // most; a divider's of a few words on one.
        assert_eq!(node(0, "title")["maxLines"], 2);
        assert_eq!((node(2, "title")["maxLines"].as_u64(), node(21, "title")["maxLines"].as_u64()), (Some(1), Some(2)));
        assert_eq!(made.layouts[0], "title-split");
        assert!(most(&node(0, "title")) > size("headline") && most(&node(0, "title")) <= size("display"));
        // What each figure counts, said in words under it, alike, past the body's size and the
        // lede's at most.
        let labels: Vec<f64> = ordinals.iter().map(|o| most(&node(6, &format!("{o}-title")))).collect();
        assert!(
            labels.iter().all(|l| *l == labels[0]) && labels[0] > size("body") && labels[0] <= size("lede"),
            "{labels:?}"
        );
        // Four years of records, each told in a sentence, go down the slide, each beside its year,
        // its words set as a typesetter sets them (G.16).
        assert_eq!(made.layouts[13], "milestones");
        assert_eq!(node(13, "third-date")["text"], "2023");
        assert_eq!(node(13, "third")["text"], "Canada\u{2019}s wildfires burn 18 million hectares");
        // A side's statement stands in the middle of its card, on as many as three lines, past
        // three-quarters of the slide's headline and under its size, so that each fills its card
        // and the headline still leads: the history's long one too (G.20).
        assert_eq!(node(20, "first")["align"], json!({ "y": "center" }));
        assert_eq!(made.layouts[9], "myth-fact");
        for side in [node(9, "first"), node(20, "first")] {
            assert_eq!(side["maxLines"], 3);
            assert!(most(&side) > size("headline") * 0.75 && most(&side) < size("headline"), "{side:#?}");
        }
        // A divider's title stands on what supports it, however many lines it takes (G.20).
        let layouts: Value = serde_json::from_str(&dusk()).unwrap();
        for divider in ["section", "section-photo"] {
            assert_eq!(layouts["layouts"][divider]["slots"]["title"]["align"]["y"], "end", "{divider}");
        }
        // A head-to-head's sides as figures, what they count said with the row's criterion.
        assert_eq!(made.layouts[12], "head-to-head");
        assert_eq!(node(12, "first-criterion")["text"], "Extreme heat: people exposed every five years");
        assert_eq!(
            (node(12, "first-left")["text"].as_str(), node(12, "first-right")["text"].as_str()),
            (Some("14%"), Some("37%"))
        );
        assert!(most(&node(12, "first-criterion")) > size("label"));
        // A slide's foot names the part of the talk it is in; a divider's and the cover's do not.
        // It navigates, so it is not among the words a slide's density counts: the next steps'
        // forty are not forty-four.
        assert_eq!(node(6, "kicker")["text"], "The Signal");
        assert_eq!(node(22, "kicker")["text"], "What We Can Do");
        assert_eq!(node(22, "kicker")["semantic"], "navigation");
        assert_eq!(node(22, "kicker")["align"], json!({ "y": "end" }));
        assert!(!made.created.findings.iter().any(|f| f.code == "W210"), "{:#?}", made.created.findings);
        assert!(node(7, "kicker").is_null() && node(0, "kicker").is_null() && node(24, "kicker").is_null());
    }

    /// A slide's subtitle, where its layout has no place for one, goes in the note at its foot when
    /// the slide has no source or fact for it there; a fact stays, never stacked with it; and a
    /// chart's stays on its value axis, out of the note (lift-up dogfood).
    #[test]
    fn a_note_takes_the_subtitle_where_the_layout_has_no_place_for_it() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let mut n = climate();
        let i = n.slides.iter().position(|s| s.chart_data.is_none() && s.layout_suggestion != "title").unwrap();
        n.slides[i].supporting_facts = None;
        n.slides[i].subtitle = "Ten years of readings, by station".into();
        let forced = BTreeMap::from([(i, "table".to_string())]);
        let note = |n: &Narrative, forced: &BTreeMap<usize, String>, at: usize| {
            let made = deck_as(n, &theme, &Pictures::new(), forced).unwrap();
            let deck = &made.deck;
            let props = deck["states"][at]["props"].as_object().unwrap();
            let id = props.keys().find(|id| deck["nodes"][id.as_str()]["at"]["in"] == "note")?;
            deck["nodes"][id.as_str()].get("text").cloned()
        };
        assert_eq!(note(&n, &forced, i), Some(json!("Ten years of readings, by station")));
        n.slides[i].supporting_facts = Some("NOAA, 2015\u{2013}2024".into());
        assert_eq!(note(&n, &forced, i), Some(json!("NOAA, 2015\u{2013}2024")), "the fact stays");
        // A chart's subtitle is its value axis's, and its note keeps to its source or its fact.
        let c = n.slides.iter().position(|s| s.chart_data.is_some()).unwrap();
        n.slides[c].supporting_facts = None;
        n.slides[c].chart_data.as_mut().unwrap().source_id = None;
        n.slides[c].subtitle = "Gigatonnes a year".into();
        let figure = BTreeMap::from([(c, "figure".to_string())]);
        assert_eq!(note(&n, &figure, c), None, "a chart's note takes no subtitle");
    }

    /// A chart makes its claim (dogfood pass, charts): a line of several series lights the one its
    /// headline names, as a whole word; its value axis says what it plots from the subtitle, with
    /// the unit, where the subtitle is short enough to; its caption says where it comes from, a
    /// line of its own under a line's takeaway, and never what the axis says; and every slide keeps
    /// its subtitle with the narrative.
    #[test]
    fn a_chart_makes_its_claim_and_says_what_it_shows() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let chart = |series: Value, unit: &str| json!({ "categories": ["Q4", "Q1", "Q2", "Q3"], "series": series, "unit": unit, "sourceId": "Churn dashboard" });
        let slides = vec![
            answer::normalize(&json!({ "slideTitle": "Q3 review", "layoutSuggestion": "Title" })),
            answer::normalize(&json!({
                "slideTitle": "SMB churn climbed to 4.9% while mid-market churn fell",
                "subtitle": "Quarterly logo churn by segment, %",
                "layoutSuggestion": "Line Chart",
                "keyBullets": ["SMB churn has risen four quarters in a row"],
                "chartData": chart(json!([
                    { "name": "Mid-market", "values": [3.1, 2.9, 2.6, 2.4] },
                    { "name": "SMB", "values": [3.8, 4.1, 4.5, 4.9] },
                ]), "%"),
            })),
            answer::normalize(&json!({
                "slideTitle": "ARR grew fivefold this year to $910K",
                "subtitle": "Annual recurring revenue by month, $K",
                "layoutSuggestion": "Line Chart",
                "keyBullets": ["ARR reached $910K in September, up 5\u{d7} since January"],
                "chartData": chart(json!([{ "name": "ARR", "values": [180, 420, 700, 910] }]), "$K"),
            })),
            answer::normalize(&json!({
                "slideTitle": "Three questions make up most of every queue",
                "subtitle": "Share of tickets by topic, 1,140 teams",
                "layoutSuggestion": "Bar Chart",
                "keyBullets": ["Order status, login, and billing are 62% of all tickets"],
                "chartData": {
                    "categories": ["Order status", "Login", "Billing", "Other"],
                    "series": [{ "name": "Share of tickets", "values": [28, 19, 15, 38] }],
                    "unit": "%", "sourceId": "Relay ticket benchmark",
                },
            })),
            answer::normalize(&json!({ "slideTitle": "Thank you", "layoutSuggestion": "Closing" })),
        ];
        let made = deck(&narrative("A review.", slides.clone()), &theme, &Pictures::new()).unwrap();
        let firsts: Vec<&str> = (made.deck["states"].as_array().unwrap().iter())
            .filter(|s| s.get("slide").is_none())
            .map(|s| s["id"].as_str().unwrap())
            .collect();
        let node = |i: usize, slot: &str| made.deck["nodes"][format!("{}-{slot}", firsts[i])].clone();
        assert_eq!(made.layouts[1..4], ["line-chart", "line-chart", "bar-chart"]);
        // Two series: SMB's is lit, the one the headline is about, and the axis says what both plot.
        let churn = node(1, "main");
        assert_eq!(churn["annotations"], json!([{ "kind": "highlight", "at": { "series": "SMB" } }]));
        assert_eq!(churn["y"]["title"], "Quarterly logo churn by segment, %");
        // The takeaway alone in the note, which grows to its size, and the source in its own place
        // (dogfood pass 4).
        assert_eq!(node(1, "note")["text"], "SMB churn has risen four quarters in a row");
        assert_eq!(node(1, "source")["text"], "Source: Churn dashboard");
        assert_eq!(node(1, "source")["semantic"], "source");
        // One series: the subtitle titles it too.
        assert_eq!(node(2, "main")["y"]["title"], "Annual recurring revenue by month, $K");
        assert_eq!(node(2, "note")["text"], "ARR reached $910K in September, up 5\u{d7} since January");
        assert_eq!(node(2, "source")["text"], "Source: Churn dashboard");
        // A bar chart's axis takes the unit its subtitle lacks, and its caption is its source.
        assert_eq!(node(3, "main")["y"]["title"], "Share of tickets by topic, 1,140 teams (%)");
        assert_eq!(node(3, "note")["text"], "Source: Relay ticket benchmark");
        // Every subtitle is kept with the narrative, whether or not a place of its layout sets it.
        let beats: Vec<&Value> = (made.deck["spine"]["sections"].as_array().unwrap().iter())
            .flat_map(|s| s["beats"].as_array().unwrap())
            .collect();
        assert_eq!(beats[3]["media"]["narrato"]["subtitle"], "Share of tickets by topic, 1,140 teams");
        assert!(beats[0]["media"]["narrato"].get("subtitle").is_none(), "the cover has none");
        // A bundle's older copy of the theme, its line chart with no place for a source: the source
        // stands under the takeaway in the caption's role.
        let mut older = theme.clone();
        older.layouts.get_mut("line-chart").unwrap().slots.shift_remove("source");
        let made = deck(&narrative("A review.", slides), &older, &Pictures::new()).unwrap();
        let first = (made.deck["states"].as_array().unwrap().iter()).filter(|s| s.get("slide").is_none()).nth(1);
        let note = &made.deck["nodes"][format!("{}-note", first.unwrap()["id"].as_str().unwrap())];
        assert_eq!(
            note["runs"],
            json!([
                { "text": "SMB churn has risen four quarters in a row" },
                { "text": "\nSource: Churn dashboard", "role": "caption" },
            ])
        );
    }

    /// A line labels the figure its headline names where its ends do not (dogfood pass 4), and an
    /// axis titled by a subtitle that names the unit takes it no second time.
    #[test]
    fn a_line_labels_the_figure_its_headline_names() {
        let chart: ChartData = serde_json::from_value(json!({
            "categories": ["2021", "2022", "2023", "2024"],
            "series": [{ "name": "Outage hours", "values": [24, 286, 40, 2] }],
            "unit": "hours",
        }))
        .unwrap();
        let said = |title: &str| named_figure(&chart, None, [Some(title), None]);
        assert_eq!(
            said("Outage hours fell from 286 to 2 in three years"),
            Some(json!({ "kind": "callout", "at": { "x": "2022" }, "text": "286" }))
        );
        assert_eq!(said("Outage hours fell to 2 by 2024"), None, "an end is labelled already, a year is no figure");
        assert_eq!(figures("$1,140 and 4.9%, in 2024."), [1140.0, 4.9, 2024.0]);
        // A takeaway that names the headline's first and last figures again gives way; one that
        // adds to them, or a headline of one figure, keeps it.
        let headline = "Outage hours fell from 286 to 2 in three years";
        assert!(restates(headline, "Outage hours fell to 2 in 2026, after 286 in the storm year"));
        assert!(!restates(headline, "Outage hours fell to 2 in 2026, the lowest in the county"));
        assert!(!restates("ARR reached $910K", "ARR reached $910K in September"));
        // Where it gives way, the latest figure is said from the data: what it counts, and when.
        let outages: ChartData = serde_json::from_value(json!({
            "categories": ["2021", "2026"], "series": [{ "name": "Outage hours", "values": [24, 2] }], "unit": "hours",
        }))
        .unwrap();
        let said = |subtitle: Option<&str>| counted(&outages, subtitle.map(String::from));
        assert_eq!(said(Some("Hours without power per year, Port Alder")).as_deref(), Some("without power in 2026"));
        assert_eq!(said(None).as_deref(), Some("outage hours in 2026"));
        assert_eq!((grouped(1140.0, 0), grouped(-2.5, 1)), ("1,140".to_string(), "\u{2212}2.5".to_string()));
        let node = chart_node(
            &chart,
            "d",
            "line-chart",
            ["Outage hours fell from 286 to 2", ""],
            Some("Hours without power per year, Port Alder"),
            "en",
            json!({}),
        );
        assert_eq!(node["y"]["title"], "Hours without power per year, Port Alder");
        assert_eq!(node["annotations"][1]["text"], "286", "{node:#}");
    }

    /// A series is named as a whole word: "US" is not in "business", nor "EU" in "neutral".
    #[test]
    fn a_series_is_named_as_a_whole_word() {
        let chart: ChartData = serde_json::from_value(json!({
            "categories": ["2023", "2024"],
            "series": [{ "name": "US", "values": [1, 2] }, { "name": "EU", "values": [2, 3] }],
        }))
        .unwrap();
        assert_eq!(named_series(&chart, [Some("Business users grew"), Some("neutral, maintained")]), None);
        assert_eq!(named_series(&chart, [Some("Business users grew in the EU"), None]).as_deref(), Some("EU"));
        assert_eq!(named_series(&chart, [Some("The US, then the EU"), None]).as_deref(), Some("US"));
        // A subtitle too long for an axis titles none.
        let long = "Share of overbilled dollars by error type, across 38 pilot shippers and four quarters";
        assert_eq!((on_axis(Some(long)), on_axis(Some(" Churn, % "))), (None, Some("Churn, %")));
    }

    /// A slide's foot names its part of the talk only while the talk is in the beat its divider
    /// opened, and a divider stands by the number of the agenda item it opens; a deck with one
    /// divider, or no agenda, numbers none (narrative wayfinding).
    #[test]
    fn a_running_head_ends_with_its_part_and_a_divider_counts_by_the_agenda() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let slide = |layout: &str, title: &str, beat: usize, bullets: &[&str]| {
            answer::normalize(&json!({
                "slideTitle": title, "layoutSuggestion": layout, "beatIndex": beat, "keyBullets": bullets,
                "speakerNotes": "Say it plainly.",
            }))
        };
        let agenda =
            ["Context: why it matters", "Method: how we work", "Check yourself: three questions", "Transfer: use it"];
        let body = ["One: a point", "Two: another point", "Three: a last point"];
        let made = |slides: Vec<Slide>| {
            let built = deck(&narrative("A lecture.", slides), &theme, &Pictures::new()).unwrap();
            let firsts: Vec<String> = (built.deck["states"].as_array().unwrap().iter())
                .filter(|s| s.get("slide").is_none())
                .map(|s| s["id"].as_str().unwrap().to_string())
                .collect();
            (built, firsts)
        };
        let text =
            |built: &Made, state: &str, slot: &str| built.deck["nodes"][format!("{state}-{slot}")]["text"].clone();
        // One divider, the agenda's third part: no number, and the slides after its beat say no part.
        let (built, s) = made(vec![
            slide("Title", "Learning to learn", 0, &[]),
            slide("Agenda", "Today", 0, &agenda),
            slide("Bullet List", "Why it matters", 1, &body),
            slide("Section Header", "Check yourself", 2, &[]),
            slide("Bullet List", "Three questions to ask", 2, &body),
            slide("Bullet List", "Use it tomorrow", 3, &body),
            slide("Closing", "Thank you", 4, &[]),
        ]);
        assert!(built.layouts[3].starts_with("section"), "{:?}", built.layouts);
        assert!(text(&built, &s[3], "number").is_null(), "one divider counts nothing");
        assert_eq!(text(&built, &s[4], "kicker"), "Check yourself");
        assert!(text(&built, &s[5], "kicker").is_null(), "the talk has moved on: {}", text(&built, &s[5], "kicker"));
        // Two dividers, the agenda's second and third parts: numbered so.
        let (built, s) = made(vec![
            slide("Title", "Learning to learn", 0, &[]),
            slide("Agenda", "Today", 0, &agenda),
            slide("Section Header", "How we work", 1, &[]),
            slide("Bullet List", "Three habits", 1, &body),
            slide("Section Header", "Check yourself", 2, &[]),
            slide("Bullet List", "Three questions to ask", 2, &body),
            slide("Closing", "Thank you", 3, &[]),
        ]);
        assert_eq!((text(&built, &s[2], "number"), text(&built, &s[4], "number")), (json!("02"), json!("03")));
        assert_eq!(text(&built, &s[3], "kicker"), "How we work");
        // With no agenda, no divider is numbered.
        let (built, s) = made(vec![
            slide("Title", "Learning to learn", 0, &[]),
            slide("Section Header", "How we work", 1, &[]),
            slide("Bullet List", "Three habits", 1, &body),
            slide("Section Header", "Check yourself", 2, &[]),
            slide("Closing", "Thank you", 3, &[]),
        ]);
        assert!(text(&built, &s[1], "number").is_null() && text(&built, &s[3], "number").is_null());
    }

    #[test]
    fn a_talk_on_climate_change_reads_as_it_was_written() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let made = deck(&climate(), &theme, &Pictures::new()).unwrap();
        let said = |n: usize| {
            let state = made.deck["states"][n]["id"].as_str().unwrap().to_string();
            shown(&made.deck, &state)
        };
        let has = |said: &[(String, String)], slot: &str, words: &str| {
            assert!(said.iter().any(|(s, w)| s == slot && w == words), "{slot} = {words:?} in {said:#?}");
        };
        // Every divider the model asked for stays one, numbered in order.
        assert_eq!(made.layouts.iter().filter(|l| *l == "section").count(), 5, "{:?}", made.layouts);
        has(&said(21), "number", "05");
        // A figure with its unit is the figure, never a year among its words.
        let vital = said(6);
        has(&vital, "first-figure", "4.5 mm/yr");
        has(&vital, "first-title", "Sea level rise");
        has(&vital, "first", "Double the 1990s rate");
        has(&vital, "second-figure", "\u{2212}13%");
        // A big number says what it means beside it, and its source under it.
        let hottest = said(3);
        has(&hottest, "number", "1.55\u{b0}C");
        has(&hottest, "claim", "Above pre-industrial, the first calendar year past 1.5\u{b0}C");
        has(&hottest, "under", "WMO State of the Global Climate 2024");
        // A line chart's callout is its latest figure, and its takeaway stands under it.
        let co2 = said(5);
        has(&co2, "callout", "424 ppm");
        has(&co2, "note", "From 280 ppm before industry to 424 ppm in 2024");
        has(&said(4), "callout", "1.55\u{b0}C");
        // A line along years runs on time, 2010 to 2024 as long as fourteen years are (G.18); bars
        // of years stay a step apart.
        let warming = placed(&made.deck, 4, "main");
        assert_eq!(warming["x"], json!({ "field": "category", "type": "temporal" }));
        let source = warming["data"].as_str().unwrap().trim_start_matches('@');
        assert_eq!(made.deck["data"][source]["schema"]["category"], "date");
        assert_eq!(placed(&made.deck, 16, "main")["x"]["type"], "ordinal");
        // A line of one series is drawn in the signal color, the year its headline names marked:
        // "Since 1980", a band from it to the last year. Its figure grows as large as its column
        // takes, to a scorecard's, and the words under it to the body's (G.19).
        assert_eq!(
            warming["annotations"],
            json!([
                { "kind": "highlight", "at": {} },
                { "kind": "band", "at": { "x": ["1980", "2024"] }, "text": "Since 1980" },
            ])
        );
        let carbon = placed(&made.deck, 5, "main")["annotations"].as_array().unwrap();
        assert_eq!(carbon.len(), 1, "a headline that names no year in its span marks none: {carbon:?}");
        assert_eq!(carbon[0]["kind"], "highlight");
        let callout = placed(&made.deck, 4, "callout");
        assert_eq!(
            (&callout["fit"], &callout["maxSize"], &callout["maxLines"]),
            (&json!("grow"), &json!(112.0), &json!(1))
        );
        let note = placed(&made.deck, 4, "note");
        assert_eq!((&note["fit"], &note["maxSize"]), (&json!("grow"), &json!(32.0)));
        // A head-to-head reads "A versus B", and each side's answer: where one side is a bare
        // figure, the other's is too, and what both count goes with the criterion.
        let half = said(12);
        has(&half, "first-title", "1.5\u{b0}C");
        has(&half, "second-title", "2\u{b0}C");
        has(&half, "first-criterion", "Extreme heat: people exposed every five years");
        has(&half, "first-left", "14%");
        has(&half, "first-right", "37%");
        has(&half, "second-left", "70\u{2013}90% lost");
        has(&half, "third-right", "Once a decade");
        // Long category names run down the side; every value has one bar's decimals.
        let energy = placed(&made.deck, 8, "main");
        assert_eq!(energy["orient"], "horizontal");
        assert_eq!(energy["y"]["format"], ",.1f");
        // The bar the slide is about takes the signal color: the year last, or the one category
        // the takeaway names. Two bars are two figures (G.15), each in the unit at one precision,
        // the one the slide is not about standing back.
        assert_eq!(placed(&made.deck, 16, "main")["annotations"][0]["at"]["x"], "2023");
        assert_eq!(made.layouts[18], "two-figures");
        let invest = said(18);
        has(&invest, "first-figure", "$2.0T");
        has(&invest, "second-figure", "$1.1T");
        has(&invest, "first", "Clean energy");
        has(&invest, "note", "Global energy investment in 2024, trillions of dollars");
        assert!(placed(&made.deck, 18, "first-figure").get("style").is_none());
        assert_eq!(placed(&made.deck, 18, "second-figure")["style"], json!({ "color": "onSurfaceMuted" }));
        // With no picture made, a slide waits for none: no theme art stands in for one.
        assert!(
            made.deck["nodes"].as_object().unwrap().values().all(|n| n["type"] != "shader" && n["type"] != "image")
        );
        assert_eq!((made.layouts[0].as_str(), made.layouts[20].as_str()), ("title-center", "myth-fact"));
        // Four next steps written as labelled items are numbered cards; a Q&A stays one.
        assert_eq!((made.layouts[22].as_str(), made.layouts[23].as_str()), ("numbered-cards", "qa"));
        assert_eq!(made.layouts[19], "numbered-cards");
    }

    /// A made deck reads to someone who hears it as it reads to someone who sees it (dogfood 2):
    /// each item's figure, title, and words together, the running head after them; a chart said
    /// from its data, not by its headline again; the deck in English where no language is named.
    #[test]
    fn a_made_deck_reads_item_by_item_its_charts_by_their_data() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let made = deck(&climate(), &theme, &Pictures::new()).unwrap();
        assert_eq!(made.deck["meta"]["lang"], "en");
        // The vital signs: four items, each a figure, a title, and a line, read in turn.
        let order: Vec<String> = shown(&made.deck, made.deck["states"][6]["id"].as_str().unwrap())
            .into_iter()
            .map(|(slot, _)| slot)
            .collect();
        let items: Vec<&str> =
            order.iter().filter(|s| ordinal(s).is_some()).map(|s| ORDINALS[ordinal(s).unwrap().0]).collect();
        assert!(
            items
                .windows(2)
                .all(|w| ORDINALS.iter().position(|o| *o == w[0]) <= ORDINALS.iter().position(|o| *o == w[1])),
            "item by item: {order:?}"
        );
        assert_eq!(
            order.iter().position(|s| s == "first-figure").map(|i| &order[i..i + 3]),
            Some(&["first-figure".to_string(), "first-title".into(), "first".into()][..]),
            "{order:?}"
        );
        // The running head, at the slide's foot, is read after what the slide says.
        assert_eq!(placed(&made.deck, 6, "kicker")["align"], json!({ "y": "end" }));
        assert_eq!(order.last().map(String::as_str), Some("kicker"), "{order:?}");
        // A chart is described by its data: its kind and what it counts, then its values where
        // they are few, else its highest and lowest; never by its slide's headline.
        for n in [4, 5, 8, 16] {
            let chart = placed(&made.deck, n, "main");
            let alt = chart["alt"].as_str().unwrap();
            let title = &climate().slides[n].slide_title;
            assert_ne!(alt, typeset_en(title), "state {n}");
            let kind = if chart["kind"] == "line" { "Line chart" } else { "Bar chart" };
            assert!(alt.starts_with(kind), "state {n}: {alt}");
        }
        let energy = placed(&made.deck, 8, "main")["alt"].as_str().unwrap().to_string();
        let data = climate().slides[8].chart_data.clone().unwrap();
        assert!(
            data.categories.iter().all(|c| energy.contains(c.as_str())),
            "every value said where they are few: {energy}"
        );
        let warming = placed(&made.deck, 4, "main")["alt"].as_str().unwrap().to_string();
        // What it counts is what its axis says: its subtitle (dogfood: charts).
        assert!(
            warming.starts_with("Line chart: Global temperature anomaly, \u{b0}C vs 1850\u{2013}1900. 1880 "),
            "{warming}"
        );
        // More than a dozen values: each series' highest and lowest.
        let months = ChartData {
            categories: (1..=13).map(|m| format!("M{m}")).collect(),
            series: vec![
                answer::Series { name: Some("North".into()), values: (1..=13).map(f64::from).collect() },
                answer::Series { name: Some("South".into()), values: (1..=13).map(|v| f64::from(20 - v)).collect() },
            ],
            unit: None,
            source_id: None,
        };
        assert_eq!(
            described(&months, "bar", None, "en"),
            "Bar chart: North, South. North: highest M13 13, lowest M1 1; South: highest M1 19, lowest M13 7"
        );
    }

    /// An email or a web address in a made deck's words links to it (PLAN 2.70, dogfood 2); a word
    /// with a dot in it that is neither does not.
    #[test]
    fn contact_lines_link_their_emails_and_addresses() {
        assert_eq!(
            linked("priya@manifest.dev \u{b7} manifest.dev", true),
            Some(vec![
                json!({ "text": "priya@manifest.dev", "link": { "href": "mailto:priya@manifest.dev" } }),
                json!({ "text": " \u{b7} " }),
                json!({ "text": "manifest.dev", "link": { "href": "https://manifest.dev" } }),
            ])
        );
        assert_eq!(
            linked("Read the playbook (portalderpower.org/playbook).", true),
            Some(vec![
                json!({ "text": "Read the playbook (" }),
                json!({ "text": "portalderpower.org/playbook", "link": { "href": "https://portalderpower.org/playbook" } }),
                json!({ "text": ")." }),
            ])
        );
        assert_eq!(
            linked("See https://example.com/a for more", true),
            Some(vec![
                json!({ "text": "See " }),
                json!({ "text": "https://example.com/a", "link": { "href": "https://example.com/a" } }),
                json!({ "text": " for more" }),
            ])
        );
        for plain in
            ["Built on Node.js 3.5, e.g. U.S. ports", "A 4.2% rise", "Ask @team", "Revenue: $4.1M", "Built on ASP.NET"]
        {
            assert_eq!(linked(plain, true), None, "{plain}");
        }
        // A heading's bare domain names a firm; an address with `www.` or a scheme links anywhere.
        assert_eq!(linked("Booking.com books more nights", false), None);
        assert_eq!(linked("Sign up at relay.app", false), None);
        assert_eq!(
            linked("Sign up at relay.app", true).unwrap()[1],
            json!({ "text": "relay.app", "link": { "href": "https://relay.app" } })
        );
        assert_eq!(
            linked("Visit www.relay.app now", false).unwrap()[1],
            json!({ "text": "www.relay.app", "link": { "href": "https://www.relay.app" } })
        );
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let mut talk = climate();
        let last = talk.slides.len() - 1;
        talk.slides[last].subtitle = "priya@manifest.dev \u{b7} manifest.dev".into();
        let made = deck(&talk, &theme, &Pictures::new()).unwrap();
        let runs: Vec<&Value> =
            made.deck["nodes"].as_object().unwrap().values().filter_map(|n| n["runs"].as_array()).flatten().collect();
        let hrefs: Vec<&str> = runs.iter().filter_map(|r| r["link"]["href"].as_str()).collect();
        assert_eq!(hrefs, ["mailto:priya@manifest.dev", "https://manifest.dev"], "{runs:?}");
        assert!(
            made.deck["nodes"]
                .as_object()
                .unwrap()
                .values()
                .all(|n| n.get("runs").is_none() || n.get("text").is_none())
        );
    }

    /// A deck in another language keeps no English word of the theme's or the generator's own: no
    /// lane named "Does", a source said "Fuente" (dogfood 2).
    #[test]
    fn a_deck_in_another_language_keeps_no_english_word_of_its_own() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let mut talk = climate();
        talk.brief.language = Some("Spanish".into());
        talk.slides[4].chart_data.as_mut().unwrap().source_id = Some("NASA GISS".into());
        let journey = 9;
        talk.slides[journey].key_bullets.push("Feels the cost first".into());
        let layouts = BTreeMap::from([(journey, "journey".to_string())]);
        let made = deck_as(&talk, &theme, &Pictures::new(), &layouts).unwrap();
        assert_eq!(made.layouts[journey], "journey");
        assert_eq!(made.deck["meta"]["lang"], "es");
        let said: Vec<String> = shown(&made.deck, made.deck["states"][journey]["id"].as_str().unwrap())
            .into_iter()
            .map(|(_, w)| w)
            .collect();
        for english in ["Does", "Thinks", "Feels"] {
            assert!(!said.iter().any(|w| w == english), "{english} in {said:?}");
        }
        for spanish in ["Hace", "Piensa", "Siente"] {
            assert!(said.iter().any(|w| w == spanish), "{spanish} in {said:?}");
        }
        let source = words(placed(&made.deck, 4, "source"));
        assert_eq!(source, "Fuente: NASA GISS");
        assert!(placed(&made.deck, 4, "main")["alt"].as_str().unwrap().starts_with("Gráfico de líneas"));
        // A year's months and a 2×2's axes, in Spanish, never left empty.
        let mut talk2 = talk.clone();
        talk2.slides[9].key_bullets = vec!["One".into(), "Two".into()];
        for (layout, words) in [("year", &["ene", "dic"][..]), ("matrix", &["Más valor", "Menos esfuerzo"][..])] {
            let layouts = BTreeMap::from([(9, layout.to_string())]);
            let made = deck_as(&talk2, &theme, &Pictures::new(), &layouts).unwrap();
            assert_eq!(made.layouts[9], layout);
            let said: Vec<String> =
                shown(&made.deck, made.deck["states"][9]["id"].as_str().unwrap()).into_iter().map(|(_, w)| w).collect();
            for word in words {
                assert!(said.iter().any(|w| w == word), "{layout}: {word} in {said:?}");
            }
        }
        assert_eq!(label("es", "Jan"), Some("ene"));
        assert_eq!(label("ja", "Total"), Some("合計"));
        assert_eq!(label("es", "Something else"), None);
        assert_eq!(label("en", "Total"), None);
        // In English, the lanes keep the theme's words, and the source says so.
        talk.brief.language = None;
        let made = deck_as(&talk, &theme, &Pictures::new(), &layouts).unwrap();
        let said: Vec<String> = shown(&made.deck, made.deck["states"][journey]["id"].as_str().unwrap())
            .into_iter()
            .map(|(_, w)| w)
            .collect();
        assert!(said.iter().any(|w| w == "Does"), "{said:?}");
        assert_eq!(words(placed(&made.deck, 4, "source")), "Source: NASA GISS");
    }

    #[test]
    fn a_slide_with_a_picture_shows_it_beside_its_words() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let pictures: Pictures = [0, 1, 2, 3, 24]
            .into_iter()
            .map(|i| (i, Placed { src: format!("assets/picture-{}.jpg", i + 1), alt: format!("picture {}", i + 1) }))
            .collect();
        let made = deck(&climate(), &theme, &pictures).unwrap();
        let layouts: Vec<&str> = [0, 1, 2, 3, 24].iter().map(|i| made.layouts[*i].as_str()).collect();
        assert_eq!(layouts, ["title-split", "agenda-photo", "section-photo", "number-photo", "title-split"]);
        let art = placed(&made.deck, 2, "art");
        assert_eq!(
            (art["type"].as_str(), art["src"].as_str(), art["alt"].as_str()),
            (Some("image"), Some("assets/picture-3.jpg"), Some("picture 3"))
        );
        // The divider keeps its number with its picture.
        assert_eq!(placed(&made.deck, 2, "number")["text"], "01");
        let images = made.deck["nodes"].as_object().unwrap().values().filter(|n| n["type"] == "image").count();
        assert_eq!(images, 5);
    }

    /// One picture asked for in an editor (PLAN 2.107): its subject in the person's words, in the
    /// theme's palette as a made deck's pictures are, framed as its place, and painted as they are.
    #[test]
    fn a_picture_asked_for_in_an_editor_is_framed_as_its_place_in_the_themes_palette() {
        use base64::Engine;
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let wide = asked_one(&theme, "  A lighthouse at dusk ", Look::Photo, 1600.0, 900.0);
        assert_eq!((wide.slide, wide.frame, wide.alt.as_str()), (0, Frame::Landscape, "A lighthouse at dusk"));
        assert!(wide.prompt.contains("SUBJECT: A lighthouse at dusk"), "{}", wide.prompt);
        assert!(wide.prompt.contains("#FF6A3D"), "in the theme's accent: {}", wide.prompt);
        assert_eq!(asked_one(&theme, "A door", Look::Illustration, 400.0, 900.0).frame, Frame::Portrait);
        assert_eq!(asked_one(&theme, "A door", Look::Illustration, 500.0, 500.0).frame, Frame::Square);
        let settings = pictures::Settings {
            provider: Provider::Openai,
            model: "gpt-image-1".into(),
            base: None,
            quality: pictures::Quality::Medium,
        };
        let mut painting = Painting::new(vec![wide], settings);
        assert_eq!(painting.requests("sk-test").len(), 1);
        let png = base64::engine::general_purpose::STANDARD.encode([0x89u8, b'P', b'N', b'G', 1, 2]);
        let body = json!({ "data": [{ "b64_json": png }] }).to_string();
        assert_eq!(painting.answer(vec![Reply { status: 200, status_text: "OK".into(), body, after: None }]), 0.0);
        let (placed, assets, unmade) = painting.finish();
        assert!(unmade.is_empty(), "{unmade:?}");
        assert_eq!(placed[&0].alt, "A lighthouse at dusk");
        assert_eq!(assets[&placed[&0].src][..4], [0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn pictures_are_asked_for_again_while_busy_as_a_png_where_a_jpeg_is_not_made_and_never_past_a_refused_key() {
        use base64::Engine;
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let asked = asking(&climate(), &theme, Look::Illustration);
        // Ten of twenty-five: the cover, the agenda, the five dividers, the ending, and two numbers.
        assert_eq!(asked.iter().map(|a| a.slide).collect::<Vec<_>>(), [0, 1, 2, 3, 7, 11, 14, 15, 21, 24]);
        // The cover and the ending take half the slide; a divider, a number, or a list its side.
        let frames: Vec<Frame> = asked.iter().map(|a| a.frame).collect();
        assert_eq!((frames[0], frames[9]), (Frame::Square, Frame::Square));
        assert!(frames[1..9].iter().all(|f| *f == Frame::Portrait), "{frames:?}");
        assert!(asked[2].prompt.contains("SUBJECT: Weather balloon rising over Antarctic ice"));
        assert!(asked[2].prompt.contains("#F2F0E9 for line/subject, #FF6A3D as the one bold accent, on #101014"));
        let settings = pictures::Settings {
            provider: Provider::Openai,
            model: "gpt-image-1".into(),
            base: None,
            quality: pictures::Quality::Medium,
        };
        let jpeg = base64::engine::general_purpose::STANDARD.encode([0xffu8, 0xd8, 0xff, 0xe0, 1, 2]);
        let png = base64::engine::general_purpose::STANDARD.encode([0x89u8, b'P', b'N', b'G', 1, 2]);
        let ok = |data: &str| Reply {
            status: 200,
            status_text: "OK".into(),
            body: json!({ "data": [{ "b64_json": data }] }).to_string(),
            after: None,
        };
        let busy = Reply {
            status: 429,
            status_text: "Too Many Requests".into(),
            body: r#"{"error":{"message":"Rate limit"}}"#.into(),
            after: Some("3".into()),
        };
        let no_jpeg = Reply {
            status: 400,
            status_text: "Bad Request".into(),
            body: r#"{"error":{"message":"Unknown parameter: 'output_format'."}}"#.into(),
            after: None,
        };
        // The first busy, the second a model that makes no JPEG, the rest made at once.
        let mut p = Painting::new(asked[..4].to_vec(), settings.clone());
        let first = p.requests("sk-test");
        assert_eq!(first.len(), 4);
        assert_eq!(p.answer(vec![busy.clone(), no_jpeg, ok(&jpeg), ok(&jpeg)]), 3.0);
        let again = p.requests("sk-test");
        assert_eq!(again.len(), 2);
        assert!(again[1].body.as_ref().unwrap().get("output_format").is_none(), "asked again as a PNG");
        assert_eq!(p.answer(vec![ok(&jpeg), ok(&png)]), 0.0);
        assert!(p.requests("sk-test").is_empty());
        let (placed, assets, unmade) = p.finish();
        assert!(unmade.is_empty(), "{unmade:?}");
        assert_eq!(placed[&1].src, "assets/picture-02.png");
        assert_eq!(placed[&0].src, "assets/picture-01.jpg");
        assert_eq!(assets.len(), 4);
        // Out of credit: the rest are never asked for, and each says why.
        let quota = Reply {
            status: 429,
            status_text: "Too Many Requests".into(),
            body: r#"{"error":{"type":"insufficient_quota","message":"You exceeded your current quota"}}"#.into(),
            after: None,
        };
        let mut p = Painting::new(asked[..3].to_vec(), settings);
        p.requests("sk-test");
        assert_eq!(p.answer(vec![ok(&jpeg), quota, busy]), 0.0);
        assert!(p.requests("sk-test").is_empty());
        let (placed, _, unmade) = p.finish();
        assert_eq!(placed.len(), 1);
        assert_eq!(unmade.len(), 2);
        assert!(unmade.iter().all(|u| u.contains("out of credit")), "{unmade:?}");
    }

    #[test]
    fn a_layout_with_no_place_for_an_item_gives_way_to_a_list() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        assert!(holds(&theme, "statement", 1) && !holds(&theme, "statement", 4));
        assert!(holds(&theme, "steps", 4) && !holds(&theme, "steps", 6));
        assert!(holds(&theme, "pros-cons", 7) && holds(&theme, "bullets", 12) && holds(&theme, "bar-chart", 3));
        // A head-to-head's first item names the sides; a receipt's last is its total.
        assert!(holds(&theme, "head-to-head", 4) && !holds(&theme, "head-to-head", 5));
        assert!(holds(&theme, "receipt", 5) && !holds(&theme, "receipt", 6));
        let (summary, slides) = catalog();
        let mut steps = slides.iter().find(|s| s.layout_suggestion == "How It Works").unwrap().clone();
        steps.key_bullets.extend(["Report: certificates reach the buyer".into(), "Renew: the lane books again".into()]);
        let made =
            deck(&narrative(&summary, vec![slides[0].clone(), steps, slides[55].clone()]), &theme, &Pictures::new())
                .unwrap();
        assert_eq!(made.layouts[1], "numbered");
    }

    #[test]
    fn a_recorded_answer_makes_a_deck_that_validates() {
        use scaena_narrato::{Generation, Provider, Settings, Step};
        let answer = |name: &str| {
            std::fs::read_to_string(root().join(format!("crates/scaena-narrato/tests/answers/{name}.json"))).unwrap()
        };
        let settings = Settings { provider: Provider::Anthropic, model: "a-model".into(), base: None, browser: false };
        let mut brief = narrative("", Vec::new()).brief;
        brief.voice = Some("Plain, confident, no jargon".into());
        brief.desired_outcomes = Some("A term sheet by spring".into());
        brief.persona_id = Some("investors".into());
        let mut g = Generation::new(brief, settings).unwrap();
        assert_eq!(g.answer(200, "OK", &answer("anthropic-arc"), None).unwrap(), Step::Slides);
        assert_eq!(g.answer(200, "OK", &answer("anthropic-slides"), None).unwrap(), Step::Done);
        let n = g.into_narrative().unwrap();
        let (made, write) = generating(&n, Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        assert!(made.created.created && made.created.errors == 0, "{:#?}", made.created.findings);
        let deck = write.unwrap().deck;
        assert_eq!(deck.states.len(), n.slides.len());
        assert_eq!(deck.meta.as_ref().and_then(|m| m.description.as_deref()), Some(n.executive_summary.as_str()));
        // The whole brief is kept (narrative FUNC-4): its voice, outcomes, persona, and modality,
        // the brief's default where none was asked.
        let kept = &deck.meta.as_ref().unwrap().extra["narrative"]["brief"];
        assert_eq!(kept["voice"], "Plain, confident, no jargon");
        assert_eq!(kept["desiredOutcomes"], "A term sheet by spring");
        assert_eq!(kept["personaId"], "investors");
        assert!(kept["modality"].as_str().is_some_and(|m| m.starts_with("In-person")), "{kept}");
        // Each slide a beat, its title the claim, the chart's figures its evidence.
        let beats: Vec<_> = deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats).collect();
        assert_eq!(beats.len(), n.slides.len());
        assert_eq!(beats[0].claim, n.slides[0].slide_title);
        assert!(beats.iter().any(|b| b.evidence.iter().any(|e| e.starts_with('@'))));
    }

    /// A provider that is busy twice is asked the slides again, after the wait each answer
    /// asks for, and the run goes on with the answer that then comes (narrative REL-1): the
    /// client's transport waits, and the progress says each ask. Six busy answers end it.
    #[test]
    fn a_busy_provider_is_asked_the_slides_again() {
        use scaena_narrato::{Generation, Provider, Settings};
        struct Scripted {
            replies: Vec<Reply>,
            asked: usize,
            paused: Vec<f64>,
        }
        impl Transport for Scripted {
            fn send(&mut self, _: &Request) -> Reply {
                self.asked += 1;
                self.replies.remove(0)
            }
            fn pause(&mut self, seconds: f64) {
                self.paused.push(seconds);
            }
        }
        let answer = |name: &str| {
            std::fs::read_to_string(root().join(format!("crates/scaena-narrato/tests/answers/{name}.json"))).unwrap()
        };
        let reply = |status: u16, text: &str, body: &str, after: Option<&str>| Reply {
            status,
            status_text: text.into(),
            body: body.into(),
            after: after.map(str::to_string),
        };
        let settings =
            || Settings { provider: Provider::Anthropic, model: "a-model".into(), base: None, browser: false };
        let mut g = Generation::new(narrative("", Vec::new()).brief, settings()).unwrap();
        g.choose_arc(Some("Problem\u{2013}Solution"));
        let mut transport = Scripted {
            replies: vec![
                reply(529, "Overloaded", r#"{"error":{"message":"Overloaded"}}"#, None),
                reply(503, "Service Unavailable", "", Some("5")),
                reply(200, "OK", &answer("anthropic-slides"), None),
            ],
            asked: 0,
            paused: Vec::new(),
        };
        let mut heard = Vec::new();
        let mut say = |p: Progress| heard.push(p);
        let (made, write) = generate(
            g,
            Some("sk-test"),
            &[],
            None,
            Looking::No,
            Path::new("dusk.theme.json"),
            dusk(),
            &font,
            &mut transport,
            &mut say,
        )
        .unwrap();
        assert_eq!((transport.asked, transport.paused), (3, vec![2.0, 5.0]));
        assert!(made.created.created && made.created.errors == 0, "{:#?}", made.created.findings);
        assert_eq!(write.unwrap().deck.states.len(), 5);
        let retried: Vec<&Progress> = heard.iter().filter(|p| matches!(p, Progress::Retrying { .. })).collect();
        assert_eq!(retried, [&Progress::Retrying { wait: 2.0, tried: 1 }, &Progress::Retrying { wait: 5.0, tried: 2 }]);
        assert!(heard.iter().any(|p| matches!(p, Progress::Wrote { slides: 5, .. })));

        // Six for now in a row: the error says so, after every try.
        let mut g = Generation::new(narrative("", Vec::new()).brief, settings()).unwrap();
        g.choose_arc(Some("Problem\u{2013}Solution"));
        let mut transport = Scripted {
            replies: (0..scaena_narrato::TRIES).map(|_| reply(0, "connection reset", "", None)).collect(),
            asked: 0,
            paused: Vec::new(),
        };
        let mut heard = Vec::new();
        let mut say = |p: Progress| heard.push(p);
        let err = generate(
            g,
            Some("sk-test"),
            &[],
            None,
            Looking::No,
            Path::new("dusk.theme.json"),
            dusk(),
            &font,
            &mut transport,
            &mut say,
        )
        .unwrap_err();
        assert_eq!(err.message, "the request did not complete: connection reset (asked 6 times)");
        assert_eq!((transport.asked, transport.paused), (6, vec![2.0, 4.0, 8.0, 16.0, 32.0]));
    }

    /// An Anthropic answer whose text is `text`.
    fn anthropic(text: &str) -> String {
        json!({ "content": [{ "type": "text", "text": text }], "stop_reason": "end_turn" }).to_string()
    }

    /// The rewrites a model would give for the texts a fit request asks about: each cut to its
    /// most characters, at a word.
    fn shortened(prompt: &str) -> String {
        let mut texts = Vec::new();
        for block in prompt.split("id: ").skip(1) {
            let (id, rest) = block.split_once('\n').unwrap();
            let most: usize =
                rest.split_once(" characters").unwrap().0.trim_start_matches("at most: ").parse().unwrap();
            let words = rest.split_once("text:\n").unwrap().1.trim();
            let mut cut = String::new();
            for word in words.split(' ') {
                if cut.chars().count() + word.chars().count() + 1 > most {
                    break;
                }
                cut = if cut.is_empty() { word.to_string() } else { format!("{cut} {word}") };
            }
            texts.push(json!({ "id": id, "text": cut }));
        }
        json!({ "texts": texts }).to_string()
    }

    /// A content slide is shown its beat's claim as what it must leave the audience with, and,
    /// where no outline was settled, its purpose as what it does; a slide that names its part
    /// (the cover, a divider, a Q&A, the agenda, the close) is shown neither, so nothing pushes
    /// a takeaway into its title (lift-up ATTR-2).
    #[test]
    fn a_content_slide_is_shown_its_claim_and_one_that_names_its_part_none() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let mut narrative = climate();
        let pictures = Pictures::new();
        let layouts = deck(&narrative, &theme, &pictures).unwrap().layouts;
        let framing = |k: usize| [0, 1, 2, 7, 11, 15, 21, 23, 24].contains(&k);
        let captions = |n: &Narrative| -> Vec<String> {
            seeing(n, &theme, &pictures, &layouts).iter().map(|s| scaena_narrato::look::caption(s, 25)).collect()
        };
        // No outline: each content slide's purpose, said as what it does, never as a takeaway.
        for (k, c) in captions(&narrative).iter().enumerate() {
            assert!(!c.contains("leave the audience"), "slide {}: {c}", k + 1);
            assert_eq!(!framing(k), c.contains("\nWhat it does in the talk: "), "slide {}: {c}", k + 1);
        }
        assert!(captions(&narrative)[3].ends_with("\nWhat it does in the talk: Evidence"));
        // An outline settled: each content slide's beat's claim, and no purpose beside it.
        let beats = (0..5)
            .map(|b| scaena_narrato::outline::Beat {
                name: format!("Beat {b}"),
                claim: format!("Claim {b}"),
                slides: 5,
            })
            .collect();
        narrative.outline = Some(scaena_narrato::outline::Outline { big_idea: "We can halve it".into(), beats });
        for (k, c) in captions(&narrative).iter().enumerate() {
            assert!(!c.contains("What it does in the talk"), "slide {}: {c}", k + 1);
            assert_eq!(!framing(k), c.contains("\nIt must leave the audience with: Claim "), "slide {}: {c}", k + 1);
        }
        assert!(captions(&narrative)[8].ends_with("\nIt must leave the audience with: Claim 2"));
    }

    #[test]
    fn what_the_model_saw_is_kept_where_lint_finds_the_slide_no_worse() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let narrative = climate();
        let pictures = Pictures::new();
        let built = deck(&narrative, &theme, &pictures).unwrap();
        let layouts = built.layouts.clone();
        let (made, write) = making(built, "Data Storytelling", Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        let before = (made, write.unwrap());
        // What the model is shown: the timeline may take numbered cards; the cover and a chart
        // keep their own.
        let seen = seeing(&narrative, &theme, &pictures, &layouts);
        assert_eq!(seen.len(), 25);
        assert!(seen[0].options.is_empty() && seen[8].options.is_empty(), "{:?}", seen[8]);
        assert_eq!(seen[13].layout.id, "milestones");
        assert_eq!(seen[13].layout.name, "Milestones");
        assert!(seen[13].options.iter().any(|o| o.id == "numbered-cards"), "{:?}", seen[13].options);
        assert!(seen[13].options.iter().all(|o| !o.draws.is_empty()));
        // Each slide drawn as a JPEG, as wide as asked.
        let drawn = drawn(&before.1, 320).unwrap();
        assert_eq!(drawn.len(), 25);
        let first = base64::engine::general_purpose::STANDARD.decode(&drawn[0].data).unwrap();
        assert!(first.starts_with(&[0xff, 0xd8]) && drawn[0].media_type == "image/jpeg");
        // The model gives the timeline numbered cards and tightens a chart's title.
        let changes = vec![
            Change {
                slide: 14,
                layout: Some("numbered-cards".into()),
                why: "Four records read as cards.".into(),
                ..Change::default()
            },
            Change {
                slide: 9,
                title: Some("Energy Drives 73% of Emissions".into()),
                why: "One line.".into(),
                ..Change::default()
            },
        ];
        let assets = BTreeMap::new();
        let shown = Shown { narrative: &narrative, theme: &theme, pictures: &pictures, assets: &assets };
        let (made, write) = looked(shown, changes, before, Path::new("dusk.theme.json"), &dusk(), &font).unwrap();
        assert_eq!(made.looked.len(), 2, "{:?}", made.unlooked);
        assert_eq!(made.layouts[13], "numbered-cards");
        assert!(made.created.created && made.created.errors == 0, "{:#?}", made.created.findings);
        let deck = write.deck.to_value().unwrap();
        let id = deck["states"][8]["id"].as_str().unwrap();
        assert_eq!(deck["nodes"][format!("{id}-header")]["text"], "Energy Drives 73% of Emissions");
        // Each card shows its year where it would count, then the event and what was measured,
        // each said once.
        let cards = deck["states"][13]["id"].as_str().unwrap();
        let said = |slot: &str| deck["nodes"][format!("{cards}-{slot}")]["text"].clone();
        assert_eq!(
            (said("first-number"), said("first-title"), said("first")),
            (json!("2021"), json!("Pacific Northwest heat dome"), json!("49.6\u{b0}C in Canada"))
        );
        // A slide whose words lint finds running past their place where they did not is worse.
        let mut after = made.clone();
        let mut long = Finding::new("W203", scaena_core::Severity::Warning, "runs past its place");
        long.state = Some(write.deck.states[3].id.clone());
        after.created.findings.push(long);
        assert_eq!(worse((&made, &write), (&after, &write)), vec![3]);
        assert_eq!(kept(made.looked.clone(), &[13]).len(), 1);
    }

    /// A long deck is shown to the model a dozen slides at a time (narrative PERF-5): each round
    /// says which slides it shows, numbered as they stand in the deck; the changes answered are
    /// folded into one; and a round the provider refuses loses its own slides' changes alone,
    /// said beside the rest.
    #[test]
    fn a_long_deck_is_looked_at_a_dozen_slides_at_a_time() {
        use scaena_narrato::{Provider, Settings};
        let (summary, slides) = catalog();
        // Fourteen slides: the cover, twelve of the catalog's content slides, and the ending.
        let mut picked = vec![slides[0].clone()];
        picked.extend(slides[2..14].iter().cloned());
        picked.push(slides[55].clone());
        assert_eq!(picked.len(), 14);
        let written = json!({ "executiveSummary": summary, "slides": picked }).to_string();
        let arc =
            std::fs::read_to_string(root().join("crates/scaena-narrato/tests/answers/anthropic-arc.json")).unwrap();
        let mut looks: Vec<(String, usize)> = Vec::new();
        let mut send = |r: &Request| {
            let body = r.body.as_ref().unwrap();
            let content = body["messages"][0]["content"].as_array().unwrap();
            let prompt = content.last().unwrap()["text"].as_str().unwrap().to_string();
            let (status, reply) = if prompt.contains("=== CATALOG ===") {
                (200, arc.clone())
            } else if prompt.contains("runs past its place") {
                (200, anthropic(r#"{"texts": []}"#))
            } else if prompt.starts_with("You have seen") {
                let drawn = content.iter().filter(|c| c["type"] == "image").count();
                let captions: Vec<&str> = content
                    .iter()
                    .filter(|c| c["type"] == "text")
                    .filter_map(|c| c["text"].as_str())
                    .filter(|t| t.starts_with("Slide "))
                    .collect();
                looks.push((prompt.clone(), drawn));
                if prompt.starts_with("You have seen the") {
                    panic!("fourteen slides go in two rounds: {prompt}");
                }
                if prompt.starts_with("You have seen slides 1\u{2013}12") {
                    assert_eq!(captions.first().map(|c| &c[..17]), Some("Slide 1 of 14, dr"), "{captions:?}");
                    // The second slide's title tightened; the rest kept.
                    let change = json!({ "slides": [{ "slide": 2, "layout": "", "title": "Agenda", "subtitle": "", "bullets": [], "why": "One word." }] });
                    (200, anthropic(&change.to_string()))
                } else {
                    assert_eq!(captions.first().map(|c| &c[..18]), Some("Slide 13 of 14, dr"), "{captions:?}");
                    (529, r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#.to_string())
                }
            } else {
                (200, anthropic(&written))
            };
            Reply {
                status,
                status_text: if status == 200 { "OK".into() } else { "Overloaded".into() },
                body: reply,
                after: None,
            }
        };
        let mut heard = Vec::new();
        let mut say = |p: Progress| heard.push(p);
        let settings = Settings { provider: Provider::Anthropic, model: "a-model".into(), base: None, browser: false };
        let g = Generation::new(narrative("", Vec::new()).brief, settings).unwrap();
        let (made, write) = generate(
            g,
            Some("sk-test"),
            &[],
            None,
            Looking::Asked,
            Path::new("dusk.theme.json"),
            dusk(),
            &font,
            &mut send,
            &mut say,
        )
        .unwrap();
        assert_eq!(looks.len(), 2, "two rounds: {looks:?}");
        assert_eq!((looks[0].1, looks[1].1), (12, 2), "a dozen slides, then the two left");
        assert!(looks[1].0.starts_with("You have seen slides 13\u{2013}14 of the 14 slides"), "{}", looks[1].0);
        assert_eq!(heard.iter().filter(|p| matches!(p, Progress::Looking { slides: 14 })).count(), 1, "{heard:?}");
        assert!(made.created.created, "{:#?}", made.created.findings);
        // The first round's change is kept, and the second round's refusal is said beside it.
        let titles: Vec<Option<&str>> = made.looked.iter().map(|c| c.title.as_deref()).collect();
        assert!(
            titles.contains(&Some("Agenda")) || made.unlooked.as_deref().is_some_and(|w| w.contains("kept as made")),
            "{made:#?}"
        );
        let unlooked = made.unlooked.as_deref().unwrap_or_default();
        assert!(unlooked.contains("slides 13\u{2013}14 not looked at: 529 Overloaded"), "{unlooked}");
        assert!(write.is_some());
    }

    /// A section is titled by what its slides say (narrative ATTR-5): its divider's title, where
    /// the run has one, first or after a slide; else the beat's name as the model wrote it; else
    /// the arc's name for the beat.
    /// The look is a polish on a deck the model has written and validated (narrative STAB-3): a
    /// slide that will not draw for the model to see leaves the deck as made, saying why, with
    /// no look asked for, and the fit still asked; and a change on a slide numbered 0 is kept
    /// as any other, never a subtraction past zero.
    #[test]
    fn a_look_that_cannot_draw_the_slides_leaves_the_deck_as_made() {
        use scaena_narrato::{Provider, Settings};
        let (summary, slides) = catalog();
        let picked = [slides[0].clone(), slides[1].clone(), slides[4].clone(), slides[54].clone(), slides[55].clone()];
        let written = json!({ "executiveSummary": summary, "slides": picked }).to_string();
        let arc =
            std::fs::read_to_string(root().join("crates/scaena-narrato/tests/answers/anthropic-arc.json")).unwrap();
        let mut asked: Vec<String> = Vec::new();
        let mut send = |r: &Request| {
            let body = r.body.as_ref().unwrap();
            let content = body["messages"][0]["content"].as_array().unwrap();
            let prompt = content.last().unwrap()["text"].as_str().unwrap().to_string();
            let reply = if prompt.contains("=== CATALOG ===") {
                arc.clone()
            } else if prompt.contains("runs past its place") {
                anthropic(r#"{"texts": []}"#)
            } else if prompt.starts_with("You have seen the") {
                panic!("the slides were not drawn, so the model is not shown them");
            } else {
                anthropic(&written)
            };
            asked.push(prompt);
            Reply { status: 200, status_text: "OK".into(), body: reply, after: None }
        };
        let mut heard = Vec::new();
        let mut say = |p: Progress| heard.push(p);
        let settings = Settings { provider: Provider::Anthropic, model: "a-model".into(), base: None, browser: false };
        let g = Generation::new(narrative("", Vec::new()).brief, settings).unwrap();
        let painter_down = |_: &Write| Err(OpsError::new("state `cover` is past the painter's largest raster"));
        let (made, write) = generate_drawn(
            g,
            Some("sk-test"),
            &[],
            None,
            Looking::Asked,
            Path::new("dusk.theme.json"),
            dusk(),
            &font,
            &mut send,
            &mut say,
            &painter_down,
        )
        .unwrap();
        assert!(made.created.created && made.created.errors == 0, "{:#?}", made.created.findings);
        assert_eq!(write.map(|w| w.deck.slides().len()), Some(5), "the deck stands as made");
        assert_eq!(
            made.unlooked.as_deref(),
            Some("the slides could not be drawn for the model: state `cover` is past the painter's largest raster")
        );
        assert!(made.looked.is_empty());
        assert!(!heard.iter().any(|p| matches!(p, Progress::Looking { .. })), "{heard:?}");
        assert_eq!(asked.len(), 2, "the arc and the slides; nothing to fit on these");
        let zero = Change { slide: 0, title: Some("x".into()), ..Change::default() };
        assert_eq!(kept(vec![zero.clone()], &[0]).len(), 1, "slide 0 names no index");
        assert_eq!(kept(vec![zero, Change { slide: 1, ..Change::default() }], &[0]).len(), 1);
    }

    /// A section is titled by what its slides say (narrative ATTR-5): its divider's title, where
    /// the run has one, first or after a slide; else the beat's name as the model wrote it; else
    /// its first slide's headline.
    #[test]
    fn a_section_is_titled_by_its_divider_else_the_models_beat_name_else_the_arcs() {
        let (summary, slides) = catalog();
        let at = |layout: &str| slides.iter().find(|s| s.layout_suggestion == layout).unwrap().clone();
        let beat = |mut slide: Slide, index: usize, name: Option<&str>| {
            slide.beat_index = Some(index);
            slide.beat_name = name.map(str::to_string);
            slide
        };
        let mut divider = at("Section Header");
        divider.slide_title = "The problem is the cost of proof".into();
        let mut later = at("Section Header");
        later.slide_title = "Where the money goes".into();
        let picked = vec![
            beat(at("Title"), 0, Some("  Hook  ")),
            beat(divider, 1, Some("Stakes")),
            beat(at("Bullet List"), 1, None),
            beat(at("Big Number"), 2, None),
            beat(at("Bullet List"), 3, Some("Ask")),
            beat(later, 3, None),
            beat(at("Closing"), 3, None),
        ];
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let made = deck(&narrative(&summary, picked), &theme, &Pictures::new()).unwrap();
        let spine = made.deck["spine"]["sections"].as_array().unwrap();
        let titles: Vec<&str> = spine.iter().map(|s| s["title"].as_str().unwrap()).collect();
        let headline = typeset_en(&at("Big Number").slide_title);
        assert_eq!(titles, ["Hook", "The problem is the cost of proof", headline.as_str(), "Where the money goes"]);
        let beats: Vec<usize> = spine.iter().map(|s| s["beats"].as_array().unwrap().len()).collect();
        assert_eq!(beats, [1, 2, 1, 3]);
        assert_eq!(spine[1]["id"], "the-problem-is-the-cost-of-proof");
    }

    /// Narrato echoes each beat's name from the arc's list, so a name that is the arc's is the
    /// template's, not the model's: a section with no divider is titled by its first slide's
    /// headline, and by the arc's name only where that slide has none.
    #[test]
    fn a_section_named_as_the_arc_names_its_beat_takes_its_first_headline() {
        let (summary, slides) = catalog();
        let names = scaena_narrato::arcs::arc("Problem\u{2013}Solution").beat_names();
        let at = |layout: &str| slides.iter().find(|s| s.layout_suggestion == layout).unwrap().clone();
        let beat = |mut slide: Slide, index: usize| {
            slide.beat_index = Some(index);
            slide.beat_name = Some(names[index].to_string());
            slide
        };
        let mut untitled = at("Statement");
        untitled.slide_title = String::new();
        let picked = vec![
            beat(at("Title"), 0),
            beat(at("Bullet List"), 1),
            beat(at("Big Number"), 1),
            beat(untitled, 2),
            beat(at("Closing"), 3),
        ];
        let headline = |i: usize| typeset_en(&picked[i].slide_title);
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let made = deck(&narrative(&summary, picked.clone()), &theme, &Pictures::new()).unwrap();
        let spine = made.deck["spine"]["sections"].as_array().unwrap();
        let titles: Vec<&str> = spine.iter().map(|s| s["title"].as_str().unwrap()).collect();
        assert_eq!(titles, [headline(0).as_str(), headline(1).as_str(), names[2], headline(4).as_str()]);
        assert!(titles.iter().zip(&names).all(|(t, n)| t != n || *t == names[2]), "{titles:?} {names:?}");
    }

    #[test]
    fn a_generation_runs_on_the_clients_transport_and_fits_what_runs_over() {
        use scaena_narrato::{Provider, Settings};
        let (summary, slides) = catalog();
        // The agenda's headline stands in a narrow column: this title runs past it at any size.
        let mut agenda = slides[1].clone();
        agenda.slide_title = "What we will cover today across the whole Gulf Coast and every port beyond it".into();
        let picked = [slides[0].clone(), agenda, slides[4].clone(), slides[54].clone(), slides[55].clone()];
        let written = json!({ "executiveSummary": summary, "slides": picked }).to_string();
        let arc =
            std::fs::read_to_string(root().join("crates/scaena-narrato/tests/answers/anthropic-arc.json")).unwrap();
        let mut asked: Vec<String> = Vec::new();
        let mut send = |r: &Request| {
            let body = r.body.as_ref().unwrap();
            let content = body["messages"][0]["content"].as_array().unwrap();
            let prompt = content.last().unwrap()["text"].as_str().unwrap().to_string();
            let reply = if prompt.contains("=== CATALOG ===") {
                arc.clone()
            } else if prompt.contains("runs past its place") {
                anthropic(&shortened(&prompt))
            } else if prompt.starts_with("You have seen the") {
                // Each slide drawn, after what is said of it; nothing the model would change.
                let drawn = content.iter().filter(|c| c["type"] == "image").count();
                assert_eq!(drawn, 5, "each slide shown");
                anthropic(r#"{"slides": []}"#)
            } else {
                anthropic(&written)
            };
            asked.push(prompt);
            Reply { status: 200, status_text: "OK".into(), body: reply, after: None }
        };
        let mut heard = Vec::new();
        let mut say = |p: Progress| heard.push(p);
        let settings = Settings { provider: Provider::Anthropic, model: "a-model".into(), base: None, browser: false };
        let g = Generation::new(narrative("", Vec::new()).brief, settings).unwrap();
        let (made, write) = generate(
            g,
            Some("sk-test"),
            &[],
            None,
            Looking::Asked,
            Path::new("dusk.theme.json"),
            dusk(),
            &font,
            &mut send,
            &mut say,
        )
        .unwrap();
        assert_eq!(asked.len(), 4, "arc, slides, the look, and the rewrite");
        assert!(heard.contains(&Progress::Looking { slides: 5 }) && heard.contains(&Progress::Looked { slides: 0 }));
        assert_eq!(made.arc, "Problem\u{2013}Solution");
        assert!(made.created.created && made.created.errors == 0, "{:#?}", made.created.findings);
        let header = made.fitted.first().expect("the agenda's title rewritten").clone();
        assert!(header.ends_with("-header"), "{header}");
        let deck = write.unwrap().deck.to_value().unwrap();
        // The model's shorter words, which now fit where they stand.
        let text = deck["nodes"][&header]["text"].as_str().unwrap();
        assert!(text.starts_with("What we will cover") && text.len() < 40, "{text}");
        let over = ["E100", "W202", "W203"];
        assert!(
            !made
                .created
                .findings
                .iter()
                .any(|f| over.contains(&f.code.as_str()) && f.node.as_deref() == Some(&header)),
            "{:#?}",
            made.created.findings
        );
        // The beat keeps the claim whole; the slide sets it shorter.
        let claims: Vec<&str> = deck["spine"]["sections"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|s| s["beats"].as_array().unwrap())
            .filter_map(|b| b["claim"].as_str())
            .collect();
        assert!(claims.contains(&"What we will cover today across the whole Gulf Coast and every port beyond it"));
        assert!(matches!(heard[0], Progress::Choosing));
        assert!(heard.iter().any(|p| matches!(p, Progress::Fitting { texts: 1 })));
        assert!(heard.iter().any(|p| matches!(p, Progress::Fitted { texts: 1 })));
    }

    /// A generated slide whose one item is a short list in the bullets layout builds it by
    /// bullet (narrative ATTR-2), as the editor's By Bullet does (PLAN 2.129): the slide shows
    /// the first point, each step one more, on a click of its own, the beat holding every step;
    /// the deck validates, and the look pass sees the slide whole and judges it as one.
    #[test]
    fn a_generated_list_builds_by_bullet() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let (summary, slides) = catalog();
        assert_eq!((slides[54].layout_suggestion.as_str(), slides[54].key_bullets.len()), ("Bullet List", 4));
        let n = narrative(&summary, vec![slides[0].clone(), slides[1].clone(), slides[54].clone(), slides[55].clone()]);
        let built = deck(&n, &theme, &Pictures::new()).unwrap();
        assert_eq!(built.layouts, ["title-center", "agenda", "bullets", "closing"]);
        let states = built.deck["states"].as_array().unwrap();
        assert_eq!(states.len(), 4 + 3, "the list's three steps");
        let slide = &states[2];
        let id = slide["id"].as_str().unwrap();
        let (list, props) =
            slide["props"].as_object().unwrap().iter().find(|(_, p)| !p["paragraphs"].is_null()).unwrap();
        assert_eq!(built.deck["nodes"][list]["enter"], "bullets");
        assert_eq!(built.deck["nodes"][list]["at"]["in"], "body");
        assert_eq!(props["paragraphs"], json!([1, 1]));
        for k in 2..=4 {
            let step = &states[k + 1];
            assert_eq!(step["id"], format!("{id}-{k}"));
            assert_eq!(step["slide"], id);
            assert_eq!(step["props"][list]["paragraphs"], json!([1, k]));
            assert!(step.get("transition").is_none() && step.get("mode").is_none());
        }
        // The agenda, a list in its own layout, and the closing come in whole.
        assert!(states[1]["props"].as_object().unwrap().values().all(|p| p["paragraphs"].is_null()));
        assert_eq!(states[6]["layout"], "closing");
        // The beat holds the slide and its steps.
        let beats: Vec<&Value> = built.deck["spine"]["sections"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|s| s["beats"].as_array().unwrap())
            .collect();
        assert_eq!(beats.len(), 4);
        assert_eq!(beats[2]["states"], json!([id, format!("{id}-2"), format!("{id}-3"), format!("{id}-4")]));
        // It validates and lints no worse, and reads as four slides: the look pass draws each
        // slide whole and judges it as one.
        let (made, write) =
            making(built, "Problem\u{2013}Solution", Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        assert!(made.created.created && made.created.errors == 0, "{:#?}", made.created.findings);
        assert!(!made.created.findings.iter().any(|f| f.code.starts_with("W32")), "{:#?}", made.created.findings);
        let write = write.unwrap();
        assert_eq!(write.deck.slides(), [vec![0], vec![1], vec![2, 3, 4, 5], vec![6]]);
        assert_eq!(drawn(&write, 64).unwrap().len(), 4);
        assert_eq!(wrongs(&made, &write).len(), 4);
        assert!(worse((&made, &write), (&made, &write)).is_empty());
        // A list of two, or of seven, comes in whole.
        let mut two = slides[54].clone();
        two.key_bullets.truncate(2);
        let mut seven = slides[54].clone();
        seven.key_bullets.extend((1..=3).map(|k| format!("One more point, number {k}")));
        for list in [two, seven] {
            let n = narrative(&summary, vec![slides[0].clone(), list, slides[55].clone()]);
            let built = deck(&n, &theme, &Pictures::new()).unwrap();
            assert_eq!(built.deck["states"].as_array().unwrap().len(), 3, "{:?}", built.layouts);
        }
    }

    /// A generated deck enters by its structure (narrative ATTR-1): each section divider and the
    /// first slide of its part push in, the closing slide dissolves in slowly, a grave turn takes
    /// the slow pace, and the content slides between keep the plain transition that morphs what
    /// they share; the cover has none. The same narrative gives the same entrances, and a deck
    /// that reads right to left pushes from the left.
    #[test]
    fn a_generated_deck_enters_by_its_structure() {
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let climate = climate();
        let built = deck(&climate, &theme, &Pictures::new()).unwrap();
        let again = deck(&climate, &theme, &Pictures::new()).unwrap();
        assert_eq!(built.deck["states"], again.deck["states"], "the same narrative, the same entrances");
        let transition = |i: usize| built.deck["states"][i]["transition"].clone();
        assert!(transition(0).is_null(), "the cover enters by nothing");
        let dividers = [2, 7, 11, 15, 21];
        assert!(dividers.iter().all(|i| built.layouts[*i] == "section"), "{:?}", built.layouts);
        let push = |slow: bool| json!({ "style": "push", "direction": "left", "duration": if slow { "slow" } else { "standard" }, "ease": "standard" });
        for i in dividers {
            assert_eq!(transition(i), push(false), "divider {i}");
        }
        // The first slide of each part pushes in too; the one after The Stakes reads grave after a
        // divider that did not, so it takes the slow pace.
        for i in [8, 16, 22] {
            assert_eq!(transition(i), push(false), "slide {i}");
        }
        assert_eq!(transition(12), push(true), "a grave turn at the part's first slide");
        // A grave turn between content slides keeps the plain transition, slowed: the hottest year
        // after the agenda, the highway to climate hell after who emits.
        let plain = |slow: bool| json!({ "duration": if slow { "slow" } else { "standard" }, "ease": "standard" });
        assert_eq!(transition(3), push(true), "the hottest year, grave after The Signal's divider");
        assert_eq!(transition(10), plain(true));
        assert_eq!(transition(6), plain(true), "every vital sign, at the grave mark after a slide above it");
        for i in [1, 4, 5, 9, 13, 14, 17, 18, 19, 20, 23] {
            assert_eq!(transition(i), plain(false), "slide {i}");
        }
        assert_eq!(transition(24), json!({ "style": "dissolve", "duration": "slow", "ease": "standard" }), "the close");
        // It validates and lints no worse: no build runs past the theme's limit with a slow entrance.
        let (made, _) = making(built, "Data Storytelling", Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        assert!(made.created.created && made.created.errors == 0, "{:#?}", made.created.findings);
        assert!(!made.created.findings.iter().any(|f| f.code.starts_with("W32")), "{:#?}", made.created.findings);
        // A deck that reads right to left pushes from the left.
        let mut arabic = climate.clone();
        arabic.brief.language = Some("Arabic".into());
        let rtl = deck(&arabic, &theme, &Pictures::new()).unwrap();
        assert_eq!(rtl.deck["states"][2]["transition"]["direction"], "right");
    }

    /// A source as `scaena-sources` reads one.
    fn source(name: &str, kind: Kind, text: &str) -> Source {
        Source { name: name.into(), kind, text: text.into(), sheets: Vec::new(), pdf: None }
    }

    #[test]
    fn sources_are_packed_as_vzestup_packs_them_a_pdf_beside_the_prompt() {
        let notes = source("notes.md", Kind::Markdown, "# Tidewater\nRevenue tripled.");
        let pdf = Source { pdf: Some(b"%PDF-1.7".to_vec()), ..source("q3.pdf", Kind::Pdf, "") };
        let g = grounding(&[notes.clone(), pdf.clone()], Provider::Anthropic).unwrap();
        let content = g.content.unwrap();
        assert!(
            content.starts_with("=== SOURCE: notes.md ===\n# Tidewater\nRevenue tripled.\n\n=== SOURCE: q3.pdf ===\n")
        );
        assert!(content.contains("attached to this request as a document"));
        assert_eq!(g.documents.len(), 1);
        assert_eq!((g.documents[0].name.as_str(), g.documents[0].data.as_str()), ("q3.pdf", "JVBERi0xLjc="));
        assert_eq!((g.given[0].kind, g.given[1].kind, g.given[0].truncated), ("markdown", "pdf", false));
        let blank = source("blank.txt", Kind::Text, "  \n ");
        assert!(grounding(&[notes.clone(), blank], Provider::Anthropic).unwrap_err().to_string().contains("blank.txt"));
        let large = Source { pdf: Some(vec![0; Provider::Gemini.document_bytes() + 1]), ..pdf };
        assert!(grounding(std::slice::from_ref(&large), Provider::Gemini).unwrap_err().to_string().contains("Gemini"));
        assert!(grounding(&[large], Provider::Anthropic).is_ok(), "Anthropic takes more in one request");
        assert!(grounding(&[], Provider::Anthropic).unwrap().content.is_none());
    }

    /// What the deck was grounded in is kept beside its brief, and each beat cites the source its
    /// slide names: a file as `source:NAME`, a page by its address, matched by name, loosely, or by
    /// address; a source the deck was not grounded in is cited by none (narrative FUNC-3). W423
    /// passes either, as it passes an address.
    #[test]
    fn each_beat_cites_the_source_its_slide_names() {
        let notes = source("notes.md", Kind::Markdown, "# Tidewater\nShipping emits about 3% of CO2.");
        let page = source("https://example.org/report", Kind::Html, "Ports electrify by 2030.");
        let g = grounding(&[notes, page], Provider::Anthropic).unwrap();
        let (summary, mut slides) = catalog();
        slides.truncate(5);
        let refs = [None, Some("notes.md"), Some("  Notes.MD "), Some("https://example.org/report"), Some("q3.pdf")];
        for (slide, named) in slides.iter_mut().zip(refs) {
            slide.source_ref = named.map(str::to_string);
        }
        let mut n = narrative(&summary, slides);
        n.sources = provenance(&g.given);
        let (made, write) = generating(&n, Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        let deck = write.unwrap().deck.to_value().unwrap();
        let sources = &deck["meta"]["narrative"]["sources"];
        assert_eq!(
            sources,
            &json!([
                { "name": "notes.md", "kind": "markdown", "bytes": g.given[0].bytes, "truncated": false },
                { "name": "https://example.org/report", "kind": "web page", "url": "https://example.org/report",
                  "bytes": g.given[1].bytes, "truncated": false },
            ])
        );
        let beats: Vec<&Value> = (deck["spine"]["sections"].as_array().unwrap().iter())
            .flat_map(|s| s["beats"].as_array().unwrap())
            .collect();
        let cites = |i: usize| -> Vec<&str> {
            (beats[i]["evidence"].as_array().into_iter().flatten())
                .filter_map(Value::as_str)
                .filter(|e| !e.starts_with('@'))
                .collect()
        };
        assert_eq!(cites(0), Vec::<&str>::new(), "the cover names no source");
        assert_eq!(cites(1), ["source:notes.md"]);
        assert_eq!(cites(2), ["source:notes.md"], "named loosely");
        assert_eq!(cites(3), ["https://example.org/report"], "a page by its address");
        assert_eq!(cites(4), Vec::<&str>::new(), "a source the deck was not grounded in");
        assert!(!made.created.findings.iter().any(|f| f.code == "W423"), "{:?}", made.created.findings);
        // A deck grounded in nothing says nothing of sources.
        let (summary, slides) = catalog();
        let (_, write) =
            generating(&narrative(&summary, slides[..2].to_vec()), Path::new("dusk.theme.json"), dusk(), &font)
                .unwrap();
        assert!(write.unwrap().deck.to_value().unwrap()["meta"]["narrative"].get("sources").is_none());
    }

    #[test]
    fn each_sheet_given_is_kept_as_a_data_source_of_the_deck() {
        let (summary, slides) = catalog();
        let (_, write) =
            generating(&narrative(&summary, slides[..3].to_vec()), Path::new("dusk.theme.json"), dusk(), &font)
                .unwrap();
        let rows = |r: &[&[&str]]| r.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect();
        let sheet = |name: &str, r: &[&[&str]]| scaena_sources::Sheet { name: name.into(), rows: rows(r) };
        let csv = Source {
            sheets: vec![sheet("q3.csv", &[&["region", "revenue"], &["Gulf", "4.1"], &["Atlantic", "2.2"]])],
            ..source("q3.csv", Kind::Csv, "region,revenue")
        };
        let book = Source {
            sheets: vec![
                sheet("By port", &[&["port", "share"], &["Houston", "41"]]),
                sheet("Notes", &[&["only a header"]]),
            ],
            ..source("Fleet Plan.xlsx", Kind::Sheets, "## Sheet: By port")
        };
        let (write, kept) = keeping(write.unwrap(), &scaena_sources::tables(&[csv, book])).unwrap();
        let ids: Vec<Option<&str>> = kept.iter().map(|k| k.id.as_deref()).collect();
        assert_eq!(ids, [Some("q3"), Some("fleet-plan-by-port"), None]);
        assert_eq!(kept[2].why.as_deref(), Some("no row under its header"));
        let deck = write.deck.to_value().unwrap();
        assert_eq!(deck["data"]["q3"]["source"], "data/q3.csv");
        assert_eq!(deck["data"]["fleet-plan-by-port"]["schema"]["share"], "number");
        assert_eq!(write.files["data/q3.csv"], b"region,revenue\nGulf,4.1\nAtlantic,2.2\n");
    }

    /// Items that each name an icon are set under the icons their words find (PLAN G.23): three
    /// side by side, on cards and in columns in turn; four down the slide; six in two rows of
    /// three. Each icon is the set's own path on its view, in the theme's accent, as decoration
    /// named by the icon, and rises with its item. A slide where a word finds none keeps its
    /// layout of words. The deck lints with no error.
    #[cfg(feature = "shipped")]
    #[test]
    fn items_that_name_icons_are_set_under_them() {
        let slide = |kind: &str, title: &str, bullets: &[&str], icons: &[&str]| Slide {
            slide_title: title.into(),
            layout_suggestion: kind.into(),
            key_bullets: bullets.iter().map(|b| b.to_string()).collect(),
            icons: icons.iter().map(|i| i.to_string()).collect(),
            ..Slide::default()
        };
        let three = ["Fast: a crossing in a day", "Safe: checked at every port", "Shared: one ledger for all"];
        let four = ["Plan: the route set a week out", "Track: each crate seen", "Agree: one price", "Arrive: on time"];
        let six = [
            "Quick: a day",
            "Locked: sealed",
            "Global: forty ports",
            "Timed: to the hour",
            "Heard: one line",
            "Rated: five stars",
        ];
        let slides = vec![
            slide("Title", "Tidewater", &[], &[]),
            slide("Bullet List", "Why Tidewater wins", &three, &[" Lightning ", "shield", "users"]),
            slide("Statement", "Cargo can pay for the carbon it moves", &[], &[]),
            slide("Bullet List", "How we work", &four, &["compass", "chart", "handshake", "flag"]),
            slide("Gallery", "What it means for a port", &three, &["rocket", "target", "heart"]),
            slide("Bullet List", "What a shipper gets", &six, &["lightning", "lock", "globe", "clock", "chat", "star"]),
            slide("Statement", "Every crossing counts", &[], &[]),
            slide("Gallery", "What it means for a crew", &three, &["rocket", "zzqx", "leaf"]),
            slide("Closing", "Sail with us", &[], &[]),
        ];
        let mut n = narrative("A pitch.", slides);
        let words = n.icon_words();
        assert_eq!(words[0], "lightning", "each word trimmed and in lower case, once");
        assert_eq!(words.iter().filter(|w| *w == "lightning").count(), 1);
        n.icons = icons_found(&words);
        assert!(n.icons.contains_key("chart") && !n.icons.contains_key("zzqx"), "{:?}", n.icons.keys());

        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let made = deck(&n, &theme, &Pictures::new()).unwrap();
        assert_eq!(made.layouts[1], "icon-cards");
        assert_eq!(made.layouts[3], "icon-rows");
        assert_eq!(made.layouts[4], "icon-columns");
        assert_eq!(made.layouts[5], "icon-grid");
        assert_eq!(made.layouts[7], "three-columns", "a word that finds no icon leaves the slide its words");

        // The first card's icon: the set's own, on its view, in the accent, named by the icon.
        let icon = placed(&made.deck, 1, "first-icon");
        let lightning = &n.icons["lightning"];
        assert_eq!(icon["kind"], "path");
        assert_eq!(icon["path"], json!(lightning.path));
        assert_eq!(icon["view"], json!([0, 0, 256, 256]), "a whole number written whole");
        assert_eq!((icon["fill"].as_str(), icon["alt"].as_str()), (Some("accent"), Some("Lightning")));
        assert_eq!(icon["semantic"], "decoration");
        // Each rises with its item: in the choreography's step for the first item.
        let id = (made.deck["states"][1]["props"].as_object().unwrap().keys())
            .find(|id| made.deck["nodes"][id.as_str()]["at"]["in"] == "first-icon")
            .unwrap();
        let first = &made.deck["states"][1]["choreography"][0];
        assert!(first["target"].as_array().unwrap().iter().any(|t| t == id.as_str()), "{first}");
        // Six in two rows: the sixth place holds the icon `star` finds.
        assert_eq!(placed(&made.deck, 5, "sixth-icon")["alt"], "Star");

        let (made, _) = generating(&n, Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        let errors: Vec<String> = (made.created.findings.iter())
            .filter(|f| f.severity == scaena_core::Severity::Error)
            .map(|f| format!("{} {}", f.code, f.message))
            .collect();
        assert!(errors.is_empty(), "{errors:#?}");
    }

    /// A word with no letter or digit (`-`, `—`, an emoji), which a model writes for an item it
    /// cannot picture, names no icon (PLAN G.23): the set's first icon is never taken for it, and
    /// its slide keeps its layout of words.
    #[cfg(feature = "shipped")]
    #[test]
    fn a_word_with_no_letter_names_no_icon() {
        let none: Vec<String> = ["-", "—", "$", "\u{1F680}"].iter().map(|w| w.to_string()).collect();
        assert!(icons_found(&none).is_empty(), "{:?}", icons_found(&none).keys());
        let three = ["Fast: a crossing in a day", "Safe: checked at every port", "Shared: one ledger for all"];
        let slides = vec![
            Slide { slide_title: "Tidewater".into(), layout_suggestion: "Title".into(), ..Slide::default() },
            Slide {
                slide_title: "What it means for a crew".into(),
                layout_suggestion: "Gallery".into(),
                key_bullets: three.iter().map(|b| b.to_string()).collect(),
                icons: ["rocket", "-", "leaf"].iter().map(|i| i.to_string()).collect(),
                ..Slide::default()
            },
            Slide { slide_title: "Sail with us".into(), layout_suggestion: "Closing".into(), ..Slide::default() },
        ];
        let mut n = narrative("A pitch.", slides);
        assert_eq!(n.icon_words(), ["rocket", "leaf"]);
        n.icons = icons_found(&n.icon_words());
        let theme: Theme = serde_json::from_str(&dusk()).unwrap();
        let made = deck(&n, &theme, &Pictures::new()).unwrap();
        assert_eq!(made.layouts[1], "three-columns", "its words, under no icon");
        let acorn = made.deck["nodes"].as_object().unwrap().values().any(|node| node["alt"] == "Acorn");
        assert!(!acorn, "no item is set under the set's first icon");
    }

    #[test]
    fn a_catalog_of_every_layout_makes_a_deck_that_lints_clean() {
        let (summary, slides) = catalog();
        let n = narrative(&summary, slides);
        let (made, write) = generating(&n, Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        assert!(made.created.created, "{:#?}", made.created.findings);
        let write = write.unwrap();
        let errors: Vec<&Finding> =
            made.created.findings.iter().filter(|f| f.severity == scaena_core::Severity::Error).collect();
        for f in &made.created.findings {
            eprintln!("{} {:?} {}", f.code, f.path, f.message);
        }
        eprintln!("fixed {}", made.fixed.len());
        if let Ok(dir) = std::env::var("SCAENA_NARRATO_OUT") {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            let b = crate::Bundle {
                root: dir.clone(),
                deck_file: "deck.json".into(),
                deck: write.deck.clone(),
                theme_json: None,
                files: scaena_store::Files::Dir(dir),
                author: "user".into(),
            };
            crate::lint::write(&b, write.clone()).unwrap();
        }
        assert!(errors.is_empty(), "{errors:#?}");
        // A slide for each of the catalog's, and a step for each point past the first of the
        // short lists set in the bullets layout (ATTR-2).
        assert_eq!(write.deck.slides().len(), 56);
        let steps: usize = (write.deck.states.iter()).filter(|s| s.slide.is_some()).count();
        let bulleted = (made.layouts.iter().zip(&n.slides))
            .filter(|(l, s)| l.starts_with("bullets") && (3..=6).contains(&s.key_bullets.len()))
            .map(|(_, s)| s.key_bullets.len() - 1)
            .sum::<usize>();
        assert!(steps > 0 && steps == bulleted, "{steps} steps, {bulleted} points past the first");
    }
}
