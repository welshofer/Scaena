//! # scaena-generator
//!
//! Decks made from a prompt in the browser (PLAN G.5, ADR-0026), as a WASM module of its own,
//! which the editor loads the first time it makes one: Narrato's requests and answers, deep
//! research's, the sources a deck is grounded in, read from their bytes or fetched by plans of
//! GETs, and the deck a narrative builds in a theme. The page's worker makes each request with
//! `fetch` and hands each answer back; the editor's module makes the deck a bundle (`Making`), as
//! `scaena generate` makes it. Nothing here touches the network, and the editor's module carries
//! none of it (SPEC §15).

use base64::Engine;
use scaena_narrato::fit::Long;
use scaena_narrato::research::{Asked, Next, Report};
use scaena_narrato::{Brief, Document, Provider, Settings};
use scaena_ops::narrative::Reply;
use scaena_sources::fetch::{Fetch, Get, Reach, Step as Fetched};
use scaena_sources::{Given, Kind, Source};
use serde::Deserialize;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

fn error(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// The provider `name` names.
fn provider(name: &str) -> Result<Provider, JsError> {
    Provider::named(name).ok_or_else(|| error(format!("`{name}` is no provider: anthropic, openai, or gemini")))
}

/// Who a deck is asked of: a provider, a model, and the address of its API where it is not the
/// provider's own.
#[derive(Deserialize)]
struct Asking {
    provider: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    base: Option<String>,
}

impl Asking {
    fn read(asking: &str) -> Result<(Asking, Provider), JsError> {
        let asking: Asking = serde_json::from_str(asking).map_err(|e| error(format!("who is asked: {e}")))?;
        let provider = provider(&asking.provider)?;
        Ok((asking, provider))
    }
}

/// A brief, Narrato's as JSON (`topic`, `audience`, `goal`, `length`, `modality`, …).
fn brief(brief: &str) -> Result<Brief, JsError> {
    serde_json::from_str(brief).map_err(|e| error(format!("the brief: {e}")))
}

/// What the page offers a brief, as JSON: Narrato's goals, lengths, and ways a deck is
/// delivered (`{ id, label }`), the audiences its personas name, its arcs by name, and the
/// defaults a field left empty takes.
#[wasm_bindgen]
pub fn choices() -> String {
    scaena_narrato::brief::choices().to_string()
}

/// A deck's narrative health (PLAN G.9, SPEC §7.7), as `scaena health --json` gives it: `deck`, as
/// `deck.json` holds it, read as its slides and the beats that tell them, beside `lint`, the
/// findings the editor's last lint gave as JSON, so a slide lint finds broken comes first
/// (lift-up 10-24b STAB-1).
#[wasm_bindgen]
pub fn health(deck: &str, lint: Option<String>) -> Result<String, JsError> {
    let deck = scaena_core::Deck::from_json(deck).map_err(error)?;
    let lint: Vec<scaena_core::Finding> = match lint {
        Some(lint) => serde_json::from_str(&lint).map_err(error)?,
        None => Vec::new(),
    };
    let health = scaena_ops::health::health_in(&deck, &lint).map_err(error)?;
    serde_json::to_string(&health).map_err(error)
}

/// The sources a deck is grounded in (PLAN G.3, G.4, G.8): files read from their bytes, what a
/// link names fetched by the worker as a [`Fetching`] plans it, and deep research's report.
#[wasm_bindgen]
#[derive(Default)]
pub struct Sources {
    read: Vec<Source>,
}

#[wasm_bindgen]
impl Sources {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Sources {
        Sources::default()
    }

    /// The file `name`, read by its extension (Markdown, text, RTF, HTML, JSON, CSV, Excel,
    /// Word, PowerPoint, or a PDF, which the model reads as itself): `{ name, kind }`, or why it
    /// does not read.
    pub fn add(&mut self, name: &str, bytes: &[u8]) -> Result<String, JsError> {
        let source = scaena_sources::read(name, bytes).map_err(error)?;
        self.keep(source)
    }

    /// What `spec` names over the network, as GETs for the worker to make (`github:OWNER/REPO`,
    /// a GitHub address, or a web page), with the user's GitHub token for a private
    /// repository; none where it names a file. SEC and Google, which serve no other page, say
    /// where they are read.
    pub fn fetching(spec: &str, token: Option<String>) -> Result<Option<Fetching>, JsError> {
        let Some(remote) = scaena_sources::fetch::remote(spec).map_err(error)? else { return Ok(None) };
        let reach = Reach { contact: None, token, browser: true };
        let (fetch, first) = Fetch::new(remote, &reach).map_err(error)?;
        Ok(Some(Fetching { fetch, next: Some(first), done: None }))
    }

    /// The source `fetching` read: `{ name, kind }`.
    pub fn take(&mut self, fetching: Fetching) -> Result<String, JsError> {
        let source = fetching.done.ok_or_else(|| error("the source is not read yet"))?;
        self.keep(source)
    }

    /// Deep research's report and its sources, as a source of their own (PLAN G.8):
    /// `{ name, kind }`.
    pub fn research(&mut self, research: &Research) -> Result<String, JsError> {
        let report = research.report.as_ref().ok_or_else(|| error("the research has written no report yet"))?;
        let source = Source {
            name: scaena_narrato::research::source_name(&research.topic),
            kind: Kind::Research,
            text: report.source_text(),
            sheets: Vec::new(),
            pdf: None,
        };
        self.keep(source)
    }

    /// How many sources are read.
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.read.len()
    }

    /// Each sheet given, as the data source the deck keeps of it, JSON `[{ named, hint, csv }]`,
    /// `csv` null where no row stands under its header: what the editor's `Making.keep` takes.
    pub fn tables(&self) -> String {
        let tables: Vec<Value> = scaena_sources::tables(&self.read)
            .into_iter()
            .map(|t| {
                let csv = t.csv.map(|c| String::from_utf8_lossy(&c).into_owned());
                json!({ "named": t.named, "hint": t.hint, "csv": csv })
            })
            .collect();
        Value::Array(tables).to_string()
    }

    fn keep(&mut self, source: Source) -> Result<String, JsError> {
        if source.pdf.is_none() && source.text.trim().is_empty() {
            return Err(error(format!("{}: holds no words to ground a deck in", source.name)));
        }
        let said = json!({ "name": source.name, "kind": source.kind.name() }).to_string();
        self.read.push(source);
        Ok(said)
    }
}

