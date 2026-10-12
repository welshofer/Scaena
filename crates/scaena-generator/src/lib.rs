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
use scaena_narrato::brief::{DEFAULT_AUDIENCE, DEFAULT_GOAL, DEFAULT_LENGTH, GOALS, LENGTHS, MODALITIES};
use scaena_narrato::fit::Long;
use scaena_narrato::research::{Asked, Next, Report};
use scaena_narrato::{Brief, Document, Provider, Settings};
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
    let modalities: Vec<Value> = MODALITIES
        .iter()
        .map(|(id, said)| json!({ "id": id, "label": said.split(" \u{2014} ").next().unwrap_or(said) }))
        .collect();
    let audiences: Vec<&str> = scaena_narrato::arcs::personas().iter().map(|p| p.name.as_str()).collect();
    let arcs: Vec<&str> = scaena_narrato::arcs::arcs().iter().map(|a| a.arc.as_str()).collect();
    json!({
        "goals": GOALS,
        "lengths": LENGTHS,
        "modalities": modalities,
        "audiences": audiences,
        "arcs": arcs,
        "defaults": { "audience": DEFAULT_AUDIENCE, "goal": DEFAULT_GOAL, "length": DEFAULT_LENGTH },
    })
    .to_string()
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
        let inner = scaena_narrato::Generation::new(brief, settings).map_err(error)?;
        Ok(Generation { inner, documents, given: ground.given })
    }

    /// What each source gave the prompt, JSON `[{ name, kind, bytes, truncated }]`.
    pub fn given(&self) -> String {
        serde_json::to_string(&self.given).expect("what was given is JSON")
    }

    /// The arc, by its name, or the heuristic's: none is asked of the model.
    #[wasm_bindgen(js_name = chooseArc)]
    pub fn choose_arc(&mut self, name: Option<String>) {
        self.inner.choose_arc(name.as_deref());
    }

    /// Where it stands: `arc`, `slides`, or `done`.
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

    /// The answer to the last request read: where the generation stands, JSON `{ step, slides,
    /// arc?, fallback?, written?, truncated? }`, `slides` the fewest asked for and `written` how
    /// many came back.
    pub fn answer(&mut self, status: u16, status_text: &str, body: &str) -> Result<String, JsError> {
        self.inner.answer(status, status_text, body).map_err(error)?;
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
            "fallback": self.inner.fallback(),
            "written": narrative.map(|n| n.slides.len()),
            "truncated": narrative.map(|n| n.truncated),
        })
        .to_string()
    }

    /// The deck the narrative builds in `theme`, the theme's JSON (`scaena_ops::narrative::deck`):
    /// JSON `{ deck, layouts, arc, fallback? }`, which the editor's `Making` makes a bundle.
    pub fn deck(&self, theme: &str) -> Result<String, JsError> {
        let narrative = self.inner.narrative().ok_or_else(|| error("the narrative is not written yet"))?;
        let model: scaena_core::model::theme::Theme =
            serde_json::from_str(theme).map_err(|e| error(format!("the theme: {e}")))?;
        let made = scaena_ops::narrative::deck(narrative, &model).map_err(error)?;
        let mut out = serde_json::to_value(&made).map_err(error)?;
        out["arc"] = json!(narrative.arc);
        if let Some(why) = self.inner.fallback() {
            out["fallback"] = json!(why);
        }
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

    /// The rewrites an answer to `fitRequest` gives, JSON `[[id, text]]`, each shorter than the
    /// text it rewrites: what `Making.fitted` takes.
    #[wasm_bindgen(js_name = fitAnswer)]
    pub fn fit_answer(&self, status: u16, status_text: &str, body: &str, longs: &str) -> Result<String, JsError> {
        let longs: Vec<Long> = serde_json::from_str(longs).map_err(error)?;
        let rewrites = self.inner.fit_answer(status, status_text, body, &longs).map_err(error)?;
        Ok(serde_json::to_string(&rewrites).expect("rewrites are JSON"))
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
        let at: Value = serde_json::from_str(&g.answer(200, "OK", &answer("anthropic-arc")).unwrap()).unwrap();
        assert_eq!((at["step"].as_str(), at["arc"].as_str()), (Some("slides"), Some("Problem\u{2013}Solution")));
        let ask: Value = serde_json::from_str(&g.request("sk-test").unwrap()).unwrap();
        let prompt = ask["body"]["messages"][0]["content"][0]["text"].as_str().unwrap();
        assert!(prompt.contains("=== SOURCE: notes.md ===") && prompt.contains("Revenue tripled in Q3."));
        let at: Value = serde_json::from_str(&g.answer(200, "OK", &answer("anthropic-slides")).unwrap()).unwrap();
        assert_eq!((at["step"].as_str(), at["written"].as_u64()), (Some("done"), Some(5)));
        assert!(g.request("sk-test").is_none());

        let dusk = std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap();
        let built: Value = serde_json::from_str(&g.deck(&dusk).unwrap()).unwrap();
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
