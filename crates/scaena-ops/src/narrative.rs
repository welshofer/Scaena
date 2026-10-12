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
use scaena_core::model::theme::{Example, Prompt, PromptItems, Slot, Theme};
use scaena_core::model::values::Range;
use scaena_core::{Deck, Finding};
use scaena_narrato::fit::Long;
use scaena_narrato::layouts::{self, Varied};
use scaena_narrato::look::{Change, Choice, Seen};
use scaena_narrato::pictures::{self, Frame, Look, Palette};
use scaena_narrato::{ChartData, Document, Generation, Narrative, Picture, Provider, Request, Slide, Step};
pub use scaena_sources::Given;
use scaena_sources::{Source, Table};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
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
/// more for a key whose rate limit counts pictures by the minute.
const TRIES: usize = 6;
/// The longest wait between two asks, in seconds.
const LONGEST_WAIT: f64 = 60.0;

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
    /// it shows; their bytes, by those paths; and the reason each picture not made was not.
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
                    unmade.push(format!("slide {}: {why}", a.slide + 1));
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
    /// The slides came back, cut short or whole.
    Wrote { slides: usize, truncated: bool },
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
/// it was, saying why; a picture not made leaves its slide in a layout that waits for none.
/// `say` hears how far it has come. Nothing is written.
#[allow(clippy::too_many_arguments)]
pub fn generate(
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
) -> Result<(Generated, Option<Write>), OpsError> {
    while let Some(request) = key.and_then(|key| generation.request(key, documents)) {
        let step = generation.step();
        say(if step == Step::Arc { Progress::Choosing } else { Progress::Writing { slides: generation.target() } });
        let reply = transport.send(&request);
        generation.answer(reply.status, &reply.status_text, &reply.body).map_err(|e| OpsError::new(e.to_string()))?;
        if step == Step::Arc {
            let arc = generation.arc().map(|a| a.selection.clone()).unwrap_or_default();
            say(Progress::Chose { arc, fallback: generation.fallback().map(str::to_string) });
        }
    }
    let narrative = (generation.narrative().cloned())
        .ok_or_else(|| OpsError::new("the narrative is not written: a key asks the model for it"))?;
    say(Progress::Wrote { slides: narrative.slides.len(), truncated: narrative.truncated });
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
    let changes = match (looking, key) {
        (Looking::Answered(text), _) => Some(scaena_narrato::look::read(text, &seen).map_err(OpsError::new)),
        (Looking::Asked, Some(key)) => {
            let drawn = drawn(&write, LOOK_WIDTH)?;
            generation.look_request(key, &seen, &drawn).map(|request| {
                say(Progress::Looking { slides: seen.len() });
                let reply = transport.send(&request);
                (generation.look_answer(reply.status, &reply.status_text, &reply.body, &seen))
                    .map_err(|e| OpsError::new(e.to_string()))
            })
        }
        _ => None,
    };
    match changes {
        Some(Ok(changes)) => {
            let shown = Shown { narrative: &narrative, theme: &model, pictures: &pictures, assets: &assets };
            (made, write) = looked(shown, changes, (made, write), theme_file, &theme, font)?;
            say(Progress::Looked { slides: made.looked.len() });
        }
        Some(Err(why)) => made.unlooked = Some(why.to_string()),
        None => {}
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

/// `write` with `rewrites`, each a text node's id and the words the model rewrote it in to fit
/// its place ([`overflowing`]), set in their places, and each fix lint then offers applied, as
/// [`generating`] applies them; what `made` says follows.
pub fn fitted(made: &mut Generated, mut write: Write, rewrites: Vec<(String, String)>) -> Result<Write, OpsError> {
    if rewrites.is_empty() {
        return Ok(write);
    }
    let mut doc = write.deck.to_value()?;
    for (id, text) in &rewrites {
        doc["nodes"][id]["text"] = json!(typeset(text));
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
    (narrative.slides.iter().zip(&varied).zip(layouts).enumerate())
        .map(|(i, ((slide, v), own))| {
            let items = items(slide, v);
            let options = others(v.layout.identity(), slide, &items, theme, pictures.contains_key(&i), own);
            Seen {
                slide: i + 1,
                layout: choice(own),
                options: options.iter().map(|o| choice(o)).collect(),
                title: slide.slide_title.clone(),
                subtitle: slide.subtitle.clone(),
                bullets: slide.key_bullets.clone(),
            }
        })
        .collect()
}

/// The layouts a slide may take in place of `own`, the one it took (PLAN G.13): for a slide of
/// items that a layout of places sets as well as a list, the theme's layouts with a place for
/// each item that set them as they are written (each under its label where each has one, each
/// as its figure where each has one), and a list of them; none for a cover, a divider, a quote,
/// a chart, a slide with a picture, or a slide whose layout reads its items in its own way.
fn others(identity: &str, slide: &Slide, items: &[Item], theme: &Theme, pictured: bool, own: &str) -> Vec<String> {
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

/// What lint finds wrong with each slide of a made deck: its errors, and its warnings.
fn wrongs(made: &Generated, write: &Write) -> Vec<(usize, usize)> {
    use scaena_core::Severity;
    (write.deck.states.iter())
        .map(|state| {
            let on = || made.created.findings.iter().filter(|f| f.state.as_deref() == Some(state.id.as_str()));
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
    changes.retain(|c| !refused.contains(&(c.slide - 1)));
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

/// Each state of the deck `write` makes, at rest, drawn `width` pixels wide by the CPU painter,
/// as a JPEG, in their order: what the model looks at (PLAN G.13).
pub fn drawn(write: &Write, width: u32) -> Result<Vec<Picture>, OpsError> {
    let mut drawing = Drawing::new(write)?;
    (write.deck.states.iter())
        .map(|state| {
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

/// The texts of `doc` that grow (`fit: grow`, as [`Slots::grown`] marks them) held to one size
/// in each set, so that each set reads as one: a set is the parts a slide's layout sets side by
/// side (the words under each step, the figures of a scorecard), or the deck's section titles.
/// Each grows as far as its place allows, laid out by the engine, then all of its set are held to
/// the size the one with the least room reached. Whether any size changed.
fn alike(doc: &mut Value, view: &View) -> Result<bool, OpsError> {
    let deck = Deck::from_json(&doc.to_string()).context("the deck")?;
    let Some(theme) = view.theme(&deck)? else { return Ok(false) };
    let theme = scaena_engine::theme::Theme::from_json(&theme)?;
    let mut engine = crate::lint::engine_in(&deck, view, &theme, None)?;
    let data = crate::lint::data_in(&deck, view)?;
    // Each growing text: its set, its state, its id, and its role's size.
    let mut members: Vec<(String, String, String, f64)> = Vec::new();
    for state in doc["states"].as_array().into_iter().flatten() {
        let (Some(id), Some(shown)) = (state["id"].as_str(), state["props"].as_object()) else { continue };
        let layout = state["layout"].as_str().unwrap_or_default();
        for node in shown.keys() {
            let n = &doc["nodes"][node];
            let (Some("grow"), Some(slot), Some(role)) =
                (n["fit"].as_str(), n["at"]["in"].as_str(), n["role"].as_str())
            else {
                continue;
            };
            let Some(size) = theme.typography.roles.get(role).map(|r| r.size) else { continue };
            let set = match ordinal(slot) {
                Some((_, part)) => format!("{id}\u{1f}{part}\u{1f}{role}"),
                None if slot == "title" && layout.starts_with("section") => format!("\u{1f}{layout}\u{1f}{role}"),
                None => continue,
            };
            members.push((set, id.to_string(), node.clone(), size));
        }
    }
    // The size each reaches, its state laid out once.
    let mut reached: BTreeMap<String, f64> = BTreeMap::new();
    let mut states: Vec<&str> = members.iter().map(|m| m.1.as_str()).collect();
    states.dedup();
    for state in states {
        let ids: Vec<&str> = members.iter().filter(|m| m.1 == state).map(|m| m.2.as_str()).collect();
        let req = scaena_engine::FrameRequest {
            deck: &deck,
            theme: &theme,
            data: &data,
            state,
            t_ms: f64::INFINITY,
            format: None,
        };
        // A slide the engine cannot lay out keeps its sizes; lint says why.
        let Ok(placed) = engine.text_layouts(&req, &ids) else { continue };
        for (id, p) in ids.iter().zip(&placed) {
            let size = members.iter().find(|m| m.2 == *id).map_or(0.0, |m| m.3);
            reached.insert(id.to_string(), f64::from(p.scale) * size);
        }
    }
    let mut least: BTreeMap<&str, f64> = BTreeMap::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (set, _, id, _) in &members {
        let Some(size) = reached.get(id) else { continue };
        let at = least.entry(set.as_str()).or_insert(f64::INFINITY);
        *at = at.min(*size);
        *counts.entry(set.as_str()).or_default() += 1;
    }
    let mut changed = false;
    for (set, _, id, _) in &members {
        let (Some(size), Some(2..)) = (least.get(set.as_str()), counts.get(set.as_str())) else { continue };
        // Figures side by side keep the air between them: a tenth under the size the widest
        // reached its place at.
        let figures = doc["nodes"][id]["role"].as_str().is_some_and(figure_role);
        let size = if figures { size * 0.9 } else { *size };
        // A quarter of a unit under, so that every one of them fits at it.
        let size = (size * 4.0).floor() / 4.0;
        let node = &mut doc["nodes"][id];
        if node["maxSize"].as_f64().is_none_or(|most| size < most) {
            node["maxSize"] = json!(size);
            changed = true;
        }
    }
    Ok(changed)
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

/// `doc` with each fix `found` offers applied, each once, as `lint --fix` applies them.
fn settle(mut doc: Value, found: Vec<Finding>, view: &View) -> Result<Settled, OpsError> {
    let mut fixed = Vec::new();
    for f in found.iter().filter(|f| f.fix.is_some()) {
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
    if alike(&mut doc, &view)? {
        let deck = Deck::from_json(&doc.to_string()).context("the deck set alike")?;
        created.findings = lint_in(&deck, &view)?.findings;
    }
    // Then each fix lint offers, each checked by laying its state out again with it.
    let settled = settle(doc, created.findings, &view)?;
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
    let swap = |text: &str| -> String {
        text.chars().map(|c| if missing.contains(&c) { plain(c).to_string() } else { c.to_string() }).collect()
    };
    let Some(n) = doc.pointer_mut(&format!("/nodes/{node}")) else { return };
    if let Some(text) = n["text"].as_str() {
        n["text"] = json!(swap(text));
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
    let names = arc.beat_names();
    let count = narrative.slides.len();
    let varied = varied(narrative);

    let mut deck = Builder::default();
    let mut sections: Vec<Value> = Vec::new();
    let mut last_beat = None;
    let mut chosen = Vec::with_capacity(count);
    let mut section_number = 0;
    let mut running: Option<String> = None;
    for (i, (slide, v)) in narrative.slides.iter().zip(&varied).enumerate() {
        let beat = (slide.beat_index.filter(|b| *b < names.len()))
            .unwrap_or_else(|| scaena_narrato::beats::beat_of(i, count, names.len()));
        let items = items(slide, v);
        let picture = pictures.get(&i);
        let given = layouts.get(&i).filter(|l| theme.layouts.contains_key(*l) && holds(theme, l, items.len()));
        let layout = match given {
            Some(layout) => layout.clone(),
            None => layout_for(v.layout.identity(), slide, &items, i, theme, picture.is_some()),
        };
        if matches!(layout.as_str(), "section" | "section-photo") {
            section_number += 1;
        }
        let state = deck.state_id(&slide.slide_title, &layout);
        let slots = Slots {
            theme,
            layout: &layout,
            state: &state,
            slide,
            body: &v.body,
            items: &items,
            section: section_number,
            running: running.as_deref(),
            picture,
        };
        let made = deck.slide(slots, i > 0)?;
        if layout.starts_with("section") && !slide.slide_title.is_empty() {
            running = Some(slide.slide_title.clone());
        }
        chosen.push(layout);

        // The spine: a section for each run of slides in one beat, a beat for each slide.
        if last_beat != Some(beat) {
            let name = names.get(beat).copied().unwrap_or("Section");
            let id = deck.unique(&slug(name, "section"), Taken::Section);
            sections.push(json!({ "id": id, "title": name, "beats": [] }));
            last_beat = Some(beat);
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
        let mut entry = json!({ "id": beat_id, "claim": claim, "states": [state], "duration": seconds });
        if !slide.speaker_notes.is_empty() {
            entry["notes"] = json!(slide.speaker_notes);
        }
        if let Some(data) = &made.data {
            entry["evidence"] = json!([format!("@{data}")]);
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
    if let Some(lang) = brief.language.as_deref().and_then(language_tag) {
        meta["lang"] = json!(lang);
    }
    if narrative.truncated {
        meta["narrative"]["truncated"] = json!(true);
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
            let layout = layout_for(identities[i], slide, &items(slide, &varied[i]), i, theme, true);
            let slot =
                theme.layouts.get(&layout)?.slots.values().find(|s| matches!(s.example, Some(Example::Picture(_))))?;
            let frame = frame_of(theme, slot);
            let subject = pictures::subject(slide);
            let prompt = pictures::prompt(look, &style, &subject, pictures::composition(identities[i]), frame);
            Some(Asked { slide: i, prompt, frame, alt: subject })
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
    Value::Object(m)
}

/// The BCP 47 tag of a language the brief names by its English name.
fn language_tag(language: &str) -> Option<&'static str> {
    Some(match language {
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
        if self.figure_first() { Some(head(&self.detail).0) } else { self.label.clone() }
    }
}

/// A slide's items: its bullets, each without the mark a model may lead it with.
fn items(slide: &Slide, v: &Varied) -> Vec<Item> {
    // A cover and a divider say their line alone; a Q&A, set as a divider, keeps its questions.
    if matches!(v.layout.identity(), "title" | "section") {
        return Vec::new();
    }
    slide.key_bullets.iter().map(|b| item(b)).filter(|i| !i.whole.is_empty()).collect()
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
    let rest: String = chars[end..].iter().take(3).collect();
    let suffix = ["%", "\u{d7}", "x", "M", "B", "K", "k", "bn", "m", "pp", "pts"]
        .iter()
        .filter(|s| rest.starts_with(*s))
        .max_by_key(|s| s.len())
        .map_or(0, |s| s.chars().count());
    // A suffix is the figure's only where the word stops there: "5 M" no, "5M" yes, "5Mbps" no.
    let after = chars.get(end + suffix).copied();
    let suffix = if suffix > 0 && after.is_none_or(|c| !c.is_alphanumeric()) { suffix } else { 0 };
    Some(chars[begin..end + suffix].iter().collect())
}

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
fn layout_for(identity: &str, slide: &Slide, items: &[Item], index: usize, theme: &Theme, pictured: bool) -> String {
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
        "faq" => "faq",
        "glossary" => "glossary",
        "checklist" => "checklist",
        "ask" => "the-ask",
        "qa" => "qa",
        "recipe-card" => "recipe",
        _ => "bullets",
    };
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

/// What a slide's slots are filled from.
struct Slots<'a> {
    theme: &'a Theme,
    layout: &'a str,
    state: &'a str,
    slide: &'a Slide,
    /// Its body after the variety pass: a bullet a cover or a closing took for its line.
    body: &'a str,
    items: &'a [Item],
    /// Which section divider this is, from 1.
    section: usize,
    /// The title of the part of the talk the slide is in: its last divider's.
    running: Option<&'a str>,
    /// The picture made for the slide, if one was.
    picture: Option<&'a Placed>,
}

/// What one slide added beyond its nodes: the data source its chart reads.
struct SlideMade {
    data: Option<String>,
}

/// What fills a slot.
enum Fill {
    Text(String),
    List(Vec<String>, &'static str),
    /// The prompt's own words: a column's or a lane's label.
    Prompt,
    Shape,
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

/// Roles that set a figure.
fn figure_role(role: &str) -> bool {
    matches!(role, "figure" | "hero" | "value" | "numeral")
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
        Some(u) => format!("{n} {u}"),
    }
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

    fn fact(&self) -> Option<String> {
        self.slide.supporting_facts.clone().filter(|t| !t.is_empty())
    }

    fn item(&self, k: usize) -> Option<&Item> {
        self.items.get(k)
    }

    fn wholes(&self) -> Vec<String> {
        self.items.iter().map(|i| i.whole.clone()).collect()
    }

    /// Words a card holds: each item whole where every item is short, else what each names, so
    /// the cards of one slide read alike.
    fn short(&self, item: &Item) -> String {
        let long = self.items.iter().any(|i| i.label.is_some() && i.whole.chars().count() > 36);
        match &item.label {
            Some(label) if long => label.clone(),
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
    /// figure, and its headline for another figure but a date; a section's or a cover's title to
    /// the theme's display size, and what supports a section's to the lede; a quote to half again
    /// its size. None where it keeps its role's size.
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
            (Some(_), "caption") => at_least("body"),
            // What a figure or a row is, named in a label, as large as words read from the room.
            (Some((_, "title" | "criterion")), "label") => at_least("body"),
            (Some((_, "figure")), "figure") => at_least("numeral"),
            // Two figures, as large as the slide takes: up to the hero's size.
            (Some((_, "figure")), "hero") if self.layout == "two-figures" => size("hero"),
            (Some(_), "figure") if !dated => at_least("headline"),
            (Some(_), _) => None,
            (None, "headline") if name == "title" && (section || self.layout == "title-split") => at_least("display"),
            (None, "body") if name == "support" && section => at_least("lede"),
            (None, "quote") if name == "quote" => size("quote").map(|q| q * 1.5),
            // A line's figure, as large as its column takes, up to a scorecard's; what it says
            // under it, as words read from the room (G.19).
            (None, "figure") if name == "callout" && self.layout == "line-chart" => at_least("numeral"),
            (None, "caption") if name == "note" && self.layout == "line-chart" => at_least("body"),
            _ => None,
        }
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
        if keeps_prompt(name) {
            return match ordinal(name) {
                Some((k, _)) => self.item(k).map(|_| Fill::Prompt),
                None => Some(Fill::Prompt),
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
                return Some(Fill::List(listed(&item.detail).iter().map(|p| sentence(p)).collect(), kind));
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
            "number" if matches!(layout, "section" | "section-photo") => Some(format!("{:02}", self.section)),
            "number" => self.item(0).map(|i| i.figure().unwrap_or_else(|| i.whole.clone())),
            "quote" => self.item(0).map(|i| unquoted(&i.whole)).or_else(|| self.title()),
            "who" => self.item(1).map(|i| i.whole.clone()).or_else(|| self.subtitle()),
            "meaning" => self.item(0).map(|i| i.whole.clone()).or_else(|| self.subtitle()),
            "example" => self.item(1).map(|i| i.whole.clone()),
            "takeaway" => self.item(0).map(|i| i.whole.clone()).or_else(|| self.subtitle()),
            "callout" => {
                self.fact().as_deref().and_then(figure).or_else(|| self.item(0).and_then(|i| figure(&i.whole)))
            }
            "under" | "detail" => self.fact().or_else(|| self.item(1).map(|i| i.whole.clone())),
            "note" => {
                let source =
                    self.slide.chart_data.as_ref().and_then(|c| c.source_id.clone()).map(|s| format!("Source: {s}"));
                let fallback = if layout == "full" { self.subtitle() } else { None };
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
            || (sibling("title") && figure_role(title_role))
            || (sibling("number") && self.numbered_by_figure());
        let name_apart = (sibling("title") && !figure_role(title_role)) || sibling("date") || sibling("item");
        // A date sets a timeline's figure; elsewhere a figure's place takes the figure.
        let dated = matches!(self.layout, "timeline" | "milestones");
        match part {
            "title" if figure_role(role) && dated => item.label.clone().or_else(|| figure(&item.whole)),
            "title" if figure_role(role) => item.figure().or_else(|| item.label.clone()),
            "title" if item.figure_first() && !sibling("") => Some(sentence(&item.detail)),
            "title" | "item" | "task" if item.figure_first() => item.name().map(|n| sentence(&n)),
            "title" | "item" | "task" => item.name(),
            "date" => item.label.clone(),
            "figure" | "target" | "price" | "amount" => {
                item.figure().or_else(|| item.label.clone().filter(|_| part != "figure"))
            }
            "note" => said(item).1,
            "" => Some(match (&item.label, figure_apart, name_apart) {
                // "$4.1M: Q3 revenue, up 12%" with its figure and its name apart: "Up 12%".
                (Some(_), true, true) if item.figure_first() => sentence(&head(&item.detail).1),
                (Some(_), true, _) if item.figure_first() => sentence(&item.detail),
                // "Leads: 1,200, 28% go on" with its figure apart: "28% go on".
                (Some(_), true, true) => sentence(&after_figure(&item.detail).unwrap_or_else(|| item.detail.clone())),
                (Some(_), _, true) => sentence(&item.detail),
                // Its figure apart, its name here: "Houston: 41% of volume" → "Houston";
                // "Revenue: $4.1M, up 12%" → "Revenue, up 12%".
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
                (self.slide.chart_data.as_ref()).and_then(|c| c.source_id.clone()).map(|s| format!("Source: {s}"))
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
            ("line-chart", "note", _) => {
                let takeaway =
                    self.item(0).map(|i| if i.figure_first() { sentence(&i.detail) } else { i.whole.clone() });
                let source = (self.slide.chart_data.as_ref())
                    .and_then(|c| c.source_id.clone())
                    .map(|s| format!("Source: {s}"))
                    .or_else(|| self.fact());
                text(match (takeaway.filter(|t| !t.is_empty()), source) {
                    (Some(t), Some(s)) => Some(format!("{t} \u{b7} {s}")),
                    (t, s) => t.or(s),
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
                    "" => Some(words),
                    "note" => who,
                    _ => None,
                })
            }
            // "Name, role: credit".
            ("team" | "team-four" | "team-six", _, Some((k, part))) if part != "art" => {
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
            // Each a quadrant's title, placed on the axes.
            ("matrix", _, Some((k, ""))) => text(self.item(k).map(|i| self.short(i))),
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

    /// The items for and against: those whose label is one of `pro`'s words, and the rest; else
    /// the first half and the second.
    fn sides(&self, pro: &[&str]) -> (Vec<String>, Vec<String>) {
        let is_pro = |i: &Item| i.label.as_deref().is_some_and(|l| pro.iter().any(|p| l.to_lowercase().starts_with(p)));
        let labelled = self.items.iter().all(|i| i.label.is_some());
        if labelled && self.items.iter().any(is_pro) {
            let (a, b): (Vec<&Item>, Vec<&Item>) = self.items.iter().partition(|i| is_pro(i));
            let details = |side: Vec<&Item>| side.iter().map(|i| sentence(&i.detail)).collect();
            return (details(a), details(b));
        }
        let half = self.items.len().div_ceil(2);
        (self.wholes()[..half].to_vec(), self.wholes()[half..].to_vec())
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
            "under" | "note" if self.fact().as_deref() == Some(text) || text.starts_with("Source: ") => Some("source"),
            "who" => Some("source"),
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

    /// The slide `s` describes: its nodes, and its state showing them, in after a fade where
    /// another slide went before.
    fn slide(&mut self, s: Slots, after: bool) -> Result<SlideMade, OpsError> {
        let template = s
            .theme
            .layouts
            .get(s.layout)
            .ok_or_else(|| OpsError::new(format!("the theme has no layout `{}`", s.layout)))?;
        let mut props = Map::new();
        let mut made = SlideMade { data: None };
        let mut claimed = false;
        // What builds once the transition ends (G.17): each item's nodes, by its place's ordinal
        // (`first-…`, `second-…`), and each chart.
        let mut built: Vec<(usize, Vec<String>)> = Vec::new();
        let mut charts: Vec<String> = Vec::new();
        for (name, slot) in &template.slots {
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
                    if let Some(most) = s.grown(name, slot.role.as_deref().unwrap_or(role)) {
                        node["fit"] = json!("grow");
                        node["maxSize"] = json!(most);
                        // A figure grows on its one line; a cover's or a divider's title on as
                        // few as its words want, one for a few and two for more, so that growing
                        // never sets one of its words on a line of its own.
                        if figure_role(role) {
                            node["maxLines"] = json!(1);
                        } else if name == "title" {
                            node["maxLines"] = json!(if text.split_whitespace().count() <= 3 { 1 } else { 2 });
                        }
                    }
                    // A side's statement stands in the middle of its card, on as many as three lines
                    // as it grows to fill the card (G.20); the running head stands on the slide's
                    // foot, as a footer does.
                    match (s.layout, ordinal(name)) {
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
                    // size of the theme's lede, under its titles still.
                    // An agenda's short lines up to the size of its titles.
                    let most = if s.layout.starts_with("agenda") { "title" } else { "lede" };
                    if name == "body"
                        && let Some(most) = s.theme.typography.roles.get(most).map(|r| r.size)
                    {
                        node["fit"] = json!("grow");
                        node["maxSize"] = json!(most);
                    }
                    node
                }
                Fill::Prompt => {
                    let (Some(role), Some((words, _))) = (&slot.role, slot.prompt.as_ref().and_then(Prompt::text))
                    else {
                        continue;
                    };
                    json!({ "type": "text", "role": role, "text": words, "at": at, "semantic": "navigation" })
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
                        node["points"] = json!(points);
                    }
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
                    chart_node(chart, &data, s.layout, &s.slide.slide_title, takeaway, at)
                }
            };
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
        let mut state = json!({ "id": s.state, "mode": "absolute", "layout": s.layout, "props": props });
        if after {
            state["transition"] = json!({ "duration": "standard", "ease": "standard" });
        }
        if let Some(choreography) = builds(s.theme, built, &charts) {
            state["choreography"] = choreography;
        }
        self.states.push(state);
        Ok(made)
    }
}

/// How a slide builds once its transition ends (G.17): each of its items rising after the one
/// before it, as the theme's `rise` has it, the gap between them more than half the theme's
/// `standard` duration, so that no more than two move at once; and each chart growing from its
/// baseline, a mark at a time, as the theme's `grow` has it. Two to six items build, within the
/// theme's `maxBuild` (lint W321); one item, or more, comes in with the slide.
fn builds(theme: &Theme, mut built: Vec<(usize, Vec<String>)>, charts: &[String]) -> Option<Value> {
    let mut items = Vec::new();
    let standard = theme.motion.durations.get("standard").map_or(420.0, |d| d.0);
    let gap = (standard * 0.6).round();
    // The transition, the gaps, and the last item's own rise.
    let span = |n: usize| 2.0 * standard + gap * n.saturating_sub(1) as f64;
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
fn square(items: &[Item]) -> bool {
    let widest = items.iter().filter_map(Item::figure).map(|f| f.chars().count()).max().unwrap_or(0);
    let longest = items.iter().filter_map(Item::name).map(|n| n.chars().count()).max().unwrap_or(0);
    widest > 7 || longest > 32
}

/// Whether a timeline's moments are told in sentences, more words than a quarter of the slide
/// sets on a line or two (G.16).
fn told(items: &[Item]) -> bool {
    items.iter().any(|i| i.detail.split_whitespace().count() > 4)
}

/// `narrative` with the words each slide shows set as a typesetter sets them ([`typeset`]).
fn typeset_all(narrative: &Narrative) -> Narrative {
    fn set(s: &mut String) {
        *s = typeset(s);
    }
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
fn typeset(text: &str) -> String {
    if text.contains(['`', '{', '}', '<', '>']) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let digit = |i: usize| chars.get(i).is_some_and(char::is_ascii_digit);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let before = i.checked_sub(1).map(|b| chars[b]);
        let after = chars.get(i + 1).copied();
        // A quotation opens at the start, after a space, or after a bracket, a dash, or a mark
        // that opens another.
        let opens = before.is_none_or(|b| b.is_whitespace() || "([\u{2014}\u{2013}\u{201c}\u{2018}".contains(b));
        match chars[i] {
            // An apostrophe in a word or after it ("Canada's", "countries'") and one for a decade's
            // first digits ("'90s") curl one way; a single quotation opening, the other.
            '\'' if opens && !(digit(i + 1) && digit(i + 2)) => out.push('\u{2018}'),
            '\'' => out.push('\u{2019}'),
            '"' if opens => out.push('\u{201c}'),
            '"' => out.push('\u{201d}'),
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
            c => out.push(c),
        }
        i += 1;
    }
    out
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
fn chart_node(chart: &ChartData, data: &str, layout: &str, title: &str, takeaway: Option<&str>, at: Value) -> Value {
    let kind = if layout == "line-chart" { "line" } else { "bar" };
    let several = chart.series.len() > 1;
    // Every value with the decimals of the most precise of them, two at most: 2.0 beside 1.1.
    let decimals = decimals(chart);
    let y_title = match (chart.series.first().and_then(|s| s.name.as_deref()), chart.unit.as_deref()) {
        (Some(name), Some(unit)) if !several && !name.contains(unit) => Some(format!("{name} ({unit})")),
        (Some(name), _) if !several => Some(name.to_string()),
        (_, Some(unit)) => Some(unit.to_string()),
        _ => None,
    };
    let mut y = json!({ "field": "value", "type": "quantitative", "format": format!(",.{decimals}f") });
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
        "alt": title,
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
    // A line of one series is what its slide is about, drawn in the signal color, and the year
    // its headline names is marked on it (G.19).
    if kind == "line" && !several {
        let mut notes = vec![json!({ "kind": "highlight", "at": { "x": chart.categories } })];
        notes.extend(period(chart, [Some(title), takeaway]));
        node["annotations"] = json!(notes);
    }
    node
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
            let said = node["text"].as_str().map_or_else(|| node["type"].as_str().unwrap().to_string(), str::to_string);
            out.push((slot, said));
        }
        out
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

    #[test]
    fn words_are_set_as_a_typesetter_sets_them() {
        assert_eq!(typeset("Canada's wildfires"), "Canada\u{2019}s wildfires");
        assert_eq!(typeset("the '90s, the countries' pledges"), "the \u{2019}90s, the countries\u{2019} pledges");
        assert_eq!(
            typeset("\"A highway to 'climate hell'\""),
            "\u{201c}A highway to \u{2018}climate hell\u{2019}\u{201d}"
        );
        assert_eq!(
            typeset("2021-2024, 70-90% lost, 1.5-2\u{b0}C"),
            "2021\u{2013}2024, 70\u{2013}90% lost, 1.5\u{2013}2\u{b0}C"
        );
        assert_eq!(typeset("Climate - where we stand..."), "Climate \u{2013} where we stand\u{2026}");
        // A figure's minus, a date's hyphens, and a name's keep theirs; code keeps its quotes.
        assert_eq!(typeset("-13% by 2024-10-12, COVID-19"), "-13% by 2024-10-12, COVID-19");
        assert_eq!(typeset("`let s = \"x\";`"), "`let s = \"x\";`");
        // Set once, words stay as they are.
        let once = typeset("Canada's \"record\" 2021-2024...");
        assert_eq!(typeset(&once), once);
        // A figure's minus is not a bullet's mark.
        let i = item("-13%: Arctic summer ice per decade");
        assert_eq!((i.label.as_deref(), i.figure().as_deref()), (Some("-13%"), Some("-13%")));
        assert_eq!(item("- Measure our travel").whole, "Measure our travel");
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
            "team-four",
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
        let section = made("Section Header");
        has(&section, "number", "01");
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
        // A quote grows past its size, to half again at most.
        let quote = most(&node(10, "quote"));
        assert!(quote > size("quote") && quote <= size("quote") * 1.5, "{quote}");
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
        let years: Vec<Value> = made.deck["data"][source]["source"]["inline"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["category"].clone())
            .collect();
        assert_eq!(
            warming["annotations"],
            json!([
                { "kind": "highlight", "at": { "x": years } },
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
        let mut steps = slides.iter().find(|s| s.layout_suggestion == "Steps").unwrap().clone();
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
        let mut g = Generation::new(narrative("", Vec::new()).brief, settings).unwrap();
        assert_eq!(g.answer(200, "OK", &answer("anthropic-arc")).unwrap(), Step::Slides);
        assert_eq!(g.answer(200, "OK", &answer("anthropic-slides")).unwrap(), Step::Done);
        let n = g.into_narrative().unwrap();
        let (made, write) = generating(&n, Path::new("dusk.theme.json"), dusk(), &font).unwrap();
        assert!(made.created.created && made.created.errors == 0, "{:#?}", made.created.findings);
        let deck = write.unwrap().deck;
        assert_eq!(deck.states.len(), n.slides.len());
        assert_eq!(deck.meta.as_ref().and_then(|m| m.description.as_deref()), Some(n.executive_summary.as_str()));
        // Each slide a beat, its title the claim, the chart's figures its evidence.
        let beats: Vec<_> = deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats).collect();
        assert_eq!(beats.len(), n.slides.len());
        assert_eq!(beats[0].claim, n.slides[0].slide_title);
        assert!(beats.iter().any(|b| b.evidence.iter().any(|e| e.starts_with('@'))));
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
        assert_eq!(write.deck.states.len(), 56);
    }
}