/// A source being fetched by the worker, a GET at a time (`scaena_sources::fetch`).
#[wasm_bindgen]
pub struct Fetching {
    fetch: Fetch,
    next: Option<Get>,
    done: Option<Source>,
}

#[wasm_bindgen]
impl Fetching {
    /// The request to make now, `{ url, headers, most }` as JSON, `most` the bytes to read of its
    /// answer at most; none once the source is read.
    pub fn request(&self) -> Option<String> {
        let get = self.next.as_ref()?;
        let headers: serde_json::Map<String, Value> =
            get.headers.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
        Some(json!({ "url": get.url, "headers": headers, "most": get.most }).to_string())
    }

    /// The answer to the request: its status, its content type, and its body.
    pub fn answer(&mut self, status: u16, kind: &str, body: &[u8]) -> Result<(), JsError> {
        match self.fetch.answer(status, kind, body).map_err(error)? {
            Fetched::Get(next) => self.next = Some(next),
            Fetched::Done(source) => {
                self.next = None;
                self.done = Some(source);
            }
        }
        Ok(())
    }
}

/// Deep research on a deck's topic (PLAN G.8): Gemini's research agent, polled, or Anthropic's
/// model with web search, a paused turn resumed.
#[wasm_bindgen]
pub struct Research {
    inner: scaena_narrato::research::Research,
    topic: String,
    report: Option<Report>,
}

#[wasm_bindgen]
impl Research {
    /// Research for `brief` as Narrato reads it, its defaults filled, by `asking` (`{ provider,
    /// model, base? }`). An OpenAI key runs none, and says so.
    #[wasm_bindgen(constructor)]
    pub fn new(brief: &str, asking: &str) -> Result<Research, JsError> {
        let filled = self::brief(brief)?.validated().map_err(error)?;
        let asked = Asked { topic: filled.topic, audience: filled.audience, goal: filled.goal, length: filled.length };
        let (asking, provider) = Asking::read(asking)?;
        let inner =
            scaena_narrato::research::Research::new(&asked, provider, &asking.model, asking.base.as_deref(), true)
                .map_err(error)?;
        Ok(Research { inner, topic: asked.topic, report: None })
    }

    /// The first request, `{ method, url, headers, body }` as JSON.
    pub fn start(&self, key: &str) -> String {
        serde_json::to_string(&self.inner.start(key)).expect("a request is JSON")
    }

    /// The answer to the last request: JSON `{ send }`, the request to make at once; `{ wait,
    /// send }`, to make after `wait` milliseconds; or `{ done: { sources, cut } }`, the report
    /// written. With each, `progress`: what the agent last said it was doing.
    pub fn answer(&mut self, key: &str, status: u16, status_text: &str, body: &str) -> Result<String, JsError> {
        let next = self.inner.answer(key, status, status_text, body).map_err(error)?;
        let progress = self.inner.progress.clone();
        Ok(match next {
            Next::Send(r) => json!({ "send": r, "progress": progress }),
            Next::Wait { ms, then } => json!({ "wait": ms, "send": then, "progress": progress }),
            Next::Done(report) => {
                let done = json!({ "sources": report.sources.len(), "cut": report.cut });
                self.report = Some(report);
                json!({ "done": done, "progress": progress })
            }
        }
        .to_string())
    }
}

/// A deck's narrative being written (`scaena_narrato::Generation`): the arc, then the slides;
/// then the deck it builds in a theme; then, once the editor's module has made it a bundle, the
/// rewrite of what still runs past its place.
/// A slide drawn, as a page hands it over: its media type and its bytes in base64.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Drawn {
    media_type: String,
    data: String,
}

#[wasm_bindgen]
pub struct Generation {
    inner: scaena_narrato::Generation,
    documents: Vec<Document>,
    given: Vec<Given>,
}

#[wasm_bindgen]
impl Generation {
    /// A generation of `brief`, Narrato's brief as JSON, grounded in `sources`, by `asking`
    /// (`{ provider, model, base? }`): the sources packed as Vzestup packs them, each PDF handed
    /// to the model beside the slides' prompt. Refused with why where Narrato refuses the brief
    /// (a source with a model's control markers), or the PDFs are past what the provider takes.
    #[wasm_bindgen(constructor)]
    pub fn new(brief: &str, asking: &str, sources: &Sources) -> Result<Generation, JsError> {
        let mut brief = self::brief(brief)?;
        let (asking, provider) = Asking::read(asking)?;
        let ground =
            scaena_sources::ground(&sources.read, provider.document_bytes(), provider.name()).map_err(error)?;
        brief.source_content = ground.content;
        let documents = (ground.pdfs.iter())
            .map(|(name, pdf)| Document {
                name: name.clone(),
                media_type: "application/pdf".into(),
                data: base64::engine::general_purpose::STANDARD.encode(pdf),
            })
            .collect();
        let settings = Settings { provider, model: asking.model, base: asking.base, browser: true };
        let mut inner = scaena_narrato::Generation::new(brief, settings).map_err(error)?;
        inner.give_sources(scaena_ops::narrative::provenance(&ground.given));
        Ok(Generation { inner, documents, given: ground.given })
    }

    /// What each source gave the prompt, JSON `[{ name, kind, url?, bytes, truncated }]`.
    pub fn given(&self) -> String {
        serde_json::to_string(&self.given).expect("what was given is JSON")
    }

    /// The arc, by its name, or the heuristic's: none is asked of the model.
    #[wasm_bindgen(js_name = chooseArc)]
    pub fn choose_arc(&mut self, name: Option<String>) {
        self.inner.choose_arc(name.as_deref());
    }

    /// Ask for the outline before the slides (narrative FUNC-1): once the arc is chosen, the next
    /// request is the outline's, whose answer `outline` gives, to settle or take as it is.
    #[wasm_bindgen(js_name = outlineFirst)]
    pub fn outline_first(&mut self) -> Result<(), JsError> {
        self.inner.outline_first().map_err(error)
    }

    /// The outline, JSON `{ bigIdea, beats: [{ name, claim, slides }] }`, once the model wrote it
    /// or the person settled one; none before.
    pub fn outline(&self) -> Option<String> {
        self.inner.outline().map(|o| serde_json::to_string(o).expect("an outline is JSON"))
    }

    /// The beats the slides are spread over at this length, JSON `[{ name, claim, slides }]`: the
    /// outline's where one stands, else the arc's with no claims; none before the arc is chosen.
    pub fn plan(&self) -> Option<String> {
        self.inner.plan().map(|p| serde_json::to_string(&p).expect("a plan is JSON"))
    }

    /// Write the slides without an outline, the arc chosen standing (lift-up REL-3): where the
    /// outline was asked twice and did not read (`unread`), and the person passes it over.
    #[wasm_bindgen(js_name = skipOutline)]
    pub fn skip_outline(&mut self) -> Result<String, JsError> {
        self.inner.skip_outline().map_err(error)?;
        Ok(self.said())
    }

    /// The outline as the person settled it, JSON as `outline` gives it: what the slides are
    /// written to (narrative FUNC-1). Refused with why where no beat keeps a claim.
    pub fn settle(&mut self, outline: &str) -> Result<String, JsError> {
        let outline: scaena_narrato::Outline =
            serde_json::from_str(outline).map_err(|e| error(format!("the outline: {e}")))?;
        self.inner.settle(&outline).map_err(error)?;
        Ok(self.said())
    }

    /// Where it stands, as `answer` says it, with no answer read: JSON `{ step, slides, arc?,
    /// reasoning?, fallback?, written?, truncated?, outline?, unread? }`.
    pub fn standing(&self) -> String {
        self.said()
    }

    /// Where it stands: `arc`, `outline`, `slides`, or `done`.
    pub fn step(&self) -> String {
        serde_json::to_value(self.inner.step()).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
    }

    /// The fewest slides the slides' prompt asks for.
    pub fn target(&self) -> usize {
        self.inner.target()
    }

    /// The next request, `{ method, url, headers, body }` as JSON, the key in its headers; none
    /// once the narrative is written.
    pub fn request(&self, key: &str) -> Option<String> {
        let request = self.inner.request(key, &self.documents)?;
        Some(serde_json::to_string(&request).expect("a request is JSON"))
    }

    /// The answer to the last request read, `after` its `Retry-After`: where the generation
    /// stands, JSON `{ step, slides, arc?, fallback?, written?, truncated?, wait? }`, `slides` the
    /// fewest asked for, `written` how many came back, and `wait` the seconds to wait before the
    /// same request is made again, where the answer was for now (narrative REL-1).
    pub fn answer(
        &mut self,
        status: u16,
        status_text: &str,
        body: &str,
        after: Option<String>,
    ) -> Result<String, JsError> {
        self.inner.answer(status, status_text, body, after.as_deref()).map_err(error)?;
        Ok(self.said())
    }

    /// Slides written elsewhere, in the JSON Narrato writes them in: the narrative with no
    /// call, the arc chosen first.
    pub fn written(&mut self, text: &str) -> Result<String, JsError> {
        self.inner.written(text, false).map_err(error)?;
        Ok(self.said())
    }

    fn said(&self) -> String {
        let narrative = self.inner.narrative();
        json!({
            "step": self.step(),
            "slides": self.inner.target(),
            "arc": self.inner.arc().map(|a| a.selection.clone()),
            "reasoning": self.inner.arc().map(|a| a.reasoning.clone()),
            "fallback": self.inner.fallback(),
            "written": narrative.map(|n| n.slides.len()),
            "truncated": narrative.map(|n| n.truncated),
            "outline": self.inner.outline(),
            "unread": self.inner.unread(),
            "wait": self.inner.again(),
            "tried": self.inner.tried(),
        })
        .to_string()
    }

    /// The words the narrative's slides name icons by (PLAN G.23), JSON `[word]`, each once and in
    /// lower case; `[]` before the narrative is written. The page finds each in the set that
    /// ships, as `scaena_ops::icons` finds one, and hands them back to `giveIcons`.
    #[wasm_bindgen(js_name = iconWords)]
    pub fn icon_words(&self) -> String {
        serde_json::to_string(&self.inner.icon_words()).expect("words are JSON")
    }

    /// The icons the page found for `iconWords`, JSON `{ word: { name, path, view } }`, a word
    /// that found none left out: what `deck` sets each item under (PLAN G.23).
    #[wasm_bindgen(js_name = giveIcons)]
    pub fn give_icons(&mut self, icons: &str) -> Result<(), JsError> {
        let icons = serde_json::from_str(icons).map_err(|e| error(format!("the icons: {e}")))?;
        self.inner.give_icons(icons);
        Ok(())
    }

    /// The pictures the narrative's slides ask for in `theme`, the theme's JSON, made as `look`
    /// (`illustration` or `photo`; PLAN G.10): JSON `[{ slide, prompt, frame, alt }]`, which a
    /// `Painting` makes.
    pub fn pictures(&self, theme: &str, look: &str) -> Result<String, JsError> {
        let narrative = self.inner.narrative().ok_or_else(|| error("the narrative is not written yet"))?;
        let model: scaena_core::model::theme::Theme =
            serde_json::from_str(theme).map_err(|e| error(format!("the theme: {e}")))?;
        let look = scaena_narrato::pictures::Look::named(look).ok_or_else(|| error(format!("`{look}` is no look")))?;
        Ok(serde_json::to_string(&scaena_ops::narrative::asking(narrative, &model, look))
            .expect("pictures asked are JSON"))
    }

    /// The deck the narrative builds in `theme`, the theme's JSON (`scaena_ops::narrative::deck`),
    /// each slide `pictures` (a `Painted`'s `placed`, JSON; `{}` for none) holds a picture for
    /// shown with it: JSON `{ deck, layouts, arc, fallback? }`, which the editor's `Making`
    /// makes a bundle with the pictures' bytes.
    pub fn deck(&self, theme: &str, pictures: &str) -> Result<String, JsError> {
        let narrative = self.inner.narrative().ok_or_else(|| error("the narrative is not written yet"))?;
        let model: scaena_core::model::theme::Theme =
            serde_json::from_str(theme).map_err(|e| error(format!("the theme: {e}")))?;
        let pictures: scaena_ops::narrative::Pictures =
            serde_json::from_str(pictures).map_err(|e| error(format!("the pictures: {e}")))?;
        let made = scaena_ops::narrative::deck(narrative, &model, &pictures).map_err(error)?;
        let mut out = serde_json::to_value(&made).map_err(error)?;
        out["arc"] = json!(narrative.arc);
        if let Some(why) = self.inner.fallback() {
            out["fallback"] = json!(why);
        }
        Ok(out.to_string())
    }

    /// What the model is shown of the deck the narrative made in `theme` (the theme's JSON) with
    /// `pictures` (as `deck` takes them), each slide in the layout `layouts` (JSON `[id]`, as
    /// `Making.report` says them) gives it (PLAN G.13): JSON `[{ slide, layout, options, title,
    /// subtitle, bullets }]`, which `lookRequest` and `lookAnswer` take.
    pub fn seeing(&self, theme: &str, pictures: &str, layouts: &str) -> Result<String, JsError> {
        let narrative = self.inner.narrative().ok_or_else(|| error("the narrative is not written yet"))?;
        let model: scaena_core::model::theme::Theme =
            serde_json::from_str(theme).map_err(|e| error(format!("the theme: {e}")))?;
        let pictures: scaena_ops::narrative::Pictures =
            serde_json::from_str(pictures).map_err(|e| error(format!("the pictures: {e}")))?;
        let layouts: Vec<String> = serde_json::from_str(layouts).map_err(|e| error(format!("the layouts: {e}")))?;
        let seen = scaena_ops::narrative::seeing(narrative, &model, &pictures, &layouts);
        Ok(serde_json::to_string(&seen).expect("what the model sees is JSON"))
    }

    /// The request that shows the model `seen` (as `seeing` gives it), each slide drawn as
    /// `drawn` holds it (JSON `[{ mediaType, data }]`, its bytes in base64), and asks what it
    /// would change (PLAN G.13); none before the narrative is written. `edges` is the slides just
    /// before and after them, as `seeing` gives each, or null (JSON `[before, after]`, lift-up
    /// PERF-1).
    #[wasm_bindgen(js_name = lookRequest)]
    pub fn look_request(&self, key: &str, seen: &str, drawn: &str, edges: &str) -> Result<Option<String>, JsError> {
        let seen: Vec<scaena_narrato::look::Seen> = serde_json::from_str(seen).map_err(error)?;
        let [before, after]: [Option<scaena_narrato::look::Seen>; 2] = serde_json::from_str(edges).map_err(error)?;
        let drawn: Vec<Drawn> = serde_json::from_str(drawn).map_err(error)?;
        let pictures: Vec<scaena_narrato::Picture> = (drawn.into_iter())
            .map(|d| scaena_narrato::Picture { caption: String::new(), media_type: d.media_type, data: d.data })
            .collect();
        let request = self.inner.look_request(key, &seen, &pictures, [before.as_ref(), after.as_ref()]);
        Ok(request.map(|r| serde_json::to_string(&r).expect("a request is JSON")))
    }

    /// The changes an answer to `lookRequest` asks for, each one its slide may take: JSON
    /// `[{ slide, layout?, title?, subtitle?, bullets?, why }]`.
    #[wasm_bindgen(js_name = lookAnswer)]
    pub fn look_answer(&self, status: u16, status_text: &str, body: &str, seen: &str) -> Result<String, JsError> {
        let seen: Vec<scaena_narrato::look::Seen> = serde_json::from_str(seen).map_err(error)?;
        let changes = self.inner.look_answer(status, status_text, body, &seen).map_err(error)?;
        Ok(serde_json::to_string(&changes).expect("changes are JSON"))
    }

    /// The deck `deck` builds, built again with `changes` (as `lookAnswer` gives them, PLAN
    /// G.13): their words in the narrative, each slide in the layout one asks for. What
    /// `Making.relook` takes.
    #[wasm_bindgen(js_name = deckChanged)]
    pub fn deck_changed(&self, theme: &str, pictures: &str, changes: &str) -> Result<String, JsError> {
        let narrative = self.inner.narrative().ok_or_else(|| error("the narrative is not written yet"))?;
        let model: scaena_core::model::theme::Theme =
            serde_json::from_str(theme).map_err(|e| error(format!("the theme: {e}")))?;
        let pictures: scaena_ops::narrative::Pictures =
            serde_json::from_str(pictures).map_err(|e| error(format!("the pictures: {e}")))?;
        let changes: Vec<scaena_narrato::look::Change> = serde_json::from_str(changes).map_err(error)?;
        let (changed, layouts) = scaena_ops::narrative::changed(narrative, &changes);
        let made = scaena_ops::narrative::deck_as(&changed, &model, &pictures, &layouts).map_err(error)?;
        let mut out = serde_json::to_value(&made).map_err(error)?;
        out["arc"] = json!(narrative.arc);
        Ok(out.to_string())
    }

    /// The request that has `longs` (JSON `[{ id, text, most }]`, as `Making.longs` gives them)
    /// rewritten to fit their places; none before the narrative is written.
    #[wasm_bindgen(js_name = fitRequest)]
    pub fn fit_request(&self, key: &str, longs: &str) -> Result<Option<String>, JsError> {
        let longs: Vec<Long> = serde_json::from_str(longs).map_err(error)?;
        let request = self.inner.fit_request(key, &longs);
        Ok(request.map(|r| serde_json::to_string(&r).expect("a request is JSON")))
    }

    /// How many seconds to wait before asking a busy provider again for a look or a fit, the
    /// request asked `tried` times so far (from 0); none where the answer is for good or the tries
    /// are spent (lift-up REL-1). `after` is the reply's `Retry-After`, where it had one.
    #[wasm_bindgen(js_name = retryAfter)]
    pub fn retry_after(&self, status: u16, body: &str, after: Option<String>, tried: usize) -> Option<f64> {
        scaena_narrato::retry_after(status, body, after.as_deref(), tried)
    }

    /// The rewrites an answer to `fitRequest` gives, JSON `[[id, text]]`, each shorter than the
    /// text it rewrites: what `Making.fitted` takes.
    #[wasm_bindgen(js_name = fitAnswer)]
    pub fn fit_answer(&self, status: u16, status_text: &str, body: &str, longs: &str) -> Result<String, JsError> {
        let longs: Vec<Long> = serde_json::from_str(longs).map_err(error)?;
        let rewrites = self.inner.fit_answer(status, status_text, body, &longs).map_err(error)?;
        Ok(serde_json::to_string(&rewrites).expect("rewrites are JSON"))
    }
}

/// One picture a person asks for in an editor (PLAN 2.107): of `subject`, in their words, in
/// `theme`'s palette (the theme's JSON), made as `look` (`illustration` or `photo`), for a place
/// `width` × `height` on the slide: JSON `[{ slide, prompt, frame, alt }]`, which a `Painting`
/// makes.
#[wasm_bindgen(js_name = pictureAsked)]
pub fn picture_asked(theme: &str, subject: &str, look: &str, width: f64, height: f64) -> Result<String, JsError> {
    let model: scaena_core::model::theme::Theme =
        serde_json::from_str(theme).map_err(|e| error(format!("the theme: {e}")))?;
    let look = scaena_narrato::pictures::Look::named(look).ok_or_else(|| error(format!("`{look}` is no look")))?;
    if subject.trim().is_empty() {
        return Err(error("say what the picture shows"));
    }
    let asked = scaena_ops::narrative::asked_one(&model, subject, look, width, height);
    Ok(serde_json::to_string(&[asked]).expect("a picture asked is JSON"))
}

/// The request that lists the image models a key can use (PLAN G.10), JSON, for `provider`
/// (`openai`, `gemini`) at `base`, the provider's own where none.
#[wasm_bindgen(js_name = pictureModelsRequest)]
pub fn picture_models_request(provider: &str, key: &str, base: Option<String>) -> Result<String, JsError> {
    let provider = Provider::named(provider).ok_or_else(|| error(format!("`{provider}` is no provider")))?;
    Ok(serde_json::to_string(&provider.models(key, base.as_deref(), true)).expect("a request is JSON"))
}

/// The image models an answer to `pictureModelsRequest` lists, best first, JSON `[id]`.
#[wasm_bindgen(js_name = pictureModels)]
pub fn picture_models(provider: &str, status: u16, status_text: &str, body: &str) -> Result<String, JsError> {
    let provider = Provider::named(provider).ok_or_else(|| error(format!("`{provider}` is no provider")))?;
    let models = scaena_narrato::pictures::models(provider, status, status_text, body).map_err(error)?;
    Ok(serde_json::to_string(&models).expect("models are JSON"))
}

/// Pictures being made (`scaena_ops::narrative::Painting`, PLAN G.10), the page making the
/// calls: each round's `requests`, each answer handed to `reply`, then `round` says how long to
/// wait before the next; `finish` once `requests` gives none.
#[wasm_bindgen]
pub struct Painting {
    inner: scaena_ops::narrative::Painting,
    replies: Vec<Reply>,
}

#[wasm_bindgen]
impl Painting {
    /// The pictures `asked` for (`Generation.pictures`), by `settings` (JSON `{ provider, model,
    /// base?, quality? }`).
    #[wasm_bindgen(constructor)]
    pub fn new(asked: &str, settings: &str) -> Result<Painting, JsError> {
        let asked: Vec<scaena_ops::narrative::Asked> = serde_json::from_str(asked).map_err(error)?;
        let settings: scaena_narrato::pictures::Settings = serde_json::from_str(settings).map_err(error)?;
        Ok(Painting { inner: scaena_ops::narrative::Painting::new(asked, settings), replies: Vec::new() })
    }

    /// This round's requests, JSON `[{ method, url, headers, body }]`, the key in their
    /// headers; none once each picture is made or given up.
    pub fn requests(&mut self, key: &str) -> String {
        self.replies.clear();
        serde_json::to_string(&self.inner.requests(key)).expect("requests are JSON")
    }

    /// The answer to this round's `n`th request, in order: its status, status line, body, and
    /// `Retry-After`.
    pub fn reply(&mut self, status: u16, status_text: &str, body: &str, after: Option<String>) {
        self.replies.push(Reply { status, status_text: status_text.into(), body: body.into(), after });
    }

    /// The round's answers read: how many seconds to wait before the next round.
    pub fn round(&mut self) -> f64 {
        self.inner.answer(std::mem::take(&mut self.replies))
    }

    /// The pictures made.
    pub fn finish(self) -> Painted {
        let (placed, assets, unmade) = self.inner.finish();
        Painted { placed, assets, unmade }
    }
}

/// Pictures made: where each slide's is kept and what it shows, its bytes, and why each picture
/// not made was not.
#[wasm_bindgen]
pub struct Painted {
    placed: scaena_ops::narrative::Pictures,
    assets: std::collections::BTreeMap<String, Vec<u8>>,
    unmade: Vec<String>,
}

#[wasm_bindgen]
impl Painted {
    /// What `Generation.deck` takes: JSON `{ slide: { src, alt } }`.
    pub fn placed(&self) -> String {
        serde_json::to_string(&self.placed).expect("pictures are JSON")
    }

    /// The paths the pictures are kept at in the bundle.
    pub fn paths(&self) -> Vec<String> {
        self.assets.keys().cloned().collect()
    }

    /// The bytes kept at `path`.
    pub fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.assets.get(path).cloned()
    }

    /// Why each picture not made was not, JSON `[reason]`.
    pub fn unmade(&self) -> String {
        serde_json::to_string(&self.unmade).expect("reasons are JSON")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn answer(name: &str) -> String {
        std::fs::read_to_string(root().join(format!("crates/scaena-narrato/tests/answers/{name}.json"))).unwrap()
    }

    const BRIEF: &str = r#"{ "topic": "Pitch a Series A round for our carbon-negative cargo platform.", "audience": "Investors", "goal": "Persuade" }"#;
    const ASKING: &str = r#"{ "provider": "anthropic", "model": "a-model" }"#;

    #[test]
    fn the_choices_are_narratos() {
        let c: Value = serde_json::from_str(&choices()).unwrap();
        assert_eq!(c["goals"][1], "Persuade");
        assert_eq!(c["lengths"][0], "5-10 slides");
        assert_eq!(
            (c["modalities"][0]["id"].as_str(), c["modalities"][0]["label"].as_str()),
            (Some("in-person"), Some("In-person"))
        );
        assert!(c["arcs"].as_array().unwrap().iter().any(|a| a == "Problem\u{2013}Solution"));
        assert!(c["audiences"].as_array().unwrap().iter().any(|a| a == "Investors"));
        assert_eq!(c["defaults"]["length"], "5-10 slides");
    }

    /// A generation as the worker drives it, on recorded answers: the sources read, the arc and
    /// the slides asked and answered, the deck built in Dusk, and the bundle made of it as the
    /// editor's module makes it.
    #[test]
    fn a_deck_is_generated_from_sources_as_the_worker_drives_it() {
        let mut sources = Sources::new();
        let said: Value =
            serde_json::from_str(&sources.add("notes.md", b"# Tidewater\nRevenue tripled in Q3.").unwrap()).unwrap();
        assert_eq!(said, json!({ "name": "notes.md", "kind": "markdown" }));
        sources.add("q3.csv", b"region,revenue\nGulf,4.1\nAtlantic,2.2\n").unwrap();
        assert_eq!(sources.count(), 2);
        let tables: Value = serde_json::from_str(&sources.tables()).unwrap();
        assert_eq!(tables[0]["hint"], "q3");
        assert_eq!(tables[0]["csv"], "region,revenue\nGulf,4.1\nAtlantic,2.2\n");

        let mut g = Generation::new(BRIEF, ASKING, &sources).unwrap();
        let given: Value = serde_json::from_str(&g.given()).unwrap();
        assert_eq!(given[0]["name"], "notes.md");
        assert_eq!(g.step(), "arc");
        let ask: Value = serde_json::from_str(&g.request("sk-test").unwrap()).unwrap();
        assert_eq!(ask["headers"]["anthropic-dangerous-direct-browser-access"], "true", "a page calls the API itself");
        let text = ask["body"]["messages"][0]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Topic: Pitch a Series A round"));
        let at: Value = serde_json::from_str(&g.answer(200, "OK", &answer("anthropic-arc"), None).unwrap()).unwrap();
        assert_eq!((at["step"].as_str(), at["arc"].as_str()), (Some("slides"), Some("Problem\u{2013}Solution")));
        let ask: Value = serde_json::from_str(&g.request("sk-test").unwrap()).unwrap();
        let prompt = ask["body"]["messages"][0]["content"][0]["text"].as_str().unwrap();
        assert!(prompt.contains("=== SOURCE: notes.md ===") && prompt.contains("Revenue tripled in Q3."));
        let at: Value = serde_json::from_str(&g.answer(200, "OK", &answer("anthropic-slides"), None).unwrap()).unwrap();
        assert_eq!((at["step"].as_str(), at["written"].as_u64()), (Some("done"), Some(5)));
        assert!(g.request("sk-test").is_none());

        let dusk = std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap();
        let built: Value = serde_json::from_str(&g.deck(&dusk, "{}").unwrap()).unwrap();
        assert_eq!(built["arc"], "Problem\u{2013}Solution");
        assert_eq!(built["layouts"].as_array().unwrap().len(), 5);
        // What the editor's module makes of it: the bundle `scaena generate` makes.
        let made: scaena_ops::narrative::Made = serde_json::from_value(built.clone()).unwrap();
        let font = |file: &str| -> Result<Vec<u8>, scaena_ops::OpsError> {
            Ok(std::fs::read(root().join("docs/examples").join(file))?)
        };
        let (generated, write) =
            scaena_ops::narrative::making(made, "Problem\u{2013}Solution", Path::new("dusk.theme.json"), dusk, &font)
                .unwrap();
        assert!(generated.created.created, "{:#?}", generated.created.findings);
        assert!(write.is_some());

        // A text that runs past its place is asked for shorter, and the answer read.
        let longs = r#"[{ "id": "a-title", "text": "Freight emissions are still climbing across every corridor", "most": 30 }]"#;
        let fit: Value = serde_json::from_str(&g.fit_request("sk-test", longs).unwrap().unwrap()).unwrap();
        assert!(fit["body"]["messages"][0]["content"][0]["text"].as_str().unwrap().contains("id: a-title"));
        let answer = json!({ "content": [{ "type": "text", "text": r#"{"texts": [{"id": "a-title", "text": "Emissions keep climbing"}]}"# }] });
        let rewrites = g.fit_answer(200, "OK", &answer.to_string(), longs).unwrap();
        assert_eq!(rewrites, r#"[["a-title","Emissions keep climbing"]]"#);
    }

    #[test]
    fn a_link_is_fetched_by_a_plan_of_gets() {
        let mut fetching = Sources::fetching("github:a/b", None).unwrap().unwrap();
        let first: Value = serde_json::from_str(&fetching.request().unwrap()).unwrap();
        assert_eq!(first["url"], "https://api.github.com/repos/a/b");
        fetching.answer(200, "application/json", br#"{"default_branch": "main"}"#).unwrap();
        let tree: Value = serde_json::from_str(&fetching.request().unwrap()).unwrap();
        assert_eq!(tree["url"], "https://api.github.com/repos/a/b/git/trees/main?recursive=1");
        assert!(Sources::fetching("notes.md", None).unwrap().is_none(), "a file is read, not fetched");
    }

    #[test]
    fn research_is_asked_for_the_brief_as_narrato_reads_it() {
        let gemini = r#"{ "provider": "gemini", "model": "gemini-3-pro" }"#;
        let mut r = Research::new(BRIEF, gemini).unwrap();
        let start: Value = serde_json::from_str(&r.start("k")).unwrap();
        assert!(start["url"].as_str().unwrap().ends_with("/v1beta/interactions"));
        let next: Value = serde_json::from_str(&r.answer("k", 200, "OK", r#"{"id": "i-1"}"#).unwrap()).unwrap();
        assert!(next["send"]["url"].as_str().unwrap().ends_with("/v1beta/interactions/i-1"), "polled at once");
        let busy = r#"{"status": "in_progress", "progress": "Reading 12 sources"}"#;
        let next: Value = serde_json::from_str(&r.answer("k", 200, "OK", busy).unwrap()).unwrap();
        assert_eq!((next["wait"].as_u64(), next["progress"].as_str()), (Some(30_000), Some("Reading 12 sources")));
    }
}

/// A map being fetched from Geoapify (ADR-0030, PLAN O.1c), the page making each GET by `fetch`
/// with the user's key in the slot the request names (`keyedUrl`): its first request, then each
/// answer handed back, until it offers places to pick from or the map is fetched.
#[wasm_bindgen]
pub struct MapFetching {
    plan: scaena_objects::map::MapPlan,
    first: Option<scaena_objects::Get>,
    done: Option<scaena_objects::map::Map>,
}

#[wasm_bindgen]
impl MapFetching {
    /// The plan for `ask` (JSON, as `scaena_objects::map::Asked`), the browser setting its own
    /// User-Agent.
    #[wasm_bindgen(constructor)]
    pub fn new(ask: &str) -> Result<MapFetching, JsError> {
        let asked: scaena_objects::map::Asked = serde_json::from_str(ask).map_err(error)?;
        let ask = asked.ask(true).map_err(error)?;
        let (plan, first) = scaena_objects::map::MapPlan::new(ask).map_err(error)?;
        Ok(MapFetching { plan, first: Some(first), done: None })
    }

    /// The first request, JSON `{ url, headers, most, service, key }`.
    pub fn first(&mut self) -> Result<String, JsError> {
        let get = self.first.take().ok_or_else(|| error("the first request was asked for already"))?;
        Ok(serde_json::to_string(&get).expect("a request is JSON"))
    }

    /// The answer to the last request: JSON `{ get }`, the next; `{ choose: [place] }`, the places
    /// a name matched; or `{ done: { fetched, pin, place } }`, the map, its picture by `png`; its
    /// record says its answer came `at` (RFC 3339, UTC, by the page's clock).
    pub fn answer(&mut self, status: u16, kind: &str, body: Vec<u8>, at: &str) -> Result<String, JsError> {
        use scaena_objects::{Answer, Plan, Step};
        let answer = Answer { status, kind: kind.into(), body };
        match self.plan.answer(&answer).map_err(error)? {
            Step::Get(get) => Ok(json!({ "get": get }).to_string()),
            Step::Choose(found) => Ok(json!({ "choose": found }).to_string()),
            Step::Done(map) => {
                let said = json!({ "done": { "fetched": map.fetched(at), "pin": map.pin, "place": map.place_name() } });
                self.done = Some(map);
                Ok(said.to_string())
            }
        }
    }

    /// The map's picture, once it is fetched.
    pub fn png(&mut self) -> Result<Vec<u8>, JsError> {
        Ok(self.done.take().ok_or_else(|| error("no map is fetched yet"))?.png)
    }
}

/// The URL to send for `get` (JSON, a `MapFetching` request), `key` in its query where the key goes
/// there: what the page sends and never writes down.
#[wasm_bindgen(js_name = keyedUrl)]
pub fn keyed_url(get: &str, key: &str) -> Result<String, JsError> {
    let get: scaena_objects::Get = serde_json::from_str(get).map_err(error)?;
    Ok(get.keyed_url(key))
}

/// Where a slide goes, as the page names it: `{ "after": STATE }`, `{ "replace": STATE }`, or
/// `{}` for the end (lift-up FUNC-1).
fn place(place: &str) -> Result<scaena_ops::narrative::Place, JsError> {
    placed(place).map(|(place, _)| place)
}

/// Where a slide goes, from `{ "after" }`, `{ "replace" }`, or neither, and the beat of the spine it
/// joins, from `{ "beat" }` (lift-up M1).
fn placed(place: &str) -> Result<(scaena_ops::narrative::Place, Option<String>), JsError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Named {
        #[serde(default)]
        after: Option<String>,
        #[serde(default)]
        replace: Option<String>,
        #[serde(default)]
        beat: Option<String>,
    }
    let named: Named = serde_json::from_str(place).map_err(|e| error(format!("where the slide goes: {e}")))?;
    let place = match (named.after, named.replace) {
        (Some(_), Some(_)) => return Err(error("give `after` or `replace`, not both")),
        (Some(state), None) => scaena_ops::narrative::Place::After(state),
        (None, Some(state)) => scaena_ops::narrative::Place::Replace(state),
        (None, None) => scaena_ops::narrative::Place::End,
    };
    Ok((place, named.beat))
}

/// One slide written for a deck (lift-up FUNC-1), as Write This Slide Again… and Add a Slide
/// About… write it: what to ask the model, and the slide its answer holds built for the deck, the
/// bundle's data files handed over first, which the rows of the chart a slide draws are read from.
/// The editor's module makes the slide built one patch (`Player.slidePolishing`).
#[wasm_bindgen]
pub struct SlideWriting {
    files: std::collections::BTreeMap<String, Vec<u8>>,
}

#[wasm_bindgen]
impl SlideWriting {
    #[wasm_bindgen(constructor)]
    pub fn new() -> SlideWriting {
        SlideWriting { files: Default::default() }
    }

    /// The bundle's file at `path`.
    pub fn add(&mut self, path: &str, bytes: Vec<u8>) {
        self.files.insert(path.to_string(), bytes);
    }

    /// What to ask for one slide of `deck` (`deck.json`) at `place`, with the person's `ask`:
    /// `{ system, prompt }`, the request's two halves.
    pub fn asked(&self, deck: &str, place_json: &str, ask: &str) -> Result<String, JsError> {
        let doc: Value = serde_json::from_str(deck).map_err(error)?;
        let one = scaena_ops::narrative::one_asked(&doc, &place(place_json)?, ask, &self.files).map_err(error)?;
        let asked =
            json!({ "system": scaena_narrato::one::ONE_SLIDE_SYSTEM, "prompt": scaena_narrato::one::prompt(&one) });
        Ok(asked.to_string())
    }

    /// `slide` (a slide as Narrato writes one, as `oneAnswered` reads it) built for `deck` in
    /// `theme` (its JSON) at `place`, as the deck's generator builds a slide, each icon its words
    /// name from `icons` (`{ word: { name, path, view } }`, as `giveIcons` takes them): the
    /// `Written` the editor's module polishes, `{ patch, states, layout, beat, what }`.
    pub fn built(
        &self,
        deck: &str,
        theme: &str,
        slide: &str,
        place_json: &str,
        icons: &str,
    ) -> Result<String, JsError> {
        let doc: Value = serde_json::from_str(deck).map_err(error)?;
        let theme: scaena_core::model::theme::Theme =
            serde_json::from_str(theme).map_err(|e| error(format!("the theme: {e}")))?;
        let slide: scaena_ops::narrative::Slide =
            serde_json::from_str(slide).map_err(|e| error(format!("the slide: {e}")))?;
        let icons = serde_json::from_str(icons).map_err(|e| error(format!("the icons: {e}")))?;
        let (place, beat) = placed(place_json)?;
        let written =
            scaena_ops::narrative::slide_into(&doc, &theme, slide, &place, beat.as_deref(), icons, &self.files)
                .map_err(error)?;
        serde_json::to_string(&written).map_err(error)
    }
}

impl Default for SlideWriting {
    fn default() -> Self {
        Self::new()
    }
}

/// The one slide a model's `answer` holds, as Narrato writes a slide: what
/// `SlideWriting.built` builds.
#[wasm_bindgen(js_name = oneAnswered)]
pub fn one_answered(answer: &str) -> Result<String, JsError> {
    let slide = scaena_narrato::one::answer(answer).map_err(error)?;
    serde_json::to_string(&slide).map_err(error)
}
